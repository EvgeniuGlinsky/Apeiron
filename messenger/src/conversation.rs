//! The owner of one contact's conversation: its `Chat`, its `Pair`, and the rule of the order.
//!
//! A round is **receive → choose what to put → commit → put → commit**. The state item that
//! acknowledges what was just decrypted is made in the middle step and committed with the
//! messages before it is put. The other way round (`engine::round` as it is), a crash between
//! the put and the commit leaves the peer believing we have messages we never kept — it stops
//! re-putting them — and the state item's `seq` taken by a value the restored pair does not
//! know, so the next one is refused by the nodes.

use std::collections::BTreeMap;

use apeiron_core::{Chat, Identity};
use apeiron_store::repo::Contact;
use apeiron_store::Storage;
use apeiron_transport::dht::Dht;
use apeiron_transport::engine;
use apeiron_transport::pair::{Event, IntroStatus, Pair};
use zeroize::Zeroizing;

use crate::record::{ConversationRecord, MessageRecord, Origin, Status};
use crate::{MessengerError, StoreAccess, Update};

/// What a contact is, as the list shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContactStatus {
    /// I accepted their invitation; they have not taken my reply yet.
    Waiting,
    Live,
    /// Their invitation expired unanswered. Still listened to, in case of a late answer.
    NotAccepted,
    /// Someone else answered their invitation first.
    Taken,
    /// The conversation's records do not load.
    Damaged,
}

pub struct ContactView {
    pub contact: Contact,
    pub status: ContactStatus,
}

/// How a new history entry is linked into the conversation record.
#[derive(Clone, Copy)]
enum Link {
    None,
    /// My message with this first index: its status follows the peer's acknowledgements.
    Pending(u64),
}

/// Changed history entries: row and new bytes.
type Changed = Vec<(i64, Zeroizing<Vec<u8>>)>;

pub struct Conversation {
    contact: Contact,
    chat: Chat,
    pair: Pair,
    origin: Origin,
    intro_message: Option<i64>,
    pending: BTreeMap<u64, i64>,
    /// The conversation record as this owner last loaded or wrote it.
    committed: Zeroizing<Vec<u8>>,
    /// Set while an operation is under way; left set by a failure. The `Chat` may then have
    /// advanced past what is stored, and this value must not be used again.
    poisoned: bool,
}

impl Conversation {
    /// Loads the conversation with `contact_id`: exactly one session, and the pair state stored
    /// for that session (`Pair::restore` refuses another's).
    pub fn load(store: &Storage, me: &Identity, contact_id: i64) -> Result<Self, MessengerError> {
        let contact = store
            .contact(contact_id)?
            .ok_or(MessengerError::NoContact)?;
        let mut chats = store.load_chats(contact_id)?;
        let chat = match (chats.pop(), chats.is_empty()) {
            (Some(chat), true) => chat,
            _ => return Err(MessengerError::Corrupt("not exactly one session")),
        };
        let committed = store
            .load_pair_state(contact_id)?
            .ok_or(MessengerError::Corrupt("no conversation record"))?;
        let record = ConversationRecord::decode(&committed)?;
        let pair = Pair::restore(me, &contact.peer, &chat.session_id(), &record.pair)?;
        Ok(Self {
            contact,
            chat,
            pair,
            origin: record.origin,
            intro_message: record.intro_message,
            pending: record.pending,
            committed,
            poisoned: false,
        })
    }

    pub fn contact(&self) -> &Contact {
        &self.contact
    }

    pub fn origin(&self) -> Origin {
        self.origin
    }

    pub fn status(&self) -> ContactStatus {
        match self.pair.intro_status() {
            IntroStatus::None => ContactStatus::Live,
            IntroStatus::Waiting => ContactStatus::Waiting,
            IntroStatus::Expired => ContactStatus::NotAccepted,
            IntroStatus::Taken => ContactStatus::Taken,
        }
    }

    /// Whether this conversation still works or waits — the opposite of one that can be
    /// replaced by a new introduction.
    pub fn is_working(&self) -> bool {
        matches!(
            self.pair.intro_status(),
            IntroStatus::None | IntroStatus::Waiting
        )
    }

    fn start(&mut self) -> Result<(), MessengerError> {
        if self.poisoned {
            return Err(MessengerError::Reload);
        }
        self.poisoned = true;
        Ok(())
    }

    /// Writes `text` into the conversation: encrypted, its parts signed and committed with the
    /// session. Nothing goes to the network here; the next round puts it.
    pub fn send(
        &mut self,
        store: &impl StoreAccess,
        text: &str,
        now: u64,
    ) -> Result<i64, MessengerError> {
        if text.trim().is_empty() {
            return Err(MessengerError::Empty);
        }
        if self.pair.intro_status() == IntroStatus::Taken || self.pair.is_closed(now) {
            return Err(MessengerError::Closed);
        }
        self.start()?;
        let first = self.pair.send(&mut self.chat, text, now)?;
        let ids = self.commit(
            store,
            vec![(
                MessageRecord::mine(Some(first), now, text),
                Link::Pending(first),
            )],
            Vec::new(),
        )?;
        self.poisoned = false;
        ids.first()
            .copied()
            .ok_or(MessengerError::Corrupt("no message id"))
    }

    /// One round: what arrived is received and committed, then what is due is put.
    pub fn round(
        &mut self,
        store: &impl StoreAccess,
        dht: &impl Dht,
        now: u64,
    ) -> Result<Vec<Update>, MessengerError> {
        if self.pair.is_closed(now) {
            return Ok(Vec::new());
        }
        self.start()?;
        let mut events = engine::receive_round(&mut self.pair, &mut self.chat, dht, now)?;
        // The state item made here says what was just received: committed before it is put.
        let mut due = self.pair.due(now)?;
        events.append(&mut due.events);
        let mut updates = self.apply(store, events, now)?;
        let events = engine::put_due(&mut self.pair, dht, due, now);
        updates.extend(self.apply(store, events, now)?);
        self.poisoned = false;
        Ok(updates)
    }

    /// Turns the transport's events into history and commits them with the conversation.
    fn apply(
        &mut self,
        store: &impl StoreAccess,
        events: Vec<Event>,
        now: u64,
    ) -> Result<Vec<Update>, MessengerError> {
        let contact = self.contact.id;
        let mut new = Vec::new();
        let mut status = Vec::new();
        let mut updates = Vec::new();
        for event in events {
            match event {
                Event::Received { first, text } => {
                    new.push((MessageRecord::theirs(Some(first), now, &text), Link::None))
                }
                Event::Lost { from, to, why } => {
                    new.push((MessageRecord::lost(from, to, why, now), Link::None))
                }
                Event::Sent { first } => {
                    if let Some(&id) = self.pending.get(&first) {
                        status.push((id, Status::Sent));
                    }
                }
                Event::Delivered { first } => {
                    if let Some(id) = self.pending.remove(&first) {
                        status.push((id, Status::Delivered));
                    }
                }
                Event::NotDelivered { first } => {
                    if let Some(id) = self.pending.remove(&first) {
                        status.push((id, Status::NotDelivered));
                    }
                }
                Event::Squatted { first } => {
                    if let Some(id) = self.pending.remove(&first) {
                        status.push((id, Status::AddressTaken));
                    }
                }
                Event::IntroSent => status.extend(self.intro_message.map(|id| (id, Status::Sent))),
                Event::Accepted => {
                    status.extend(self.intro_message.take().map(|id| (id, Status::Delivered)));
                    updates.push(Update::Contact { contact });
                }
                Event::InvitationTaken => {
                    // Nothing of this conversation will ever arrive.
                    status.extend(
                        self.intro_message
                            .take()
                            .into_iter()
                            .chain(std::mem::take(&mut self.pending).into_values())
                            .map(|id| (id, Status::NotDelivered)),
                    );
                    updates.push(Update::Contact { contact });
                }
                Event::NotAccepted => updates.push(Update::Contact { contact }),
            }
        }
        updates.extend(
            status
                .iter()
                .map(|&(message, _)| Update::Changed { contact, message }),
        );
        let ids = self.commit(store, new, status)?;
        updates.extend(
            ids.into_iter()
                .map(|message| Update::Message { contact, message }),
        );
        Ok(updates)
    }

    /// Commits the session, the pair and the history changes in one transaction — if anything
    /// changed, and only if the stored conversation is still the one this owner holds.
    fn commit(
        &mut self,
        store: &impl StoreAccess,
        new: Vec<(MessageRecord, Link)>,
        status: Vec<(i64, Status)>,
    ) -> Result<Vec<i64>, MessengerError> {
        let base = ConversationRecord {
            origin: self.origin,
            intro_message: self.intro_message,
            pending: self.pending.clone(),
            pair: self.pair.to_bytes()?,
        };
        // A decryption or an encryption always moves the pair too, so an unchanged record
        // means an unchanged session.
        if new.is_empty() && status.is_empty() && *base.encode() == *self.committed {
            return Ok(Vec::new());
        }
        let contact = self.contact.id;
        let plain: Vec<Zeroizing<Vec<u8>>> = new.iter().map(|(m, _)| m.encode()).collect();
        let links: Vec<Link> = new.iter().map(|&(_, link)| link).collect();
        let expected = &self.committed;
        let chat = &self.chat;
        let (ids, written) = store.with(|s| {
            // One owner: nobody else has written this conversation since we loaded or wrote it.
            let on_disk = s.load_pair_state(contact)?;
            if on_disk.as_ref().map(|b| b.as_slice()) != Some(expected.as_slice()) {
                return Err(MessengerError::Reload);
            }
            let mut changed = Vec::new();
            for &(id, to) in &status {
                if let Some(old) = s.message(contact, id)? {
                    let mut record = MessageRecord::decode(&old)?;
                    if record.advance(to) {
                        changed.push((id, record.encode()));
                    }
                }
            }
            let mut written = None;
            let ids = s.commit_conversation(contact, chat, &plain, &changed, |ids| {
                let mut record = base;
                for (link, &id) in links.iter().zip(ids) {
                    match link {
                        Link::None => {}
                        Link::Pending(first) => {
                            record.pending.insert(*first, id);
                        }
                    }
                }
                let bytes = record.encode();
                written = Some((record.pending, record.intro_message, bytes.clone()));
                bytes
            })?;
            Ok((ids, written))
        })?;
        if let Some((pending, intro_message, bytes)) = written {
            self.pending = pending;
            self.intro_message = intro_message;
            self.committed = bytes;
        }
        Ok(ids)
    }

    /// My messages this conversation still waits on, as "not delivered": for the step that
    /// replaces it with a new introduction.
    pub(crate) fn abandoned(&self, store: &Storage) -> Result<Changed, MessengerError> {
        let mut out = Vec::new();
        for id in self.intro_message.iter().chain(self.pending.values()) {
            if let Some(old) = store.message(self.contact.id, *id)? {
                let mut record = MessageRecord::decode(&old)?;
                if record.advance(Status::NotDelivered) {
                    out.push((*id, record.encode()));
                }
            }
        }
        Ok(out)
    }
}

/// A page of the history with `contact`: at most `limit` entries older than `before`, newest
/// first. The order is the order in which they appeared on this phone.
pub fn history(
    store: &impl StoreAccess,
    contact: i64,
    before: Option<i64>,
    limit: u32,
) -> Result<Vec<(i64, MessageRecord)>, MessengerError> {
    store.with(|s| {
        s.messages(contact, before, limit)?
            .into_iter()
            .map(|(id, plain)| Ok((id, MessageRecord::decode(&plain)?)))
            .collect()
    })
}

/// Every contact with its status.
pub fn contacts(
    store: &impl StoreAccess,
    me: &Identity,
) -> Result<Vec<ContactView>, MessengerError> {
    store.with(|s| {
        Ok(s.contacts()?
            .into_iter()
            .map(|contact| {
                let status = Conversation::load(s, me, contact.id)
                    .map_or(ContactStatus::Damaged, |c| c.status());
                ContactView { contact, status }
            })
            .collect())
    })
}

/// A round for every conversation that is not closed. A conversation that fails is skipped
/// and reported, except for a locked vault, which ends the whole thing.
pub fn round_all(
    store: &impl StoreAccess,
    me: &Identity,
    dht: &impl Dht,
    now: u64,
) -> Result<Vec<Update>, MessengerError> {
    let ids: Vec<i64> = store.with(|s| Ok(s.contacts()?.iter().map(|c| c.id).collect()))?;
    let mut updates = Vec::new();
    for id in ids {
        let loaded = store.with(|s| Conversation::load(s, me, id));
        let result = loaded.and_then(|mut c| c.round(store, dht, now));
        match result {
            Ok(u) => updates.extend(u),
            Err(MessengerError::Locked) => return Err(MessengerError::Locked),
            Err(_) => {}
        }
    }
    Ok(updates)
}

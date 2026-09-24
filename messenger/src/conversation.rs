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

use crate::record::{ConversationRecord, MessageKind, MessageRecord, Origin, Status};
use crate::{MessengerError, StoreAccess, Update};

/// How far back a read receipt is looked for in my own messages. The newest one it covers is
/// usually among the last few; one older than this stays "delivered", which costs nothing.
const READ_SCAN: u32 = 200;

/// How many of the peer's entries the list counts as unread at most; it shows "99+" above.
const UNREAD_SCAN: u32 = 100;

/// The internal value holding the read-receipt setting: absent means on.
const READ_RECEIPTS_KEY: &str = "messenger/read-receipts";

/// Whether read receipts are sent — and, in return, shown (`docs/transport.md` §3). On unless
/// the owner turned them off.
pub fn read_receipts(store: &impl StoreAccess) -> Result<bool, MessengerError> {
    store.with(read_receipts_in)
}

fn read_receipts_in(s: &Storage) -> Result<bool, MessengerError> {
    Ok(s.meta_get(READ_RECEIPTS_KEY)?.as_deref() != Some(&[0][..]))
}

/// Turns read receipts on or off. What is already sent stays with the peer; the next state item
/// of every conversation is made without the mark.
pub fn set_read_receipts(store: &impl StoreAccess, on: bool) -> Result<(), MessengerError> {
    store.with(|s| Ok(s.meta_set(READ_RECEIPTS_KEY, &[u8::from(on)])?))
}

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
    /// The newest entry of the history, with its row.
    pub last: Option<(i64, MessageRecord)>,
    /// The peer's entries after what was last shown, counted up to [`UNREAD_SCAN`].
    pub unread: u32,
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
    read_mark: i64,
    /// The read-receipt setting as it was at load.
    receipts: bool,
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
        let mut pair = Pair::restore(me, &contact.peer, &chat.session_id(), &record.pair)?;
        let receipts = read_receipts_in(store)?;
        pair.set_receipts(receipts);
        Ok(Self {
            contact,
            chat,
            pair,
            origin: record.origin,
            intro_message: record.intro_message,
            pending: record.pending,
            read_mark: record.read_mark,
            receipts,
            committed,
            poisoned: false,
        })
    }

    /// The history up to row `upto` has been shown to the person. Returns whether anything
    /// changed. The receipt is the first index of the peer's newest message among those rows:
    /// the peer's messages arrive in order, so every one before it has been shown too.
    pub fn mark_read(
        &mut self,
        store: &impl StoreAccess,
        upto: i64,
    ) -> Result<bool, MessengerError> {
        if upto <= self.read_mark {
            return Ok(false);
        }
        let contact = self.contact.id;
        let first = store.with(|s| {
            for (_, plain) in s.messages(contact, Some(upto.saturating_add(1)), READ_SCAN)? {
                let m = MessageRecord::decode(&plain)?;
                if m.kind == MessageKind::Theirs && m.index.is_some() {
                    return Ok(m.index);
                }
            }
            Ok(None)
        })?;
        self.start()?;
        self.read_mark = upto;
        if let Some(first) = first {
            self.pair.mark_read(first);
        }
        self.commit(store, Vec::new(), Vec::new(), None)?;
        self.poisoned = false;
        Ok(true)
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
        let (ids, _) = self.commit(
            store,
            vec![(
                MessageRecord::mine(Some(first), now, text),
                Link::Pending(first),
            )],
            Vec::new(),
            None,
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
        let mut peer_read = None;
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
                // Shown only by whoever sends them too: turned off, both directions go quiet.
                Event::Read { to } if self.receipts => peer_read = peer_read.max(Some(to)),
                Event::Read { .. } => {}
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
        let (ids, read) = self.commit(store, new, status, peer_read)?;
        updates.extend(
            ids.into_iter()
                .map(|message| Update::Message { contact, message }),
        );
        updates.extend(
            read.into_iter()
                .map(|message| Update::Changed { contact, message }),
        );
        Ok(updates)
    }

    /// Commits the session, the pair and the history changes in one transaction — if anything
    /// changed, and only if the stored conversation is still the one this owner holds.
    ///
    /// `peer_read`: the peer's read receipt — my messages whose first index is below it become
    /// "read". Returns the rows of the new entries and of the messages that became read.
    fn commit(
        &mut self,
        store: &impl StoreAccess,
        new: Vec<(MessageRecord, Link)>,
        status: Vec<(i64, Status)>,
        peer_read: Option<u64>,
    ) -> Result<(Vec<i64>, Vec<i64>), MessengerError> {
        let base = ConversationRecord {
            origin: self.origin,
            intro_message: self.intro_message,
            read_mark: self.read_mark,
            pending: self.pending.clone(),
            pair: self.pair.to_bytes()?,
        };
        // A decryption or an encryption always moves the pair too, so an unchanged record
        // means an unchanged session.
        if new.is_empty()
            && status.is_empty()
            && peer_read.is_none()
            && *base.encode() == *self.committed
        {
            return Ok((Vec::new(), Vec::new()));
        }
        let contact = self.contact.id;
        let plain: Vec<Zeroizing<Vec<u8>>> = new.iter().map(|(m, _)| m.encode()).collect();
        let links: Vec<Link> = new.iter().map(|&(_, link)| link).collect();
        let expected = &self.committed;
        let chat = &self.chat;
        let (ids, read, written) = store.with(|s| {
            // One owner: nobody else has written this conversation since we loaded or wrote it.
            let on_disk = s.load_pair_state(contact)?;
            if on_disk.as_ref().map(|b| b.as_slice()) != Some(expected.as_slice()) {
                return Err(MessengerError::Reload);
            }
            // One entry per row, so a message delivered and read in the same round is written
            // once, as read.
            let mut touched: BTreeMap<i64, MessageRecord> = BTreeMap::new();
            for &(id, to) in &status {
                if let Some(old) = s.message(contact, id)? {
                    let mut record = MessageRecord::decode(&old)?;
                    if record.advance(to) {
                        touched.insert(id, record);
                    }
                }
            }
            let mut read = Vec::new();
            if let Some(to) = peer_read {
                for (id, bytes) in s.messages(contact, None, READ_SCAN)? {
                    let earlier = touched.remove(&id);
                    let dirty = earlier.is_some();
                    let mut record = match earlier {
                        Some(r) => r,
                        None => MessageRecord::decode(&bytes)?,
                    };
                    // Only mine are counted in my direction's indices; theirs never advance.
                    let covered = record.index.is_some_and(|i| i < to);
                    let done = covered && record.kind == MessageKind::Mine(Status::Read);
                    let advanced = covered && record.advance(Status::Read);
                    if advanced {
                        read.push(id);
                    }
                    if advanced || dirty {
                        touched.insert(id, record);
                    }
                    if done {
                        break; // the receipts before this one reached everything older
                    }
                }
            }
            let changed: Changed = touched
                .into_iter()
                .map(|(id, record)| (id, record.encode()))
                .collect();
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
            Ok((ids, read, written))
        })?;
        if let Some((pending, intro_message, bytes)) = written {
            self.pending = pending;
            self.intro_message = intro_message;
            self.committed = bytes;
        }
        Ok((ids, read))
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

/// Every contact with its status, its newest entry and how many of the peer's are unread; the
/// most recently active first. An entry that does not open is left out of the preview rather
/// than taking the whole list down.
pub fn contacts(
    store: &impl StoreAccess,
    me: &Identity,
) -> Result<Vec<ContactView>, MessengerError> {
    let mut all = store.with(|s| {
        let mut out = Vec::new();
        for contact in s.contacts()? {
            let (status, read_mark) = match Conversation::load(s, me, contact.id) {
                Ok(c) => (c.status(), Some(c.read_mark)),
                Err(_) => (ContactStatus::Damaged, None),
            };
            let mut last = None;
            let mut unread = 0;
            for (id, bytes) in s.messages(contact.id, None, UNREAD_SCAN)? {
                if last.is_some() && read_mark.is_none_or(|mark| id <= mark) {
                    break;
                }
                let Ok(m) = MessageRecord::decode(&bytes) else {
                    continue;
                };
                if m.kind == MessageKind::Theirs && read_mark.is_some_and(|mark| id > mark) {
                    unread += 1;
                }
                if last.is_none() {
                    last = Some((id, m));
                }
            }
            out.push(ContactView {
                contact,
                status,
                last,
                unread,
            });
        }
        Ok(out)
    })?;
    // Rows of the history are numbered in the order entries appeared on this phone, across
    // every contact, so the newest row is the latest activity.
    all.sort_by_key(|v| std::cmp::Reverse((v.last.as_ref().map(|(id, _)| *id), v.contact.id)));
    Ok(all)
}

/// What the verification screen shows: the safety number both sides share, and the two
/// fingerprints it is made of — for the case where the numbers differ and the two people have to
/// find out whose key is not the one the other holds.
pub struct Verification {
    pub safety_number: String,
    pub my_fingerprint: String,
    /// The peer's fingerprint as this side holds it.
    pub their_fingerprint: String,
}

/// The verification of `contact`. The bridge shows this and computes nothing of its own, so the
/// test of symmetry through two stores covers exactly what the screen shows.
pub fn verification(
    store: &impl StoreAccess,
    me: &Identity,
    contact: i64,
) -> Result<Verification, MessengerError> {
    let peer = store.with(|s| Ok(s.contact(contact)?.ok_or(MessengerError::NoContact)?.peer))?;
    let mine = me.public();
    Ok(Verification {
        safety_number: mine.safety_number(&peer),
        my_fingerprint: mine.fingerprint(),
        their_fingerprint: peer.fingerprint(),
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

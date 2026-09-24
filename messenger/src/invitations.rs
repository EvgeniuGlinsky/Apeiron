//! Getting to know each other, with the store (`docs/transport.md` §8).
//!
//! The account is read and written inside one [`StoreAccess::with`] every time, and never held
//! across the network: an account kept through a round's seconds on the network would be
//! written back over a one-time key another call has just published.
//!
//! **Who is already a contact.** The one who accepts refuses an inviter it has a working
//! conversation with ([`MessengerError::AlreadyContact`]): pasting the same invitation twice
//! would otherwise replace a live conversation by one whose reply is squatted by its own first
//! reply. A conversation that is closed, or that does not load, is replaced — that is the way
//! back. The inviter replaces a conversation when a reply comes from a contact — except when
//! two invitations crossed, see [`poll_invitations`].

use apeiron_core::Identity;
use apeiron_store::repo::Introduction;
use apeiron_store::Storage;
use apeiron_transport::address::day_of;
use apeiron_transport::dht::Dht;
use apeiron_transport::invite::{self, Invitation};
use apeiron_transport::pair::INTRO_GRACE_DAYS;
use zeroize::Zeroizing;

use crate::conversation::Conversation;
use crate::record::{ConversationRecord, InvitationRecord, MessageRecord, Origin};
use crate::{MessengerError, StoreAccess, Update};

pub struct InvitationView {
    pub id: i64,
    /// Who it is for: the name the contact will get.
    pub name: String,
    pub created: u64,
    /// The last UTC day on which it can be accepted.
    pub expires_day: u32,
    /// The text to send, `apeiron:…`.
    pub text: String,
}

struct Open {
    id: i64,
    name: String,
    invitation: Invitation,
    created: u64,
}

impl Open {
    fn view(&self) -> InvitationView {
        InvitationView {
            id: self.id,
            name: self.name.clone(),
            created: self.created,
            expires_day: self.invitation.expires(),
            text: self.invitation.to_text(),
        }
    }
}

fn open_invitations(s: &Storage) -> Result<Vec<Open>, MessengerError> {
    s.invitations()?
        .into_iter()
        .map(|(id, plain)| {
            let record = InvitationRecord::decode(&plain)?;
            Ok(Open {
                id,
                name: record.name,
                invitation: Invitation::from_bytes(&record.invitation)?,
                created: record.created,
            })
        })
        .collect()
}

/// A new invitation for `name`. Saved with the account that published its one-time key, in
/// one transaction.
pub fn create_invitation(
    store: &impl StoreAccess,
    me: &Identity,
    name: &str,
    now: u64,
) -> Result<InvitationView, MessengerError> {
    store.with(|s| {
        let mut account = s.load_account()?.ok_or(MessengerError::NoAccount)?;
        let invitation = Invitation::create(me, &mut account, day_of(now))?;
        let record = InvitationRecord {
            created: now,
            invitation: invitation.to_bytes(),
            name: name.to_string(),
        };
        let id = s.save_invitation(&record.encode(), &account)?;
        Ok(Open {
            id,
            name: record.name,
            invitation,
            created: now,
        }
        .view())
    })
}

/// The invitations that wait for an answer.
pub fn invitations(store: &impl StoreAccess) -> Result<Vec<InvitationView>, MessengerError> {
    store.with(|s| Ok(open_invitations(s)?.iter().map(Open::view).collect()))
}

/// Withdraws an invitation: forgotten, and its one-time key removed from the account.
pub fn cancel_invitation(store: &impl StoreAccess, id: i64) -> Result<(), MessengerError> {
    store.with(|s| {
        let open = open_invitations(s)?
            .into_iter()
            .find(|o| o.id == id)
            .ok_or(MessengerError::NoInvitation)?;
        retire(s, &open)
    })
}

fn retire(s: &Storage, open: &Open) -> Result<(), MessengerError> {
    let mut account = s.load_account()?.ok_or(MessengerError::NoAccount)?;
    account.remove_one_time_key(open.invitation.one_time_key());
    s.retire_invitation(open.id, &account)?;
    Ok(())
}

/// Asks every open invitation's inbox, opens the replies found, and retires the invitations
/// past their expiry (a day of grace after it, as the one who accepts keeps re-putting).
///
/// A value in an inbox that does not open is not a reply to us: ignored, and the invitation
/// waits on.
pub fn poll_invitations(
    store: &impl StoreAccess,
    me: &Identity,
    dht: &impl Dht,
    now: u64,
) -> Result<Vec<Update>, MessengerError> {
    let today = day_of(now);
    let mut updates = Vec::new();
    let mut waiting = Vec::new();
    for open in store.with(open_invitations)? {
        if today > open.invitation.expires().saturating_add(INTRO_GRACE_DAYS) {
            store.with(|s| retire(s, &open))?;
            updates.push(Update::InvitationGone {
                invitation: open.id,
            });
        } else {
            waiting.push(open);
        }
    }
    let keys = waiting
        .iter()
        .map(|o| o.invitation.inbox_key())
        .collect::<Result<Vec<_>, _>>()?;
    for (open, found) in waiting.iter().zip(dht.get_first_many(&keys)) {
        let Ok(Some(found)) = found else { continue };
        match store.with(|s| join(s, me, open, &found.value, now)) {
            Ok(contact) => {
                updates.extend(contact.map(|contact| Update::Contact { contact }));
                updates.push(Update::InvitationGone {
                    invitation: open.id,
                });
            }
            // Not a reply that opens for us, or one that fails a check: not ours.
            Err(MessengerError::Transport(_)) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(updates)
}

/// The inviter opens a reply. Returns the contact, or `None` when the reply lost to a crossed
/// invitation and was dropped.
fn join(
    s: &Storage,
    me: &Identity,
    open: &Open,
    value: &[u8],
    now: u64,
) -> Result<Option<i64>, MessengerError> {
    let mut account = s.load_account()?.ok_or(MessengerError::NoAccount)?;
    let joined = invite::open_reply(me, &mut account, &open.invitation, value)?;

    let mut changed = Vec::new();
    if let Some(existing) = s.find_contact(&joined.peer)? {
        if let Ok(current) = Conversation::load(s, me, existing.id) {
            // Crossed invitations: I accepted theirs, they accepted mine. Each side must keep
            // the same one of the two sessions, whatever state either has reached, so the
            // choice is by origin, never by status: the invitation of the lower identity wins.
            if current.origin() == Origin::Accepted
                && current.is_working()
                && me.public().to_bytes() > joined.peer.to_bytes()
            {
                // Theirs wins: my session from accepting it stays. The one-time key of mine
                // this reply used is already out of the account copy; saved with the retiring.
                s.retire_invitation(open.id, &account)?;
                return Ok(None);
            }
            changed = current.abandoned(s)?;
        }
    }

    let first: Vec<Zeroizing<Vec<u8>>> = if joined.first_text.trim().is_empty() {
        Vec::new()
    } else {
        vec![MessageRecord::theirs(None, now, &joined.first_text).encode()]
    };
    let record = ConversationRecord {
        origin: Origin::Invited,
        intro_message: None,
        read_mark: 0,
        pending: Default::default(),
        pair: joined.pair.to_bytes()?,
    };
    let (contact, _) = s.introduce(
        Introduction {
            peer: &joined.peer,
            name: &open.name,
            chat: &joined.chat,
            account: &account,
            messages: &first,
            changed: &changed,
            answered: Some(open.id),
        },
        |_| record.encode(),
    )?;
    Ok(Some(contact))
}

/// Accepts an invitation pasted as text, names the inviter `name`, and says `first_text`
/// (may be empty). The reply is put by the conversation's first round.
pub fn accept_invitation(
    store: &impl StoreAccess,
    me: &Identity,
    text: &str,
    name: &str,
    first_text: &str,
    now: u64,
) -> Result<i64, MessengerError> {
    let invitation = Invitation::from_text(text)?;
    store.with(|s| {
        let mut changed = Vec::new();
        if let Some(existing) = s.find_contact(invitation.inviter())? {
            if let Ok(current) = Conversation::load(s, me, existing.id) {
                if current.is_working() {
                    return Err(MessengerError::AlreadyContact);
                }
                changed = current.abandoned(s)?;
            }
        }
        let mut account = s.load_account()?.ok_or(MessengerError::NoAccount)?;
        let accepted = invite::accept(me, &mut account, &invitation, first_text, day_of(now))?;
        let first: Vec<Zeroizing<Vec<u8>>> = if first_text.trim().is_empty() {
            Vec::new()
        } else {
            vec![MessageRecord::mine(None, now, first_text).encode()]
        };
        let pair = accepted.pair.to_bytes()?;
        let (contact, _) = s.introduce(
            Introduction {
                peer: invitation.inviter(),
                name,
                chat: &accepted.chat,
                account: &account,
                messages: &first,
                changed: &changed,
                answered: None,
            },
            |ids| {
                ConversationRecord {
                    origin: Origin::Accepted,
                    intro_message: ids.first().copied(),
                    read_mark: 0,
                    pending: Default::default(),
                    pair,
                }
                .encode()
            },
        )?;
        Ok(contact)
    })
}

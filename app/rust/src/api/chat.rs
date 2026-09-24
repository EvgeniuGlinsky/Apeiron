//! Conversations, as seen from Dart: thin calls into `apeiron-messenger`.
//!
//! What crosses: names, statuses, and the visible page of a history — its texts included,
//! since they are what the screen shows (R-004: the history lives in Rust and reaches Dart a
//! page at a time). Keys, sessions and pair states never do.

use apeiron_messenger::{
    self as messenger, ContactStatus, MessageKind, MessageRecord, MessengerError, Status,
};

use crate::messaging::{identity, load, network, now, one_round_at_a_time, BridgeStore};

pub enum ContactState {
    /// I accepted their invitation; they have not taken my reply yet.
    Waiting,
    Live,
    /// Their invitation expired unanswered.
    NotAccepted,
    /// Someone else answered their invitation first.
    Taken,
    /// The conversation does not load.
    Damaged,
}

pub struct ContactItem {
    pub id: i64,
    pub name: String,
    pub state: ContactState,
    pub verified: bool,
    /// The newest entry of the history, for the list's preview.
    pub last: Option<MessageItem>,
    /// The contact's messages not shown yet; 100 means "100 or more".
    pub unread: u32,
}

pub struct InvitationItem {
    pub id: i64,
    pub name: String,
    /// The last UTC day, counted from 1970-01-01, on which it can be accepted.
    pub expires_day: u32,
    /// The text to send: `apeiron:…`.
    pub text: String,
}

pub enum MessageState {
    Queued,
    Sent,
    Delivered,
    /// The contact has been shown it.
    Read,
    NotDelivered,
    AddressTaken,
    Received,
    /// Some of the peer's messages will never be read.
    Lost,
}

pub struct MessageItem {
    pub id: i64,
    pub mine: bool,
    pub state: MessageState,
    /// Unix seconds: written (mine) or decrypted (theirs).
    pub at: i64,
    pub text: String,
}

fn text(e: MessengerError) -> String {
    e.to_string()
}

fn contact_item(view: messenger::ContactView) -> ContactItem {
    ContactItem {
        last: view.last.map(|(id, m)| message_item(id, m)),
        unread: view.unread,
        id: view.contact.id,
        name: view.contact.name,
        state: match view.status {
            ContactStatus::Waiting => ContactState::Waiting,
            ContactStatus::Live => ContactState::Live,
            ContactStatus::NotAccepted => ContactState::NotAccepted,
            ContactStatus::Taken => ContactState::Taken,
            ContactStatus::Damaged => ContactState::Damaged,
        },
        verified: view.contact.verified,
    }
}

fn invitation_item(view: messenger::InvitationView) -> InvitationItem {
    InvitationItem {
        id: view.id,
        name: view.name,
        expires_day: view.expires_day,
        text: view.text,
    }
}

fn message_item(id: i64, m: MessageRecord) -> MessageItem {
    let (mine, state) = match m.kind {
        MessageKind::Mine(s) => (
            true,
            match s {
                Status::Queued => MessageState::Queued,
                Status::Sent => MessageState::Sent,
                Status::Delivered => MessageState::Delivered,
                Status::Read => MessageState::Read,
                Status::NotDelivered => MessageState::NotDelivered,
                Status::AddressTaken => MessageState::AddressTaken,
            },
        ),
        MessageKind::Theirs => (false, MessageState::Received),
        MessageKind::Lost { .. } => (false, MessageState::Lost),
    };
    MessageItem {
        id,
        mine,
        state,
        at: i64::try_from(m.at).unwrap_or(i64::MAX),
        text: m.text.to_string(),
    }
}

/// Every contact with its state.
#[flutter_rust_bridge::frb]
pub fn chat_contacts() -> Result<Vec<ContactItem>, String> {
    let me = identity()?;
    Ok(messenger::contacts(&BridgeStore, &me)
        .map_err(text)?
        .into_iter()
        .map(contact_item)
        .collect())
}

/// The invitations waiting for an answer.
#[flutter_rust_bridge::frb]
pub fn chat_invitations() -> Result<Vec<InvitationItem>, String> {
    Ok(messenger::invitations(&BridgeStore)
        .map_err(text)?
        .into_iter()
        .map(invitation_item)
        .collect())
}

/// A new invitation for `name` (the name the contact will get).
#[flutter_rust_bridge::frb]
pub fn chat_create_invitation(name: String) -> Result<InvitationItem, String> {
    let me = identity()?;
    messenger::create_invitation(&BridgeStore, &me, name.trim(), now())
        .map(invitation_item)
        .map_err(text)
}

#[flutter_rust_bridge::frb]
pub fn chat_cancel_invitation(id: i64) -> Result<(), String> {
    messenger::cancel_invitation(&BridgeStore, id).map_err(text)
}

/// Accepts a pasted invitation. Returns the new contact.
#[flutter_rust_bridge::frb]
pub fn chat_accept_invitation(
    text_of_invitation: String,
    name: String,
    first_text: String,
) -> Result<i64, String> {
    let me = identity()?;
    messenger::accept_invitation(
        &BridgeStore,
        &me,
        &text_of_invitation,
        name.trim(),
        first_text.trim(),
        now(),
    )
    .map_err(text)
}

/// A page of the history: at most `limit` entries older than `before`, newest first.
#[flutter_rust_bridge::frb]
pub fn chat_history(
    contact: i64,
    before: Option<i64>,
    limit: u32,
) -> Result<Vec<MessageItem>, String> {
    Ok(messenger::history(&BridgeStore, contact, before, limit)
        .map_err(text)?
        .into_iter()
        .map(|(id, m)| message_item(id, m))
        .collect())
}

/// Writes a message; the next round puts it.
#[flutter_rust_bridge::frb]
pub fn chat_send(contact: i64, message: String) -> Result<i64, String> {
    let me = identity()?;
    let mut conversation = load(&me, contact).map_err(text)?;
    conversation
        .send(&BridgeStore, message.trim(), now())
        .map_err(text)
}

/// The history up to row `upto` has been shown: the unread count clears, and — with receipts
/// on — the next round tells the contact. Waits for a round in flight rather than making it
/// fail on the owner check.
#[flutter_rust_bridge::frb]
pub fn chat_mark_read(contact: i64, upto: i64) -> Result<bool, String> {
    let _one = one_round_at_a_time()?;
    let me = identity()?;
    load(&me, contact)
        .and_then(|mut c| c.mark_read(&BridgeStore, upto))
        .map_err(text)
}

/// Whether read receipts are sent and shown.
#[flutter_rust_bridge::frb]
pub fn chat_read_receipts() -> Result<bool, String> {
    messenger::read_receipts(&BridgeStore).map_err(text)
}

#[flutter_rust_bridge::frb]
pub fn chat_set_read_receipts(enabled: bool) -> Result<(), String> {
    messenger::set_read_receipts(&BridgeStore, enabled).map_err(text)
}

/// A round of one conversation through the DHT. Returns whether anything changed.
#[flutter_rust_bridge::frb]
pub fn chat_round(contact: i64) -> Result<bool, String> {
    let _one = one_round_at_a_time()?;
    let me = identity()?;
    let dht = network()?;
    let mut conversation = load(&me, contact).map_err(text)?;
    let updates = conversation
        .round(&BridgeStore, dht.as_ref(), now())
        .map_err(text)?;
    Ok(!updates.is_empty())
}

/// Asks every open invitation's inbox and runs a round of every conversation. Returns whether
/// anything changed.
#[flutter_rust_bridge::frb]
pub fn chat_poll() -> Result<bool, String> {
    let _one = one_round_at_a_time()?;
    let me = identity()?;
    let dht = network()?;
    let mut updates =
        messenger::poll_invitations(&BridgeStore, &me, dht.as_ref(), now()).map_err(text)?;
    updates.extend(messenger::round_all(&BridgeStore, &me, dht.as_ref(), now()).map_err(text)?);
    Ok(!updates.is_empty())
}

/// What the verification screen shows (`apeiron_messenger::Verification`).
pub struct VerificationItem {
    /// The same string on both sides.
    pub safety_number: String,
    pub my_fingerprint: String,
    /// The contact's fingerprint as this phone holds it.
    pub their_fingerprint: String,
}

/// The safety number with the contact, and the two fingerprints it is made of.
#[flutter_rust_bridge::frb]
pub fn chat_verification(contact: i64) -> Result<VerificationItem, String> {
    let me = identity()?;
    let v = messenger::verification(&BridgeStore, &me, contact).map_err(text)?;
    Ok(VerificationItem {
        safety_number: v.safety_number,
        my_fingerprint: v.my_fingerprint,
        their_fingerprint: v.their_fingerprint,
    })
}

/// Marks the contact as verified — only ever the owner's own act — or takes the mark back.
#[flutter_rust_bridge::frb]
pub fn chat_set_verified(contact: i64, verified: bool) -> Result<(), String> {
    crate::api::vault::with_open(|s, _| s.set_verified(contact, verified))?
        .ok_or_else(|| "the vault is locked".to_string())?
        .map_err(|e| e.to_string())
}

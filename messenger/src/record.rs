//! What this crate keeps in the store's sealed records (`docs/transport.md` §9).
//!
//! The store seals bytes in their place and knows nothing of their layout; the layouts are
//! here, versioned, written and read field by field with every length checked.
//!
//! - **Message** (`messages` table): `format (1) ‖ kind (1) ‖ status (1) ‖ index flag (1)
//!   [‖ index (8)] ‖ at (8) ‖ [to (8), for a loss] ‖ text`.
//! - **Conversation** (`pair_state` table): `format (1) ‖ origin (1) ‖ intro message flag (1)
//!   [‖ id (8)] ‖ read mark (8, from format 2) ‖ pending count (4) ‖ (first index (8) ‖
//!   message id (8))* ‖ the pair's bytes`.
//! - **Invitation** (`invitations` table): `format (1) ‖ created (8) ‖ invitation (261) ‖
//!   name`.

use std::collections::BTreeMap;

use apeiron_transport::invite::INVITATION_BYTES;
use apeiron_transport::pair::LostWhy;
use zeroize::Zeroizing;

use crate::MessengerError;

/// Each record kind has its own version: a new field in one must not make the others unreadable.
const MESSAGE_FORMAT: u8 = 1;
/// 2 added the read mark. A format 1 record reads with the mark at 0: everything the peer wrote
/// before the update counts as unread until the chat is opened once.
const CONVERSATION_FORMAT: u8 = 2;
const INVITATION_FORMAT: u8 = 1;

fn corrupt(what: &'static str) -> MessengerError {
    MessengerError::Corrupt(what)
}

/// Where one of my messages stands. Only ever moves forward: after a crash `Sent` may be
/// reported again, and `Delivered` may come without `Sent` before it. The numbers are what the
/// record holds, not an order: `Read` came last and ranks above `Delivered`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Written, not yet on the network.
    Queued = 0,
    /// Every part has reached the DHT at least once.
    Sent = 1,
    /// The peer has every part.
    Delivered = 2,
    /// Given up after a week, or its conversation was replaced or never established.
    NotDelivered = 3,
    /// An address it was to use already held something else (`docs/transport.md` §2).
    AddressTaken = 4,
    /// The peer has been shown it: their read receipt.
    Read = 5,
}

impl Status {
    fn from_u8(v: u8) -> Result<Self, MessengerError> {
        Ok(match v {
            0 => Self::Queued,
            1 => Self::Sent,
            2 => Self::Delivered,
            3 => Self::NotDelivered,
            4 => Self::AddressTaken,
            5 => Self::Read,
            _ => return Err(corrupt("message status")),
        })
    }

    /// Whether `to` is a step forward from here. What arrived cannot turn into "not delivered",
    /// only into "read"; "read", "not delivered" and "address taken" are where it ends.
    fn may_become(self, to: Self) -> bool {
        let rank = |s: Self| match s {
            Self::Queued => 0,
            Self::Sent => 1,
            Self::Delivered => 2,
            Self::Read => 3,
            Self::NotDelivered | Self::AddressTaken => 4,
        };
        match self {
            Self::Queued | Self::Sent => rank(to) > rank(self),
            Self::Delivered => to == Self::Read,
            Self::Read | Self::NotDelivered | Self::AddressTaken => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageKind {
    Mine(Status),
    Theirs,
    /// The peer's indices `index..to` will never be read.
    Lost {
        to: u64,
        why: LostWhy,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageRecord {
    pub kind: MessageKind,
    /// The index of the message's first part. `None` for the first text of an introduction,
    /// which travels in the invitation's reply, not in the schedule.
    pub index: Option<u64>,
    /// Mine: when it was written. Theirs: when it was decrypted — an envelope carries no time
    /// of its sender.
    pub at: u64,
    pub text: Zeroizing<String>,
}

impl MessageRecord {
    pub fn mine(index: Option<u64>, at: u64, text: &str) -> Self {
        Self {
            kind: MessageKind::Mine(Status::Queued),
            index,
            at,
            text: Zeroizing::new(text.to_string()),
        }
    }

    pub fn theirs(index: Option<u64>, at: u64, text: &str) -> Self {
        Self {
            kind: MessageKind::Theirs,
            index,
            at,
            text: Zeroizing::new(text.to_string()),
        }
    }

    pub fn lost(from: u64, to: u64, why: LostWhy, at: u64) -> Self {
        Self {
            kind: MessageKind::Lost { to, why },
            index: Some(from),
            at,
            text: Zeroizing::new(String::new()),
        }
    }

    /// Moves my message's status forward; returns whether it changed. A final status, or a
    /// step back, changes nothing.
    pub fn advance(&mut self, to: Status) -> bool {
        match &mut self.kind {
            MessageKind::Mine(now) if now.may_become(to) => {
                *now = to;
                true
            }
            _ => false,
        }
    }

    pub fn encode(&self) -> Zeroizing<Vec<u8>> {
        let mut out = Zeroizing::new(Vec::with_capacity(28 + self.text.len()));
        out.push(MESSAGE_FORMAT);
        let (kind, status) = match &self.kind {
            MessageKind::Mine(s) => (1, *s as u8),
            MessageKind::Theirs => (2, 0),
            MessageKind::Lost { why, .. } => (
                3,
                match why {
                    LostWhy::GivenUp => 1,
                    LostWhy::Undecryptable => 2,
                },
            ),
        };
        out.push(kind);
        out.push(status);
        match self.index {
            Some(i) => {
                out.push(1);
                out.extend_from_slice(&i.to_be_bytes());
            }
            None => out.push(0),
        }
        out.extend_from_slice(&self.at.to_be_bytes());
        if let MessageKind::Lost { to, .. } = self.kind {
            out.extend_from_slice(&to.to_be_bytes());
        }
        out.extend_from_slice(self.text.as_bytes());
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, MessengerError> {
        let mut r = Reader(bytes);
        if r.u8()? != MESSAGE_FORMAT {
            return Err(corrupt("message format"));
        }
        let kind = r.u8()?;
        let status = r.u8()?;
        let index = match r.u8()? {
            0 => None,
            1 => Some(r.u64()?),
            _ => return Err(corrupt("message index flag")),
        };
        let at = r.u64()?;
        let kind = match kind {
            1 => MessageKind::Mine(Status::from_u8(status)?),
            2 if status == 0 => MessageKind::Theirs,
            3 => MessageKind::Lost {
                to: r.u64()?,
                why: match status {
                    1 => LostWhy::GivenUp,
                    2 => LostWhy::Undecryptable,
                    _ => return Err(corrupt("loss reason")),
                },
            },
            _ => return Err(corrupt("message kind")),
        };
        let text = std::str::from_utf8(r.0).map_err(|_| corrupt("message text"))?;
        Ok(Self {
            kind,
            index,
            at,
            text: Zeroizing::new(text.to_string()),
        })
    }
}

/// How this side got the conversation — which decides who wins when two invitations cross
/// (`docs/transport.md` §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// I made the invitation and opened the reply.
    Invited = 1,
    /// I accepted the peer's invitation.
    Accepted = 2,
}

/// What `pair_state` holds for a conversation.
pub struct ConversationRecord {
    pub origin: Origin,
    /// My first text of an introduction, while its fate is open.
    pub intro_message: Option<i64>,
    /// The history up to this row has been shown to the person; the peer's entries after it are
    /// unread. 0 — nothing shown yet.
    pub read_mark: i64,
    /// My messages not yet delivered: first index → message row.
    pub pending: BTreeMap<u64, i64>,
    pub pair: Zeroizing<Vec<u8>>,
}

impl ConversationRecord {
    pub fn encode(&self) -> Zeroizing<Vec<u8>> {
        let mut out = Zeroizing::new(Vec::with_capacity(
            24 + 16 * self.pending.len() + self.pair.len(),
        ));
        out.push(CONVERSATION_FORMAT);
        out.push(self.origin as u8);
        match self.intro_message {
            Some(id) => {
                out.push(1);
                out.extend_from_slice(&id.to_be_bytes());
            }
            None => out.push(0),
        }
        out.extend_from_slice(&self.read_mark.to_be_bytes());
        let count = u32::try_from(self.pending.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&count.to_be_bytes());
        for (first, id) in &self.pending {
            out.extend_from_slice(&first.to_be_bytes());
            out.extend_from_slice(&id.to_be_bytes());
        }
        out.extend_from_slice(&self.pair);
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, MessengerError> {
        let mut r = Reader(bytes);
        let format = r.u8()?;
        if !(1..=CONVERSATION_FORMAT).contains(&format) {
            return Err(corrupt("conversation format"));
        }
        let origin = match r.u8()? {
            1 => Origin::Invited,
            2 => Origin::Accepted,
            _ => return Err(corrupt("conversation origin")),
        };
        let intro_message = match r.u8()? {
            0 => None,
            1 => Some(r.i64()?),
            _ => return Err(corrupt("intro message flag")),
        };
        let read_mark = if format >= 2 { r.i64()? } else { 0 };
        let count = usize::try_from(r.u32()?).map_err(|_| corrupt("pending count"))?;
        if count.saturating_mul(16) > r.0.len() {
            return Err(corrupt("pending beyond the data"));
        }
        let mut pending = BTreeMap::new();
        for _ in 0..count {
            pending.insert(r.u64()?, r.i64()?);
        }
        Ok(Self {
            origin,
            intro_message,
            read_mark,
            pending,
            pair: Zeroizing::new(r.0.to_vec()),
        })
    }
}

/// What `invitations` holds for one invitation.
pub struct InvitationRecord {
    pub created: u64,
    pub invitation: Vec<u8>,
    /// Who it is for, as the owner put it: the name the contact gets.
    pub name: String,
}

impl InvitationRecord {
    pub fn encode(&self) -> Zeroizing<Vec<u8>> {
        let mut out = Zeroizing::new(Vec::with_capacity(
            9 + self.invitation.len() + self.name.len(),
        ));
        out.push(INVITATION_FORMAT);
        out.extend_from_slice(&self.created.to_be_bytes());
        out.extend_from_slice(&self.invitation);
        out.extend_from_slice(self.name.as_bytes());
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, MessengerError> {
        let mut r = Reader(bytes);
        if r.u8()? != INVITATION_FORMAT {
            return Err(corrupt("invitation format"));
        }
        let created = r.u64()?;
        let invitation = r.take(INVITATION_BYTES)?.to_vec();
        let name = std::str::from_utf8(r.0)
            .map_err(|_| corrupt("invitation name"))?
            .to_string();
        Ok(Self {
            created,
            invitation,
            name,
        })
    }
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], MessengerError> {
        let (head, rest) = self.0.split_at_checked(n).ok_or(corrupt("truncated"))?;
        self.0 = rest;
        Ok(head)
    }
    fn u8(&mut self) -> Result<u8, MessengerError> {
        let [b] = <[u8; 1]>::try_from(self.take(1)?).map_err(|_| corrupt("truncated"))?;
        Ok(b)
    }
    fn u32(&mut self) -> Result<u32, MessengerError> {
        Ok(u32::from_be_bytes(
            self.take(4)?.try_into().map_err(|_| corrupt("truncated"))?,
        ))
    }
    fn u64(&mut self) -> Result<u64, MessengerError> {
        Ok(u64::from_be_bytes(
            self.take(8)?.try_into().map_err(|_| corrupt("truncated"))?,
        ))
    }
    fn i64(&mut self) -> Result<i64, MessengerError> {
        Ok(i64::from_be_bytes(
            self.take(8)?.try_into().map_err(|_| corrupt("truncated"))?,
        ))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;

    /// Every kind reads back as written; what is not a record of ours is refused, not misread.
    #[test]
    fn records_round_trip_and_refuse_what_is_not_theirs() {
        let mut sent = MessageRecord::mine(Some(7), 1_000, "привет");
        assert!(sent.advance(Status::Sent));
        let messages = [
            sent,
            MessageRecord::mine(None, 1, ""),
            MessageRecord::theirs(Some(3), 2, "ответ"),
            MessageRecord::lost(4, 9, LostWhy::Undecryptable, 3),
        ];
        for m in &messages {
            assert_eq!(&MessageRecord::decode(&m.encode()).unwrap(), m);
        }
        let bytes = messages[0].encode();
        for bad in [
            &bytes[..3],
            &[9u8, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0][..],
            &[1u8, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0][..],
            &[1u8, 1, 9, 0, 0, 0, 0, 0, 0, 0, 0, 0][..],
        ] {
            assert!(MessageRecord::decode(bad).is_err(), "{bad:?}");
        }

        let conversation = ConversationRecord {
            origin: Origin::Accepted,
            intro_message: Some(5),
            read_mark: 4,
            pending: BTreeMap::from([(0, 6), (3, 8)]),
            pair: Zeroizing::new(vec![1, 2, 3]),
        };
        let back = ConversationRecord::decode(&conversation.encode()).unwrap();
        assert_eq!(back.origin, Origin::Accepted);
        assert_eq!(back.intro_message, Some(5));
        assert_eq!(back.read_mark, 4);
        assert_eq!(back.pending, conversation.pending);
        assert_eq!(*back.pair, vec![1, 2, 3]);
        let mut lying = conversation.encode().to_vec();
        lying[19] = 200; // a pending count beyond the data
        assert!(ConversationRecord::decode(&lying).is_err());
    }

    /// Records as format 1 wrote them — the phones hold such — laid out by hand.
    #[test]
    fn records_of_format_1_still_read() {
        let mut message = vec![1u8, 1, 2, 1];
        message.extend_from_slice(&7u64.to_be_bytes());
        message.extend_from_slice(&1_000u64.to_be_bytes());
        message.extend_from_slice("да".as_bytes());
        let m = MessageRecord::decode(&message).unwrap();
        assert_eq!(m.kind, MessageKind::Mine(Status::Delivered));
        assert_eq!((m.index, m.at, m.text.as_str()), (Some(7), 1_000, "да"));

        let mut conversation = vec![1u8, 2, 1];
        conversation.extend_from_slice(&5i64.to_be_bytes());
        conversation.extend_from_slice(&1u32.to_be_bytes());
        conversation.extend_from_slice(&3u64.to_be_bytes());
        conversation.extend_from_slice(&8i64.to_be_bytes());
        conversation.extend_from_slice(&[9, 9]);
        let c = ConversationRecord::decode(&conversation).unwrap();
        assert_eq!(
            (c.origin, c.intro_message, c.read_mark),
            (Origin::Accepted, Some(5), 0)
        );
        assert_eq!(c.pending, BTreeMap::from([(3, 8)]));
        assert_eq!(*c.pair, vec![9, 9]);
    }

    #[test]
    fn a_status_only_moves_forward() {
        let mut m = MessageRecord::mine(Some(0), 0, "x");
        assert!(m.advance(Status::Delivered));
        assert!(!m.advance(Status::Sent), "went back");
        assert!(
            !m.advance(Status::NotDelivered),
            "what arrived turned undelivered"
        );
        assert!(m.advance(Status::Read));
        assert!(!m.advance(Status::Delivered), "left a final status");
        let mut theirs = MessageRecord::theirs(Some(0), 0, "y");
        assert!(!theirs.advance(Status::Sent));
    }
}

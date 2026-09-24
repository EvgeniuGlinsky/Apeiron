//! The owner of a conversation: the store, the ratchet and the DHT put together.
//!
//! `apeiron-transport` knows how a conversation travels, `apeiron-store` how it is kept; this
//! crate is the one place that does both in the right order, so that the bridge to Flutter
//! stays a thin layer of calls. Three rules live here and nowhere else:
//!
//! - **Nothing that acknowledges is put before it is stored** (`docs/transport.md` §6). A round
//!   receives, commits what it received together with the state item saying so, and only then
//!   puts ([`conversation`]).
//! - **One owner per conversation** — checked, not assumed: every commit first compares the
//!   stored conversation record with the one this owner last loaded or wrote.
//! - **An introduction is one transaction** with the account whose one-time key it spent, and
//!   the invitation it answered is forgotten in the same step ([`invitations`]).
//!
//! The time is passed in (`now`, Unix seconds), as in the transport.

use apeiron_store::{Storage, StorageError};
use apeiron_transport::TransportError;

pub mod conversation;
pub mod invitations;
pub mod record;

pub use conversation::{contacts, history, round_all, ContactStatus, ContactView, Conversation};
pub use invitations::{
    accept_invitation, cancel_invitation, create_invitation, invitations, poll_invitations,
    InvitationView,
};
pub use record::{MessageKind, MessageRecord, Status};

#[derive(Debug, thiserror::Error)]
pub enum MessengerError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error("the vault is locked")]
    Locked,
    #[error("there is no identity yet")]
    NoIdentity,
    #[error("there is no device account yet")]
    NoAccount,
    #[error("no such contact")]
    NoContact,
    #[error("no such invitation")]
    NoInvitation,
    /// The inviter is already a contact with a conversation that works or waits. Accepting
    /// again would replace it — with a session whose reply may never land.
    #[error("this person is already a contact")]
    AlreadyContact,
    #[error("nothing will come of this conversation any more")]
    Closed,
    #[error("an empty message")]
    Empty,
    /// The conversation was changed by someone else, or an operation failed half-way: this
    /// value must be dropped and the conversation loaded again.
    #[error("the conversation must be loaded again")]
    Reload,
    #[error("a stored record is damaged: {0}")]
    Corrupt(&'static str),
}

/// Access to the storage, for the moments something is read or written.
///
/// A round spends seconds on the network, and the app keeps its `Storage` under a lock the
/// interface needs too. So the storage is never held across the network: every read and every
/// commit is one call of [`StoreAccess::with`]. `Storage` itself implements it; the bridge
/// implements it over its lock, answering [`MessengerError::Locked`] once the vault is locked.
pub trait StoreAccess {
    fn with<R>(
        &self,
        f: impl FnOnce(&Storage) -> Result<R, MessengerError>,
    ) -> Result<R, MessengerError>;
}

impl StoreAccess for Storage {
    fn with<R>(
        &self,
        f: impl FnOnce(&Storage) -> Result<R, MessengerError>,
    ) -> Result<R, MessengerError> {
        f(self)
    }
}

/// Something the interface should show anew.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Update {
    /// A new entry in the history of `contact`: a message, or a note that some were lost.
    Message { contact: i64, message: i64 },
    /// The status of one of my messages changed.
    Changed { contact: i64, message: i64 },
    /// A contact appeared, or its status changed.
    Contact { contact: i64 },
    /// An invitation will not be answered any more: it expired, or it was answered.
    InvitationGone { invitation: i64 },
}

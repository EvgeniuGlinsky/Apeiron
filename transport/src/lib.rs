//! Delivery without servers: envelopes in Mainline DHT (BEP 44).
//!
//! The design is `docs/transport.md`, the decision R-012 in `docs/threat-log.md`. Envelopes are
//! BEP 44 mutable items in Mainline DHT — millions of machines that nobody owns — instead of a
//! relay of our own. The sender re-puts what has not been acknowledged; the receiver asks.
//! Nobody listens in the background, because staying reachable through carrier NAT is what
//! drains the battery.
//!
//! - [`address`] — where items live: addresses only the two people can compute;
//! - [`envelope`] and [`item`] — what an item is: one length, sealed, signed once;
//! - [`invite`] — getting to know each other through a one-time inbox;
//! - [`pair`] — the state of one conversation, pure;
//! - [`engine`] — a round of sending and one of receiving against a [`dht::Dht`];
//! - [`mainline_dht`] — the real one;
//! - [`probe`] — the go/no-go measurement that came first.

use apeiron_core::{AeadError, ChatError, IdentityError};

pub mod address;
pub mod dht;
pub mod engine;
pub mod envelope;
pub mod invite;
pub mod item;
pub mod mainline_dht;
pub mod pair;
pub mod probe;

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error(transparent)]
    Identity(#[from] IdentityError),
    #[error(transparent)]
    Chat(#[from] ChatError),
    #[error("sealing an item failed: {0}")]
    Seal(#[from] AeadError),
    #[error(transparent)]
    Envelope(#[from] envelope::EnvelopeError),
    #[error("the value is not an item of this conversation")]
    NotOurs,
    #[error("invitation: {0}")]
    Invitation(String),
    #[error("the text needs {parts} parts; at most {} fit", envelope::MAX_PARTS)]
    TooLong { parts: usize },
    #[error("key derivation failed")]
    Derivation,
    #[error("internal error: {0}")]
    Internal(&'static str),
}

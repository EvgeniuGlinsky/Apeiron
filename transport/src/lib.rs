//! Delivery without servers.
//!
//! The plan (`docs/threat-log.md`, R-012): envelopes are BEP 44 mutable items in Mainline
//! DHT — millions of machines that nobody owns — instead of a relay of our own. The sender
//! re-puts what has not been acknowledged; the receiver asks. Nobody listens in the
//! background, because staying reachable through carrier NAT is what drains the battery.
//!
//! Nothing is built on that until [`probe`] has shown that the DHT holds envelopes long
//! enough, from a phone on a mobile network as well as from a desktop.

pub mod probe;

//! Boundary between Flutter and the core.
//!
//! Rule for everything declared here: only public data goes out.
//! Keys, plaintext and protocol state stay in Rust (R-004).

pub mod chat;
pub mod identity;
pub mod pin;
pub mod probe;
pub mod vault;

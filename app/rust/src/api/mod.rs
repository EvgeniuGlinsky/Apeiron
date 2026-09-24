//! Boundary between Flutter and the core.
//!
//! Rule for everything declared here: only public data goes out.
//! Keys, plaintext and protocol state stay in Rust (R-004).

pub mod identity;
pub mod vault;

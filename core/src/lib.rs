//! The messenger core.
//!
//! Everything that has to be correct lives here: cryptography, protocol state,
//! storage. There is deliberately no FFI in this layer: the bridge to Flutter is a
//! separate thin crate, so the core can be tested and audited independently of the
//! interface.
//!
//! # Rules that apply across the crate
//!
//! * Plaintext and key material do not leave Rust (decision R-004,
//!   `docs/threat-log.md`). Dart only receives what is on screen right now.
//! * There are no home-grown cryptographic primitives here and there never will be.
//!   Everything is taken ready-made from vetted implementations (§18 of the research).
//! * Panics are forbidden by lints: in cryptographic code a panic turns into a denial
//!   of service, and sometimes into a leak through the error message.

pub mod aead;
pub mod identity;
pub mod prekey;
pub mod random;
pub mod session;
pub mod sigchain;

pub use aead::{open, purpose, seal, AeadError, SecretKey};
pub use identity::{Identity, IdentityError, PairSecret, PublicIdentity, SecretBytes};
pub use prekey::{PrekeyBundle, PrekeyError, UnverifiedPrekeyBundle};
pub use session::{pickle_account, unpickle_account, Chat, ChatError};
pub use sigchain::{ChainSigner, ChainState, EntryBody, Sigchain, SigchainError};

pub use random::{random_bytes, RandomError};
/// Re-export of `vodozemac`.
///
/// Our public API exposes its types (keys, messages), so a user of the core must
/// work with exactly the same version of the crate. The re-export guarantees it:
/// there is simply nowhere else to get a different one.
pub use vodozemac;

/// The format version this core understands.
pub const PROTOCOL_VERSION: u8 = 1;

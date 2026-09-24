//! Bridge to the device identity.
//!
//! Only public data crosses this boundary. Secret keys stay in Rust
//! — decision R-004 in `docs/threat-log.md`.
//!
//! The reason is not tidiness but the fact that wiping memory is impossible in
//! Dart: the garbage collector copies objects when compacting the heap and gives
//! no guarantee that the previous copy of a string has been wiped. Anything that
//! reached Dart must be considered left in memory until the process ends.
//!
//! The identity no longer lives in a separate static: it sits in the open
//! vault (`super::vault`) and survives an app restart. Before the vault
//! existed, every launch produced a new identity — that is, a new fingerprint
//! for a person who had already read the previous one aloud during verification.

use apeiron_core::{Identity, PublicIdentity};

use super::vault;

/// What is allowed to be shown. Contains no secrets.
pub struct PublicIdentityView {
    /// Thirty digits in six groups — what is read aloud during verification.
    pub fingerprint: String,
    /// Public Ed25519 signing key, hex.
    pub signing_key_hex: String,
    /// Public X25519 key-agreement key, hex.
    pub agreement_key_hex: String,
}

impl From<&PublicIdentity> for PublicIdentityView {
    fn from(p: &PublicIdentity) -> Self {
        Self {
            fingerprint: p.fingerprint(),
            signing_key_hex: hex::encode(p.verifying_key().as_bytes()),
            agreement_key_hex: hex::encode(p.agreement_key().as_bytes()),
        }
    }
}

const LOCKED: &str = "личность заблокирована";

fn view(identity: &Identity) -> PublicIdentityView {
    PublicIdentityView::from(&identity.public())
}

/// Creates a new identity and saves it.
///
/// The device account and the sigchain (identity log) are created with it: apart
/// they are meaningless. The operation is irreversible — changing the identity
/// breaks all existing conversations — and the vault must be open by this point.
#[flutter_rust_bridge::frb]
pub fn generate_identity() -> Result<PublicIdentityView, String> {
    vault::create_identity()?;
    current_identity()?.ok_or_else(|| "личность не сохранилась".to_string())
}

/// The current identity, if the vault is open.
#[flutter_rust_bridge::frb]
pub fn current_identity() -> Result<Option<PublicIdentityView>, String> {
    vault::with_identity(view)
}

/// Locking: locks the vault and wipes the keys.
///
/// Called when the app goes to the background and when the screen turns off —
/// decision R-001.
#[flutter_rust_bridge::frb]
pub fn lock_identity() -> Result<(), String> {
    vault::lock_vault()
}

/// Safety number with a peer, from their public identity.
///
/// Both sides get the same string. A mismatch means someone is between
/// you, and the conversation must not be started.
#[flutter_rust_bridge::frb]
pub fn safety_number_with(peer_public_hex: String) -> Result<String, String> {
    let bytes = hex::decode(peer_public_hex.trim())
        .map_err(|_| "ключ собеседника не является шестнадцатеричной строкой".to_string())?;
    let peer = PublicIdentity::from_bytes(&bytes).map_err(|e| e.to_string())?;
    vault::with_identity(|me| me.public().safety_number(&peer))?.ok_or_else(|| LOCKED.to_string())
}

/// The whole public identity, hex — what is encoded in the QR code when adding
/// a contact.
#[flutter_rust_bridge::frb]
pub fn public_identity_hex() -> Result<String, String> {
    vault::with_identity(|me| hex::encode(me.public().to_bytes()))?
        .ok_or_else(|| LOCKED.to_string())
}

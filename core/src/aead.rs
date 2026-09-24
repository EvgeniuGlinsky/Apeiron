//! Symmetric encryption and key derivation for local storage.
//!
//! There is not a single home-grown primitive here, only an assembly of ready-made
//! ones (§18 of the research, "do not write your own encryption primitive or mode").
//!
//! # What was chosen and why
//!
//! **XChaCha20-Poly1305, not ChaCha20-Poly1305.** The difference is the length of the
//! nonce: 192 bits versus 96. With 96 bits random nonces
//! become dangerous: the probability of a collision by the birthday paradox
//! stops being negligible already at billions of records, so they are conventionally
//! treated as a counter. A counter, in turn, requires reliably persisting state between
//! launches, and a phone gets switched off at an arbitrary moment. With 192 bits
//! a random nonce is safe without any state: that is the reason for the choice.
//! The construction itself is standard and rests on the same ChaCha20-Poly1305,
//! whose correctness is checked by the official RFC 8439 vectors
//! (`core/tests/rfc_vectors.rs`).
//!
//! **HKDF-SHA256 for deriving subkeys.** Each purpose gets its own key from one master
//! key, and the purpose label is mandatory: without it the same key would end up in
//! different subsystems, and a bug in one would become a bug in all of them.
//!
//! # What is not here yet
//!
//! For now the master key is **created in memory and stored nowhere**. Per decision
//! R-002 it must live in hardware storage (StrongBox/TEE), and that requires JNI and
//! an on-device check. Until then the core writes nothing secret to disk: no scheme
//! is better than a weak scheme passed off as a strong one.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::random::{random_bytes, RandomError};

/// Key length.
pub const KEY_BYTES: usize = 32;
/// XChaCha20 nonce length.
pub const NONCE_BYTES: usize = 24;
/// Poly1305 authentication tag length.
pub const TAG_BYTES: usize = 16;
/// Opaque lookup tag length.
pub const LOOKUP_TAG_BYTES: usize = 16;

/// Domain separator for key derivation.
const KDF_DOMAIN: &[u8] = b"apeiron/kdf/v1";

/// Domain separator for lookup tags.
const LOOKUP_DOMAIN: &[u8] = b"apeiron/lookup/v1";

/// Purpose labels for [`SecretKey::derive`].
///
/// A registry, not strings at the call site. The reason is simple and unpleasant: a
/// typo in a label gives **a different key**, everything keeps working, and this is
/// discovered exactly when the data has already been written under the wrong key. The
/// compiler catches a typo in a constant name; in a string literal it catches nothing.
///
/// Names follow the project convention `apeiron/<area>/v1`. The version at the end is
/// not decoration: by changing a label you make everything written under the previous
/// one unreadable.
pub mod purpose {
    /// The identity secret.
    pub const IDENTITY: &str = "apeiron/storage/identity/v1";
    /// Olm account state: this device's keys.
    pub const ACCOUNT: &str = "apeiron/storage/account/v1";
    /// Ratchet state for each conversation.
    pub const SESSION: &str = "apeiron/storage/session/v1";
    /// The sigchain (identity log).
    pub const SIGCHAIN: &str = "apeiron/storage/sigchain/v1";
    /// Contact records.
    pub const CONTACT: &str = "apeiron/storage/contact/v1";
    /// Internal storage records.
    pub const META: &str = "apeiron/storage/meta/v1";
    /// Lookup tags.
    ///
    /// A separate branch, unrelated to the decryption keys: knowing a tag brings one no
    /// closer to the content. The same technique as `K_addr` in the research
    /// (§16.2), applied to the local database.
    pub const TAG: &str = "apeiron/storage/tag/v1";
    /// The key that seals the database key, from the output of the PIN's hardware chain
    /// (R-011). Not derived from the database key — it is what protects it.
    pub const PIN_WRAP: &str = "apeiron/storage/pin-wrap/v1";
    /// The history of messages (schema v2).
    pub const MESSAGE: &str = "apeiron/storage/message/v1";
    /// The transport state of each conversation (schema v2).
    pub const PAIR_STATE: &str = "apeiron/storage/pair-state/v1";
    /// Invitations that wait for an answer (schema v2).
    pub const INVITATION: &str = "apeiron/storage/invitation/v1";
    /// Signed items the background job re-puts (schema v2). Derived from the **background**
    /// key, not from the database key: the job runs while the vault is locked
    /// (`docs/transport.md` §9).
    pub const OUTBOX: &str = "apeiron/storage/outbox/v1";

    /// All labels at once, to check that there are no duplicates among them.
    pub const ALL: &[&str] = &[
        IDENTITY, ACCOUNT, SESSION, SIGCHAIN, CONTACT, META, TAG, PIN_WRAP, MESSAGE, PAIR_STATE,
        INVITATION, OUTBOX,
    ];
}

/// What can go wrong.
#[derive(Debug, thiserror::Error)]
pub enum AeadError {
    #[error("sealing failed: {0}")]
    Seal(String),

    #[error(
        "THE RECORD FAILED THE AUTHENTICITY CHECK. It is damaged or substituted; \
         its contents must not be trusted."
    )]
    Open,

    #[error("record shorter than its service fields: {0} bytes, minimum {1}")]
    TooShort(usize, usize),

    #[error(transparent)]
    Random(#[from] RandomError),
}

/// A key that wipes itself when destroyed.
///
/// The wrapper is not for looks: a bare `[u8; 32]` stays in memory after it goes out
/// of scope, and finding it in a process dump is a matter of technique.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretKey([u8; KEY_BYTES]);

impl SecretKey {
    /// A new random key.
    pub fn generate() -> Result<Self, RandomError> {
        Ok(Self(random_bytes::<KEY_BYTES>()?))
    }

    /// A key from ready-made bytes, for example obtained from hardware storage.
    pub fn from_bytes(bytes: [u8; KEY_BYTES]) -> Self {
        Self(bytes)
    }

    /// A subkey for a specific purpose.
    ///
    /// The purpose label ([`purpose`](Self::derive)) is mandatory and must be
    /// unique: two purposes with one label get one key, and this is exactly the
    /// case where the bug does not show itself until the break-in.
    pub fn derive(&self, purpose: &str) -> Self {
        let hk = Hkdf::<Sha256>::new(Some(KDF_DOMAIN), &self.0);
        let mut out = [0u8; KEY_BYTES];
        // An error here is possible only when requesting a length above 255×32 bytes;
        // our length is fixed, so the branch is unreachable. But panicking is still
        // not allowed, and instead an empty key is returned, which will immediately
        // break any authenticity check. There is no silent weakness.
        if hk.expand(purpose.as_bytes(), &mut out).is_err() {
            out.zeroize();
        }
        Self(out)
    }

    /// An opaque tag for looking up by a value that must not be stored in the
    /// clear.
    ///
    /// Needed because lookups must happen but disclosure must not. If the peer's
    /// public key were stored as is, the list of peers could be read from the
    /// database file without any key, which is exactly what the database was meant
    /// to protect. The tag, by contrast, is deterministic (fit for a unique index) and
    /// says nothing without the master key.
    ///
    /// This is the same technique as the separate `K_addr` branch in the research (§16.2):
    /// knowing an address brings one no closer to the content. The key for tags is derived
    /// under a separate purpose label and is unrelated to the decryption keys.
    ///
    /// Sixteen bytes are enough: the tag is neither a secret nor a signature; its job
    /// is to distinguish values, not to resist guessing.
    pub fn tag(&self, value: &[u8]) -> [u8; LOOKUP_TAG_BYTES] {
        let hk = Hkdf::<Sha256>::new(Some(KDF_DOMAIN), &self.0);
        let mut info = Vec::with_capacity(LOOKUP_DOMAIN.len() + value.len());
        info.extend_from_slice(LOOKUP_DOMAIN);
        info.extend_from_slice(value);

        let mut out = [0u8; LOOKUP_TAG_BYTES];
        // As in derive: the branch is unreachable at this length, but panicking is
        // not allowed, and a zero tag will break the unique index at once and loudly.
        if hk.expand(&info, &mut out).is_err() {
            out.zeroize();
        }
        out
    }

    fn cipher(&self) -> Result<XChaCha20Poly1305, AeadError> {
        XChaCha20Poly1305::new_from_slice(&self.0).map_err(|e| AeadError::Seal(e.to_string()))
    }
}

impl std::fmt::Debug for SecretKey {
    /// The key must not be printed: log lines outlive the process.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretKey(<hidden>)")
    }
}

/// Seals data.
///
/// Format: `nonce (24) || ciphertext || tag (16)`. The nonce is stored alongside in
/// the clear; this is normal and necessary: it does not have to be secret, it has to
/// be non-repeating.
///
/// `aad` is what is not encrypted but is protected against substitution: for example,
/// a record identifier. By substituting it, an adversary gets a failed check, not
/// different content.
pub fn seal(key: &SecretKey, aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, AeadError> {
    let nonce_bytes = random_bytes::<NONCE_BYTES>()?;
    let nonce = XNonce::from(nonce_bytes);

    let ciphertext = key
        .cipher()?
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|e| AeadError::Seal(e.to_string()))?;

    let mut out = Vec::with_capacity(NONCE_BYTES + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Opens data, checking authenticity.
///
/// A failure means exactly one thing: the record is not the one that was sealed with
/// this key. Telling corruption from substitution is impossible and unnecessary: they
/// must be handled the same way.
pub fn open(key: &SecretKey, aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>, AeadError> {
    let minimum = NONCE_BYTES + TAG_BYTES;
    if sealed.len() < minimum {
        return Err(AeadError::TooShort(sealed.len(), minimum));
    }

    let (nonce_bytes, ciphertext) = sealed.split_at(NONCE_BYTES);
    let nonce = XNonce::try_from(nonce_bytes).map_err(|_| AeadError::Open)?;

    key.cipher()?
        .decrypt(
            &nonce,
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| AeadError::Open)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )]

    use super::*;

    #[test]
    fn roundtrip() {
        let key = SecretKey::generate().unwrap();
        let sealed = seal(&key, "record 7".as_bytes(), "hello".as_bytes()).unwrap();
        let opened = open(&key, "record 7".as_bytes(), &sealed).unwrap();
        assert_eq!(opened, "hello".as_bytes());
    }

    #[test]
    fn same_plaintext_seals_differently() {
        // Identical plaintext must give different ciphertext: otherwise the storage
        // shows which records match.
        let key = SecretKey::generate().unwrap();
        let a = seal(&key, b"", "one and the same".as_bytes()).unwrap();
        let b = seal(&key, b"", "one and the same".as_bytes()).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn tampered_ciphertext_is_rejected() {
        let key = SecretKey::generate().unwrap();
        let mut sealed = seal(&key, b"", "text".as_bytes()).unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 1;
        assert!(matches!(open(&key, b"", &sealed), Err(AeadError::Open)));
    }

    #[test]
    fn tampered_nonce_is_rejected() {
        let key = SecretKey::generate().unwrap();
        let mut sealed = seal(&key, b"", "text".as_bytes()).unwrap();
        sealed[0] ^= 1;
        assert!(matches!(open(&key, b"", &sealed), Err(AeadError::Open)));
    }

    #[test]
    fn substituted_aad_is_rejected() {
        // A record moved to someone else's place must not be readable.
        let key = SecretKey::generate().unwrap();
        let sealed = seal(&key, "record 7".as_bytes(), "text".as_bytes()).unwrap();
        assert!(matches!(
            open(&key, "record 8".as_bytes(), &sealed),
            Err(AeadError::Open)
        ));
    }

    #[test]
    fn another_key_is_rejected() {
        let key = SecretKey::generate().unwrap();
        let other = SecretKey::generate().unwrap();
        let sealed = seal(&key, b"", "text".as_bytes()).unwrap();
        assert!(matches!(open(&other, b"", &sealed), Err(AeadError::Open)));
    }

    #[test]
    fn truncated_record_is_rejected() {
        let key = SecretKey::generate().unwrap();
        let sealed = seal(&key, b"", "text".as_bytes()).unwrap();
        assert!(matches!(
            open(&key, b"", &sealed[..NONCE_BYTES]),
            Err(AeadError::TooShort(_, _))
        ));
    }

    #[test]
    fn purposes_give_different_keys() {
        let master = SecretKey::generate().unwrap();
        let a = master.derive("message storage");
        let b = master.derive("contact storage");
        let sealed = seal(&a, b"", "text".as_bytes()).unwrap();
        assert!(
            open(&b, b"", &sealed).is_err(),
            "purpose labels must separate keys"
        );
        assert_eq!(open(&a, b"", &sealed).unwrap(), "text".as_bytes());
    }

    #[test]
    fn derivation_is_deterministic() {
        let master = SecretKey::from_bytes([7u8; KEY_BYTES]);
        let a = master.derive("one");
        let b = master.derive("one");
        let sealed = seal(&a, b"", "text".as_bytes()).unwrap();
        assert_eq!(open(&b, b"", &sealed).unwrap(), "text".as_bytes());
    }

    #[test]
    fn lookup_tags_differ_by_value_and_by_key() {
        let a = SecretKey::generate().expect("the OS provides randomness");
        let b = SecretKey::generate().expect("the OS provides randomness");

        assert_eq!(
            a.tag("one".as_bytes()),
            a.tag("one".as_bytes()),
            "the tag must be stable"
        );
        assert_ne!(a.tag("one".as_bytes()), a.tag("two".as_bytes()));
        assert_ne!(
            a.tag("one".as_bytes()),
            b.tag("one".as_bytes()),
            "with a different key the tag must be different"
        );
        assert_ne!(a.tag("one".as_bytes()), [0u8; LOOKUP_TAG_BYTES]);
    }

    #[test]
    fn purpose_labels_are_unique() {
        // Two subsystems with one label get one key, and this is exactly the
        // case where the bug does not show itself until the break-in.
        let mut seen = std::collections::BTreeSet::new();
        for label in purpose::ALL {
            assert!(seen.insert(*label), "purpose label is repeated: {label}");
        }
        assert_eq!(seen.len(), purpose::ALL.len());
    }

    #[test]
    fn every_purpose_gives_its_own_key() {
        let master = SecretKey::generate().expect("the OS provides randomness");
        let mut keys = std::collections::BTreeSet::new();
        for label in purpose::ALL {
            let derived = master.derive(label);
            assert!(
                keys.insert(derived.0),
                "two purpose labels gave one key: {label}"
            );
        }
    }

    #[test]
    fn debug_does_not_print_the_key() {
        let key = SecretKey::from_bytes([0xAB; KEY_BYTES]);
        let shown = format!("{key:?}");
        assert!(
            !shown.contains("ab"),
            "the key must not end up in the string: {shown}"
        );
        assert!(!shown.contains("171"));
    }
}

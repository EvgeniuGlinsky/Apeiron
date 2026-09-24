//! The long-term identity of a device.
//!
//! Two independent key pairs:
//!   * Ed25519: signing (the key log, proof of authorship);
//!   * X25519: key agreement (entry into the double ratchet).
//!
//! They are deliberately not derived from one another: tying one key type to another is
//! a source of subtle bugs, and the link between them is established anyway by the signed
//! sigchain (identity log).
//!
//! The secret parts do not leave Rust: decision R-004 in `docs/threat-log.md`.
//! `SigningKey` and `StaticSecret` wipe themselves when destroyed (feature `zeroize`),
//! so a custom `Drop` is not needed here and deliberately not written.

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::random::{random_bytes, RandomError};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey as X25519Public, StaticSecret};

/// Groups in a fingerprint read aloud during verification.
pub const FINGERPRINT_GROUPS: usize = 6;
/// Digits in each group.
pub const FINGERPRINT_DIGITS_PER_GROUP: usize = 5;

/// Bytes in a serialized public identity: 32 (Ed25519) + 32 (X25519).
pub const PUBLIC_IDENTITY_BYTES: usize = 64;

/// Domain separator for the fingerprint hash. Changing it changes every fingerprint:
/// a deliberately breaking change that requires users to verify again.
const FINGERPRINT_DOMAIN: &[u8] = b"apeiron/fingerprint/v1";
const SAFETY_NUMBER_DOMAIN: &[u8] = b"apeiron/safety-number/v1";

#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    #[error("wrong length: expected {expected} bytes, got {got}")]
    Length { expected: usize, got: usize },
    #[error("the bytes do not form a valid Ed25519 key")]
    MalformedVerifyingKey,
    #[error("the signature does not verify")]
    BadSignature,
    #[error("the peer's agreement key gives no secret: it is a low-order point")]
    NonContributory,
    #[error("an identity cannot share a pair secret with itself")]
    SelfAgreement,
}

/// Domain separator of the pair secret (`docs/transport.md` §2).
const PAIR_DOMAIN: &[u8] = b"apeiron/pair/v1";

/// The secret two identities share: `K_pair` of `docs/transport.md`.
///
/// Wiped when dropped, like every other secret here, and never printed.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct PairSecret([u8; 32]);

impl PairSecret {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for PairSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PairSecret(<hidden>)")
    }
}

/// The public part of an identity. Passed around freely, contains no secrets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicIdentity {
    verifying: VerifyingKey,
    agreement: X25519Public,
}

impl PublicIdentity {
    pub fn verifying_key(&self) -> &VerifyingKey {
        &self.verifying
    }

    pub fn agreement_key(&self) -> &X25519Public {
        &self.agreement
    }

    pub fn to_bytes(&self) -> [u8; PUBLIC_IDENTITY_BYTES] {
        let mut out = [0u8; PUBLIC_IDENTITY_BYTES];
        out[..32].copy_from_slice(self.verifying.as_bytes());
        out[32..].copy_from_slice(self.agreement.as_bytes());
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, IdentityError> {
        if bytes.len() != PUBLIC_IDENTITY_BYTES {
            return Err(IdentityError::Length {
                expected: PUBLIC_IDENTITY_BYTES,
                got: bytes.len(),
            });
        }
        let (v, a) = bytes.split_at(32);
        let varr: [u8; 32] = v.try_into().map_err(|_| IdentityError::Length {
            expected: 32,
            got: v.len(),
        })?;
        let aarr: [u8; 32] = a.try_into().map_err(|_| IdentityError::Length {
            expected: 32,
            got: a.len(),
        })?;
        let verifying =
            VerifyingKey::from_bytes(&varr).map_err(|_| IdentityError::MalformedVerifyingKey)?;
        Ok(Self {
            verifying,
            agreement: X25519Public::from(aarr),
        })
    }

    /// The fingerprint of a single identity: 30 digits in six groups.
    ///
    /// Shown in the profile. For verification with a peer use
    /// [`PublicIdentity::safety_number`]: it protects against substitution of both sides at once.
    pub fn fingerprint(&self) -> String {
        let mut h = Sha256::new();
        h.update(FINGERPRINT_DOMAIN);
        h.update(self.to_bytes());
        digits_from_hash(&h.finalize())
    }

    /// The safety number for a pair of peers.
    ///
    /// Symmetric: both sides get the same string regardless of who added whom.
    /// This is what gets compared by voice or via QR before a conversation starts.
    /// Without this verification a strong cipher is fully defeated by an active
    /// intermediary; see the demonstration in `radio-mesh-demo/s07_ratchet.py`, section F.
    pub fn safety_number(&self, other: &PublicIdentity) -> String {
        let a = self.to_bytes();
        let b = other.to_bytes();
        // Order them so the result does not depend on who computes it.
        let (first, second) = if a <= b { (&a, &b) } else { (&b, &a) };
        let mut h = Sha256::new();
        h.update(SAFETY_NUMBER_DOMAIN);
        h.update(first);
        h.update(second);
        digits_from_hash(&h.finalize())
    }

    /// Signature verification, **strict**.
    ///
    /// Ordinary Ed25519 verification accepts a non-canonical signature encoding and
    /// small-order points. This barely affects strength, but it means the same message
    /// can have several differing signatures, each of which is valid. Where the
    /// signature goes into a hash (and for us it does: the sigchain rests on this),
    /// this would turn into two different "identical" chains. Strict verification
    /// does not allow that.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), IdentityError> {
        self.verifying
            .verify_strict(message, signature)
            .map_err(|_| IdentityError::BadSignature)
    }
}

/// Length of an exported identity secret: two seeds of 32 bytes each.
pub const SECRET_IDENTITY_BYTES: usize = 64;

/// The identity secret as bytes: the only way to take it out of
/// [`Identity`].
///
/// The wrapper is not for looks. A bare `[u8; 64]` stays in memory after it goes
/// out of scope, gets printed to the log by the first careless debug line, and is
/// silently copied. Here: it is wiped on destruction, `Debug` shows nothing, and the
/// type name says plainly what is inside.
///
/// The only legitimate purpose is to seal the contents into storage
/// (`apeiron-store`) immediately. Anything else is a bug.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretBytes([u8; SECRET_IDENTITY_BYTES]);

impl SecretBytes {
    /// Bytes for sealing. They must not be copied anywhere else.
    pub fn as_bytes(&self) -> &[u8; SECRET_IDENTITY_BYTES] {
        &self.0
    }
}

impl std::fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretBytes(<hidden>)")
    }
}

/// The secret identity. Not serialized outward and does not cross the FFI boundary.
pub struct Identity {
    signing: SigningKey,
    agreement: StaticSecret,
}

impl Identity {
    /// A new identity from the system source of randomness.
    ///
    /// Returns an error rather than panicking: the OS refusing randomness is a rare
    /// but possible outcome (sandboxing, descriptor exhaustion, very early startup),
    /// and deciding what to do about it is the caller's job. The surrounding libraries
    /// panic at this point; we must not. More in `crate::random`.
    pub fn generate() -> Result<Self, RandomError> {
        // Two independent secrets from two independent requests: signing and
        // agreement must not share a seed, otherwise compromise of one
        // becomes compromise of the other.
        let mut signing_seed = random_bytes::<32>()?;
        let mut agreement_seed = random_bytes::<32>()?;

        let identity = Self {
            signing: SigningKey::from_bytes(&signing_seed),
            agreement: StaticSecret::from(agreement_seed),
        };

        // The seeds have been copied into the keys; they are no longer needed here.
        signing_seed.zeroize();
        agreement_seed.zeroize();

        Ok(identity)
    }

    /// Restores an identity from what
    /// [`Identity::export_secret`] returned.
    ///
    /// The only check here is on length: any 64 bytes define a valid key pair, and
    /// there is no such thing as a "wrong" secret. So substituting the storage file
    /// would produce not a parse error but **a different identity**, and it is caught
    /// not here but by the record authenticity check (AEAD) before this function is
    /// called. The secret must not be read from an unverified source.
    pub fn from_secret_bytes(bytes: &[u8]) -> Result<Self, IdentityError> {
        if bytes.len() != SECRET_IDENTITY_BYTES {
            return Err(IdentityError::Length {
                expected: SECRET_IDENTITY_BYTES,
                got: bytes.len(),
            });
        }
        let mut signing_seed = [0u8; 32];
        let mut agreement_seed = [0u8; 32];
        signing_seed.copy_from_slice(bytes.get(..32).ok_or(IdentityError::Length {
            expected: SECRET_IDENTITY_BYTES,
            got: bytes.len(),
        })?);
        agreement_seed.copy_from_slice(bytes.get(32..64).ok_or(IdentityError::Length {
            expected: SECRET_IDENTITY_BYTES,
            got: bytes.len(),
        })?);

        let identity = Self {
            signing: SigningKey::from_bytes(&signing_seed),
            agreement: StaticSecret::from(agreement_seed),
        };

        signing_seed.zeroize();
        agreement_seed.zeroize();

        Ok(identity)
    }

    /// Exports the secret, only in order to seal it right away.
    ///
    /// Until persistent storage appeared this method deliberately did not exist, and
    /// it appeared with one proviso: the returned type wipes itself and prints
    /// nothing. The secret still does not cross the FFI boundary
    /// (R-004): only the public part goes to Dart.
    pub fn export_secret(&self) -> SecretBytes {
        let mut out = [0u8; SECRET_IDENTITY_BYTES];
        // Same layout as generate(): first the signing seed, then the agreement
        // secret. The order is part of the storage format; changing it
        // means breaking what has already been written.
        let signing = self.signing.to_bytes();
        let agreement = self.agreement.to_bytes();
        out[..32].copy_from_slice(&signing);
        out[32..].copy_from_slice(&agreement);
        SecretBytes(out)
    }

    pub fn public(&self) -> PublicIdentity {
        PublicIdentity {
            verifying: self.signing.verifying_key(),
            agreement: X25519Public::from(&self.agreement),
        }
    }

    pub fn sign(&self, message: &[u8]) -> Signature {
        self.signing.sign(message)
    }

    /// Diffie-Hellman shared secret. The result is raw material for HKDF, not a key:
    /// it must not be used directly.
    pub fn diffie_hellman(&self, peer: &X25519Public) -> x25519_dalek::SharedSecret {
        self.agreement.diffie_hellman(peer)
    }

    /// The secret this identity shares with `peer`, which only the two of them can compute:
    /// the addresses of their envelopes follow from it (`docs/transport.md` §2).
    ///
    /// `HKDF-SHA256(X25519(mine, theirs), info = "apeiron/pair/v1" ‖ lower ‖ higher)`, the two
    /// 64-byte public identities ordered bytewise, so both sides get the same key.
    ///
    /// Refused where it would not be secret: a low-order agreement key gives the same result
    /// for every secret, so anyone who knows the two public identities could compute it; and the
    /// agreement with oneself is not a pair at all — both directions would share addresses.
    pub fn pair_secret(&self, peer: &PublicIdentity) -> Result<PairSecret, IdentityError> {
        let mine = self.public().to_bytes();
        let theirs = peer.to_bytes();
        if mine == theirs {
            return Err(IdentityError::SelfAgreement);
        }
        let shared = self.agreement.diffie_hellman(peer.agreement_key());
        if !shared.was_contributory() {
            return Err(IdentityError::NonContributory);
        }
        let (lower, higher) = if mine <= theirs {
            (mine, theirs)
        } else {
            (theirs, mine)
        };
        let mut info = Vec::with_capacity(PAIR_DOMAIN.len() + 2 * PUBLIC_IDENTITY_BYTES);
        info.extend_from_slice(PAIR_DOMAIN);
        info.extend_from_slice(&lower);
        info.extend_from_slice(&higher);
        let mut out = [0u8; 32];
        // Unreachable at this length; if it ever happened, a zero key would be computable by
        // anyone, so it is an error, not a silently weak key.
        hkdf::Hkdf::<Sha256>::new(None, shared.as_bytes())
            .expand(&info, &mut out)
            .map_err(|_| IdentityError::NonContributory)?;
        Ok(PairSecret(out))
    }
}

impl std::fmt::Debug for Identity {
    /// Deliberately does not print the secret parts: log lines outlive the process.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Identity")
            .field("public", &self.public().fingerprint())
            .finish_non_exhaustive()
    }
}

/// How many hash bytes go into one group.
///
/// Not the same as [`FINGERPRINT_DIGITS_PER_GROUP`], though the numbers coincide:
/// there it is digits for a human, here bytes for arithmetic. Five bytes (40 bits)
/// are more than enough for five decimal digits: the remainder modulo 10⁵ is
/// distributed almost uniformly, with a bias on the order of 10⁻⁷.
const FINGERPRINT_BYTES_PER_GROUP: usize = 5;

/// Turns a hash into digits readable aloud: six groups of five.
fn digits_from_hash(hash: &[u8]) -> String {
    let (groups, _tail) = hash.as_chunks::<FINGERPRINT_BYTES_PER_GROUP>();
    groups
        .iter()
        .take(FINGERPRINT_GROUPS)
        .map(|chunk| {
            let v = chunk.iter().fold(0u64, |acc, b| (acc << 8) | u64::from(*b));
            format!(
                "{:0width$}",
                v % 100_000,
                width = FINGERPRINT_DIGITS_PER_GROUP
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    // In tests unwrap/expect are appropriate: a test failure is itself the error
    // message. The ban remains in force for all other code in the crate.
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )]

    use super::*;

    #[test]
    fn public_identity_roundtrip() {
        let id = Identity::generate().expect("the OS provides randomness");
        let pub_a = id.public();
        let bytes = pub_a.to_bytes();
        let pub_b = PublicIdentity::from_bytes(&bytes).expect("our own bytes must parse");
        assert_eq!(pub_a, pub_b);
    }

    #[test]
    fn from_bytes_rejects_wrong_length() {
        assert!(PublicIdentity::from_bytes(&[0u8; 63]).is_err());
        assert!(PublicIdentity::from_bytes(&[0u8; 65]).is_err());
    }

    #[test]
    fn signature_verifies_and_tampering_is_caught() {
        let id = Identity::generate().expect("the OS provides randomness");
        let pubkey = id.public();
        let msg = b"road to myworld";
        let sig = id.sign(msg);
        assert!(pubkey.verify(msg, &sig).is_ok());
        assert!(pubkey.verify(b"road to myworld!", &sig).is_err());
    }

    #[test]
    fn diffie_hellman_agrees_both_ways() {
        let a = Identity::generate().expect("the OS provides randomness");
        let b = Identity::generate().expect("the OS provides randomness");
        let ab = a.diffie_hellman(b.public().agreement_key());
        let ba = b.diffie_hellman(a.public().agreement_key());
        assert_eq!(ab.as_bytes(), ba.as_bytes());
    }

    #[test]
    fn pair_secret_is_the_same_on_both_sides_and_differs_between_pairs() {
        let a = Identity::generate().unwrap();
        let b = Identity::generate().unwrap();
        let c = Identity::generate().unwrap();
        let ab = a.pair_secret(&b.public()).unwrap();
        let ba = b.pair_secret(&a.public()).unwrap();
        assert_eq!(ab.as_bytes(), ba.as_bytes());
        let ac = a.pair_secret(&c.public()).unwrap();
        assert_ne!(ab.as_bytes(), ac.as_bytes());
        // Not the raw Diffie-Hellman output: the identities are mixed in.
        let raw = a.diffie_hellman(b.public().agreement_key());
        assert_ne!(ab.as_bytes(), raw.as_bytes());
    }

    #[test]
    fn pair_secret_with_oneself_is_refused() {
        let a = Identity::generate().unwrap();
        assert!(matches!(
            a.pair_secret(&a.public()),
            Err(IdentityError::SelfAgreement)
        ));
    }

    /// A peer who presents a low-order agreement key would make the pair secret computable by
    /// anyone who knows the two public identities.
    #[test]
    fn pair_secret_with_a_low_order_key_is_refused() {
        let a = Identity::generate().unwrap();
        let real = Identity::generate().unwrap().public().to_bytes();
        // The identity point and a point of order 8 on Curve25519 (both low order).
        let low_order: [[u8; 32]; 2] = [
            [0; 32],
            [
                0xe0, 0xeb, 0x7a, 0x7c, 0x3b, 0x41, 0xb8, 0xae, 0x16, 0x56, 0xe3, 0xfa, 0xf1, 0x9f,
                0xc4, 0x6a, 0xda, 0x09, 0x8d, 0xeb, 0x9c, 0x32, 0xb1, 0xfd, 0x86, 0x62, 0x05, 0x16,
                0x5f, 0x49, 0xb8, 0x00,
            ],
        ];
        for point in low_order {
            let mut bytes = real;
            bytes[32..].copy_from_slice(&point);
            let peer = PublicIdentity::from_bytes(&bytes).unwrap();
            assert!(matches!(
                a.pair_secret(&peer),
                Err(IdentityError::NonContributory)
            ));
        }
    }

    #[test]
    fn pair_secret_is_not_printed() {
        let a = Identity::generate().unwrap();
        let b = Identity::generate().unwrap();
        let s = a.pair_secret(&b.public()).unwrap();
        assert_eq!(format!("{s:?}"), "PairSecret(<hidden>)");
    }

    #[test]
    fn fingerprint_shape_is_stable() {
        let fp = Identity::generate()
            .expect("the OS provides randomness")
            .public()
            .fingerprint();
        let groups: Vec<&str> = fp.split(' ').collect();
        assert_eq!(groups.len(), FINGERPRINT_GROUPS);
        for g in groups {
            assert_eq!(g.len(), FINGERPRINT_DIGITS_PER_GROUP);
            assert!(g.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn safety_number_is_symmetric() {
        let a = Identity::generate()
            .expect("the OS provides randomness")
            .public();
        let b = Identity::generate()
            .expect("the OS provides randomness")
            .public();
        assert_eq!(a.safety_number(&b), b.safety_number(&a));
    }

    #[test]
    fn safety_number_changes_if_a_key_is_swapped() {
        // This is intermediary detection: a substituted key gives a different safety number.
        let a = Identity::generate()
            .expect("the OS provides randomness")
            .public();
        let b = Identity::generate()
            .expect("the OS provides randomness")
            .public();
        let impostor = Identity::generate()
            .expect("the OS provides randomness")
            .public();
        assert_ne!(a.safety_number(&b), a.safety_number(&impostor));
    }

    #[test]
    fn fingerprint_differs_from_safety_number() {
        // Separating the hashing domains must give different values.
        let a = Identity::generate()
            .expect("the OS provides randomness")
            .public();
        assert_ne!(a.fingerprint(), a.safety_number(&a));
    }
}

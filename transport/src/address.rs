//! Where items live, and who can make them (`docs/transport.md` §2).
//!
//! Every message part goes to its own address, used once; every day each side has one address
//! for its state. An address is an Ed25519 key pair derived from the pair secret, so to anyone
//! else the addresses of one conversation are unrelated random keys, and nobody else can make an
//! item at one of them: DHT nodes check the signature.

use apeiron_core::{PairSecret, PublicIdentity, SecretKey};
use hkdf::Hkdf;
use mainline::SigningKey;
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::TransportError;

const DIRECTION_DOMAIN: &[u8] = b"apeiron/addr/v1";
const SEED_DOMAIN: &[u8] = b"apeiron/addr/seed/v1";
const STATE_DOMAIN: &[u8] = b"apeiron/state/v1";
const KEY_DOMAIN: &[u8] = b"apeiron/addr/key/v1";
const ENVELOPE_DOMAIN: &[u8] = b"apeiron/addr/envelope/v1";

/// `HKDF-SHA256(ikm, info = parts joined)`, 32 bytes.
fn expand(ikm: &[u8], parts: &[&[u8]]) -> Result<[u8; 32], TransportError> {
    let info: Vec<u8> = parts.concat();
    let mut out = [0u8; 32];
    Hkdf::<Sha256>::new(None, ikm)
        .expand(&info, &mut out)
        .map_err(|_| TransportError::Derivation)?;
    Ok(out)
}

/// The key of one direction of one conversation: `K_dir`.
///
/// Bound to the Olm session: the same two identities introduced again get a new schedule, so
/// a new conversation never starts at an index whose old signed item anyone may still re-put.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct DirectionKey([u8; 32]);

impl DirectionKey {
    /// The direction `from → to` of the conversation `session_id` of this pair.
    pub fn new(
        pair: &PairSecret,
        from: &PublicIdentity,
        to: &PublicIdentity,
        session_id: &str,
    ) -> Result<Self, TransportError> {
        expand(
            pair.as_bytes(),
            &[
                DIRECTION_DOMAIN,
                &from.to_bytes(),
                &to.to_bytes(),
                session_id.as_bytes(),
            ],
        )
        .map(Self)
    }

    /// The address of message part `index`.
    pub fn message(&self, index: u64) -> Result<Slot, TransportError> {
        Slot::from_seed(&expand(&self.0, &[SEED_DOMAIN, &index.to_be_bytes()])?)
    }

    /// The address of this direction's state on UTC day `day`.
    pub fn state(&self, day: u32) -> Result<Slot, TransportError> {
        Slot::from_seed(&expand(&self.0, &[STATE_DOMAIN, &day.to_be_bytes()])?)
    }
}

impl std::fmt::Debug for DirectionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DirectionKey(<hidden>)")
    }
}

/// One address: the key that signs its item, and the key of the item's outer layer.
pub struct Slot {
    signer: SigningKey,
    envelope: SecretKey,
}

impl Slot {
    /// A slot from its seed. Also used for the inbox of an invitation (`docs/transport.md` §8).
    pub fn from_seed(seed: &[u8; 32]) -> Result<Self, TransportError> {
        let mut key = expand(seed, &[KEY_DOMAIN])?;
        let signer = SigningKey::from_bytes(&key);
        key.zeroize();
        Ok(Self {
            signer,
            envelope: SecretKey::from_bytes(expand(seed, &[ENVELOPE_DOMAIN])?),
        })
    }

    /// The BEP 44 key of the item: what the DHT sees and what is asked for.
    pub fn public_key(&self) -> [u8; 32] {
        self.signer.verifying_key().to_bytes()
    }

    pub(crate) fn signer(&self) -> &SigningKey {
        &self.signer
    }

    pub(crate) fn envelope_key(&self) -> &SecretKey {
        &self.envelope
    }
}

/// The UTC day of a unix time, as state addresses count it.
pub fn day_of(unix_s: u64) -> u32 {
    u32::try_from(unix_s / 86_400).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use apeiron_core::Identity;

    fn pair() -> (Identity, Identity, PairSecret) {
        let a = Identity::generate().unwrap();
        let b = Identity::generate().unwrap();
        let s = a.pair_secret(&b.public()).unwrap();
        (a, b, s)
    }

    #[test]
    fn both_sides_compute_the_same_addresses() {
        let (a, b, _) = pair();
        let ab = a.pair_secret(&b.public()).unwrap();
        let ba = b.pair_secret(&a.public()).unwrap();
        let from_a = DirectionKey::new(&ab, &a.public(), &b.public(), "s").unwrap();
        let from_b = DirectionKey::new(&ba, &a.public(), &b.public(), "s").unwrap();
        for n in [0, 1, 1_000_000] {
            assert_eq!(
                from_a.message(n).unwrap().public_key(),
                from_b.message(n).unwrap().public_key()
            );
        }
        assert_eq!(
            from_a.state(20_000).unwrap().public_key(),
            from_b.state(20_000).unwrap().public_key()
        );
    }

    #[test]
    fn directions_indices_days_and_sessions_are_separate() {
        let (a, b, s) = pair();
        let ab = DirectionKey::new(&s, &a.public(), &b.public(), "s1").unwrap();
        let ba = DirectionKey::new(&s, &b.public(), &a.public(), "s1").unwrap();
        let ab2 = DirectionKey::new(&s, &a.public(), &b.public(), "s2").unwrap();
        let keys = [
            ab.message(0).unwrap().public_key(),
            ab.message(1).unwrap().public_key(),
            ba.message(0).unwrap().public_key(),
            ab2.message(0).unwrap().public_key(),
            ab.state(0).unwrap().public_key(),
            ab.state(1).unwrap().public_key(),
            ba.state(0).unwrap().public_key(),
        ];
        for (i, x) in keys.iter().enumerate() {
            for y in keys.iter().skip(i + 1) {
                assert_ne!(x, y);
            }
        }
    }

    #[test]
    fn another_pair_cannot_compute_the_addresses() {
        let (a, b, s) = pair();
        let c = Identity::generate().unwrap();
        let other = a.pair_secret(&c.public()).unwrap();
        let real = DirectionKey::new(&s, &a.public(), &b.public(), "s").unwrap();
        let guess = DirectionKey::new(&other, &a.public(), &b.public(), "s").unwrap();
        assert_ne!(
            real.message(0).unwrap().public_key(),
            guess.message(0).unwrap().public_key()
        );
    }

    #[test]
    fn keys_are_not_printed() {
        let (a, b, s) = pair();
        let d = DirectionKey::new(&s, &a.public(), &b.public(), "s").unwrap();
        assert_eq!(format!("{d:?}"), "DirectionKey(<hidden>)");
    }

    #[test]
    fn days_are_utc_days() {
        assert_eq!(day_of(0), 0);
        assert_eq!(day_of(86_399), 0);
        assert_eq!(day_of(86_400), 1);
        // 24.09.2026, the day of the measurement.
        assert_eq!(day_of(1_790_245_730), 20_720);
    }
}

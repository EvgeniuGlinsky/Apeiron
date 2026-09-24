//! The prekey bundle: what a conversation starts with.
//!
//! To write first to someone who is not online right now, you need their key. A prekey
//! bundle is a set of a device's public keys that lies where it can be picked up: at a
//! blind relay, in a QR code, on paper.
//!
//! # What exactly is protected here
//!
//! The Olm double ratchet gives encryption strength but **says nothing about whose key
//! you took**. Substituting the bundle in the middle fully defeats strong cryptography:
//! both sides see "encryption works", while a third party reads
//! (see `s07_ratchet.py`, section F, and the `mitm_*` test below).
//!
//! That is why the bundle is **signed by the long-term identity** ([`Identity`]), and signature
//! verification cannot be forgotten through carelessness: a bundle parsed from bytes has the
//! type [`UnverifiedPrekeyBundle`], and a session can be built only from a [`PrekeyBundle`],
//! which cannot be obtained other than through [`UnverifiedPrekeyBundle::verify`].
//! Forgetting the check is impossible: code without it will not compile.
//!
//! The signature, however, **does not answer the question of whose identity this is**.
//! Only fingerprint verification by voice or in person answers that; see
//! [`PublicIdentity::safety_number`].

use ed25519_dalek::{Signature, SignatureError};
use vodozemac::{olm::Account, Curve25519PublicKey, Ed25519PublicKey, KeyError};

use crate::identity::{Identity, IdentityError, PublicIdentity, PUBLIC_IDENTITY_BYTES};

/// Domain separator for the bundle signature.
///
/// Changing it makes old bundles unverifiable: a deliberately breaking change. The
/// separator is needed so that a bundle signature cannot be presented as the signature
/// of something else: the same identity signs both bundles and log entries, and their
/// domains must not overlap.
const PREKEY_DOMAIN: &[u8] = b"apeiron/prekey-bundle/v1";

/// Length of a Curve25519 and Ed25519 key in bytes.
const KEY_BYTES: usize = 32;
/// Length of an Ed25519 signature in bytes.
const SIGNATURE_BYTES: usize = 64;

/// Length of a serialized bundle.
pub const PREKEY_BUNDLE_BYTES: usize = PUBLIC_IDENTITY_BYTES + KEY_BYTES * 3 + SIGNATURE_BYTES;

/// What can go wrong with a prekey bundle.
#[derive(Debug, thiserror::Error)]
pub enum PrekeyError {
    #[error("пакет пред-ключей: ожидалось {expected} байт, получено {got}")]
    Length { expected: usize, got: usize },

    #[error("пакет пред-ключей: личность не разбирается: {0}")]
    Identity(#[from] IdentityError),

    #[error("пакет пред-ключей: ключ не разбирается: {0}")]
    Key(#[from] KeyError),

    #[error("пакет пред-ключей: подпись не разбирается")]
    MalformedSignature,

    #[error(
        "ПОДПИСЬ ПАКЕТА НЕВЕРНА. Ключи в нём не принадлежат заявленной личности — \
         либо пакет подменён по дороге, либо повреждён. Переписку начинать нельзя."
    )]
    BadSignature,

    #[error("у устройства не осталось неизрасходованных одноразовых ключей")]
    NoOneTimeKeys,
}

/// A prekey bundle **whose signature has not been verified yet**.
///
/// The only useful thing that can be done with it is to call
/// [`UnverifiedPrekeyBundle::verify`]. This is not a polite wish but the way the types
/// are built: a [`PrekeyBundle`] cannot be assembled any other way.
#[derive(Debug, Clone)]
pub struct UnverifiedPrekeyBundle {
    inner: PrekeyBundle,
}

/// A verified prekey bundle.
///
/// "Verified" here means exactly one thing: the keys in it are signed by the identity
/// that is stated in it. That this identity belongs to the person you have in mind is
/// checked **only** by fingerprint verification.
#[derive(Debug, Clone)]
pub struct PrekeyBundle {
    identity: PublicIdentity,
    device_curve: Curve25519PublicKey,
    device_ed: Ed25519PublicKey,
    one_time: Curve25519PublicKey,
    signature: Signature,
}

impl PrekeyBundle {
    /// Assembles a bundle for one's own device, taking one one-time key.
    ///
    /// One-time keys are one-time for a reason: every bundle handed out must carry its
    /// own. Reusing a key weakens the initial agreement to having no forward secrecy
    /// on the first message.
    pub fn create(identity: &Identity, account: &mut Account) -> Result<Self, PrekeyError> {
        // The question must be about **not yet handed out** keys: `one_time_keys()`
        // returns only those, while `stored_one_time_key_count()` also counts the
        // ones already handed out. They are easy to mix up, and then a second bundle
        // cannot be assembled at all; that is exactly what tripped up the first version.
        if account.one_time_keys().is_empty() {
            account.generate_one_time_keys(1);
        }

        let one_time = *account
            .one_time_keys()
            .values()
            .next()
            .ok_or(PrekeyError::NoOneTimeKeys)?;

        // The key counts as handed out immediately: otherwise on the next call we would
        // hand out the same one, and a one-time key is one-time for a reason.
        account.mark_keys_as_published();

        let device_curve = account.curve25519_key();
        let device_ed = account.ed25519_key();
        let public = identity.public();

        let signature = identity.sign(&signed_payload(
            &public,
            &device_curve,
            &device_ed,
            &one_time,
        ));

        Ok(Self {
            identity: public,
            device_curve,
            device_ed,
            one_time,
            signature,
        })
    }

    /// The long-term identity that signed the bundle.
    pub fn identity(&self) -> &PublicIdentity {
        &self.identity
    }

    /// The device key for agreement (Curve25519).
    pub fn device_curve_key(&self) -> Curve25519PublicKey {
        self.device_curve
    }

    /// The device signing key (Ed25519).
    pub fn device_ed_key(&self) -> Ed25519PublicKey {
        self.device_ed
    }

    /// The one-time key.
    pub fn one_time_key(&self) -> Curve25519PublicKey {
        self.one_time
    }

    /// Serialization into canonical form, which is also what gets signed.
    pub fn to_bytes(&self) -> [u8; PREKEY_BUNDLE_BYTES] {
        let mut out = [0u8; PREKEY_BUNDLE_BYTES];
        let mut at = 0;
        let mut put = |src: &[u8]| {
            let end = at + src.len();
            if let Some(slot) = out.get_mut(at..end) {
                slot.copy_from_slice(src);
            }
            at = end;
        };
        put(&self.identity.to_bytes());
        put(&self.device_curve.to_bytes());
        put(self.device_ed.as_bytes());
        put(&self.one_time.to_bytes());
        put(&self.signature.to_bytes());
        out
    }

    /// Parses a bundle. The signature is **not verified**: that is what
    /// [`UnverifiedPrekeyBundle::verify`] is for, and it cannot be bypassed.
    pub fn parse(bytes: &[u8]) -> Result<UnverifiedPrekeyBundle, PrekeyError> {
        if bytes.len() != PREKEY_BUNDLE_BYTES {
            return Err(PrekeyError::Length {
                expected: PREKEY_BUNDLE_BYTES,
                got: bytes.len(),
            });
        }

        let mut at = 0;
        let mut take = |n: usize| -> &[u8] {
            let slice = bytes.get(at..at + n).unwrap_or(&[]);
            at += n;
            slice
        };

        let identity = PublicIdentity::from_bytes(take(PUBLIC_IDENTITY_BYTES))?;
        let device_curve = Curve25519PublicKey::from_slice(take(KEY_BYTES))?;
        let device_ed = ed25519_from_slice(take(KEY_BYTES))?;
        let one_time = Curve25519PublicKey::from_slice(take(KEY_BYTES))?;
        let signature = Signature::from_slice(take(SIGNATURE_BYTES))
            .map_err(|_: SignatureError| PrekeyError::MalformedSignature)?;

        Ok(UnverifiedPrekeyBundle {
            inner: Self {
                identity,
                device_curve,
                device_ed,
                one_time,
                signature,
            },
        })
    }
}

impl UnverifiedPrekeyBundle {
    /// The identity **claimed** in the bundle. Looking at it before the signature is
    /// verified is fine only to decide whether to bother at all.
    pub fn claimed_identity(&self) -> &PublicIdentity {
        &self.inner.identity
    }

    /// Verifies the signature and returns a bundle fit for use.
    pub fn verify(self) -> Result<PrekeyBundle, PrekeyError> {
        let payload = signed_payload(
            &self.inner.identity,
            &self.inner.device_curve,
            &self.inner.device_ed,
            &self.inner.one_time,
        );
        self.inner
            .identity
            .verify(&payload, &self.inner.signature)
            .map_err(|_| PrekeyError::BadSignature)?;
        Ok(self.inner)
    }
}

/// What gets signed: the domain separator and all the bundle's keys in a row.
///
/// Including the identity itself in the signature is mandatory: otherwise a signature
/// lifted from one bundle could be presented in a bundle with a different identity.
fn signed_payload(
    identity: &PublicIdentity,
    device_curve: &Curve25519PublicKey,
    device_ed: &Ed25519PublicKey,
    one_time: &Curve25519PublicKey,
) -> Vec<u8> {
    let mut payload =
        Vec::with_capacity(PREKEY_DOMAIN.len() + PUBLIC_IDENTITY_BYTES + KEY_BYTES * 3);
    payload.extend_from_slice(PREKEY_DOMAIN);
    payload.extend_from_slice(&identity.to_bytes());
    payload.extend_from_slice(&device_curve.to_bytes());
    payload.extend_from_slice(device_ed.as_bytes());
    payload.extend_from_slice(&one_time.to_bytes());
    payload
}

fn ed25519_from_slice(bytes: &[u8]) -> Result<Ed25519PublicKey, PrekeyError> {
    let array: [u8; KEY_BYTES] = bytes.try_into().map_err(|_| PrekeyError::Length {
        expected: KEY_BYTES,
        got: bytes.len(),
    })?;
    Ok(Ed25519PublicKey::from_slice(&array)?)
}

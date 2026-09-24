//! A BEP 44 item, sealed and signed once, stored exactly as made.
//!
//! The signature covers `seq ‖ value` only, so a stored item can be put again by anyone who has
//! it — including the background job while the vault is locked, which holds no keys at all.

use apeiron_core::{open, seal};
use ed25519_dalek::{Signature, Signer, VerifyingKey};
use mainline::MutableItem;

use crate::address::Slot;
use crate::envelope::{Body, ITEM_BYTES};
use crate::TransportError;

/// Additional data of the outer layer: the version of the item format.
const AAD: &[u8] = b"apeiron/envelope/v1";

/// `key (32) ‖ seq (8) ‖ signature (64) ‖ value`.
const STORED_HEAD_BYTES: usize = 32 + 8 + 64;

#[derive(Clone, PartialEq, Eq)]
pub struct SignedItem {
    /// The BEP 44 key: the address.
    pub key: [u8; 32],
    pub seq: i64,
    pub value: Vec<u8>,
    pub signature: [u8; 64],
}

impl std::fmt::Debug for SignedItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignedItem")
            .field("key", &hex::encode(self.key))
            .field("seq", &self.seq)
            .finish_non_exhaustive()
    }
}

impl SignedItem {
    /// Seals `body` under the slot's outer key and signs it with the slot's key.
    pub fn seal(slot: &Slot, seq: i64, body: &Body) -> Result<Self, TransportError> {
        let inner = body.encode()?;
        let value = seal(slot.envelope_key(), AAD, &inner)?;
        if value.len() != ITEM_BYTES {
            return Err(TransportError::Internal(
                "an item came out of the wrong length",
            ));
        }
        let signature = slot.signer().sign(&signable(seq, &value)).to_bytes();
        Ok(Self {
            key: slot.public_key(),
            seq,
            value,
            signature,
        })
    }

    /// Signs a value made elsewhere — the inbox reply of an invitation, which has its own outer
    /// layer (`crate::invite`). Still exactly [`ITEM_BYTES`] long, like every item.
    pub(crate) fn sign_value(
        slot: &Slot,
        seq: i64,
        value: Vec<u8>,
    ) -> Result<Self, TransportError> {
        if value.len() != ITEM_BYTES {
            return Err(TransportError::Internal(
                "an item came out of the wrong length",
            ));
        }
        let signature = slot.signer().sign(&signable(seq, &value)).to_bytes();
        Ok(Self {
            key: slot.public_key(),
            seq,
            value,
            signature,
        })
    }

    /// The item as `mainline` puts it.
    pub fn to_mutable(&self) -> MutableItem {
        MutableItem::new_signed_unchecked(self.key, self.signature, &self.value, self.seq, None)
    }

    /// Whether the signature is valid — what every DHT node checks before storing.
    pub fn verifies(&self) -> bool {
        VerifyingKey::from_bytes(&self.key)
            .and_then(|k| {
                k.verify_strict(
                    &signable(self.seq, &self.value),
                    &Signature::from_bytes(&self.signature),
                )
            })
            .is_ok()
    }

    /// For storage: everything needed to put the item again.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(STORED_HEAD_BYTES + self.value.len());
        out.extend_from_slice(&self.key);
        out.extend_from_slice(&self.seq.to_be_bytes());
        out.extend_from_slice(&self.signature);
        out.extend_from_slice(&self.value);
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TransportError> {
        let (head, value) = bytes
            .split_at_checked(STORED_HEAD_BYTES)
            .ok_or(TransportError::Internal("a stored item is too short"))?;
        let (key, rest) = head.split_at(32);
        let (seq, signature) = rest.split_at(8);
        let item = Self {
            key: key
                .try_into()
                .map_err(|_| TransportError::Internal("stored key"))?,
            seq: i64::from_be_bytes(
                seq.try_into()
                    .map_err(|_| TransportError::Internal("stored seq"))?,
            ),
            signature: signature
                .try_into()
                .map_err(|_| TransportError::Internal("stored signature"))?,
            value: value.to_vec(),
        };
        if !item.verifies() {
            return Err(TransportError::Internal("a stored item does not verify"));
        }
        Ok(item)
    }
}

/// Opens a value found at the slot's address.
pub fn open_value(slot: &Slot, value: &[u8]) -> Result<Body, TransportError> {
    if value.len() != ITEM_BYTES {
        return Err(TransportError::NotOurs);
    }
    let inner = open(slot.envelope_key(), AAD, value).map_err(|_| TransportError::NotOurs)?;
    Ok(Body::decode(&inner)?)
}

/// What BEP 44 signs, without a salt: `3:seqi{seq}e1:v{len}:{value}`.
fn signable(seq: i64, value: &[u8]) -> Vec<u8> {
    let mut out = format!("3:seqi{seq}e1:v{}:", value.len()).into_bytes();
    out.extend_from_slice(value);
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;
    use crate::envelope::{Part, State};

    fn slot(n: u8) -> Slot {
        Slot::from_seed(&[n; 32]).unwrap()
    }

    fn body() -> Body {
        Body::Part(Part {
            part: 0,
            parts: 1,
            olm_type: 0,
            olm: b"not really olm".to_vec(),
        })
    }

    #[test]
    fn an_item_is_one_length_verifies_and_opens() {
        let s = slot(1);
        for b in [body(), Body::State(State::default())] {
            let item = SignedItem::seal(&s, 1, &b).unwrap();
            assert_eq!(item.value.len(), ITEM_BYTES);
            assert!(item.verifies());
            assert_eq!(open_value(&s, &item.value).unwrap(), b);
        }
    }

    /// The signature is the one mainline makes, so the real DHT accepts the item.
    #[test]
    fn the_signature_is_the_bep44_one() {
        let s = slot(2);
        let item = SignedItem::seal(&s, 3, &body()).unwrap();
        let theirs = MutableItem::new(s.signer().clone(), &item.value, 3, None);
        assert_eq!(theirs.signature(), &item.signature);
        assert_eq!(theirs.key(), &item.key);
    }

    #[test]
    fn stored_bytes_round_trip_and_are_checked() {
        let item = SignedItem::seal(&slot(3), 1, &body()).unwrap();
        assert_eq!(SignedItem::from_bytes(&item.to_bytes()).unwrap(), item);
        let mut bad = item.to_bytes();
        let last = bad.len() - 1;
        bad[last] ^= 1;
        assert!(SignedItem::from_bytes(&bad).is_err());
    }

    #[test]
    fn a_value_does_not_open_elsewhere_or_tampered() {
        let item = SignedItem::seal(&slot(4), 1, &body()).unwrap();
        assert!(matches!(
            open_value(&slot(5), &item.value),
            Err(TransportError::NotOurs)
        ));
        let mut v = item.value.clone();
        v[100] ^= 1;
        assert!(matches!(
            open_value(&slot(4), &v),
            Err(TransportError::NotOurs)
        ));
        assert!(matches!(
            open_value(&slot(4), &v[..899]),
            Err(TransportError::NotOurs)
        ));
    }

    /// The outer layer hides what is inside: the same body sealed twice gives unrelated values.
    #[test]
    fn values_look_random() {
        let s = slot(6);
        let a = SignedItem::seal(&s, 1, &body()).unwrap();
        let b = SignedItem::seal(&s, 1, &body()).unwrap();
        assert_ne!(a.value, b.value);
        assert!(!a
            .value
            .windows(b"not really olm".len())
            .any(|w| w == b"not really olm"));
    }
}

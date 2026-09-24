//! What is inside an item, under its outer layer (`docs/transport.md` §3).
//!
//! Every item is exactly [`ITEM_BYTES`] long, and inside the outer layer every body is padded to
//! [`INNER_BYTES`], so the DHT sees one length and random-looking bytes whatever is carried.
//! Decoding is strict: an unknown version or kind, a length that does not fit, or non-zero
//! padding is refused rather than read around.

use apeiron_core::aead::{NONCE_BYTES, TAG_BYTES};

/// Length of every item's value: the measured size (`docs/transport.md` §3).
pub const ITEM_BYTES: usize = 900;

/// Length of what the outer layer protects.
pub const INNER_BYTES: usize = ITEM_BYTES - NONCE_BYTES - TAG_BYTES;

const VERSION: u8 = 1;
const KIND_PART: u8 = 1;
const KIND_STATE: u8 = 2;

/// `version ‖ kind ‖ body_len (2)`.
const HEADER_BYTES: usize = 4;

/// `part ‖ parts ‖ olm_type`.
const PART_HEADER_BYTES: usize = 3;

/// The longest Olm message one part can carry.
pub const MAX_OLM_BYTES: usize = INNER_BYTES - HEADER_BYTES - PART_HEADER_BYTES;

/// Parts of one message at most: Olm keeps 40 skipped keys per chain, and a background job
/// has minutes, not hours.
pub const MAX_PARTS: u8 = 32;

const STATE_BYTES: usize = 32;

/// One part of a message.
///
/// A message is a run of consecutive indices, so it needs no identifier of its own: part `k` of
/// a message whose first part is at index `first` sits at `first + k`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub part: u8,
    pub parts: u8,
    /// vodozemac's message type: 0 — pre-key, 1 — normal.
    pub olm_type: u8,
    pub olm: Vec<u8>,
}

/// What one side tells the other about the conversation (`docs/transport.md` §3).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct State {
    /// How many of the peer's indices this side has, without a gap.
    pub next_recv: u64,
    /// Which of the 64 indices after `next_recv` this side also has: bit `i` is
    /// `next_recv + 1 + i`.
    pub recv_bits: u64,
    /// How many indices this side has used towards the peer.
    pub next_send: u64,
    /// The lowest index this side still re-puts; below it, it has given up.
    pub send_floor: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    Part(Part),
    State(State),
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EnvelopeError {
    #[error("the body has {0} bytes, not {INNER_BYTES}")]
    Length(usize),
    #[error("unknown format version {0}")]
    Version(u8),
    #[error("unknown kind {0}")]
    Kind(u8),
    #[error("the body does not fit: {0}")]
    Malformed(&'static str),
    #[error("an Olm message of {0} bytes does not fit into a part")]
    TooLong(usize),
}

impl Body {
    /// Exactly [`INNER_BYTES`] bytes.
    pub fn encode(&self) -> Result<Vec<u8>, EnvelopeError> {
        let (kind, body) = match self {
            Body::Part(p) => {
                if p.olm.len() > MAX_OLM_BYTES {
                    return Err(EnvelopeError::TooLong(p.olm.len()));
                }
                if p.parts == 0 || p.parts > MAX_PARTS || p.part >= p.parts || p.olm_type > 1 {
                    return Err(EnvelopeError::Malformed("part numbering or Olm type"));
                }
                let mut b = Vec::with_capacity(PART_HEADER_BYTES + p.olm.len());
                b.extend_from_slice(&[p.part, p.parts, p.olm_type]);
                b.extend_from_slice(&p.olm);
                (KIND_PART, b)
            }
            Body::State(s) => {
                let mut b = Vec::with_capacity(STATE_BYTES);
                for v in [s.next_recv, s.recv_bits, s.next_send, s.send_floor] {
                    b.extend_from_slice(&v.to_be_bytes());
                }
                (KIND_STATE, b)
            }
        };
        let len = u16::try_from(body.len()).map_err(|_| EnvelopeError::TooLong(body.len()))?;
        let mut out = Vec::with_capacity(INNER_BYTES);
        out.extend_from_slice(&[VERSION, kind]);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&body);
        if out.len() > INNER_BYTES {
            return Err(EnvelopeError::TooLong(body.len()));
        }
        out.resize(INNER_BYTES, 0);
        Ok(out)
    }

    pub fn decode(inner: &[u8]) -> Result<Self, EnvelopeError> {
        if inner.len() != INNER_BYTES {
            return Err(EnvelopeError::Length(inner.len()));
        }
        let (header, rest) = inner.split_at(HEADER_BYTES);
        let [version, kind, len_hi, len_lo] = <[u8; HEADER_BYTES]>::try_from(header)
            .map_err(|_| EnvelopeError::Malformed("header"))?;
        if version != VERSION {
            return Err(EnvelopeError::Version(version));
        }
        let len = usize::from(u16::from_be_bytes([len_hi, len_lo]));
        let body = rest
            .get(..len)
            .ok_or(EnvelopeError::Malformed("length beyond the body"))?;
        let padding = rest.get(len..).unwrap_or_default();
        if padding.iter().any(|&b| b != 0) {
            return Err(EnvelopeError::Malformed("non-zero padding"));
        }
        match kind {
            KIND_PART => {
                let (head, olm) = body
                    .split_at_checked(PART_HEADER_BYTES)
                    .ok_or(EnvelopeError::Malformed("part header"))?;
                let [part, parts, olm_type] = <[u8; PART_HEADER_BYTES]>::try_from(head)
                    .map_err(|_| EnvelopeError::Malformed("part header"))?;
                if parts == 0 || parts > MAX_PARTS || part >= parts || olm_type > 1 {
                    return Err(EnvelopeError::Malformed("part numbering or Olm type"));
                }
                if olm.is_empty() {
                    return Err(EnvelopeError::Malformed("empty Olm message"));
                }
                Ok(Body::Part(Part {
                    part,
                    parts,
                    olm_type,
                    olm: olm.to_vec(),
                }))
            }
            KIND_STATE => {
                if body.len() != STATE_BYTES {
                    return Err(EnvelopeError::Malformed("state length"));
                }
                let (words, _) = body.as_chunks::<8>();
                let mut v = [0u64; 4];
                for (dst, w) in v.iter_mut().zip(words) {
                    *dst = u64::from_be_bytes(*w);
                }
                let [next_recv, recv_bits, next_send, send_floor] = v;
                if send_floor > next_send {
                    return Err(EnvelopeError::Malformed("floor above the sent count"));
                }
                Ok(Body::State(State {
                    next_recv,
                    recv_bits,
                    next_send,
                    send_floor,
                }))
            }
            other => Err(EnvelopeError::Kind(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;

    fn part(len: usize) -> Body {
        Body::Part(Part {
            part: 2,
            parts: 5,
            olm_type: 1,
            olm: vec![0xab; len],
        })
    }

    #[test]
    fn sizes_are_the_documented_ones() {
        assert_eq!(INNER_BYTES, 860);
        assert_eq!(MAX_OLM_BYTES, 853);
    }

    #[test]
    fn bodies_round_trip_at_one_length() {
        let state = Body::State(State {
            next_recv: 7,
            recv_bits: 0b1011,
            next_send: 12,
            send_floor: 3,
        });
        for body in [part(1), part(MAX_OLM_BYTES), state] {
            let inner = body.encode().unwrap();
            assert_eq!(inner.len(), INNER_BYTES);
            assert_eq!(Body::decode(&inner).unwrap(), body);
        }
    }

    #[test]
    fn a_part_too_long_is_refused_not_cut() {
        assert_eq!(
            part(MAX_OLM_BYTES + 1).encode(),
            Err(EnvelopeError::TooLong(MAX_OLM_BYTES + 1))
        );
    }

    #[test]
    fn malformed_bodies_are_refused() {
        let good = part(10).encode().unwrap();

        let mut v = good.clone();
        v[0] = 9;
        assert_eq!(Body::decode(&v), Err(EnvelopeError::Version(9)));

        let mut v = good.clone();
        v[1] = 7;
        assert_eq!(Body::decode(&v), Err(EnvelopeError::Kind(7)));

        let mut v = good.clone();
        v[INNER_BYTES - 1] = 1;
        assert!(matches!(Body::decode(&v), Err(EnvelopeError::Malformed(_))));

        let mut v = good.clone();
        v[2..4].copy_from_slice(&u16::MAX.to_be_bytes());
        assert!(matches!(Body::decode(&v), Err(EnvelopeError::Malformed(_))));

        // Part 5 of 5 does not exist (parts are counted from 0).
        let mut v = good.clone();
        v[4] = 5;
        assert!(matches!(Body::decode(&v), Err(EnvelopeError::Malformed(_))));

        assert_eq!(
            Body::decode(&good[1..]),
            Err(EnvelopeError::Length(INNER_BYTES - 1))
        );
    }

    #[test]
    fn a_state_with_the_floor_above_what_was_sent_is_refused() {
        let s = Body::State(State {
            next_recv: 0,
            recv_bits: 0,
            next_send: 2,
            send_floor: 3,
        });
        assert!(matches!(
            Body::decode(&s.encode().unwrap()),
            Err(EnvelopeError::Malformed(_))
        ));
    }
}

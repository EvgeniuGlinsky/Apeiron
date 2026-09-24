//! Go/no-go measurement: can Mainline DHT hold envelopes long enough to be a dead drop?
//!
//! Every probe item is derived from a 32-byte seed: the signing key, the BEP 44 salt and
//! a 900-byte value (an envelope's size; BEP 44 caps a value at 1000 bytes). So a phone
//! can report what it put as a short list of seeds, and the desktop can fetch those items
//! hours later — and the other way round, the desktop puts items with *public* seeds of the
//! day, which a phone can fetch without anything being sent to it.
//!
//! The seeds and values are random test data. Nothing here is secret.

// The blocking API of `mainline` is marked deprecated in favour of the async one. For a
// sequential measurement the blocking API is simpler and easier to read; the transport
// proper will use the async one.
#![allow(deprecated)]

use std::time::{Duration, Instant};

use mainline::{Dht, MutableItem, SigningKey};
use sha2::{Digest, Sha256};

/// Size of a probe value: an envelope's size.
pub const VALUE_BYTES: usize = 900;

/// Everything about one probe item follows from this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seed(pub [u8; 32]);

impl Seed {
    /// A fresh random seed.
    pub fn random() -> Result<Self, String> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|e| format!("the OS refused randomness: {e}"))?;
        Ok(Self(bytes))
    }

    /// A seed anyone can compute: item `index` of day `day` (e.g. "2026-09-24").
    pub fn public(day: &str, index: u32) -> Self {
        Self(hash(&[
            b"apeiron/probe/public/v1",
            day.as_bytes(),
            &index.to_be_bytes(),
        ]))
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(text: &str) -> Result<Self, String> {
        let bytes = hex::decode(text.trim()).map_err(|e| format!("bad seed: {e}"))?;
        bytes
            .try_into()
            .map(Self)
            .map_err(|_| "a seed is 32 bytes".to_string())
    }

    fn signer(&self) -> SigningKey {
        SigningKey::from_bytes(&hash(&[b"apeiron/probe/key/v1", &self.0]))
    }

    fn salt(&self) -> [u8; 8] {
        let h = hash(&[b"apeiron/probe/salt/v1", &self.0]);
        let mut salt = [0u8; 8];
        for (dst, src) in salt.iter_mut().zip(h) {
            *dst = src;
        }
        salt
    }

    fn value(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(VALUE_BYTES + 32);
        let mut block = 0u32;
        while out.len() < VALUE_BYTES {
            out.extend_from_slice(&hash(&[
                b"apeiron/probe/value/v1",
                &self.0,
                &block.to_be_bytes(),
            ]));
            block = block.saturating_add(1);
        }
        out.truncate(VALUE_BYTES);
        out
    }

    fn public_key(&self) -> [u8; 32] {
        self.signer().verifying_key().to_bytes()
    }
}

fn hash(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

fn ms(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

/// A fresh client node, bootstrapped: its own id, port and routing table. Returns the node
/// and how long bootstrapping took.
pub fn node() -> Result<(Dht, u64), String> {
    let started = Instant::now();
    let dht = Dht::client().map_err(|e| format!("cannot open a UDP socket: {e}"))?;
    if !dht.bootstrapped() {
        return Err("bootstrap failed: no DHT node answered".to_string());
    }
    Ok((dht, ms(started.elapsed())))
}

/// Result of one put.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PutOutcome {
    pub ok: bool,
    pub ms: u64,
    pub error: String,
}

/// Puts the item of `seed`.
pub fn put(dht: &Dht, seed: &Seed) -> PutOutcome {
    let salt = seed.salt();
    let item = MutableItem::new(seed.signer(), &seed.value(), 1, Some(&salt));
    let started = Instant::now();
    let result = dht.put_mutable(item, None);
    PutOutcome {
        ok: result.is_ok(),
        ms: ms(started.elapsed()),
        error: result.err().map(|e| e.to_string()).unwrap_or_default(),
    }
}

/// Result of one get.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetOutcome {
    /// Some node returned exactly the expected value.
    pub found: bool,
    /// Some node returned a different value under the same key and salt.
    pub wrong: bool,
    /// Time to the first response, if any.
    pub first_ms: Option<u64>,
    /// Time until the query finished.
    pub query_ms: u64,
    pub responses: usize,
}

/// Looks up the item of `seed`.
pub fn get(dht: &Dht, seed: &Seed) -> GetOutcome {
    let salt = seed.salt();
    let expected = seed.value();
    let started = Instant::now();
    let mut out = GetOutcome {
        found: false,
        wrong: false,
        first_ms: None,
        query_ms: 0,
        responses: 0,
    };
    for item in dht.get_mutable(&seed.public_key(), Some(&salt), None) {
        out.responses = out.responses.saturating_add(1);
        if out.first_ms.is_none() {
            out.first_ms = Some(ms(started.elapsed()));
        }
        if item.value() == expected.as_slice() {
            out.found = true;
        } else {
            out.wrong = true;
        }
    }
    out.query_ms = ms(started.elapsed());
    out
}

/// The `p`-th percentile (0–100) of `values`, nearest rank. Zero for no values.
pub fn percentile(values: &[u64], p: usize) -> u64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let Some(last) = sorted.len().checked_sub(1) else {
        return 0;
    };
    let idx = last.saturating_mul(p.min(100)).div_ceil(100);
    sorted.get(idx).copied().unwrap_or(0)
}

/// Summary line of a batch of gets, the same on the phone and on the desktop.
pub fn summarize_gets(outcomes: &[GetOutcome]) -> String {
    let found = outcomes.iter().filter(|o| o.found).count();
    let firsts: Vec<u64> = outcomes
        .iter()
        .filter(|o| o.found)
        .filter_map(|o| o.first_ms)
        .collect();
    let wrong = outcomes.iter().filter(|o| o.wrong).count();
    format!(
        "found {found}/{}; first response p50 {} ms, p95 {} ms; wrong values {wrong}",
        outcomes.len(),
        percentile(&firsts, 50),
        percentile(&firsts, 95),
    )
}

/// Summary line of a batch of puts.
pub fn summarize_puts(outcomes: &[PutOutcome]) -> String {
    let ok = outcomes.iter().filter(|o| o.ok).count();
    let times: Vec<u64> = outcomes.iter().map(|o| o.ms).collect();
    let first_error = outcomes
        .iter()
        .find(|o| !o.ok)
        .map(|o| format!("; first error: {}", o.error))
        .unwrap_or_default();
    format!(
        "put {ok}/{}; put time p50 {} ms, p95 {} ms{first_error}",
        outcomes.len(),
        percentile(&times, 50),
        percentile(&times, 95),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_follow_from_the_seed() {
        let a = Seed([1; 32]);
        assert_eq!(a.value(), a.value());
        assert_eq!(a.value().len(), VALUE_BYTES);
        assert_ne!(a.value(), Seed([2; 32]).value());
        assert_ne!(a.public_key(), Seed([2; 32]).public_key());
    }

    #[test]
    fn public_seeds_depend_on_day_and_index() {
        assert_eq!(Seed::public("2026-09-24", 0), Seed::public("2026-09-24", 0));
        assert_ne!(Seed::public("2026-09-24", 0), Seed::public("2026-09-24", 1));
        assert_ne!(Seed::public("2026-09-24", 0), Seed::public("2026-09-25", 0));
    }

    #[test]
    fn seed_round_trips_through_hex() {
        let s = Seed([0xab; 32]);
        assert_eq!(Seed::from_hex(&s.to_hex()), Ok(s));
        assert!(Seed::from_hex("abcd").is_err());
    }

    #[test]
    fn percentile_is_nearest_rank() {
        assert_eq!(percentile(&[], 50), 0);
        assert_eq!(percentile(&[5], 95), 5);
        assert_eq!(percentile(&[4, 1, 3, 2], 50), 3);
        assert_eq!(percentile(&[4, 1, 3, 2], 95), 4);
    }

    /// Put and get through a local test network, no internet needed.
    #[test]
    fn put_then_get_on_a_local_testnet() {
        let Ok(testnet) = mainline::Testnet::builder(10).build() else {
            return; // no UDP on this machine: nothing to test here
        };
        let Ok(dht) = Dht::builder()
            .bootstrap(&testnet.bootstrap)
            .bind_address(std::net::Ipv4Addr::LOCALHOST)
            .build()
        else {
            return;
        };
        assert!(dht.bootstrapped(), "the local testnet did not bootstrap");
        let seed = Seed([7; 32]);
        let put_outcome = put(&dht, &seed);
        assert!(put_outcome.ok, "put failed: {}", put_outcome.error);
        let got = get(&dht, &seed);
        assert!(got.found);
        assert!(!got.wrong);
        assert!(!get(&dht, &Seed([8; 32])).found);
    }
}

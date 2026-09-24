//! The real DHT behind [`Dht`]: Mainline, through `mainline` (`docs/transport.md` §10, step 2).
//!
//! `mainline` verifies the signature of every item a node returns, so what comes back from
//! [`MainlineDht::get_first`] is an item signed by the address's key — whose value still has to
//! open with the pair's keys before it means anything (`crate::item::open_value`).

// The blocking API of `mainline` is marked deprecated in favour of the async one. The rounds of
// the transport are sequential and short; the async API comes with the active polling task.
#![allow(deprecated)]

use std::time::{Duration, Instant};

use crate::dht::{Dht, DhtError, Found};

/// How long [`MainlineDht::get_latest`] keeps listening after the first answer.
const LATEST_WINDOW: Duration = Duration::from_millis(500);
use crate::item::SignedItem;

pub struct MainlineDht {
    dht: mainline::Dht,
}

impl MainlineDht {
    /// A client node, bootstrapped through the well-known nodes.
    pub fn bootstrap() -> Result<Self, DhtError> {
        let dht = mainline::Dht::client().map_err(|e| DhtError::Unreachable(e.to_string()))?;
        Self::ready(dht)
    }

    /// A client node bootstrapped from a routing table saved earlier ([`Self::routing_table`]),
    /// so the background job need not go through the four well-known nodes every time.
    pub fn from_nodes(nodes: &[String]) -> Result<Self, DhtError> {
        let dht = mainline::Dht::builder()
            .bootstrap(nodes)
            .build()
            .map_err(|e| DhtError::Unreachable(e.to_string()))?;
        Self::ready(dht)
    }

    /// A node of a local test network (`mainline::Testnet`), bound to localhost.
    pub fn local(bootstrap: &[String]) -> Result<Self, DhtError> {
        let dht = mainline::Dht::builder()
            .bootstrap(bootstrap)
            .bind_address(std::net::Ipv4Addr::LOCALHOST)
            .build()
            .map_err(|e| DhtError::Unreachable(e.to_string()))?;
        Self::ready(dht)
    }

    fn ready(dht: mainline::Dht) -> Result<Self, DhtError> {
        if !dht.bootstrapped() {
            return Err(DhtError::Unreachable(
                "bootstrap failed: no DHT node answered".to_string(),
            ));
        }
        Ok(Self { dht })
    }

    /// The nodes this one knows, to bootstrap from next time.
    pub fn routing_table(&self) -> Vec<String> {
        self.dht.to_bootstrap()
    }
}

impl Dht for MainlineDht {
    fn put(&self, item: &SignedItem) -> Result<(), DhtError> {
        self.dht
            .put_mutable(item.to_mutable(), None)
            .map(|_| ())
            .map_err(|e| DhtError::Refused(e.to_string()))
    }

    fn put_many(&self, items: &[SignedItem]) -> Vec<Result<(), DhtError>> {
        std::thread::scope(|scope| {
            let handles: Vec<_> = items
                .iter()
                .map(|item| scope.spawn(move || self.put(item)))
                .collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .unwrap_or(Err(DhtError::Unreachable("a put thread died".to_string())))
                })
                .collect()
        })
    }

    /// The highest `seq` among the answers that come within [`LATEST_WINDOW`] of the first
    /// one — not of the whole query, which waits for the slowest nodes and took seconds.
    ///
    /// A state item is the only thing asked for this way, and an older one costs nothing: the
    /// pair takes every field of the peer's state only upwards, so a stale answer cannot take
    /// anything back, and the next round sees the newer one.
    fn get_latest(&self, key: &[u8; 32]) -> Result<Option<Found>, DhtError> {
        let (tx, rx) = std::sync::mpsc::channel();
        let dht = self.dht.clone();
        let key = *key;
        // The lookup runs on in its thread after we stop listening; it ends with its query.
        std::thread::spawn(move || {
            for item in dht.get_mutable(&key, None, None) {
                if tx.send(item).is_err() {
                    break;
                }
            }
        });
        // Nothing at all: the query ended without an answer.
        let Ok(mut best) = rx.recv() else {
            return Ok(None);
        };
        let deadline = Instant::now() + LATEST_WINDOW;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match rx.recv_timeout(left) {
                Ok(item) if item.seq() > best.seq() => best = item,
                Ok(_) => {}
                Err(_) => break,
            }
        }
        Ok(Some(Found {
            seq: best.seq(),
            value: best.value().to_vec(),
        }))
    }

    fn get_first(&self, key: &[u8; 32]) -> Result<Option<Found>, DhtError> {
        // The first item stops the lookup: a message part never changes, so the rest of the
        // query could not tell anything new. Dropping the iterator early is safe with
        // `mainline` 8.0.0 (see `crate::probe::get`).
        Ok(self
            .dht
            .get_mutable(key, None, None)
            .next()
            .map(|item| Found {
                seq: item.seq(),
                value: item.value().to_vec(),
            }))
    }

    fn get_first_many(&self, keys: &[[u8; 32]]) -> Vec<Result<Option<Found>, DhtError>> {
        std::thread::scope(|scope| {
            let handles: Vec<_> = keys
                .iter()
                .map(|key| scope.spawn(move || self.get_first(key)))
                .collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join().unwrap_or(Err(DhtError::Unreachable(
                        "a lookup thread died".to_string(),
                    )))
                })
                .collect()
        })
    }
}

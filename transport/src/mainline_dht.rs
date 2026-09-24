//! The real DHT behind [`Dht`]: Mainline, through `mainline` (`docs/transport.md` §10, step 2).
//!
//! `mainline` verifies the signature of every item a node returns, so what comes back from
//! [`MainlineDht::get_first`] is an item signed by the address's key — whose value still has to
//! open with the pair's keys before it means anything (`crate::item::open_value`).

// The blocking API of `mainline` is marked deprecated in favour of the async one. The rounds of
// the transport are sequential and short; the async API comes with the active polling task.
#![allow(deprecated)]

use crate::dht::{Dht, DhtError, Found};
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

    fn get_latest(&self, key: &[u8; 32]) -> Result<Option<Found>, DhtError> {
        Ok(self
            .dht
            .get_mutable_most_recent(key, None)
            .map(|item| Found {
                seq: item.seq(),
                value: item.value().to_vec(),
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

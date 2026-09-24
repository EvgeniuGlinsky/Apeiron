//! What the transport needs from the DHT, and a stand-in for tests.
//!
//! The real one is Mainline DHT through `mainline` (step 2 of `docs/transport.md` §10).
//! [`FakeDht`] behaves like BEP 44 nodes do — it checks signatures and refuses an older `seq` —
//! and can lose, forget and hand out items on demand, so every failure the network can cause is
//! reproducible in a test.

use std::collections::{BTreeMap, HashSet};
use std::sync::Mutex;

use crate::item::SignedItem;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum DhtError {
    #[error("the DHT is unreachable: {0}")]
    Unreachable(String),
    #[error("the put was refused: {0}")]
    Refused(String),
}

/// A value found at an address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub seq: i64,
    pub value: Vec<u8>,
}

pub trait Dht {
    fn put(&self, item: &SignedItem) -> Result<(), DhtError>;

    /// The value with the highest `seq` any node returns. For state items, which change.
    fn get_latest(&self, key: &[u8; 32]) -> Result<Option<Found>, DhtError>;

    /// Any valid value, as soon as one arrives. For message parts, which never change.
    fn get_first(&self, key: &[u8; 32]) -> Result<Option<Found>, DhtError>;

    /// [`Dht::get_first`] for many addresses. The real DHT asks them all at once.
    fn get_first_many(&self, keys: &[[u8; 32]]) -> Vec<Result<Option<Found>, DhtError>> {
        keys.iter().map(|k| self.get_first(k)).collect()
    }
}

/// A DHT in memory, with the failures of the real one on demand.
#[derive(Default)]
pub struct FakeDht {
    inner: Mutex<FakeState>,
}

#[derive(Default)]
struct FakeState {
    items: BTreeMap<[u8; 32], Found>,
    /// Puts to lose, counted down.
    lose_puts: usize,
    /// Addresses whose puts are lost for good.
    black_holes: HashSet<[u8; 32]>,
    /// While set, nothing is reachable.
    offline: bool,
    puts: usize,
    gets: usize,
}

impl FakeDht {
    pub fn new() -> Self {
        Self::default()
    }

    fn state(&self) -> std::sync::MutexGuard<'_, FakeState> {
        // A poisoned lock in a test double means a test already panicked.
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The next `n` puts are accepted and silently lost, as a put that reached no node.
    pub fn lose_next_puts(&self, n: usize) {
        self.state().lose_puts = n;
    }

    /// Every put to `key` is lost from now on.
    pub fn black_hole(&self, key: [u8; 32]) {
        self.state().black_holes.insert(key);
    }

    /// Every stored item expires, as after hours without a re-put.
    pub fn expire_all(&self) {
        self.state().items.clear();
    }

    /// One item expires.
    pub fn expire(&self, key: &[u8; 32]) {
        self.state().items.remove(key);
    }

    pub fn set_offline(&self, offline: bool) {
        self.state().offline = offline;
    }

    /// Puts an item as someone else would: no signature check, whatever `seq`.
    pub fn plant(&self, key: [u8; 32], seq: i64, value: Vec<u8>) {
        self.state().items.insert(key, Found { seq, value });
    }

    pub fn stored(&self, key: &[u8; 32]) -> Option<Found> {
        self.state().items.get(key).cloned()
    }

    pub fn puts(&self) -> usize {
        self.state().puts
    }

    pub fn gets(&self) -> usize {
        self.state().gets
    }
}

impl Dht for FakeDht {
    fn put(&self, item: &SignedItem) -> Result<(), DhtError> {
        let mut s = self.state();
        if s.offline {
            return Err(DhtError::Unreachable("offline".to_string()));
        }
        s.puts = s.puts.saturating_add(1);
        if !item.verifies() {
            return Err(DhtError::Refused("bad signature".to_string()));
        }
        if let Some(old) = s.items.get(&item.key) {
            // BEP 44: a lower seq is refused; an equal seq only with the same value.
            if item.seq < old.seq || (item.seq == old.seq && item.value != old.value) {
                return Err(DhtError::Refused(
                    "seq not higher than the stored one".to_string(),
                ));
            }
        }
        if s.lose_puts > 0 {
            s.lose_puts -= 1;
            return Ok(());
        }
        if s.black_holes.contains(&item.key) {
            return Ok(());
        }
        s.items.insert(
            item.key,
            Found {
                seq: item.seq,
                value: item.value.clone(),
            },
        );
        Ok(())
    }

    fn get_latest(&self, key: &[u8; 32]) -> Result<Option<Found>, DhtError> {
        let mut s = self.state();
        if s.offline {
            return Err(DhtError::Unreachable("offline".to_string()));
        }
        s.gets = s.gets.saturating_add(1);
        Ok(s.items.get(key).cloned())
    }

    fn get_first(&self, key: &[u8; 32]) -> Result<Option<Found>, DhtError> {
        self.get_latest(key)
    }
}

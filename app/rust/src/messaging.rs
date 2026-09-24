//! The service's side of the bridge: storage access over the vault's lock, the DHT node, the
//! clock.
//!
//! Outside `api/` on purpose: flutter_rust_bridge parses everything there, and none of this
//! crosses the boundary.

use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use apeiron_core::Identity;
use apeiron_messenger::{Conversation, MessengerError, StoreAccess};
use apeiron_store::Storage;
use apeiron_transport::mainline_dht::MainlineDht;

use crate::api::vault::with_open;

/// The storage behind the vault's lock. Each access takes the lock for as long as it lasts
/// and not a moment longer: a round's network phase runs outside it, so the interface stays
/// responsive and locking the vault is never kept waiting.
pub(crate) struct BridgeStore;

impl StoreAccess for BridgeStore {
    fn with<R>(
        &self,
        f: impl FnOnce(&Storage) -> Result<R, MessengerError>,
    ) -> Result<R, MessengerError> {
        with_open(|storage, _| f(storage))
            .map_err(|_| MessengerError::Locked)?
            .unwrap_or(Err(MessengerError::Locked))
    }
}

/// A copy of the identity for the length of one call; wiped when dropped.
pub(crate) fn identity() -> Result<Identity, String> {
    with_open(|_, id| Identity::from_secret_bytes(id.export_secret().as_bytes()))?
        .ok_or_else(|| "the vault is locked".to_string())?
        .map_err(|e| e.to_string())
}

/// The conversation with `contact`, loaded for one call.
pub(crate) fn load(me: &Identity, contact: i64) -> Result<Conversation, MessengerError> {
    BridgeStore.with(|s| Conversation::load(s, me, contact))
}

pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

static NETWORK: OnceLock<Mutex<Option<Arc<MainlineDht>>>> = OnceLock::new();

fn network_slot() -> &'static Mutex<Option<Arc<MainlineDht>>> {
    NETWORK.get_or_init(|| Mutex::new(None))
}

/// The DHT node of this unlocked session, joined on first use (a few seconds).
pub(crate) fn network() -> Result<Arc<MainlineDht>, String> {
    let mut slot = network_slot()
        .lock()
        .map_err(|_| "internal lock poisoned: restart the app".to_string())?;
    if let Some(node) = slot.as_ref() {
        return Ok(Arc::clone(node));
    }
    let node = Arc::new(MainlineDht::bootstrap().map_err(|e| e.to_string())?);
    *slot = Some(Arc::clone(&node));
    Ok(node)
}

/// Leaves the DHT: called when the vault locks.
pub(crate) fn drop_network() {
    if let Ok(mut slot) = network_slot().lock() {
        *slot = None;
    }
}

static ROUNDS: Mutex<()> = Mutex::new(());

/// One round at a time: the open chat's timer and the list's must not run the same
/// conversation twice at once (the second would only be refused by the owner check).
pub(crate) fn one_round_at_a_time() -> Result<MutexGuard<'static, ()>, String> {
    ROUNDS
        .lock()
        .map_err(|_| "internal lock poisoned: restart the app".to_string())
}

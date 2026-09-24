//! A test hardware key: only for checks on the development machine.
//!
//! It exists because the whole of `apeiron-store` must be testable without a phone.
//! There is only one on-device check, and spending it on what can be found out here
//! is not acceptable.
//!
//! It gives no real protection and does not pretend to: the key lies in process memory.

// Nor must it ever be built for a device. The `testing` feature is not enabled anywhere in
// the application build, and this spot makes sure it stays that way: the line
// below turns an accidental enablement into a compile error rather than into an APK with
// a software key inside.
#[cfg(all(feature = "testing", target_os = "android"))]
compile_error!(
    "the test key vault must not get into a device build: \
     the `testing` feature is enabled together with target_os = \"android\""
);

use std::path::PathBuf;
use std::sync::Mutex;

use apeiron_core::random_bytes;
use apeiron_platform::{BootClock, HardwareKey, KeyStatus, PlatformError, SecurityLevel};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use zeroize::Zeroizing;

/// What the test key answers with instead of working.
///
/// Needed to check the most valuable property: a transient failure must not
/// turn into "key gone", and must not cost the owner an attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Behaviour {
    /// Work as usual.
    Normal,
    /// Fail transiently: the data is intact, retry.
    Transient,
    /// No key. The only thing that gives the right to start over.
    Gone,
    /// Our own error.
    Internal,
}

/// One call, for tests that check the order of operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Call {
    EnsureKey { allow_create: bool },
    Chain { rounds: u32 },
    Clock,
    Destroy,
}

struct State {
    key: Option<[u8; 32]>,
    level: SecurityLevel,
    behaviour: Behaviour,
    /// Fail the chain call with this index (counting from 0) with this behaviour, once.
    fail_chain_at: Option<(usize, Behaviour)>,
    chain_calls: usize,
    clock: BootClock,
    log: Vec<Call>,
    /// File whose content is recorded at every PIN chain (more than one round), to prove
    /// what was on disk at the moment the hardware was asked.
    watched: Option<PathBuf>,
    snapshots: Vec<Option<Vec<u8>>>,
}

/// The test hardware key.
pub struct TestVault {
    state: Mutex<State>,
}

impl TestVault {
    /// No key yet, as on first launch.
    pub fn empty() -> Self {
        Self::with_level(SecurityLevel::from_raw(SecurityLevel::TRUSTED_ENVIRONMENT))
    }

    /// The same, with a given hardware level — to check the labels and the round floors.
    pub fn with_level(level: SecurityLevel) -> Self {
        Self {
            state: Mutex::new(State {
                key: None,
                level,
                behaviour: Behaviour::Normal,
                fail_chain_at: None,
                chain_calls: 0,
                clock: BootClock {
                    boot_count: 1,
                    elapsed_ms: 1_000,
                },
                log: Vec::new(),
                watched: None,
                snapshots: Vec::new(),
            }),
        }
    }

    fn with_state<T>(&self, f: impl FnOnce(&mut State) -> T) -> Option<T> {
        self.state.lock().ok().map(|mut s| f(&mut s))
    }

    /// How to behave from now on.
    pub fn set_behaviour(&self, behaviour: Behaviour) {
        self.with_state(|s| s.behaviour = behaviour);
    }

    /// Make the chain call number `index` (from now, counting from 0) fail once.
    pub fn fail_chain_call(&self, index: usize, behaviour: Behaviour) {
        self.with_state(|s| {
            s.fail_chain_at = Some((s.chain_calls.saturating_add(index), behaviour));
        });
    }

    /// Forget the key, leaving the wrapper file in place.
    ///
    /// This is what happens in the field regularly: a firmware update, removing the screen
    /// lock, restoring data from a backup without the keys.
    pub fn forget_key(&self) {
        self.with_state(|s| s.key = None);
    }

    /// Replace the key with a different one: what a reissued key looks like.
    pub fn replace_key(&self) {
        if let Ok(k) = random_bytes::<32>() {
            self.with_state(|s| s.key = Some(k));
        }
    }

    /// Whether there is a key now.
    pub fn has_key(&self) -> bool {
        self.with_state(|s| s.key.is_some()).unwrap_or(false)
    }

    /// Move the boot clock forward.
    pub fn advance_ms(&self, ms: i64) {
        self.with_state(|s| s.clock.elapsed_ms = s.clock.elapsed_ms.saturating_add(ms));
    }

    /// Simulate a reboot: a new boot count and a small elapsed time.
    pub fn reboot(&self) {
        self.with_state(|s| {
            s.clock.boot_count = s.clock.boot_count.saturating_add(1);
            s.clock.elapsed_ms = 5_000;
        });
    }

    /// Record the content of `path` at every PIN chain from now on.
    pub fn watch_file(&self, path: PathBuf) {
        self.with_state(|s| s.watched = Some(path));
    }

    /// What the watched file held at each PIN chain, in order (`None` if it did not exist).
    pub fn snapshots(&self) -> Vec<Option<Vec<u8>>> {
        self.with_state(|s| s.snapshots.clone()).unwrap_or_default()
    }

    /// Calls so far, in order.
    pub fn log(&self) -> Vec<Call> {
        self.with_state(|s| s.log.clone()).unwrap_or_default()
    }

    fn fail(behaviour: Behaviour) -> Option<PlatformError> {
        match behaviour {
            Behaviour::Normal => None,
            Behaviour::Transient => Some(PlatformError::Transient("the fake".to_string())),
            Behaviour::Gone => Some(PlatformError::Gone),
            Behaviour::Internal => Some(PlatformError::Internal("the fake".to_string())),
        }
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, State>, PlatformError> {
        self.state
            .lock()
            .map_err(|_| PlatformError::Internal("lock poisoned".to_string()))
    }
}

impl HardwareKey for TestVault {
    fn ensure_key(&self, allow_create: bool) -> Result<KeyStatus, PlatformError> {
        let mut state = self.lock()?;
        state.log.push(Call::EnsureKey { allow_create });
        if let Some(e) = Self::fail(state.behaviour) {
            return Err(e);
        }
        if state.key.is_none() {
            if !allow_create {
                return Err(PlatformError::Gone);
            }
            state.key =
                Some(random_bytes::<32>().map_err(|e| PlatformError::Internal(e.to_string()))?);
            return Ok(KeyStatus {
                level: state.level,
                note: "created by the fake, no real protection".to_string(),
            });
        }
        Ok(KeyStatus {
            level: state.level,
            note: String::new(),
        })
    }

    fn hmac_chain(
        &self,
        input: &[u8; 32],
        rounds: u32,
    ) -> Result<Zeroizing<[u8; 32]>, PlatformError> {
        let mut state = self.lock()?;
        state.log.push(Call::Chain { rounds });
        if rounds > 1 {
            if let Some(path) = state.watched.clone() {
                state.snapshots.push(std::fs::read(path).ok());
            }
        }
        let index = state.chain_calls;
        state.chain_calls = index.saturating_add(1);
        if let Some((at, behaviour)) = state.fail_chain_at {
            if at == index {
                state.fail_chain_at = None;
                if let Some(e) = Self::fail(behaviour) {
                    return Err(e);
                }
            }
        }
        if let Some(e) = Self::fail(state.behaviour) {
            return Err(e);
        }
        let key = state.key.ok_or(PlatformError::Gone)?;
        let mut x = Zeroizing::new(*input);
        for _ in 0..rounds {
            let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(&key)
                .map_err(|e| PlatformError::Internal(e.to_string()))?;
            mac.update(x.as_ref());
            *x = mac.finalize().into_bytes().into();
        }
        Ok(x)
    }

    fn boot_clock(&self) -> Result<BootClock, PlatformError> {
        let mut state = self.lock()?;
        state.log.push(Call::Clock);
        Ok(state.clock)
    }

    fn destroy(&self) -> Result<(), PlatformError> {
        let mut state = self.lock()?;
        state.log.push(Call::Destroy);
        state.key = None;
        Ok(())
    }

    fn diagnostics(&self) -> Result<String, PlatformError> {
        let state = self.lock()?;
        Ok(format!(
            "FAKE key store, no real protection\n\
             key in memory: {}\n\
             level: {} ({})\n",
            if state.key.is_some() { "yes" } else { "no" },
            state.level.name(),
            state.level.raw()
        ))
    }
}

//! A test hardware vault: only for checks on the development machine.
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

use std::sync::Mutex;

use apeiron_core::{open, seal, SecretKey};
use apeiron_platform::{KeyStatus, KeyWrapper, PlatformError, SecurityLevel};
use zeroize::Zeroizing;

/// What the test vault answers with instead of working.
///
/// Needed to check the most valuable property: a transient failure must not
/// turn into "key gone".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Behaviour {
    /// Work as usual.
    Normal,
    /// Fail transiently: the data is intact, retry.
    Transient,
    /// The key is gone. The only thing that gives the right to start over.
    Gone,
    /// Its own error.
    Internal,
}

struct State {
    key: Option<SecretKey>,
    level: SecurityLevel,
    behaviour: Behaviour,
}

/// The test key vault.
pub struct TestVault {
    state: Mutex<State>,
}

impl TestVault {
    /// An empty vault: no key yet, as on first launch.
    pub fn empty() -> Self {
        Self::with_level(SecurityLevel::from_raw(SecurityLevel::STRONGBOX))
    }

    /// The same, but with a given hardware level, to check the labels.
    pub fn with_level(level: SecurityLevel) -> Self {
        Self {
            state: Mutex::new(State {
                key: None,
                level,
                behaviour: Behaviour::Normal,
            }),
        }
    }

    /// How to behave from now on.
    pub fn set_behaviour(&self, behaviour: Behaviour) {
        if let Ok(mut state) = self.state.lock() {
            state.behaviour = behaviour;
        }
    }

    /// Forget the key, leaving the wrapper file in place.
    ///
    /// This is what regularly happens in the field: a firmware update,
    /// removal of the screen lock, restoring data from a backup without the keys.
    pub fn forget_key(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.key = None;
        }
    }

    /// Whether there is a key right now.
    pub fn has_key(&self) -> bool {
        self.state.lock().map(|s| s.key.is_some()).unwrap_or(false)
    }

    fn fail(behaviour: Behaviour) -> Option<PlatformError> {
        match behaviour {
            Behaviour::Normal => None,
            Behaviour::Transient => Some(PlatformError::Transient("the fake".to_string())),
            Behaviour::Gone => Some(PlatformError::Gone),
            Behaviour::Internal => Some(PlatformError::Internal("the fake".to_string())),
        }
    }
}

/// The domain in which the test vault seals. Separate, so that these
/// bytes cannot be confused with database records.
const TEST_AAD: &[u8] = b"apeiron/testing/vault/v1";

impl KeyWrapper for TestVault {
    fn ensure_key(&self, allow_create: bool) -> Result<KeyStatus, PlatformError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| PlatformError::Internal("lock is poisoned".to_string()))?;
        if let Some(e) = Self::fail(state.behaviour) {
            return Err(e);
        }
        if state.key.is_none() {
            if !allow_create {
                return Err(PlatformError::Gone);
            }
            state.key =
                Some(SecretKey::generate().map_err(|e| PlatformError::Internal(e.to_string()))?);
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

    fn wrap(&self, plain: &[u8]) -> Result<Vec<u8>, PlatformError> {
        // The same limit as on the device: tens of bytes go through the KEK.
        if plain.is_empty() || plain.len() > 64 {
            return Err(PlatformError::Internal(
                "no more than 64 bytes go through the KEK".to_string(),
            ));
        }
        let state = self
            .state
            .lock()
            .map_err(|_| PlatformError::Internal("lock is poisoned".to_string()))?;
        if let Some(e) = Self::fail(state.behaviour) {
            return Err(e);
        }
        let key = state.key.as_ref().ok_or(PlatformError::Gone)?;
        seal(key, TEST_AAD, plain).map_err(|e| PlatformError::Internal(e.to_string()))
    }

    fn unwrap(&self, blob: &[u8]) -> Result<Zeroizing<Vec<u8>>, PlatformError> {
        let state = self
            .state
            .lock()
            .map_err(|_| PlatformError::Internal("lock is poisoned".to_string()))?;
        if let Some(e) = Self::fail(state.behaviour) {
            return Err(e);
        }
        let key = state.key.as_ref().ok_or(PlatformError::Gone)?;
        // A damaged wrapper is NOT "key gone": the data is in place, it is just that
        // this file cannot be trusted. The same distinction as on the device.
        open(key, TEST_AAD, blob)
            .map(Zeroizing::new)
            .map_err(|e| PlatformError::Transient(e.to_string()))
    }

    fn destroy(&self) -> Result<(), PlatformError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| PlatformError::Internal("lock is poisoned".to_string()))?;
        state.key = None;
        Ok(())
    }

    fn diagnostics(&self) -> Result<String, PlatformError> {
        let state = self
            .state
            .lock()
            .map_err(|_| PlatformError::Internal("lock is poisoned".to_string()))?;
        Ok(format!(
            "TEST key vault, no real protection\n\
             key in memory: {}\n\
             level: {} ({})\n",
            if state.key.is_some() {
                "present"
            } else {
                "absent"
            },
            state.level.name(),
            state.level.raw()
        ))
    }
}

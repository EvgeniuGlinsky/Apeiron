//! Platforms without a hardware key store.
//!
//! Lives outside `api/` deliberately: everything there is parsed by
//! flutter_rust_bridge, and this type has no reason to cross the FFI boundary.

use apeiron_platform::{BootClock, HardwareKey, KeyStatus, PlatformError};

/// Stub for platforms without a hardware store.
///
/// Desktop is frozen by the project owner's decision, and pretending there is
/// protection here is not allowed: the app will honestly say there is no store
/// instead of silently putting the key in a file next to the database.
#[derive(Default)]
pub struct NoVault;

fn unavailable() -> PlatformError {
    PlatformError::Internal(
        "this platform has no hardware key store; the build for it is frozen".to_string(),
    )
}

impl HardwareKey for NoVault {
    fn ensure_key(&self, _allow_create: bool) -> Result<KeyStatus, PlatformError> {
        Err(unavailable())
    }
    fn hmac_chain(
        &self,
        _input: &[u8; 32],
        _rounds: u32,
    ) -> Result<zeroize::Zeroizing<[u8; 32]>, PlatformError> {
        Err(unavailable())
    }
    fn boot_clock(&self) -> Result<BootClock, PlatformError> {
        Err(unavailable())
    }
    fn destroy(&self) -> Result<(), PlatformError> {
        Err(unavailable())
    }
    fn diagnostics(&self) -> Result<String, PlatformError> {
        Ok("platform: not Android, no hardware key store\n".to_string())
    }
}

//! Platforms without a hardware key store.
//!
//! Lives outside `api/` deliberately: everything there is parsed by
//! flutter_rust_bridge, and this type has no reason to cross the FFI boundary.

use apeiron_platform::{KeyStatus, KeyWrapper, PlatformError};

/// Stub for platforms without a hardware store.
///
/// Desktop is frozen by the project owner's decision, and pretending there is
/// protection here is not allowed: the app will honestly say there is no store
/// instead of silently putting the key in a file next to the database.
#[derive(Default)]
pub struct NoVault;

fn unavailable() -> PlatformError {
    PlatformError::Internal(
        "на этой платформе аппаратного хранилища ключей нет; сборка для неё заморожена".to_string(),
    )
}

impl KeyWrapper for NoVault {
    fn ensure_key(&self, _allow_create: bool) -> Result<KeyStatus, PlatformError> {
        Err(unavailable())
    }
    fn wrap(&self, _plain: &[u8]) -> Result<Vec<u8>, PlatformError> {
        Err(unavailable())
    }
    fn unwrap(&self, _blob: &[u8]) -> Result<zeroize::Zeroizing<Vec<u8>>, PlatformError> {
        Err(unavailable())
    }
    fn destroy(&self) -> Result<(), PlatformError> {
        Err(unavailable())
    }
    fn diagnostics(&self) -> Result<String, PlatformError> {
        Ok("платформа: не Android, аппаратного хранилища ключей нет\n".to_string())
    }
}

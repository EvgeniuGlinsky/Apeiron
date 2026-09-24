//! Платформы без аппаратного хранилища ключей.
//!
//! Живёт вне `api/` намеренно: всё, что лежит там, разбирает
//! flutter_rust_bridge, а этому типу через границу FFI ходить незачем.

use apeiron_platform::{KeyWrapper, PlatformError, SecurityLevel};

/// Заглушка для платформ без аппаратного хранилища.
///
/// Десктоп заморожен решением заказчика, и делать вид, что здесь есть защита,
/// нельзя: приложение честно скажет, что хранилища нет, вместо того чтобы молча
/// положить ключ в файл рядом с базой.
#[derive(Default)]
pub struct NoVault;

fn unavailable() -> PlatformError {
    PlatformError::Internal(
        "на этой платформе аппаратного хранилища ключей нет; сборка для неё заморожена".to_string(),
    )
}

impl KeyWrapper for NoVault {
    fn ensure_key(&self, _allow_create: bool) -> Result<SecurityLevel, PlatformError> {
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

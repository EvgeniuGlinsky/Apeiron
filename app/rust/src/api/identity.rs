//! Мост к личности устройства.
//!
//! Через эту границу проходит только публичное. Секретные ключи остаются в Rust
//! — решение R-004 в `docs/threat-log.md`.
//!
//! Причина не в аккуратности, а в том, что в Dart затирание памяти невозможно:
//! сборщик мусора копирует объекты при уплотнении кучи и не даёт никаких
//! гарантий, что прежняя копия строки затёрта. Всё, что попало в Dart, следует
//! считать оставшимся в памяти до конца жизни процесса.
//!
//! Личность больше не живёт в отдельном статике: она лежит в открытом
//! хранилище (`super::vault`) и переживает перезапуск приложения. До появления
//! хранилища каждый запуск порождал новую личность — то есть новый отпечаток у
//! человека, который уже прочитал прежний вслух при сверке.

use apeiron_core::{Identity, PublicIdentity};

use super::vault;

/// То, что разрешено показывать. Секретов не содержит.
pub struct PublicIdentityView {
    /// Тридцать цифр шестью группами — то, что читают вслух при сверке.
    pub fingerprint: String,
    /// Публичный ключ подписи Ed25519, hex.
    pub signing_key_hex: String,
    /// Публичный ключ согласования X25519, hex.
    pub agreement_key_hex: String,
}

impl From<&PublicIdentity> for PublicIdentityView {
    fn from(p: &PublicIdentity) -> Self {
        Self {
            fingerprint: p.fingerprint(),
            signing_key_hex: hex::encode(p.verifying_key().as_bytes()),
            agreement_key_hex: hex::encode(p.agreement_key().as_bytes()),
        }
    }
}

const LOCKED: &str = "личность заблокирована";

fn view(identity: &Identity) -> PublicIdentityView {
    PublicIdentityView::from(&identity.public())
}

/// Создаёт новую личность и сохраняет её.
///
/// Вместе с ней заводятся аккаунт устройства и журнал личности: порознь они
/// бессмысленны. Операция необратима — смена личности рвёт все существующие
/// переписки, — и хранилище к этому моменту обязано быть открыто.
#[flutter_rust_bridge::frb]
pub fn generate_identity() -> Result<PublicIdentityView, String> {
    vault::create_identity()?;
    current_identity()?.ok_or_else(|| "личность не сохранилась".to_string())
}

/// Текущая личность, если хранилище открыто.
#[flutter_rust_bridge::frb]
pub fn current_identity() -> Result<Option<PublicIdentityView>, String> {
    vault::with_identity(view)
}

/// Блокировка: запирает хранилище и затирает ключи.
///
/// Вызывается при уходе приложения в фон и при гашении экрана — решение R-001.
#[flutter_rust_bridge::frb]
pub fn lock_identity() -> Result<(), String> {
    vault::lock_vault()
}

/// Число сверки с собеседником по его публичной личности.
///
/// Обе стороны получают одну и ту же строку. Расхождение означает, что между
/// вами кто-то есть, и переписку начинать нельзя.
#[flutter_rust_bridge::frb]
pub fn safety_number_with(peer_public_hex: String) -> Result<String, String> {
    let bytes = hex::decode(peer_public_hex.trim())
        .map_err(|_| "ключ собеседника не является шестнадцатеричной строкой".to_string())?;
    let peer = PublicIdentity::from_bytes(&bytes).map_err(|e| e.to_string())?;
    vault::with_identity(|me| me.public().safety_number(&peer))?.ok_or_else(|| LOCKED.to_string())
}

/// Публичная личность целиком, hex — то, что кодируется в QR при добавлении
/// контакта.
#[flutter_rust_bridge::frb]
pub fn public_identity_hex() -> Result<String, String> {
    vault::with_identity(|me| hex::encode(me.public().to_bytes()))?
        .ok_or_else(|| LOCKED.to_string())
}

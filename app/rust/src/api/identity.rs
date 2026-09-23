//! Мост к личности устройства.
//!
//! Через эту границу проходит только публичное. Секретные ключи остаются в Rust
//! и живут за мьютексом; в Dart уходят отпечаток и публичные ключи — решение R-004
//! в `docs/threat-log.md`.
//!
//! Причина не в аккуратности, а в том, что в Dart затирание памяти невозможно:
//! сборщик мусора копирует объекты при уплотнении кучи и не даёт никаких гарантий,
//! что прежняя копия строки затёрта. Всё, что попало в Dart, следует считать
//! оставшимся в памяти до конца жизни процесса.

use std::sync::{Mutex, OnceLock};

use apeiron_core::{Identity, PublicIdentity};

/// Хранилище личности на время работы процесса.
///
/// `None` означает «заблокировано»: `Identity` уничтожен, а вместе с ним затёрты
/// и ключи. Постоянное хранение появится на этапе 2 вместе с аппаратным ключом.
static IDENTITY: OnceLock<Mutex<Option<Identity>>> = OnceLock::new();

fn store() -> &'static Mutex<Option<Identity>> {
    IDENTITY.get_or_init(|| Mutex::new(None))
}

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

const POISONED: &str = "внутренняя блокировка повреждена: перезапустите приложение";
const LOCKED: &str = "личность заблокирована";

/// Создаёт новую личность, заменяя текущую.
///
/// Прежняя уничтожается, её ключи затираются. На этапе 2 это станет необратимой
/// операцией с подтверждением: смена личности рвёт все существующие переписки.
#[flutter_rust_bridge::frb]
pub fn generate_identity() -> Result<PublicIdentityView, String> {
    let identity = Identity::generate();
    let view = PublicIdentityView::from(&identity.public());
    let mut guard = store().lock().map_err(|_| POISONED.to_string())?;
    *guard = Some(identity);
    Ok(view)
}

/// Текущая личность, если она разблокирована.
#[flutter_rust_bridge::frb]
pub fn current_identity() -> Result<Option<PublicIdentityView>, String> {
    let guard = store().lock().map_err(|_| POISONED.to_string())?;
    Ok(guard
        .as_ref()
        .map(|id| PublicIdentityView::from(&id.public())))
}

/// Блокировка: уничтожает `Identity` и затирает ключи.
///
/// Вызывается при уходе приложения в фон и при гашении экрана — решение R-001.
#[flutter_rust_bridge::frb]
pub fn lock_identity() -> Result<(), String> {
    let mut guard = store().lock().map_err(|_| POISONED.to_string())?;
    *guard = None;
    Ok(())
}

/// Число сверки с собеседником по его публичной личности.
///
/// Обе стороны получают одну и ту же строку. Расхождение означает, что между вами
/// кто-то есть, и переписку начинать нельзя.
#[flutter_rust_bridge::frb]
pub fn safety_number_with(peer_public_hex: String) -> Result<String, String> {
    let bytes = hex::decode(peer_public_hex.trim())
        .map_err(|_| "ключ собеседника не является шестнадцатеричной строкой".to_string())?;
    let peer = PublicIdentity::from_bytes(&bytes).map_err(|e| e.to_string())?;
    let guard = store().lock().map_err(|_| POISONED.to_string())?;
    let me = guard.as_ref().ok_or_else(|| LOCKED.to_string())?;
    Ok(me.public().safety_number(&peer))
}

/// Публичная личность целиком, hex — то, что кодируется в QR при добавлении контакта.
#[flutter_rust_bridge::frb]
pub fn public_identity_hex() -> Result<String, String> {
    let guard = store().lock().map_err(|_| POISONED.to_string())?;
    let me = guard.as_ref().ok_or_else(|| LOCKED.to_string())?;
    Ok(hex::encode(me.public().to_bytes()))
}

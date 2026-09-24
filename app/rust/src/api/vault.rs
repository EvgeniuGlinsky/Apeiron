//! Bridge to the vault and the hardware key.
//!
//! Only public and only descriptive data crosses this boundary. The database key
//! and the master key never go out: they live in Rust, and reach it from
//! Kotlin directly over JNI, bypassing Dart (R-004).
//!
//! # Why a refusal is a state, not an error
//!
//! An error from Rust arrives in Dart as a bare string and is printed in a red
//! banner. For "StrongBox unavailable" and "key gone" this is an unfit channel:
//! the first is not a failure at all, and the second is a situation in which a
//! person has to make a decision, not read an error message. So [`unlock_vault`]
//! does not return `Result`: it returns [`VaultStatus`], where the state is named
//! directly.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use apeiron_core::{Identity, Sigchain};
use apeiron_platform::{KeyWrapper, SecurityLevel};
use apeiron_store::{selfcheck, Storage, StorageError};

/// The hardware store of this platform.
#[cfg(target_os = "android")]
type Vault = apeiron_platform::AndroidVault;

/// On anything that is not Android, there is no hardware store.
#[cfg(not(target_os = "android"))]
type Vault = crate::desktop::NoVault;

/// The app's working state between unlocks.
///
/// `None` means "locked": `Storage` is destroyed, and with it the database key
/// and all subkeys are wiped. Unlocking unwraps the key again — and on a locked
/// phone the hardware will not do that, which is exactly what R-001 requires.
struct Session {
    storage: Storage,
    identity: Option<Identity>,
}

static STATE: OnceLock<Mutex<Option<Session>>> = OnceLock::new();

fn state() -> &'static Mutex<Option<Session>> {
    STATE.get_or_init(|| Mutex::new(None))
}

const POISONED: &str = "внутренняя блокировка повреждена: перезапустите приложение";
const LOCKED: &str = "хранилище заперто";

/// What state the vault is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultState {
    /// Open and ready.
    Opened,
    /// Locked: the database key is wiped, an unlock is needed.
    Locked,
    /// The key is gone from the secure module. Nothing can decrypt.
    KeyGone,
    /// Transient failure. The data is intact, retry.
    Retry,
    /// There is no hardware store on this platform.
    Unavailable,
}

/// What to show about the vault.
pub struct VaultStatus {
    pub state: VaultState,
    /// Explanation for a person. Empty string if there is nothing to explain.
    pub message: String,
    /// Level name: StrongBox, TEE, software, hardware without specifics.
    pub level_name: String,
    /// The raw number from `KeyInfo.getSecurityLevel()`. Shown alongside so
    /// that an unfamiliar value is visible, not replaced by the nearest familiar one.
    pub level_raw: i32,
    /// Whether the key is in hardware, as reported by the system.
    pub hardware_backed: bool,
    /// Whether this is the first run with this vault.
    pub first_run: bool,
    /// Whether the vault holds an identity.
    pub has_identity: bool,
}

impl VaultStatus {
    fn without_vault(state: VaultState, message: impl Into<String>) -> Self {
        Self {
            state,
            message: message.into(),
            level_name: "неизвестно".to_string(),
            level_raw: SecurityLevel::UNKNOWN,
            hardware_backed: false,
            first_run: false,
            has_identity: false,
        }
    }

    fn from_session(session: &Session) -> Self {
        let level = session.storage.security_level();
        Self {
            state: VaultState::Opened,
            message: String::new(),
            level_name: level.name().to_string(),
            level_raw: level.raw(),
            hardware_backed: level.is_hardware(),
            first_run: session.storage.created_now(),
            has_identity: session.identity.is_some(),
        }
    }
}

/// One line of the self-check report.
pub struct CheckLine {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

/// Opens the vault and loads the identity.
///
/// There is deliberately no `Result` here: a hardware refusal is a situation to
/// be explained, not a red banner with a Java class name.
#[flutter_rust_bridge::frb]
pub fn unlock_vault() -> VaultStatus {
    let mut guard = match state().lock() {
        Ok(g) => g,
        Err(_) => return VaultStatus::without_vault(VaultState::Retry, POISONED),
    };
    if let Some(session) = guard.as_ref() {
        return VaultStatus::from_session(session);
    }

    let dir = match storage_dir() {
        Ok(d) => d,
        Err(e) => return VaultStatus::without_vault(VaultState::Unavailable, e),
    };

    match Storage::open(&dir, &Vault::default()) {
        Ok(storage) => {
            let identity = match storage.load_identity() {
                Ok(id) => id,
                // The vault opened, but the identity cannot be read. This is not
                // "key gone": the key is in place, one record is corrupted.
                Err(e) => return VaultStatus::without_vault(VaultState::Retry, e.to_string()),
            };
            let session = Session { storage, identity };
            let status = VaultStatus::from_session(&session);
            *guard = Some(session);
            status
        }
        Err(StorageError::KeyGone) => {
            VaultStatus::without_vault(VaultState::KeyGone, StorageError::KeyGone.to_string())
        }
        Err(e) => VaultStatus::without_vault(VaultState::Retry, e.to_string()),
    }
}

/// The current state, without an attempt to open.
#[flutter_rust_bridge::frb]
pub fn vault_status() -> VaultStatus {
    match state().lock() {
        Ok(guard) => match guard.as_ref() {
            Some(session) => VaultStatus::from_session(session),
            None => VaultStatus::without_vault(VaultState::Locked, LOCKED),
        },
        Err(_) => VaultStatus::without_vault(VaultState::Retry, POISONED),
    }
}

/// Locks: destroys the database key and everything derived from it.
///
/// Called when the app goes to the background and when the screen turns off —
/// decision R-001. Unlocking will require the hardware key again, and on a
/// locked phone the hardware will refuse to use it.
#[flutter_rust_bridge::frb]
pub fn lock_vault() -> Result<(), String> {
    let mut guard = state().lock().map_err(|_| POISONED.to_string())?;
    *guard = None;
    Ok(())
}

/// Erases everything cryptographically (R-005): destroys the key, not the data.
///
/// There is nothing to demand, because there is nothing to decrypt with — even if a
/// copy of the database was already taken. The action is irreversible and is
/// invoked only deliberately.
#[flutter_rust_bridge::frb]
pub fn wipe_everything() -> Result<(), String> {
    let mut guard = state().lock().map_err(|_| POISONED.to_string())?;
    *guard = None;
    let dir = storage_dir()?;
    Storage::wipe(&dir, &Vault::default()).map_err(|e| e.to_string())
}

/// Runs the self-check on this device.
#[flutter_rust_bridge::frb]
pub fn self_check() -> Result<Vec<CheckLine>, String> {
    let guard = state().lock().map_err(|_| POISONED.to_string())?;
    let session = guard.as_ref().ok_or_else(|| LOCKED.to_string())?;
    Ok(selfcheck::run(&session.storage)
        .into_iter()
        .map(|c| CheckLine {
            name: c.name,
            passed: c.passed,
            detail: c.detail,
        })
        .collect())
}

/// Platform diagnostics, one line per fact.
///
/// Needed because there is only one on-device check: an install must answer
/// all questions at once, not just the one someone thought to ask. The report
/// is shown as is and forwarded in full. Contains no secrets.
#[flutter_rust_bridge::frb]
pub fn platform_diagnostics() -> Result<String, String> {
    let mut report = String::new();
    report.push_str(&format!("SQLite: {}\n", apeiron_store::sqlite_version()));
    match storage_dir() {
        Ok(dir) => report.push_str(&format!("каталог данных: {}\n", dir.display())),
        Err(e) => report.push_str(&format!("каталог данных: {e}\n")),
    }
    match Vault::default().diagnostics() {
        Ok(text) => report.push_str(&text),
        Err(e) => report.push_str(&format!("хранилище ключей: {e}\n")),
    }
    if let Ok(guard) = state().lock() {
        match guard.as_ref() {
            Some(session) => {
                let created = session.storage.level_at_creation();
                report.push_str(&format!(
                    "уровень при создании обёртки: {} ({})\n",
                    created.name(),
                    created.raw()
                ));
                match session.storage.meta_get(apeiron_store::META_KEY_ORIGIN) {
                    Ok(Some(note)) => report.push_str(&format!(
                        "как появился ключ: {}\n",
                        String::from_utf8_lossy(&note)
                    )),
                    // Empty means the vault was created by a build that did not
                    // record this yet. Staying silent is not allowed: otherwise the
                    // missing line would be read as "nothing special happened".
                    Ok(None) => {
                        report.push_str("как появился ключ: не записано (создан прежней сборкой)\n")
                    }
                    Err(e) => report.push_str(&format!("как появился ключ: {e}\n")),
                }
                report.push_str(&format!(
                    "личность в хранилище: {}\n",
                    if session.identity.is_some() {
                        "есть"
                    } else {
                        "нет"
                    }
                ));
            }
            None => report.push_str("хранилище: заперто\n"),
        }
    }
    Ok(report)
}

/// The app's data directory.
#[cfg(target_os = "android")]
fn storage_dir() -> Result<PathBuf, String> {
    apeiron_platform::storage_dir()
        .map(|p| p.to_path_buf())
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "android"))]
fn storage_dir() -> Result<PathBuf, String> {
    Err("на этой платформе каталог данных не определён: сборка заморожена".to_string())
}

// ── Internal, for api::identity ─────────────────────────────────────────────

/// Creates the identity, the device account and the sigchain — and saves them
/// all at once.
///
/// At once, because apart they are meaningless: an identity without an account
/// cannot exchange messages, an account without a sigchain cannot be revoked,
/// and a sigchain without an identity has nothing to start from.
pub(crate) fn create_identity() -> Result<(), String> {
    let mut guard = state().lock().map_err(|_| POISONED.to_string())?;
    let session = guard.as_mut().ok_or_else(|| LOCKED.to_string())?;

    let identity = Identity::generate().map_err(|e| e.to_string())?;
    let account = apeiron_core::vodozemac::olm::Account::new();
    let chain = Sigchain::create(&identity).map_err(|e| e.to_string())?;

    session
        .storage
        .save_identity(&identity)
        .map_err(|e| e.to_string())?;
    session
        .storage
        .save_account(&account)
        .map_err(|e| e.to_string())?;
    session
        .storage
        .save_sigchain(&chain)
        .map_err(|e| e.to_string())?;

    session.identity = Some(identity);
    Ok(())
}

/// Reads something public from the current identity.
///
/// The closure's result is handed out, not the identity itself: this way a secret
/// cannot leave this module through carelessness.
pub(crate) fn with_identity<T>(f: impl FnOnce(&Identity) -> T) -> Result<Option<T>, String> {
    let guard = state().lock().map_err(|_| POISONED.to_string())?;
    Ok(guard.as_ref().and_then(|s| s.identity.as_ref()).map(f))
}

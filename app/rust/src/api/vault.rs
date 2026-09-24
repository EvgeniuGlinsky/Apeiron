//! Bridge to the vault, the PIN and the hardware key.
//!
//! Only public and only descriptive data crosses this boundary. The database key, the
//! hardware chain's output and the PIN never go out: the key material lives in Rust and
//! reaches it from Kotlin directly over JNI, and the PIN is assembled in Rust from tapped
//! positions of a layout Rust itself drew (R-004, R-007).
//!
//! # Why a refusal is a state, not an error
//!
//! An error from Rust arrives in Dart as a bare string and is printed in a red banner.
//! For "key gone", "wrong PIN" or "wait five minutes" that is an unfit channel: each is a
//! situation in which a person has to do something, not read an error message. So the
//! functions here return [`VaultStatus`], where the state is named directly.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use apeiron_core::{random_bytes, Identity, Sigchain};
use apeiron_platform::{HardwareKey, SecurityLevel};
use apeiron_store::pin::PinState;
use apeiron_store::wrapper::{self, presence};
use apeiron_store::{selfcheck, KdfParams, Opening, Presence, Storage, StorageError};

use crate::paths::storage_dir;
use crate::pin_entry;

/// The hardware store of this platform.
#[cfg(target_os = "android")]
type Vault = apeiron_platform::AndroidVault;

/// On anything that is not Android, there is no hardware store.
#[cfg(not(target_os = "android"))]
type Vault = crate::desktop::NoVault;

/// The app's working state between unlocks.
///
/// `None` means "locked": `Storage` is destroyed, and with it the database key and all
/// subkeys are wiped. Unlocking needs the PIN and the hardware again (R-001).
struct Session {
    storage: Storage,
    identity: Option<Identity>,
}

static STATE: OnceLock<Mutex<Option<Session>>> = OnceLock::new();

fn state() -> &'static Mutex<Option<Session>> {
    STATE.get_or_init(|| Mutex::new(None))
}

/// Random mark of this process, written next to the attempt counter.
static WRITER: OnceLock<[u8; 8]> = OnceLock::new();

fn writer() -> [u8; 8] {
    *WRITER.get_or_init(|| random_bytes::<8>().unwrap_or([0; 8]))
}

/// Wrong PINs seen by this process. If the counter on disk holds more, it survived a
/// restart — the self-check needs exactly that distinction.
static FAILURES_HERE: AtomicU32 = AtomicU32::new(0);

const POISONED: &str = "internal lock poisoned: restart the app";
const LOCKED: &str = "the vault is locked";

/// What state the vault is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultState {
    /// Open and ready.
    Opened,
    /// Locked: the database key is wiped, the PIN is needed.
    Locked,
    /// No vault yet: a PIN has to be set.
    PinSetupRequired,
    /// The two entries of a new PIN differ; setting starts over.
    PinMismatch,
    /// The PIN was wrong. See `failures` and `wait_seconds`.
    WrongPin,
    /// Too many failures: wait `wait_seconds`. Nothing was tried.
    Delayed,
    /// Data of a test build before the PIN. Cannot be opened; the owner decides to reset.
    LegacyData,
    /// The key is gone from the secure module. Nothing can decrypt.
    KeyGone,
    /// The secure module holds a different key. Nothing can decrypt.
    KeyMismatch,
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
    /// The raw number from `KeyInfo.getSecurityLevel()`. Shown alongside so that an
    /// unfamiliar value is visible, not replaced by the nearest familiar one.
    pub level_raw: i32,
    /// Whether the key is in hardware, as reported by the system.
    pub hardware_backed: bool,
    /// Whether the vault was created just now.
    pub first_run: bool,
    /// Whether the vault holds an identity.
    pub has_identity: bool,
    /// Wrong PINs in a row.
    pub failures: u32,
    /// Seconds to wait before the next attempt.
    pub wait_seconds: u32,
    /// How long the last unlock took (Argon2id plus the hardware chain), in milliseconds.
    pub unlock_ms: u32,
}

impl VaultStatus {
    fn bare(state: VaultState, message: impl Into<String>) -> Self {
        Self {
            state,
            message: message.into(),
            level_name: SecurityLevel::from_raw(SecurityLevel::UNKNOWN)
                .name()
                .to_string(),
            level_raw: SecurityLevel::UNKNOWN,
            hardware_backed: false,
            first_run: false,
            has_identity: false,
            failures: 0,
            wait_seconds: 0,
            unlock_ms: 0,
        }
    }

    fn waiting(state: VaultState, failures: u32, wait_ms: u64) -> Self {
        Self {
            failures,
            wait_seconds: u32::try_from(wait_ms.div_ceil(1_000)).unwrap_or(u32::MAX),
            ..Self::bare(state, "")
        }
    }

    fn from_session(session: &Session) -> Self {
        let level = session.storage.security_level();
        let t = session.storage.timing();
        Self {
            state: VaultState::Opened,
            message: String::new(),
            level_name: level.name().to_string(),
            level_raw: level.raw(),
            hardware_backed: level.is_hardware(),
            first_run: session.storage.created_now(),
            has_identity: session.identity.is_some(),
            failures: 0,
            wait_seconds: 0,
            unlock_ms: u32::try_from(t.argon_ms.saturating_add(t.chain_ms)).unwrap_or(u32::MAX),
        }
    }

    fn from_error(e: StorageError) -> Self {
        let state = match &e {
            StorageError::KeyGone => VaultState::KeyGone,
            StorageError::KeyMismatch => VaultState::KeyMismatch,
            StorageError::Legacy => VaultState::LegacyData,
            StorageError::NoVault => VaultState::PinSetupRequired,
            _ => VaultState::Retry,
        };
        Self::bare(state, e.to_string())
    }
}

/// One line of the self-check report.
pub struct CheckLine {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

/// The current state, without an attempt to open anything.
///
/// For a locked vault this also says whether a delay is running, so that the screen can
/// show a countdown instead of a pad that would only answer "wait".
#[flutter_rust_bridge::frb]
pub fn vault_status() -> VaultStatus {
    let guard = match state().lock() {
        Ok(g) => g,
        Err(_) => return VaultStatus::bare(VaultState::Retry, POISONED),
    };
    if let Some(session) = guard.as_ref() {
        return VaultStatus::from_session(session);
    }
    let dir = match storage_dir() {
        Ok(d) => d,
        Err(e) => return VaultStatus::bare(VaultState::Unavailable, e),
    };
    match presence(&dir) {
        Ok(Presence::Absent) => VaultStatus::bare(VaultState::PinSetupRequired, ""),
        Ok(Presence::Legacy) => VaultStatus::from_error(StorageError::Legacy),
        Ok(Presence::Pin) => {
            let pin_state = PinState::load(&dir);
            let wait_ms = match Vault::default().boot_clock() {
                Ok(now) => match pin_state.gate(now) {
                    apeiron_store::pin::Gate::Open => 0,
                    apeiron_store::pin::Gate::Wait { remaining_ms, .. } => remaining_ms,
                },
                Err(_) => 0,
            };
            if wait_ms > 0 {
                VaultStatus::waiting(VaultState::Delayed, pin_state.consecutive, wait_ms)
            } else {
                VaultStatus::waiting(VaultState::Locked, pin_state.consecutive, 0)
            }
        }
        Err(e) => VaultStatus::from_error(e),
    }
}

/// Tries the PIN typed on the pad.
#[flutter_rust_bridge::frb]
pub fn unlock_with_pin() -> VaultStatus {
    let pin = match pin_entry::take() {
        Ok(p) => p,
        Err(e) => return VaultStatus::bare(VaultState::Retry, e),
    };
    let mut guard = match state().lock() {
        Ok(g) => g,
        Err(_) => return VaultStatus::bare(VaultState::Retry, POISONED),
    };
    if let Some(session) = guard.as_ref() {
        return VaultStatus::from_session(session);
    }
    let dir = match storage_dir() {
        Ok(d) => d,
        Err(e) => return VaultStatus::bare(VaultState::Unavailable, e),
    };
    match Storage::unlock(&dir, &Vault::default(), &pin, writer()) {
        Ok(Opening::Opened(storage)) => open_session(&mut guard, *storage),
        Ok(Opening::WrongPin { failures, wait_ms }) => {
            FAILURES_HERE.fetch_add(1, Ordering::Relaxed);
            VaultStatus::waiting(VaultState::WrongPin, failures, wait_ms)
        }
        Ok(Opening::Delayed { wait_ms }) => {
            let failures = PinState::load(&dir).consecutive;
            VaultStatus::waiting(VaultState::Delayed, failures, wait_ms)
        }
        Err(e) => VaultStatus::from_error(e),
    }
}

fn open_session(guard: &mut Option<Session>, storage: Storage) -> VaultStatus {
    let identity = match storage.load_identity() {
        Ok(id) => id,
        // The vault opened, but the identity cannot be read. This is not "key gone":
        // the key is in place, one record is corrupted.
        Err(e) => return VaultStatus::bare(VaultState::Retry, e.to_string()),
    };
    let session = Session { storage, identity };
    let status = VaultStatus::from_session(&session);
    *guard = Some(session);
    status
}

/// Keeps the typed digits as the first entry of a new PIN. Returns their count.
#[flutter_rust_bridge::frb]
pub fn pin_setup_first() -> Result<u32, String> {
    let len = pin_entry::keep_as_first()?;
    if (apeiron_store::MIN_PIN_DIGITS..=apeiron_store::MAX_PIN_DIGITS)
        .contains(&usize::try_from(len).unwrap_or(0))
    {
        Ok(len)
    } else {
        pin_entry::clear()?;
        Err(StorageError::BadPin.to_string())
    }
}

/// Compares the confirmation with the first entry and, if they match, creates the vault.
///
/// The comparison is here, in Rust, and not in Dart: Dart never holds either entry.
#[flutter_rust_bridge::frb]
pub fn pin_setup_confirm() -> VaultStatus {
    let (first, confirmation) = match pin_entry::take_first_and_confirmation() {
        Ok(v) => v,
        Err(e) => return VaultStatus::bare(VaultState::Retry, e),
    };
    let Some(first) = first else {
        return VaultStatus::bare(VaultState::PinMismatch, "");
    };
    if first.as_slice() != confirmation.as_slice() {
        return VaultStatus::bare(VaultState::PinMismatch, "");
    }
    let mut guard = match state().lock() {
        Ok(g) => g,
        Err(_) => return VaultStatus::bare(VaultState::Retry, POISONED),
    };
    let dir = match storage_dir() {
        Ok(d) => d,
        Err(e) => return VaultStatus::bare(VaultState::Unavailable, e),
    };
    match Storage::create(
        &dir,
        &Vault::default(),
        &first,
        KdfParams::production(),
        writer(),
    ) {
        Ok(storage) => open_session(&mut guard, storage),
        Err(e) => VaultStatus::from_error(e),
    }
}

/// Clears data of a test build before the PIN, at the owner's request.
///
/// Refuses anything else: a PIN vault is erased only through [`wipe_everything`], and
/// only from the states where the key is already lost.
#[flutter_rust_bridge::frb]
pub fn reset_legacy() -> VaultStatus {
    let dir = match storage_dir() {
        Ok(d) => d,
        Err(e) => return VaultStatus::bare(VaultState::Unavailable, e),
    };
    match presence(&dir) {
        Ok(Presence::Legacy) => {}
        Ok(_) => return vault_status(),
        Err(e) => return VaultStatus::from_error(e),
    }
    if let Err(e) = Storage::wipe(&dir, &Vault::default()) {
        return VaultStatus::from_error(e);
    }
    vault_status()
}

/// Locks: destroys the database key and everything derived from it, and forgets any
/// digits on the pad.
///
/// Called when the app goes to the background and when the screen turns off —
/// decision R-001.
#[flutter_rust_bridge::frb]
pub fn lock_vault() -> Result<(), String> {
    let mut guard = state().lock().map_err(|_| POISONED.to_string())?;
    *guard = None;
    pin_entry::clear()
}

/// Erases everything cryptographically (R-005): destroys the key, not the data.
///
/// Offered only when the key is already lost; the action is irreversible.
#[flutter_rust_bridge::frb]
pub fn wipe_everything() -> Result<(), String> {
    let mut guard = state().lock().map_err(|_| POISONED.to_string())?;
    *guard = None;
    pin_entry::clear()?;
    let dir = storage_dir()?;
    Storage::wipe(&dir, &Vault::default()).map_err(|e| e.to_string())
}

/// Runs the self-check on this device.
#[flutter_rust_bridge::frb]
pub fn self_check() -> Result<Vec<CheckLine>, String> {
    let guard = state().lock().map_err(|_| POISONED.to_string())?;
    let session = guard.as_ref().ok_or_else(|| LOCKED.to_string())?;
    let mut checks = selfcheck::run(&session.storage);
    checks.extend(selfcheck::pin_checks(
        &session.storage,
        &Vault::default(),
        FAILURES_HERE.load(Ordering::Relaxed),
    ));
    Ok(checks
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
/// Needed because there is only one on-device check: an install must answer all
/// questions at once, not just the one someone thought to ask. The report is shown as is
/// and forwarded in full. Contains no secrets.
#[flutter_rust_bridge::frb]
pub fn platform_diagnostics() -> Result<String, String> {
    let mut report = String::new();
    report.push_str(&format!("SQLite: {}\n", apeiron_store::sqlite_version()));
    let dir = storage_dir();
    match &dir {
        Ok(dir) => report.push_str(&format!("data directory: {}\n", dir.display())),
        Err(e) => report.push_str(&format!("data directory: {e}\n")),
    }
    match Vault::default().diagnostics() {
        Ok(text) => report.push_str(&text),
        Err(e) => report.push_str(&format!("key store: {e}\n")),
    }
    if let Ok(dir) = &dir {
        match wrapper::params(dir) {
            Ok(Some(p)) => report.push_str(&format!(
                "PIN vault: chain {} rounds, Argon2id {} MiB x {}, level at creation {} ({})\n",
                p.rounds,
                p.argon_kib / 1024,
                p.argon_t,
                p.level_at_creation.name(),
                p.level_at_creation.raw()
            )),
            Ok(None) => report.push_str("PIN vault: none\n"),
            Err(e) => report.push_str(&format!("PIN vault: {e}\n")),
        }
        let s = PinState::load(dir);
        report.push_str(&format!(
            "PIN counter: {} in a row, {} total, {} in this process\n",
            s.consecutive,
            s.total,
            FAILURES_HERE.load(Ordering::Relaxed)
        ));
    }
    if let Ok(guard) = state().lock() {
        match guard.as_ref() {
            Some(session) => {
                match session.storage.meta_get(apeiron_store::META_KEY_ORIGIN) {
                    Ok(Some(note)) => report.push_str(&format!(
                        "how the key came about: {}\n",
                        String::from_utf8_lossy(&note)
                    )),
                    // Staying silent is not allowed: the missing line would be read as
                    // "nothing special happened".
                    Ok(None) => report.push_str("how the key came about: not recorded\n"),
                    Err(e) => report.push_str(&format!("how the key came about: {e}\n")),
                }
                report.push_str(&format!(
                    "identity in the vault: {}\n",
                    if session.identity.is_some() {
                        "yes"
                    } else {
                        "no"
                    }
                ));
            }
            None => report.push_str("vault: locked\n"),
        }
    }
    Ok(report)
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

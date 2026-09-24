//! Мост к хранилищу и аппаратному ключу.
//!
//! Через эту границу проходит только публичное и только описательное. Ключ базы
//! и мастер-ключ наружу не выходят никогда: они живут в Rust, а до него доходят
//! из Kotlin напрямую через JNI, минуя Dart (R-004).
//!
//! # Почему отказ — это состояние, а не ошибка
//!
//! Ошибка из Rust приезжает в Dart голой строкой и печатается в красном
//! баннере. Для «StrongBox недоступен» и «ключ исчез» это негодный канал:
//! первое вообще не сбой, а второе — положение, в котором человеку надо принять
//! решение, а не прочитать сообщение об ошибке. Поэтому [`unlock_vault`] не
//! возвращает `Result`: он возвращает [`VaultStatus`], где состояние названо
//! прямо.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use apeiron_core::{Identity, Sigchain};
use apeiron_platform::{KeyWrapper, SecurityLevel};
use apeiron_store::{selfcheck, Storage, StorageError};

/// Аппаратное хранилище этой платформы.
#[cfg(target_os = "android")]
type Vault = apeiron_platform::AndroidVault;

/// На всём, что не Android, аппаратного хранилища нет.
#[cfg(not(target_os = "android"))]
type Vault = crate::desktop::NoVault;

/// Состояние работы приложения между разблокировками.
///
/// `None` означает «заперто»: `Storage` уничтожен, а вместе с ним затёрты ключ
/// базы и все подключи. Разблокировка разворачивает ключ заново — и на запертом
/// телефоне железо этого не сделает, что и требуется по R-001.
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

/// В каком положении хранилище.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultState {
    /// Открыто и готово.
    Opened,
    /// Заперто: ключ базы затёрт, нужна разблокировка.
    Locked,
    /// Ключ исчез из защищённого модуля. Расшифровать нельзя ничем.
    KeyGone,
    /// Преходящий отказ. Данные целы, надо повторить.
    Retry,
    /// Аппаратного хранилища на этой платформе нет.
    Unavailable,
}

/// Что показывать про хранилище.
pub struct VaultStatus {
    pub state: VaultState,
    /// Пояснение для человека. Пустая строка, если пояснять нечего.
    pub message: String,
    /// Название уровня: StrongBox, TEE, программный, железо без уточнения.
    pub level_name: String,
    /// Сырое число `KeyInfo.getSecurityLevel()`. Показывается рядом, чтобы
    /// незнакомое значение было видно, а не подменялось ближайшим знакомым.
    pub level_raw: i32,
    /// Лежит ли ключ в железе, по сообщению системы.
    pub hardware_backed: bool,
    /// Первый ли это запуск с этим хранилищем.
    pub first_run: bool,
    /// Есть ли в хранилище личность.
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

/// Одна строка отчёта самопроверки.
pub struct CheckLine {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

/// Открывает хранилище и загружает личность.
///
/// `Result` здесь намеренно нет: отказ железа — это положение, о котором надо
/// рассказать, а не красный баннер с именем класса Java.
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
                // Хранилище открылось, а личность не читается. Это не «ключ
                // исчез»: ключ на месте, испорчена одна запись.
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

/// Текущее положение, без попытки открыть.
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

/// Запирает: уничтожает ключ базы и всё, что из него выведено.
///
/// Вызывается при уходе приложения в фон и при гашении экрана — решение R-001.
/// Разблокировка потребует аппаратного ключа заново, а на запертом телефоне
/// железо им работать откажется.
#[flutter_rust_bridge::frb]
pub fn lock_vault() -> Result<(), String> {
    let mut guard = state().lock().map_err(|_| POISONED.to_string())?;
    *guard = None;
    Ok(())
}

/// Стирает всё криптографически (R-005): уничтожает ключ, а не данные.
///
/// Требовать нечего, потому что расшифровать нечем — даже если копию базы
/// успели снять. Действие необратимо и вызывается только осознанно.
#[flutter_rust_bridge::frb]
pub fn wipe_everything() -> Result<(), String> {
    let mut guard = state().lock().map_err(|_| POISONED.to_string())?;
    *guard = None;
    let dir = storage_dir()?;
    Storage::wipe(&dir, &Vault::default()).map_err(|e| e.to_string())
}

/// Прогоняет самопроверку на этом устройстве.
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

/// Диагностика платформы одной строкой на каждый факт.
///
/// Нужна затем, что проверка на устройстве одна: установка обязана ответить на
/// все вопросы сразу, а не на тот, который догадались задать. Отчёт
/// показывается как есть и пересылается целиком. Секретов не содержит.
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
                    // Пусто — значит хранилище создано сборкой, которая этого
                    // ещё не записывала. Молчать нельзя: иначе отсутствие
                    // строки прочтут как «ничего особенного не было».
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

/// Каталог данных приложения.
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

// ── Внутреннее, для api::identity ───────────────────────────────────────────

/// Создаёт личность, аккаунт устройства и журнал — и сохраняет всё разом.
///
/// Разом, потому что порознь они бессмысленны: личность без аккаунта не может
/// переписываться, аккаунт без журнала нельзя отозвать, а журналу без личности
/// не с чего начаться.
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

/// Читает что-нибудь публичное из текущей личности.
///
/// Наружу отдаётся результат замыкания, а не сама личность: так секрет не может
/// покинуть этот модуль по невнимательности.
pub(crate) fn with_identity<T>(f: impl FnOnce(&Identity) -> T) -> Result<Option<T>, String> {
    let guard = state().lock().map_err(|_| POISONED.to_string())?;
    Ok(guard.as_ref().and_then(|s| s.identity.as_ref()).map(f))
}

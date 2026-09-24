//! Подставное аппаратное хранилище — только для проверок на рабочей машине.
//!
//! Существует затем, что весь `apeiron-store` обязан проверяться без телефона.
//! Проверка на устройстве одна, и тратить её на то, что можно выяснить здесь,
//! нельзя.
//!
//! Настоящей защиты не даёт и не притворяется: ключ лежит в памяти процесса.

// Оно и не должно собираться под устройство. Фича `testing` не включается в
// сборке приложения нигде, и это место следит, чтобы так и осталось: строка
// ниже превращает случайное включение в ошибку компиляции, а не в APK с
// программным ключом внутри.
#[cfg(all(feature = "testing", target_os = "android"))]
compile_error!(
    "подставное хранилище ключей не должно попадать в сборку для устройства: \
     фича `testing` включена вместе с target_os = \"android\""
);

use std::sync::Mutex;

use apeiron_core::{open, seal, SecretKey};
use apeiron_platform::{KeyStatus, KeyWrapper, PlatformError, SecurityLevel};
use zeroize::Zeroizing;

/// Чем подставное хранилище отвечает вместо работы.
///
/// Нужно для проверки самого дорогого свойства: преходящий сбой не должен
/// превращаться в «ключ исчез».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Behaviour {
    /// Работать как обычно.
    Normal,
    /// Отказывать преходяще: данные целы, надо повторить.
    Transient,
    /// Ключа нет. Единственное, что даёт право начинать заново.
    Gone,
    /// Своя ошибка.
    Internal,
}

struct State {
    key: Option<SecretKey>,
    level: SecurityLevel,
    behaviour: Behaviour,
}

/// Подставное хранилище ключей.
pub struct TestVault {
    state: Mutex<State>,
}

impl TestVault {
    /// Пустое хранилище: ключа ещё нет, как при первом запуске.
    pub fn empty() -> Self {
        Self::with_level(SecurityLevel::from_raw(SecurityLevel::STRONGBOX))
    }

    /// То же, но с заданным уровнем железа — чтобы проверять надписи.
    pub fn with_level(level: SecurityLevel) -> Self {
        Self {
            state: Mutex::new(State {
                key: None,
                level,
                behaviour: Behaviour::Normal,
            }),
        }
    }

    /// Как себя вести дальше.
    pub fn set_behaviour(&self, behaviour: Behaviour) {
        if let Ok(mut state) = self.state.lock() {
            state.behaviour = behaviour;
        }
    }

    /// Забыть ключ, оставив файл обёртки на месте.
    ///
    /// Так выглядит то, что в поле случается регулярно: обновление прошивки,
    /// снятие блокировки экрана, восстановление данных из бэкапа без ключей.
    pub fn forget_key(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.key = None;
        }
    }

    /// Есть ли сейчас ключ.
    pub fn has_key(&self) -> bool {
        self.state.lock().map(|s| s.key.is_some()).unwrap_or(false)
    }

    fn fail(behaviour: Behaviour) -> Option<PlatformError> {
        match behaviour {
            Behaviour::Normal => None,
            Behaviour::Transient => Some(PlatformError::Transient("подстава".to_string())),
            Behaviour::Gone => Some(PlatformError::Gone),
            Behaviour::Internal => Some(PlatformError::Internal("подстава".to_string())),
        }
    }
}

/// Область, в которой подставное хранилище запечатывает. Отдельная, чтобы эти
/// байты нельзя было спутать с записями базы.
const TEST_AAD: &[u8] = b"apeiron/testing/vault/v1";

impl KeyWrapper for TestVault {
    fn ensure_key(&self, allow_create: bool) -> Result<KeyStatus, PlatformError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| PlatformError::Internal("блокировка повреждена".to_string()))?;
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
                note: "создан подставой, настоящей защиты нет".to_string(),
            });
        }
        Ok(KeyStatus {
            level: state.level,
            note: String::new(),
        })
    }

    fn wrap(&self, plain: &[u8]) -> Result<Vec<u8>, PlatformError> {
        // Тот же предел, что и на устройстве: через KEK проходят десятки байт.
        if plain.is_empty() || plain.len() > 64 {
            return Err(PlatformError::Internal(
                "через KEK пропускают не больше 64 байт".to_string(),
            ));
        }
        let state = self
            .state
            .lock()
            .map_err(|_| PlatformError::Internal("блокировка повреждена".to_string()))?;
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
            .map_err(|_| PlatformError::Internal("блокировка повреждена".to_string()))?;
        if let Some(e) = Self::fail(state.behaviour) {
            return Err(e);
        }
        let key = state.key.as_ref().ok_or(PlatformError::Gone)?;
        // Повреждённая обёртка — это НЕ «ключ исчез»: данные на месте, просто
        // этому файлу верить нельзя. Различие то же, что и на устройстве.
        open(key, TEST_AAD, blob)
            .map(Zeroizing::new)
            .map_err(|e| PlatformError::Transient(e.to_string()))
    }

    fn destroy(&self) -> Result<(), PlatformError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| PlatformError::Internal("блокировка повреждена".to_string()))?;
        state.key = None;
        Ok(())
    }

    fn diagnostics(&self) -> Result<String, PlatformError> {
        let state = self
            .state
            .lock()
            .map_err(|_| PlatformError::Internal("блокировка повреждена".to_string()))?;
        Ok(format!(
            "ПОДСТАВНОЕ хранилище ключей, настоящей защиты нет\n\
             ключ в памяти: {}\n\
             уровень: {} ({})\n",
            if state.key.is_some() {
                "да"
            } else {
                "нет"
            },
            state.level.name(),
            state.level.raw()
        ))
    }
}

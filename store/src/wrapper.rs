//! Обёртка ключа базы: файл `vault.bin` и всё, что с ним связано.
//!
//! # Зачем два уровня ключей
//!
//! Аппаратный ключ (KEK) не шифрует базу. Он оборачивает тридцать два байта —
//! ключ базы (DEK), — а уже из них выводятся подключи по назначениям.
//!
//! Причина в том, что **параметры ключа Keystore после создания не меняются**.
//! Когда появится пин (R-001), `apeiron.kek.v1` придётся выбросить и завести
//! `v2`. С двумя уровнями это переоборачивание тридцати двух байт; с одним —
//! перешифрование всей переписки, то есть на практике «этого не сделают
//! никогда».
//!
//! Тот же приём даёт криптографическое стирание (R-005) даром: уничтожить
//! алиас и этот файл — и база превращается в шум, даже если её успели
//! скопировать.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use apeiron_core::{random_bytes, SecretKey};
use apeiron_platform::{KeyWrapper, PlatformError, SecurityLevel};
use zeroize::Zeroizing;

use crate::StorageError;

/// Имя файла обёртки.
pub const WRAPPER_FILE: &str = "vault.bin";

/// Опознавательное слово. Шесть байт, чтобы случайный файл не прошёл.
///
/// Раскладка файла целиком:
/// `"APVLT1" (6) ‖ kdf_id (1) ‖ уровень при создании (1) ‖ запечатанное`.
///
/// Запечатанное — непрозрачные байты от аппаратного хранилища, внутри них
/// `kdf_id ‖ уровень ‖ ключ базы`. Заголовок повторён внутри намеренно: так
/// подмена открытой части файла ломает распечатывание, а не проходит молча. И
/// при этом не нужны дополнительные аутентифицируемые данные на стороне
/// Keystore — поддержку AAD у конкретной реализации StrongBox на рабочей
/// машине не проверить, а цикл с телефоном тратить на это незачем.
const MAGIC: &[u8; 6] = b"APVLT1";

/// Как из выхода KEK получается ключ базы.
///
/// `1` — ключ базы есть выход KEK как он есть. `2` появится вместе с пином:
/// тогда к нему подмешается вывод из пина, и перебор шести цифр потребует
/// присутствия этого телефона на каждой попытке. Слот заведён сейчас, потому
/// что добавить поле в уже записанный формат дороже, чем оставить его пустым.
const KDF_PLAIN: u8 = 1;

/// Длина ключа базы.
const DEK_BYTES: usize = 32;

/// Запечатанный текст обёртки: `kdf_id ‖ level ‖ DEK`.
const SEALED_PLAIN_BYTES: usize = 2 + DEK_BYTES;

/// Мьютекс на всю последовательность «проверить / создать / развернуть».
///
/// Нужен против одного вполне достижимого исхода: Dart ходит через пул потоков,
/// два параллельных вызова не находят алиас, оба зовут создание ключа, и второй
/// молча заменяет первый. Ключ базы, завёрнутый первым KEK, после этого —
/// мусор навсегда.
static OPEN_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn open_lock() -> &'static Mutex<()> {
    OPEN_LOCK.get_or_init(|| Mutex::new(()))
}

/// Ключ базы и то, что система сообщает об уровне его защиты.
pub struct OpenedVault {
    /// Ключ базы. Корень иерархии подключей.
    pub dek: SecretKey,
    /// Уровень железа **сейчас**, как его сообщает система.
    pub level: SecurityLevel,
    /// Уровень, записанный при создании обёртки.
    ///
    /// Хранится отдельно и защищён от подмены: иначе подправленный байт в файле
    /// менял бы надпись на экране, не трогая ничего больше. Расхождение с
    /// текущим — повод сказать об этом в отчёте, а не отказать в работе.
    pub level_at_creation: SecurityLevel,
    /// Была ли обёртка создана прямо сейчас (то есть первый ли это запуск).
    pub created_now: bool,
}

/// Читает обёртку или создаёт её, если это первый запуск.
pub fn load_or_create<W: KeyWrapper>(dir: &Path, wrapper: &W) -> Result<OpenedVault, StorageError> {
    let _guard = open_lock().lock().map_err(|_| StorageError::Poisoned)?;

    let path = dir.join(WRAPPER_FILE);
    if path.exists() {
        read_existing(&path, wrapper)
    } else {
        create_new(dir, &path, wrapper)
    }
}

fn read_existing<W: KeyWrapper>(path: &Path, wrapper: &W) -> Result<OpenedVault, StorageError> {
    let raw = fs::read(path)?;
    let parsed = Parsed::from_bytes(&raw)?;

    // `allow_create = false` — здесь и есть всё различие между «первым
    // запуском» и «ключ исчез». Обёртка на диске означает, что ключ был; если
    // его нет, создавать новый нельзя ни при каких условиях — это уничтожило бы
    // переписку безвозвратно.
    let level = wrapper.ensure_key(false)?;

    let plain = wrapper.unwrap(&parsed.blob)?;
    if plain.len() != SEALED_PLAIN_BYTES {
        return Err(StorageError::Wrapper(format!(
            "внутри обёртки {} байт вместо {}",
            plain.len(),
            SEALED_PLAIN_BYTES
        )));
    }

    // Заголовок повторён внутри запечатанного текста, и здесь он сверяется.
    // Подмена байта в открытой части файла после этого не проходит молча.
    // Через AAD то же самое делать нельзя: поддержка AAD у конкретной
    // реализации StrongBox — то, что не проверишь на рабочей машине.
    let inner_kdf = plain.first().copied().unwrap_or_default();
    let inner_level = plain.get(1).copied().unwrap_or_default() as i8;
    if inner_kdf != parsed.kdf_id || inner_level != parsed.level {
        return Err(StorageError::Wrapper(
            "заголовок обёртки не совпадает с запечатанным — файл подменён".to_string(),
        ));
    }
    if parsed.kdf_id != KDF_PLAIN {
        return Err(StorageError::Wrapper(format!(
            "обёртка сделана способом {}, который эта версия не умеет",
            parsed.kdf_id
        )));
    }

    let mut dek = [0u8; DEK_BYTES];
    let body = plain
        .get(2..)
        .ok_or_else(|| StorageError::Wrapper("обёртка без ключа".to_string()))?;
    dek.copy_from_slice(body);

    Ok(OpenedVault {
        dek: SecretKey::from_bytes(dek),
        level,
        level_at_creation: SecurityLevel::from_raw(i32::from(parsed.level)),
        created_now: false,
    })
}

fn create_new<W: KeyWrapper>(
    dir: &Path,
    path: &Path,
    wrapper: &W,
) -> Result<OpenedVault, StorageError> {
    let level = wrapper.ensure_key(true)?;

    let raw_level = clamp_level(level.raw());
    let mut plain = Zeroizing::new(Vec::with_capacity(SEALED_PLAIN_BYTES));
    plain.push(KDF_PLAIN);
    plain.push(raw_level as u8);
    plain.extend_from_slice(&random_bytes::<DEK_BYTES>()?);

    let blob = wrapper.wrap(&plain)?;
    if blob.is_empty() {
        return Err(StorageError::Wrapper(
            "аппаратное хранилище вернуло пустую обёртку".to_string(),
        ));
    }

    let mut file = Vec::with_capacity(MAGIC.len() + 2 + blob.len());
    file.extend_from_slice(MAGIC);
    file.push(KDF_PLAIN);
    file.push(raw_level as u8);
    file.extend_from_slice(&blob);
    write_atomically(dir, path, &file)?;

    let mut dek = [0u8; DEK_BYTES];
    let body = plain
        .get(2..)
        .ok_or_else(|| StorageError::Wrapper("обёртка без ключа".to_string()))?;
    dek.copy_from_slice(body);

    Ok(OpenedVault {
        dek: SecretKey::from_bytes(dek),
        level,
        level_at_creation: level,
        created_now: true,
    })
}

/// Уровень укладывается в знаковый байт: значений всего пять, от -2 до 2.
fn clamp_level(raw: i32) -> i8 {
    if (i32::from(i8::MIN)..=i32::from(i8::MAX)).contains(&raw) {
        raw as i8
    } else {
        SecurityLevel::UNKNOWN as i8
    }
}

/// Разобранный заголовок обёртки.
struct Parsed {
    kdf_id: u8,
    level: i8,
    /// Запечатанное аппаратным ключом. Для нас — непрозрачные байты: что там
    /// внутри, знает только та сторона, которая их сделала.
    blob: Vec<u8>,
}

impl Parsed {
    fn from_bytes(raw: &[u8]) -> Result<Self, StorageError> {
        let head = raw
            .get(..MAGIC.len() + 2)
            .ok_or_else(|| StorageError::Wrapper("файл обёртки короче заголовка".to_string()))?;
        if head.get(..MAGIC.len()) != Some(MAGIC.as_slice()) {
            return Err(StorageError::Wrapper(
                "это не файл обёртки Apeiron".to_string(),
            ));
        }
        let kdf_id = head.get(6).copied().unwrap_or_default();
        let level = head.get(7).copied().unwrap_or_default() as i8;
        let blob = raw
            .get(MAGIC.len() + 2..)
            .ok_or_else(|| StorageError::Wrapper("обёртка без тела".to_string()))?
            .to_vec();
        if blob.is_empty() {
            return Err(StorageError::Wrapper("тело обёртки пусто".to_string()));
        }
        Ok(Self {
            kdf_id,
            level,
            blob,
        })
    }
}

/// Записывает файл так, чтобы внезапное выключение телефона не оставило
/// полузапись.
///
/// Порядок неотменяем: временный файл → сброс на диск → переименование →
/// сброс каталога. Простая запись поверх оставила бы обёртку в промежуточном
/// состоянии, а это потеря всей переписки.
fn write_atomically(dir: &Path, path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    let tmp: PathBuf = path.with_extension("tmp");
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;

    // Переименование попадает в метаданные каталога, и их тоже надо сбросить.
    // На Windows каталог как файл не открыть, но там эта сборка и не работает.
    #[cfg(unix)]
    {
        let dir_handle = fs::File::open(dir)?;
        dir_handle.sync_all()?;
    }
    #[cfg(not(unix))]
    let _ = dir;

    Ok(())
}

/// Стирает обёртку. Аппаратный ключ удаляет вызывающий — порядок важен, см.
/// [`crate::Storage::wipe`].
pub fn remove(dir: &Path) -> Result<(), StorageError> {
    let path = dir.join(WRAPPER_FILE);
    if path.exists() {
        fs::remove_file(&path)?;
    }
    let tmp = path.with_extension("tmp");
    if tmp.exists() {
        fs::remove_file(&tmp)?;
    }
    Ok(())
}

impl From<PlatformError> for StorageError {
    fn from(e: PlatformError) -> Self {
        match e {
            PlatformError::Gone => StorageError::KeyGone,
            other => StorageError::Platform(other),
        }
    }
}

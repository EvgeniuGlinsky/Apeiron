//! Локальное хранилище: иерархия ключей, схема базы, запечатывание записей.
//!
//! # Что здесь защищено и чем
//!
//! Аппаратный ключ устройства оборачивает ключ базы (`wrapper`), из ключа базы
//! выводятся подключи по назначениям (`keys`), каждая запись шифруется отдельно
//! и привязывается к своему месту (`record`). Схема SQLite держит только
//! непрозрачные байты.
//!
//! # Почему SQLite, а не файлы
//!
//! Состояние храповика и запись о сообщении обязаны попадать на диск **одной
//! транзакцией**. Расхождение между ними — это не неудобство, а навсегда
//! непрочитанные сообщения: храповик ушёл вперёд, а прочитать то, что он уже
//! пропустил, нечем. Своего движка хранения не пишем по той же причине, по
//! которой не пишем своих примитивов шифрования.
//!
//! # Почему не SQLCipher
//!
//! Шифруем сами, своим AEAD — тем же XChaCha20-Poly1305, который прошёл
//! официальные векторы RFC 8439. Так криптостек остаётся одного поколения (за
//! этим следит `cargo deny`), а у записи появляется **место**: переложить её на
//! чужой идентификатор или подсунуть из другой таблицы нельзя. Шифрование файла
//! целиком этого не даёт.
//!
//! # Что отсюда утекает
//!
//! Число записей, их размеры и время изменения файла. Содержимое, имена
//! собеседников и их ключи — нет. Открытым в базе не лежит ничего: даже поиск
//! по контакту идёт по непрозрачной метке (`apeiron_core::SecretKey::tag`), а
//! не по публичному ключу.

pub mod keys;
pub mod record;
pub mod repo;
pub mod selfcheck;
pub mod wrapper;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

use std::path::{Path, PathBuf};

use apeiron_platform::{KeyWrapper, PlatformError, SecurityLevel};
use rusqlite::Connection;

pub use keys::Keys;
pub use record::{Table, SCHEMA_VERSION};

/// Имя файла базы.
pub const DATABASE_FILE: &str = "apeiron.db";

/// Служебная запись: как появился аппаратный ключ.
pub const META_KEY_ORIGIN: &str = "ключ/как появился";

/// Что может пойти не так в хранилище.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// Ключ исчез из защищённого модуля устройства.
    ///
    /// Вынесен из [`StorageError::Platform`] отдельно намеренно: это
    /// единственное состояние, из которого разрешено предлагать «начать
    /// заново». Всё остальное — «повторите», и данные целы.
    #[error(
        "КЛЮЧ ХРАНИЛИЩА ИСЧЕЗ ИЗ ЗАЩИЩЁННОГО МОДУЛЯ ЭТОГО ТЕЛЕФОНА. \
         Переписку расшифровать нельзя ничем. Единственный выход — начать заново."
    )]
    KeyGone,

    #[error(transparent)]
    Platform(PlatformError),

    #[error("обёртка ключа непригодна: {0}")]
    Wrapper(String),

    #[error("файловая ошибка: {0}")]
    Io(#[from] std::io::Error),

    #[error("ошибка базы: {0}")]
    Db(#[from] rusqlite::Error),

    #[error(transparent)]
    Aead(#[from] apeiron_core::AeadError),

    #[error(transparent)]
    Random(#[from] apeiron_core::RandomError),

    #[error(transparent)]
    Identity(#[from] apeiron_core::IdentityError),

    #[error(transparent)]
    Chat(#[from] apeiron_core::ChatError),

    #[error(transparent)]
    Sigchain(#[from] apeiron_core::SigchainError),

    #[error(
        "база записана схемой версии {found}, а эта сборка знает только {known}. \
         Читать её нельзя: старый код понял бы новые записи неправильно."
    )]
    SchemaTooNew { found: u16, known: u16 },

    #[error("внутренняя блокировка повреждена: перезапустите приложение")]
    Poisoned,
}

impl StorageError {
    /// Можно ли повторить, не потеряв данные.
    ///
    /// Ровно одно состояние отвечает «нет», и попасть в него можно только из
    /// трёх явных условий на стороне платформы. Всё прочее — повод повторить, а
    /// не стирать переписку.
    pub fn is_retryable(&self) -> bool {
        !matches!(self, Self::KeyGone)
    }
}

/// Версия SQLite, с которой собрано. Для отчёта о платформе.
pub fn sqlite_version() -> String {
    rusqlite::version().to_string()
}

/// Открытое хранилище.
pub struct Storage {
    conn: Connection,
    keys: Keys,
    level: SecurityLevel,
    level_at_creation: SecurityLevel,
    created_now: bool,
    dir: PathBuf,
}

impl std::fmt::Debug for Storage {
    /// Ключей не печатает: строки журнала переживают процесс.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Storage")
            .field("уровень", &self.level.name())
            .field("первый запуск", &self.created_now)
            .finish_non_exhaustive()
    }
}

impl Storage {
    /// Открывает хранилище, создавая его при первом запуске.
    pub fn open<W: KeyWrapper>(dir: &Path, vault: &W) -> Result<Self, StorageError> {
        std::fs::create_dir_all(dir)?;
        let opened = wrapper::load_or_create(dir, vault)?;
        let keys = Keys::derive(&opened.dek);

        let conn = Connection::open(dir.join(DATABASE_FILE))?;
        configure(&conn)?;
        prepare_schema(&conn)?;

        let storage = Self {
            conn,
            keys,
            level: opened.level,
            level_at_creation: opened.level_at_creation,
            created_now: opened.created_now,
            dir: dir.to_path_buf(),
        };

        // Заметку о том, как появился ключ, кладём в базу сразу: на стороне
        // платформы она живёт только до конца процесса, а прочитать её захотят
        // позже — когда будут разбираться, почему уровень именно такой.
        if opened.created_now && !opened.creation_note.is_empty() {
            storage.meta_set(META_KEY_ORIGIN, opened.creation_note.as_bytes())?;
        }

        Ok(storage)
    }

    /// Что система сообщает об уровне защиты ключа **сейчас**.
    pub fn security_level(&self) -> SecurityLevel {
        self.level
    }

    /// Уровень, записанный при создании обёртки.
    pub fn level_at_creation(&self) -> SecurityLevel {
        self.level_at_creation
    }

    /// Создана ли обёртка прямо сейчас, то есть первый ли это запуск.
    pub fn created_now(&self) -> bool {
        self.created_now
    }

    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    pub(crate) fn keys(&self) -> &Keys {
        &self.keys
    }

    /// Ключ для пробных записей самопроверки.
    ///
    /// Отдаётся тот же, которым запечатано служебное: проверять надо настоящим
    /// ключом, иначе проверка доказывает только то, что работает подстава.
    /// Наружу из крейта не выходит — `SecretKey` не отдаёт своих байтов.
    pub(crate) fn probe_key(&self) -> &apeiron_core::SecretKey {
        self.keys.meta()
    }

    /// Каталог, в котором лежит хранилище.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Стирает всё — криптографически (R-005).
    ///
    /// Порядок неотменяем: сначала уничтожается ключ, потом файлы. Наоборот
    /// нельзя: прерывание между шагами оставило бы живой ключ при отсутствии
    /// обёртки, а это состояние читается как «ключ есть, данных нет» и
    /// разбирается сложнее, чем обратное.
    ///
    /// Уничтожается ключ, а не данные. Это сильнее прятания: требовать нечего,
    /// потому что расшифровать нечем — даже если копию базы успели снять.
    pub fn wipe<W: KeyWrapper>(dir: &Path, vault: &W) -> Result<(), StorageError> {
        vault.destroy().map_err(StorageError::from)?;
        wrapper::remove(dir)?;

        let db = dir.join(DATABASE_FILE);
        // Журнал опережающей записи и разделяемый индекс — такие же файлы базы,
        // и оставлять их значит оставлять шифротекст там, где его не ждут.
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let path = PathBuf::from(format!("{}{suffix}", db.display()));
            if path.exists() {
                std::fs::remove_file(&path)?;
            }
        }
        Ok(())
    }
}

/// Настройки соединения.
fn configure(conn: &Connection) -> Result<(), StorageError> {
    // Телефон выключают в произвольный момент, и на этот случай долговечность
    // важнее скорости: расхождение состояния храповика с записями — это
    // навсегда непрочитанные сообщения, а не подтормаживание.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "FULL")?;
    // Каскадное удаление сессий вместе с контактом работает только так.
    conn.pragma_update(None, "foreign_keys", "ON")?;
    // Временные таблицы — в память. На диск не должно попадать ничего, чего мы
    // не запечатали сами.
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    Ok(())
}

/// Раскладка таблиц.
///
/// Открытым здесь не лежит ничего, кроме служебных чисел: `sealed` — это
/// запечатанные байты, `tag` — непрозрачная метка для поиска. Публичный ключ
/// собеседника в открытом виде хранить нельзя: список собеседников читался бы
/// из файла базы без всякого ключа, то есть ровно то, что база и должна была
/// закрыть.
const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS schema_version (
    id      INTEGER PRIMARY KEY CHECK (id = 1),
    version INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS meta (
    id     INTEGER PRIMARY KEY,
    tag    BLOB NOT NULL UNIQUE,
    sealed BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS identity (
    id     INTEGER PRIMARY KEY CHECK (id = 1),
    sealed BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS device_account (
    id     INTEGER PRIMARY KEY CHECK (id = 1),
    sealed BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS sigchain (
    id     INTEGER PRIMARY KEY CHECK (id = 1),
    sealed BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS contacts (
    id     INTEGER PRIMARY KEY,
    tag    BLOB NOT NULL UNIQUE,
    sealed BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
    id         INTEGER PRIMARY KEY,
    contact_id INTEGER NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    tag        BLOB NOT NULL UNIQUE,
    sealed     BLOB NOT NULL
);

CREATE INDEX IF NOT EXISTS sessions_by_contact ON sessions(contact_id);
";

/// Создаёт схему или доводит её до текущей версии.
fn prepare_schema(conn: &Connection) -> Result<(), StorageError> {
    conn.execute_batch(SCHEMA_SQL)?;

    let found: Option<u16> = conn
        .query_row("SELECT version FROM schema_version WHERE id = 1", [], |r| {
            r.get(0)
        })
        .ok();

    match found {
        None => {
            conn.execute(
                "INSERT INTO schema_version (id, version) VALUES (1, ?1)",
                [SCHEMA_VERSION],
            )?;
            Ok(())
        }
        Some(v) if v == SCHEMA_VERSION => Ok(()),
        // Откат приложения на базу, записанную новее, запрещён: старый код
        // понял бы новые записи неправильно и молча. Версия схемы к тому же
        // входит в проверку подлинности каждой записи, так что «неправильно»
        // здесь означает «никак».
        Some(v) if v > SCHEMA_VERSION => Err(StorageError::SchemaTooNew {
            found: v,
            known: SCHEMA_VERSION,
        }),
        Some(v) => migrate(conn, v, SCHEMA_VERSION),
    }
}

/// Переводит схему со старой версии на текущую.
///
/// Версий пока одна, и потому здесь пусто. Существует эта функция не «на
/// будущее»: первую настоящую миграцию придётся выполнять на живых данных
/// владельца, и место для неё должно быть готово заранее — вместе с правилом,
/// что **каждая миграция приносит свой тест, доказывающий, что данные пережили
/// переход**. Ставить такой тест задним числом уже не на чем.
///
/// Шаги пойдут по одному, `from -> from+1 -> ... -> to`, каждый в своей
/// транзакции, и версия будет обновляться в той же транзакции, что и данные.
fn migrate(conn: &Connection, from: u16, to: u16) -> Result<(), StorageError> {
    debug_assert!(from < to, "миграция вызвана не для повышения версии");
    let _ = (conn, from, to);
    Ok(())
}

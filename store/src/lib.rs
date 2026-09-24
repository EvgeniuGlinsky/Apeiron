//! Local storage: key hierarchy, database schema, sealing of records.
//!
//! # What is protected here and by what
//!
//! The device's hardware key wraps the database key (`wrapper`), subkeys per purpose are
//! derived from the database key (`keys`), every record is encrypted separately
//! and bound to its location (`record`). The SQLite schema holds only
//! opaque bytes.
//!
//! # Why SQLite and not files
//!
//! The ratchet state and the message record must reach the disk **in one
//! transaction**. A divergence between them is not an inconvenience but messages unread
//! forever: the ratchet has moved ahead, and there is nothing to read what it has already
//! skipped. We do not write our own storage engine for the same reason we
//! do not write our own encryption primitives.
//!
//! # Why not SQLCipher
//!
//! We encrypt ourselves, with our own AEAD: the same XChaCha20-Poly1305 that passed
//! the official RFC 8439 vectors. This way the crypto stack stays one generation (this
//! is watched by `cargo deny`), and a record gets a **location**: it cannot be moved to
//! someone else's identifier or slipped in from another table. Encrypting the whole
//! file does not give that.
//!
//! # What leaks from here
//!
//! The number of records, their sizes and the file modification time. The content, peers'
//! names and their keys do not. Nothing lies in the clear in the database: even lookup
//! by contact goes by an opaque tag (`apeiron_core::SecretKey::tag`), not
//! by the public key.

mod fsutil;
pub mod keys;
pub mod pin;
pub mod record;
pub mod repo;
pub mod selfcheck;
pub mod wrapper;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

use std::path::{Path, PathBuf};

use apeiron_platform::{HardwareKey, PlatformError, SecurityLevel};
use rusqlite::Connection;

pub use keys::Keys;
pub use record::{Table, SCHEMA_VERSION};
pub use wrapper::{KdfParams, Presence, Timing, MAX_PIN_DIGITS, MIN_PIN_DIGITS};

/// The database file name.
pub const DATABASE_FILE: &str = "apeiron.db";

/// Internal record: how the hardware key came about.
pub const META_KEY_ORIGIN: &str = "ключ/как появился";

/// What can go wrong in the storage.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// The key vanished from the device's secure module.
    ///
    /// Split out of [`StorageError::Platform`] deliberately: this is the
    /// only state from which it is allowed to offer "start
    /// over". Everything else is "retry", and the data is intact.
    #[error(
        "КЛЮЧ ХРАНИЛИЩА ИСЧЕЗ ИЗ ЗАЩИЩЁННОГО МОДУЛЯ ЭТОГО ТЕЛЕФОНА. \
         Переписку расшифровать нельзя ничем. Единственный выход — начать заново."
    )]
    KeyGone,

    /// The secure hardware holds a key, but not the one this vault was made with: the key
    /// check value in the header does not match. The same outcome as "key gone", reached
    /// by a fourth, deterministic condition, and kept apart from a wrong PIN so that the
    /// owner does not collect delays for a PIN that was right.
    #[error(
        "КЛЮЧ В ЗАЩИЩЁННОМ МОДУЛЕ НЕ ТОТ, КОТОРЫМ СДЕЛАНО ХРАНИЛИЩЕ. \
         Переписку расшифровать нельзя ничем. Единственный выход — начать заново."
    )]
    KeyMismatch,

    /// Data of a build before the PIN. This build does not open it (`docs/storage.md`).
    #[error(
        "данные тестовой сборки до появления пина: эта сборка их не открывает, начните заново"
    )]
    Legacy,

    /// No vault yet: a PIN has to be set first.
    #[error("хранилища ещё нет: задайте пин")]
    NoVault,

    /// Not a PIN this vault could have: wrong length or not only digits.
    #[error("пин — от 6 до 16 цифр")]
    BadPin,

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
    /// Whether it can be retried without losing data.
    ///
    /// Exactly one state answers "no", and it can be reached only through
    /// three explicit conditions on the platform side. Everything else is a reason to retry,
    /// not to erase the conversations.
    pub fn is_retryable(&self) -> bool {
        !matches!(self, Self::KeyGone | Self::KeyMismatch | Self::Legacy)
    }
}

/// The SQLite version it was built with. For the platform report.
pub fn sqlite_version() -> String {
    rusqlite::version().to_string()
}

/// An opened storage.
pub struct Storage {
    conn: Connection,
    keys: Keys,
    level: SecurityLevel,
    level_at_creation: SecurityLevel,
    created_now: bool,
    timing: Timing,
    rounds: u32,
    dir: PathBuf,
}

impl std::fmt::Debug for Storage {
    /// Does not print keys: log lines outlive the process.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Storage")
            .field("level", &self.level.name())
            .field("first launch", &self.created_now)
            .finish_non_exhaustive()
    }
}

/// The outcome of an unlock attempt that reached a decision.
pub enum Opening {
    Opened(Box<Storage>),
    /// The PIN is wrong: `failures` in a row, `wait_ms` before the next attempt.
    WrongPin {
        failures: u32,
        wait_ms: u64,
    },
    /// Too many failures: wait. Nothing was tried.
    Delayed {
        wait_ms: u64,
    },
}

impl Storage {
    /// Creates the storage with a new PIN. Allowed only while there is no wrapper file.
    ///
    /// `writer` is a random mark of the calling process, written next to the attempt
    /// counter for the self-check.
    pub fn create<H: HardwareKey>(
        dir: &Path,
        hw: &H,
        pin: &[u8],
        params: KdfParams,
        writer: [u8; 8],
    ) -> Result<Self, StorageError> {
        std::fs::create_dir_all(dir)?;
        let opened = wrapper::create(dir, hw, pin, params, writer)?;
        Self::from_opened(dir, opened)
    }

    /// Opens the storage with `pin`, counting the attempt (see [`wrapper::unlock`]).
    pub fn unlock<H: HardwareKey>(
        dir: &Path,
        hw: &H,
        pin: &[u8],
        writer: [u8; 8],
    ) -> Result<Opening, StorageError> {
        Ok(match wrapper::unlock(dir, hw, pin, writer)? {
            wrapper::Unlock::Opened(opened) => {
                Opening::Opened(Box::new(Self::from_opened(dir, opened)?))
            }
            wrapper::Unlock::WrongPin { failures, wait_ms } => {
                Opening::WrongPin { failures, wait_ms }
            }
            wrapper::Unlock::Delayed { wait_ms } => Opening::Delayed { wait_ms },
        })
    }

    fn from_opened(dir: &Path, opened: wrapper::OpenedVault) -> Result<Self, StorageError> {
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
            timing: opened.timing,
            rounds: opened.rounds,
            dir: dir.to_path_buf(),
        };

        // The note on how the key came about goes into the database right away: on the
        // platform side it lives only until the end of the process, and it will be wanted
        // later, when someone is figuring out why the level is what it is.
        if opened.created_now && !opened.creation_note.is_empty() {
            storage.meta_set(META_KEY_ORIGIN, opened.creation_note.as_bytes())?;
        }

        Ok(storage)
    }

    /// What the system reports about the key's protection level **now**.
    pub fn security_level(&self) -> SecurityLevel {
        self.level
    }

    /// The level recorded when the wrapper was created.
    pub fn level_at_creation(&self) -> SecurityLevel {
        self.level_at_creation
    }

    /// Whether the wrapper was created just now, i.e. whether this is the first launch.
    pub fn created_now(&self) -> bool {
        self.created_now
    }

    /// How long the derivation took when this storage was opened.
    pub fn timing(&self) -> Timing {
        self.timing
    }

    /// Rounds of the hardware chain in this vault.
    pub fn rounds(&self) -> u32 {
        self.rounds
    }

    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    pub(crate) fn keys(&self) -> &Keys {
        &self.keys
    }

    /// The key for self-check probe records.
    ///
    /// The same one that seals the internal records is handed out: the check must use the
    /// real key, otherwise it proves only that the fake works.
    /// It does not leave the crate: `SecretKey` does not give out its bytes.
    pub(crate) fn probe_key(&self) -> &apeiron_core::SecretKey {
        self.keys.meta()
    }

    /// The directory where the storage lives.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Erases everything, cryptographically (R-005).
    ///
    /// The order is non-negotiable: first the key is destroyed, then the files. The other
    /// way round is not allowed: an interruption between the steps would leave a live key with
    /// no wrapper, and that state reads as "key present, no data" and
    /// is harder to sort out than the reverse.
    ///
    /// Held under the same lock as creating and unlocking: a wipe racing a creation could
    /// otherwise leave a fresh wrapper sealed under a key that has just been destroyed.
    ///
    /// The key is destroyed, not the data. An honest limit (R-005 in the threat log):
    /// Android does not let an app ask for rollback-resistant keys, so a copy of the
    /// keystore's own files taken earlier still holds the key blob. Destroying the alias
    /// makes the data unreadable from now on, not retroactively.
    pub fn wipe<H: HardwareKey>(dir: &Path, hw: &H) -> Result<(), StorageError> {
        let _guard = wrapper::open_lock()?;
        hw.destroy().map_err(StorageError::from)?;
        wrapper::remove(dir)?;

        let db = dir.join(DATABASE_FILE);
        // The write-ahead log and the shared-memory index are database files just the same,
        // and leaving them means leaving ciphertext where nobody expects it.
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let path = PathBuf::from(format!("{}{suffix}", db.display()));
            if path.exists() {
                std::fs::remove_file(&path)?;
            }
        }
        Ok(())
    }
}

/// Connection settings.
fn configure(conn: &Connection) -> Result<(), StorageError> {
    // A phone gets switched off at an arbitrary moment, and for that case durability
    // matters more than speed: a divergence of the ratchet state from the records means
    // messages unread forever, not a slowdown.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "FULL")?;
    // Cascading deletion of sessions together with a contact works only this way.
    conn.pragma_update(None, "foreign_keys", "ON")?;
    // Temporary tables go to memory. Nothing we have not sealed ourselves
    // must reach the disk.
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    Ok(())
}

/// Table layout.
///
/// Nothing lies in the clear here except internal numbers: `sealed` is
/// sealed bytes, `tag` is an opaque lookup tag. The peer's public key
/// must not be stored in the clear: the list of peers could then be read
/// from the database file without any key, which is exactly what the database was meant
/// to close off.
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

/// Creates the schema or brings it up to the current version.
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
        // Rolling the application back onto a database written by a newer version is
        // forbidden: old code would understand new records wrongly, and silently. The schema
        // version is moreover part of every record's authenticity check, so "wrongly"
        // here means "not at all".
        Some(v) if v > SCHEMA_VERSION => Err(StorageError::SchemaTooNew {
            found: v,
            known: SCHEMA_VERSION,
        }),
        Some(v) => migrate(conn, v, SCHEMA_VERSION),
    }
}

/// Moves the schema from an old version to the current one.
///
/// So far there is only one version, and so this is empty. This function exists not "for
/// the future": the first real migration will have to run on the owner's live
/// data, and a place for it must be ready in advance, together with the rule
/// that **every migration brings its own test proving that the data survived
/// the transition**. There is nothing to add such a test to after the fact.
///
/// The steps will go one at a time, `from -> from+1 -> ... -> to`, each in its own
/// transaction, and the version will be updated in the same transaction as the data.
fn migrate(conn: &Connection, from: u16, to: u16) -> Result<(), StorageError> {
    debug_assert!(from < to, "migration called not to raise the version");
    let _ = (conn, from, to);
    Ok(())
}

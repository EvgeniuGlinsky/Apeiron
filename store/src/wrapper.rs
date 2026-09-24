//! The database key wrapper: the `vault.bin` file and everything connected with it.
//!
//! # Why two levels of keys
//!
//! The hardware key (KEK) does not encrypt the database. It wraps thirty-two bytes,
//! the database key (DEK), and the subkeys per purpose are derived from those.
//!
//! The reason is that **the parameters of a Keystore key cannot change after creation**.
//! When a PIN appears (R-001), `apeiron.kek.v1` will have to be thrown away and a `v2`
//! created. With two levels that is re-wrapping thirty-two bytes; with one it is
//! re-encrypting all the conversations, which in practice means "this will never
//! be done".
//!
//! The same technique gives cryptographic erasure (R-005) for free: destroy the
//! alias and this file, and the database turns into noise, even if someone managed to
//! copy it.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use apeiron_core::{random_bytes, SecretKey};
use apeiron_platform::{KeyWrapper, PlatformError, SecurityLevel};
use zeroize::Zeroizing;

use crate::StorageError;

/// The wrapper file name.
pub const WRAPPER_FILE: &str = "vault.bin";

/// The magic word. Six bytes, so that a random file does not pass.
///
/// Layout of the whole file:
/// `"APVLT1" (6) ‖ kdf_id (1) ‖ level at creation (1) ‖ sealed`.
///
/// The sealed part is opaque bytes from the hardware storage; inside them is
/// `kdf_id ‖ level ‖ database key`. The header is repeated inside deliberately: this way
/// substituting the public part of the file breaks opening instead of passing silently. And
/// no additional authenticated data is needed on the Keystore side
/// for this: AAD support in a particular StrongBox implementation cannot be checked on the
/// development machine, and there is no point spending a phone cycle on it.
const MAGIC: &[u8; 6] = b"APVLT1";

/// How the database key is obtained from the KEK output.
///
/// `1`: the database key is the KEK output as is. `2` will appear together with the PIN:
/// then a derivation from the PIN will be mixed in, and brute-forcing six digits will require
/// the presence of this phone for every attempt. The slot is created now because
/// adding a field to an already written format costs more than leaving it empty.
const KDF_PLAIN: u8 = 1;

/// Database key length.
const DEK_BYTES: usize = 32;

/// The sealed text of the wrapper: `kdf_id ‖ level ‖ DEK`.
const SEALED_PLAIN_BYTES: usize = 2 + DEK_BYTES;

/// A mutex over the whole "check / create / unwrap" sequence.
///
/// Needed against one quite reachable outcome: Dart goes through a thread pool,
/// two parallel calls do not find the alias, both call key creation, and the second
/// silently replaces the first. The database key wrapped by the first KEK is then
/// garbage forever.
static OPEN_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn open_lock() -> &'static Mutex<()> {
    OPEN_LOCK.get_or_init(|| Mutex::new(()))
}

/// The database key and what the system reports about its protection level.
pub struct OpenedVault {
    /// The database key. The root of the subkey hierarchy.
    pub dek: SecretKey,
    /// The hardware level **now**, as the system reports it.
    pub level: SecurityLevel,
    /// The level recorded when the wrapper was created.
    ///
    /// Stored separately and protected against substitution: otherwise a tweaked byte in the
    /// file would change the label on the screen without touching anything else. A mismatch
    /// with the current one is a reason to mention it in the report, not to refuse to work.
    pub level_at_creation: SecurityLevel,
    /// Whether the wrapper was created just now (i.e. whether this is the first launch).
    pub created_now: bool,
    /// How the hardware key came about. Empty if not during this launch.
    pub creation_note: String,
}

/// Reads the wrapper or creates it, if this is the first launch.
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

    // `allow_create = false`: this is the whole difference between "first
    // launch" and "key gone". A wrapper on disk means there was a key; if
    // it is not there, a new one must not be created under any circumstances: that would
    // destroy the conversations irrecoverably.
    let status = wrapper.ensure_key(false)?;
    let level = status.level;

    let plain = wrapper.unwrap(&parsed.blob)?;
    if plain.len() != SEALED_PLAIN_BYTES {
        return Err(StorageError::Wrapper(format!(
            "внутри обёртки {} байт вместо {}",
            plain.len(),
            SEALED_PLAIN_BYTES
        )));
    }

    // The header is repeated inside the sealed text, and here it is compared.
    // After that, substituting a byte in the public part of the file does not pass silently.
    // Doing the same via AAD is not an option: AAD support in a particular
    // StrongBox implementation is something that cannot be checked on the development machine.
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
        creation_note: status.note,
    })
}

fn create_new<W: KeyWrapper>(
    dir: &Path,
    path: &Path,
    wrapper: &W,
) -> Result<OpenedVault, StorageError> {
    let status = wrapper.ensure_key(true)?;
    let level = status.level;

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
        creation_note: status.note,
    })
}

/// The level fits into a signed byte: there are only five values, from -2 to 2.
fn clamp_level(raw: i32) -> i8 {
    if (i32::from(i8::MIN)..=i32::from(i8::MAX)).contains(&raw) {
        raw as i8
    } else {
        SecurityLevel::UNKNOWN as i8
    }
}

/// The parsed wrapper header.
struct Parsed {
    kdf_id: u8,
    level: i8,
    /// What was sealed by the hardware key. For us these are opaque bytes: what is
    /// inside is known only to the side that made them.
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

/// Writes a file so that a sudden phone shutdown does not leave a
/// half-written file.
///
/// The order is non-negotiable: temporary file → flush to disk → rename →
/// flush the directory. A plain overwrite would leave the wrapper in an intermediate
/// state, and that means losing all the conversations.
fn write_atomically(dir: &Path, path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    let tmp: PathBuf = path.with_extension("tmp");
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;

    // The rename goes into the directory metadata, and that must be flushed too.
    // On Windows a directory cannot be opened as a file, but this build does not run there anyway.
    #[cfg(unix)]
    {
        let dir_handle = fs::File::open(dir)?;
        dir_handle.sync_all()?;
    }
    #[cfg(not(unix))]
    let _ = dir;

    Ok(())
}

/// Erases the wrapper. The caller deletes the hardware key; the order matters, see
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

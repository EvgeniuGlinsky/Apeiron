//! The wrapper file `vault.bin`: the database key, sealed under a key that needs both the
//! PIN and this phone's secure hardware (R-011).
//!
//! # Derivation
//!
//! ```text
//! x0 = Argon2id(pin, salt)                          on the CPU
//! xk = HMAC_hw(... HMAC_hw(x0) ...), k rounds       in the secure hardware, one op per round
//! W  = HKDF(xk, "apeiron/storage/pin-wrap/v1")
//! vault.bin = header ‖ XChaCha20-Poly1305(W, aad = header, DEK)
//! ```
//!
//! The chain is what makes a guess expensive for someone who holds the phone: every guess
//! costs `k` sequential operations in *this* phone's hardware. Argon2id in front of it does
//! nothing against that person — they can compute it elsewhere, for all PINs, in advance —
//! and is there for the case where the hardware key has been extracted, when the chain
//! becomes free and only Argon2id and the length of the PIN remain.
//!
//! # Two levels, still
//!
//! The PIN wraps the database key (DEK); the subkeys come from the DEK. Changing the PIN
//! or the hardware key later means re-sealing thirty-two bytes, not re-encrypting the
//! conversations.
//!
//! # What is checked before an attempt is counted
//!
//! The header is parsed and its bounds are checked (a file with `k = 2³²` would otherwise
//! hang the app), and the key check value is compared. A different key in the hardware —
//! reissued by firmware, restored from somewhere — must not look like a wrong PIN: the
//! owner would collect delays forever and never learn that the key is not the one.

use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Instant;

use apeiron_core::{open, purpose, random_bytes, seal, SecretKey};
use apeiron_platform::{HardwareKey, PlatformError, SecurityLevel};
use argon2::{Algorithm, Argon2, Params, Version};
use zeroize::Zeroizing;

use crate::fsutil::{remove_with_tmp, write_atomically};
use crate::pin::{Gate, PinState, PIN_STATE_FILE};
use crate::StorageError;

/// File name of the wrapper.
pub const WRAPPER_FILE: &str = "vault.bin";

/// Identifies the file. Six bytes, so that a stray file does not parse.
const MAGIC: &[u8; 6] = b"APVLT1";

/// Builds before the PIN: the database key wrapped by an AES key in the keystore. Such
/// files are recognised and not opened (see `docs/storage.md`).
const KDF_LEGACY: u8 = 1;

/// Argon2id → HMAC chain in the secure hardware → HKDF.
const KDF_PIN: u8 = 2;

/// Shortest and longest PIN an unlock accepts, in digits. A PIN set by an earlier build
/// (6 to 16 digits) keeps working.
pub const MIN_PIN_DIGITS: usize = 4;
pub const MAX_PIN_DIGITS: usize = 16;

/// The lengths a new PIN may have — the owner's choice (24.09.2026). What each costs someone
/// with root on the phone (Galaxy A24, chain ≈ 0.8 s a guess): 4 digits — minutes, 6 — about
/// half a day, 8 — about a month. Without root the attempt counter and its delays hold all of
/// them; the interface recommends 8.
pub const PIN_LENGTHS: [usize; 3] = [4, 6, 8];

/// `magic (6) ‖ kdf (1) ‖ level (1) ‖ rounds (4) ‖ argon KiB (4) ‖ argon t (4) ‖
/// argon p (1) ‖ salt (16) ‖ key check value (16)`.
const HEADER_BYTES: usize = 6 + 1 + 1 + 4 + 4 + 4 + 1 + 16 + 16;

/// Upper bounds accepted from a header. Anything above is a damaged or planted file.
const MAX_ROUNDS: u32 = 20_000;
const MAX_ARGON_KIB: u32 = 256 * 1024;
const MAX_ARGON_T: u32 = 10;

/// What the chain should cost on this phone. Argon2id comes on top of it.
const TARGET_CHAIN_MS: u64 = 700;

/// Fewest rounds, whatever the measurement says: a cold keystore on the first run can make
/// one operation look slow and the chain short. StrongBox is tens of times slower than a
/// TEE, hence its lower floor.
const MIN_ROUNDS_TEE: u32 = 128;
const MIN_ROUNDS_STRONGBOX: u32 = 4;

/// Input of the key check value: a fixed label, padded to the chain's width.
const KCV_INPUT: [u8; 32] = *b"apeiron/key-check/v1............";

/// One mutex over every "inspect / create / unlock / wipe" sequence.
///
/// Dart calls through a thread pool. Without this, two calls could both find no wrapper
/// and both create a key, the second silently replacing the first; or a lock request
/// could slip in during the second-long chain and the vault would open in the background
/// against R-001.
static OPEN_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub(crate) fn open_lock() -> Result<MutexGuard<'static, ()>, StorageError> {
    OPEN_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| StorageError::Poisoned)
}

/// Cost parameters of the derivation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    pub argon_kib: u32,
    pub argon_t: u32,
    pub argon_p: u8,
    /// Upper bound on the calibrated number of rounds.
    pub max_rounds: u32,
}

impl KdfParams {
    /// What the app uses: 64 MiB, two passes, one lane.
    pub const fn production() -> Self {
        Self {
            argon_kib: 64 * 1024,
            argon_t: 2,
            argon_p: 1,
            max_rounds: MAX_ROUNDS,
        }
    }

    /// Cheap parameters for tests on the development machine. Never used in the app: the
    /// test vault that needs them cannot be built for Android.
    #[cfg(any(test, feature = "testing"))]
    pub const fn fast_for_tests() -> Self {
        Self {
            argon_kib: 64,
            argon_t: 1,
            argon_p: 1,
            max_rounds: 64,
        }
    }
}

/// Parsed header.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Header {
    level: i8,
    rounds: u32,
    params: KdfParams,
    salt: [u8; 16],
    kcv: [u8; 16],
}

impl Header {
    fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_BYTES);
        out.extend_from_slice(MAGIC);
        out.push(KDF_PIN);
        out.push(self.level as u8);
        out.extend_from_slice(&self.rounds.to_be_bytes());
        out.extend_from_slice(&self.params.argon_kib.to_be_bytes());
        out.extend_from_slice(&self.params.argon_t.to_be_bytes());
        out.push(self.params.argon_p);
        out.extend_from_slice(&self.salt);
        out.extend_from_slice(&self.kcv);
        out
    }

    fn parse(raw: &[u8]) -> Result<Self, String> {
        let head = raw
            .get(..HEADER_BYTES)
            .ok_or("wrapper file shorter than its header")?;
        let u32_at = |at: usize| -> Result<u32, String> {
            head.get(at..at + 4)
                .and_then(|b| b.try_into().ok())
                .map(u32::from_be_bytes)
                .ok_or_else(|| "truncated header".to_string())
        };
        let bytes16 = |at: usize| -> Result<[u8; 16], String> {
            head.get(at..at + 16)
                .and_then(|b| b.try_into().ok())
                .ok_or_else(|| "truncated header".to_string())
        };
        let header = Self {
            level: head.get(7).copied().unwrap_or_default() as i8,
            rounds: u32_at(8)?,
            params: KdfParams {
                argon_kib: u32_at(12)?,
                argon_t: u32_at(16)?,
                argon_p: head.get(20).copied().unwrap_or_default(),
                max_rounds: MAX_ROUNDS,
            },
            salt: bytes16(21)?,
            kcv: bytes16(37)?,
        };
        // Bounds before anything is computed: a planted header must not be able to hang
        // the app or make it allocate gigabytes.
        if header.rounds == 0 || header.rounds > MAX_ROUNDS {
            return Err(format!("rounds out of bounds: {}", header.rounds));
        }
        if header.params.argon_kib < 8 || header.params.argon_kib > MAX_ARGON_KIB {
            return Err(format!(
                "Argon2 memory out of bounds: {} KiB",
                header.params.argon_kib
            ));
        }
        if header.params.argon_t == 0 || header.params.argon_t > MAX_ARGON_T {
            return Err(format!(
                "Argon2 passes out of bounds: {}",
                header.params.argon_t
            ));
        }
        if header.params.argon_p != 1 {
            return Err(format!(
                "Argon2 lanes out of bounds: {}",
                header.params.argon_p
            ));
        }
        Ok(header)
    }
}

/// What lies in the data directory.
enum VaultFile {
    /// Nothing yet: first launch.
    Absent,
    /// A wrapper from a build before the PIN.
    Legacy,
    /// A PIN wrapper: header and sealed database key.
    Pin(Header, Vec<u8>),
}

fn inspect(dir: &Path) -> Result<VaultFile, StorageError> {
    let path = dir.join(WRAPPER_FILE);
    if !path.exists() {
        return Ok(VaultFile::Absent);
    }
    let raw = std::fs::read(&path)?;
    if raw.get(..MAGIC.len()) != Some(MAGIC.as_slice()) {
        return Err(StorageError::Wrapper(
            "not an Apeiron wrapper file".to_string(),
        ));
    }
    match raw.get(MAGIC.len()).copied() {
        Some(KDF_LEGACY) => Ok(VaultFile::Legacy),
        Some(KDF_PIN) => {
            let header = Header::parse(&raw).map_err(StorageError::Wrapper)?;
            let sealed = raw
                .get(HEADER_BYTES..)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| StorageError::Wrapper("wrapper without a sealed key".to_string()))?
                .to_vec();
            Ok(VaultFile::Pin(header, sealed))
        }
        other => Err(StorageError::Wrapper(format!(
            "wrapper made by method {other:?}, which this version does not know"
        ))),
    }
}

/// What is in the data directory, without opening anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// First launch: a PIN has to be set.
    Absent,
    /// Data of a build before the PIN. Cannot be opened by this build.
    Legacy,
    /// A PIN vault.
    Pin,
}

/// Looks at the data directory.
pub fn presence(dir: &Path) -> Result<Presence, StorageError> {
    let _guard = open_lock()?;
    Ok(match inspect(dir)? {
        VaultFile::Absent => Presence::Absent,
        VaultFile::Legacy => Presence::Legacy,
        VaultFile::Pin(..) => Presence::Pin,
    })
}

/// Parameters of an existing vault, for the report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VaultParams {
    pub rounds: u32,
    pub argon_kib: u32,
    pub argon_t: u32,
    pub level_at_creation: SecurityLevel,
}

/// Parameters of the PIN vault in `dir`, if there is one.
pub fn params(dir: &Path) -> Result<Option<VaultParams>, StorageError> {
    let _guard = open_lock()?;
    Ok(match inspect(dir)? {
        VaultFile::Pin(h, _) => Some(VaultParams {
            rounds: h.rounds,
            argon_kib: h.params.argon_kib,
            argon_t: h.params.argon_t,
            level_at_creation: SecurityLevel::from_raw(i32::from(h.level)),
        }),
        _ => None,
    })
}

/// How long the derivation took. For the report and for the brute-force estimate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Timing {
    pub argon_ms: u64,
    pub chain_ms: u64,
}

/// The database key and what the system reports about the hardware.
pub struct OpenedVault {
    /// The database key. Root of the subkey hierarchy.
    pub dek: SecretKey,
    /// Hardware level **now**, as the system reports it.
    pub level: SecurityLevel,
    /// Level written when the wrapper was created; authenticated as part of the header.
    pub level_at_creation: SecurityLevel,
    /// Whether the wrapper was created just now.
    pub created_now: bool,
    /// How the hardware key came to be. Empty unless created in this run.
    pub creation_note: String,
    /// How long this derivation took.
    pub timing: Timing,
    /// Rounds of the chain in this vault.
    pub rounds: u32,
}

/// The outcome of an unlock attempt that reached a decision.
pub enum Unlock {
    Opened(OpenedVault),
    /// The PIN is wrong. `failures` in a row so far; `wait_ms` before the next attempt.
    WrongPin {
        failures: u32,
        wait_ms: u64,
    },
    /// Too many failures: wait this long. Nothing was tried.
    Delayed {
        wait_ms: u64,
    },
}

/// Result of measuring the hardware.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Calibration {
    /// Fastest observed time of one HMAC in the hardware, in nanoseconds.
    pub per_op_ns: u64,
    /// Rounds that make the chain cost about [`TARGET_CHAIN_MS`] here.
    pub rounds: u32,
}

/// Measures one hardware operation and picks the number of rounds.
///
/// The first operations are discarded (cold keystore, JIT), then three batches of 64 are
/// timed and the **fastest** is taken: a slow moment would give a short chain, which is
/// the direction that must not happen.
pub fn calibrate<H: HardwareKey>(
    hw: &H,
    level: SecurityLevel,
    max_rounds: u32,
) -> Result<Calibration, StorageError> {
    let probe = random_bytes::<32>()?;
    hw.hmac_chain(&probe, 16)?;
    let mut best = u64::MAX;
    for _ in 0..3 {
        let started = Instant::now();
        hw.hmac_chain(&probe, 64)?;
        let per_op = u64::try_from(started.elapsed().as_nanos() / 64).unwrap_or(u64::MAX);
        best = best.min(per_op.max(1));
    }
    let floor = if level.raw() == SecurityLevel::STRONGBOX {
        MIN_ROUNDS_STRONGBOX
    } else {
        MIN_ROUNDS_TEE
    };
    let wanted = TARGET_CHAIN_MS.saturating_mul(1_000_000) / best;
    let rounds = u32::try_from(wanted)
        .unwrap_or(u32::MAX)
        .clamp(floor.min(max_rounds), max_rounds);
    Ok(Calibration {
        per_op_ns: best,
        rounds,
    })
}

fn check_pin(pin: &[u8]) -> Result<(), StorageError> {
    if (MIN_PIN_DIGITS..=MAX_PIN_DIGITS).contains(&pin.len()) && pin.iter().all(u8::is_ascii_digit)
    {
        Ok(())
    } else {
        Err(StorageError::BadPin)
    }
}

/// A PIN about to be set: digits only, and one of [`PIN_LENGTHS`].
fn check_new_pin(pin: &[u8]) -> Result<(), StorageError> {
    check_pin(pin)?;
    if PIN_LENGTHS.contains(&pin.len()) {
        Ok(())
    } else {
        Err(StorageError::BadPin)
    }
}

fn kcv_of<H: HardwareKey>(hw: &H) -> Result<[u8; 16], StorageError> {
    let out = hw.hmac_chain(&KCV_INPUT, 1)?;
    out.get(..16)
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| StorageError::Wrapper("short key check value".to_string()))
}

/// The wrap key from the PIN: Argon2id, then the hardware chain, then HKDF.
fn derive<H: HardwareKey>(
    hw: &H,
    header: &Header,
    pin: &[u8],
) -> Result<(SecretKey, Timing), StorageError> {
    let started = Instant::now();
    let params = Params::new(
        header.params.argon_kib,
        header.params.argon_t,
        u32::from(header.params.argon_p),
        Some(32),
    )
    .map_err(|e| StorageError::Wrapper(format!("Argon2 parameters: {e}")))?;
    let mut x0 = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(pin, &header.salt, x0.as_mut())
        .map_err(|e| StorageError::Wrapper(format!("Argon2: {e}")))?;
    let argon_ms = elapsed_ms(started);

    let started = Instant::now();
    let xk = hw.hmac_chain(&x0, header.rounds)?;
    let chain_ms = elapsed_ms(started);

    let w = SecretKey::from_bytes(*xk).derive(purpose::PIN_WRAP);
    Ok((w, Timing { argon_ms, chain_ms }))
}

fn elapsed_ms(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// The level fits in a signed byte: there are five values, from -2 to 2.
fn clamp_level(raw: i32) -> i8 {
    i8::try_from(raw).unwrap_or(SecurityLevel::UNKNOWN as i8)
}

/// Creates the vault with a new PIN. Allowed only when there is no wrapper file.
pub fn create<H: HardwareKey>(
    dir: &Path,
    hw: &H,
    pin: &[u8],
    params: KdfParams,
    writer: [u8; 8],
) -> Result<OpenedVault, StorageError> {
    let _guard = open_lock()?;
    check_new_pin(pin)?;
    if !matches!(inspect(dir)?, VaultFile::Absent) {
        return Err(StorageError::Wrapper(
            "a vault already exists here".to_string(),
        ));
    }

    // Creating the key is allowed exactly here: there is no wrapper, so nothing can be lost.
    let status = hw.ensure_key(true)?;
    let calibration = calibrate(hw, status.level, params.max_rounds)?;
    let header = Header {
        level: clamp_level(status.level.raw()),
        rounds: calibration.rounds,
        params,
        salt: random_bytes::<16>()?,
        kcv: kcv_of(hw)?,
    };
    let (w, timing) = derive(hw, &header, pin)?;
    let dek = Zeroizing::new(random_bytes::<32>()?);
    let head = header.to_bytes();
    let mut file = head.clone();
    file.extend_from_slice(&seal(&w, &head, dek.as_ref())?);

    // The counter first: a wrapper without a counter would read as "counter lost" and
    // start with a delay; a counter without a wrapper is simply overwritten next time.
    PinState {
        writer,
        ..PinState::default()
    }
    .save(dir)?;
    let path = dir.join(WRAPPER_FILE);
    write_atomically(dir, &path, &file)?;

    // Read back from disk and open it again before anything is sealed with this key. A
    // chain that is not reproducible would otherwise lock the owner out of everything
    // written from now on. Nothing is lost by removing the file here: it was just made.
    if let Err(e) = verify_from_disk(dir, hw, pin, &dek) {
        let _ = remove_with_tmp(&path);
        return Err(e);
    }

    Ok(OpenedVault {
        dek: SecretKey::from_bytes(*dek),
        level: status.level,
        level_at_creation: status.level,
        created_now: true,
        creation_note: status.note,
        timing,
        rounds: header.rounds,
    })
}

fn verify_from_disk<H: HardwareKey>(
    dir: &Path,
    hw: &H,
    pin: &[u8],
    dek: &[u8; 32],
) -> Result<(), StorageError> {
    let VaultFile::Pin(header, sealed) = inspect(dir)? else {
        return Err(StorageError::Wrapper(
            "the new wrapper did not read back".to_string(),
        ));
    };
    let (w, _) = derive(hw, &header, pin)?;
    let back = Zeroizing::new(open(&w, &header.to_bytes(), &sealed)?);
    if back.as_slice() == dek.as_slice() {
        Ok(())
    } else {
        Err(StorageError::Wrapper(
            "the new wrapper opened to a different key".to_string(),
        ))
    }
}

/// Tries to open the vault with `pin`.
///
/// Order, and why: header and bounds → key present → key check value → delay gate →
/// **count the attempt and flush** → derive → open. Only a hardware failure before the
/// chain produced its result puts the counter back.
pub fn unlock<H: HardwareKey>(
    dir: &Path,
    hw: &H,
    pin: &[u8],
    writer: [u8; 8],
) -> Result<Unlock, StorageError> {
    let _guard = open_lock()?;
    Ok(match attempt(dir, hw, pin, writer)? {
        Attempt::Opened { vault, .. } => Unlock::Opened(vault),
        Attempt::Refused(refused) => refused,
    })
}

/// Changes the PIN: opens with `current` — counted like any attempt — and seals the same
/// database key under `new`. The conversations are not touched; thirty-two bytes are
/// re-sealed, under a fresh salt.
///
/// The new file replaces the old one atomically and is read back and opened with `new`
/// before this returns; if that fails, the old file is put back. A PIN that does not open
/// what it just sealed must not be the only way in.
pub fn change_pin<H: HardwareKey>(
    dir: &Path,
    hw: &H,
    current: &[u8],
    new: &[u8],
    writer: [u8; 8],
) -> Result<Unlock, StorageError> {
    let _guard = open_lock()?;
    check_new_pin(new)?;
    let (vault, dek, header) = match attempt(dir, hw, current, writer)? {
        Attempt::Opened { vault, dek, header } => (vault, dek, header),
        Attempt::Refused(refused) => return Ok(refused),
    };
    let path = dir.join(WRAPPER_FILE);
    let old = std::fs::read(&path)?;
    let header = Header {
        salt: random_bytes::<16>()?,
        ..header
    };
    let (w, _) = derive(hw, &header, new)?;
    let head = header.to_bytes();
    let mut file = head.clone();
    file.extend_from_slice(&seal(&w, &head, dek.as_ref())?);
    write_atomically(dir, &path, &file)?;
    if let Err(e) = verify_from_disk(dir, hw, new, &dek) {
        write_atomically(dir, &path, &old)?;
        return Err(e);
    }
    Ok(Unlock::Opened(vault))
}

enum Attempt {
    Opened {
        vault: OpenedVault,
        dek: Zeroizing<[u8; 32]>,
        header: Header,
    },
    Refused(Unlock),
}

/// The body of [`unlock`], under the caller's lock.
fn attempt<H: HardwareKey>(
    dir: &Path,
    hw: &H,
    pin: &[u8],
    writer: [u8; 8],
) -> Result<Attempt, StorageError> {
    let (header, sealed) = match inspect(dir)? {
        VaultFile::Pin(h, s) => (h, s),
        VaultFile::Absent => return Err(StorageError::NoVault),
        VaultFile::Legacy => return Err(StorageError::Legacy),
    };
    // A PIN of impossible length is not a guess at any PIN this vault could have.
    check_pin(pin)?;

    // `allow_create = false` is the whole difference between "first launch" and "key gone".
    let status = hw.ensure_key(false)?;

    // Computed twice before concluding: HMAC is deterministic, but a single glitch must
    // not be enough to declare the key foreign.
    if kcv_of(hw)? != header.kcv && kcv_of(hw)? != header.kcv {
        return Err(StorageError::KeyMismatch);
    }

    let now = hw.boot_clock()?;
    let state = PinState::load(dir);
    if let Gate::Wait {
        remaining_ms,
        reanchored,
    } = state.gate(now)
    {
        if let Some(anchored) = reanchored {
            anchored.save(dir)?;
        }
        return Ok(Attempt::Refused(Unlock::Delayed {
            wait_ms: remaining_ms,
        }));
    }

    let counted = state.begin_attempt(now, writer);
    counted.save(dir)?;

    let (w, timing) = match derive(hw, &header, pin) {
        Ok(v) => v,
        Err(e) => {
            // No result from the chain means no verdict: the attempt did not happen.
            PinState::restore_after_transient(&state).save(dir)?;
            return Err(e);
        }
    };

    let head = header.to_bytes();
    match open(&w, &head, &sealed) {
        Ok(plain) => {
            let plain = Zeroizing::new(plain);
            let dek: Zeroizing<[u8; 32]> =
                Zeroizing::new(plain.as_slice().try_into().map_err(|_| {
                    StorageError::Wrapper("sealed key of wrong length".to_string())
                })?);
            // A failure to reset the counter must not keep the owner out: the verdict was
            // "correct". The worst outcome is a count that is too high.
            let _ = counted.after_success(writer).save(dir);
            Ok(Attempt::Opened {
                vault: OpenedVault {
                    dek: SecretKey::from_bytes(*dek),
                    level: status.level,
                    level_at_creation: SecurityLevel::from_raw(i32::from(header.level)),
                    created_now: false,
                    creation_note: status.note,
                    timing,
                    rounds: header.rounds,
                },
                dek,
                header,
            })
        }
        Err(_) => {
            let wait_ms = match counted.gate(now) {
                Gate::Open => 0,
                Gate::Wait { remaining_ms, .. } => remaining_ms,
            };
            Ok(Attempt::Refused(Unlock::WrongPin {
                failures: counted.consecutive,
                wait_ms,
            }))
        }
    }
}

/// Self-check: a random candidate is rejected, and how long one derivation takes.
///
/// The candidate is born here and never leaves, and the counter is not touched: this is
/// not an oracle anyone can feed. Returns `(rejected, timing)`. The chance that a random
/// 12-digit candidate is the real PIN is 10⁻¹², and a PIN is at most 16 digits.
pub fn random_candidate_rejected<H: HardwareKey>(
    dir: &Path,
    hw: &H,
) -> Result<(bool, Timing), StorageError> {
    let _guard = open_lock()?;
    let VaultFile::Pin(header, sealed) = inspect(dir)? else {
        return Err(StorageError::NoVault);
    };
    let mut candidate = Zeroizing::new(Vec::with_capacity(12));
    for b in random_bytes::<12>()? {
        candidate.push(b'0' + b % 10);
    }
    let (w, timing) = derive(hw, &header, &candidate)?;
    Ok((open(&w, &header.to_bytes(), &sealed).is_err(), timing))
}

/// Removes the wrapper and the counter. The hardware key is destroyed by the caller first
/// — the order matters, see [`crate::Storage::wipe`].
pub(crate) fn remove(dir: &Path) -> Result<(), StorageError> {
    remove_with_tmp(&dir.join(WRAPPER_FILE))?;
    remove_with_tmp(&dir.join(PIN_STATE_FILE))?;
    // The temporary name of builds before the PIN.
    let legacy_tmp = dir.join(WRAPPER_FILE).with_extension("tmp");
    if legacy_tmp.exists() {
        std::fs::remove_file(&legacy_tmp)?;
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

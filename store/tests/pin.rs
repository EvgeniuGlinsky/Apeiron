//! The PIN (R-011) against a test hardware key.
//!
//! On the phone only the real keystore is checked; everything about order, counting and
//! refusals is decided here. For every "it opens" there are several "it cannot be fooled".

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::path::Path;

use apeiron_core::Identity;
use apeiron_platform::{HardwareKey, SecurityLevel};
use apeiron_store::pin::{PinState, FREE_ATTEMPTS, PIN_STATE_FILE};
use apeiron_store::testing::{Behaviour, Call, TestVault};
use apeiron_store::wrapper::{self, presence, WRAPPER_FILE};
use apeiron_store::{KdfParams, Opening, Presence, Storage, StorageError};

const PIN: &[u8] = b"24681357";
const WRONG: &[u8] = b"13572468";
const MARK: [u8; 8] = [3; 8];

fn temp() -> tempfile::TempDir {
    tempfile::tempdir().expect("the directory is created")
}

fn create(dir: &Path, vault: &TestVault) -> Storage {
    Storage::create(dir, vault, PIN, KdfParams::fast_for_tests(), MARK).expect("created")
}

fn attempt(dir: &Path, vault: &TestVault, pin: &[u8]) -> Result<Opening, StorageError> {
    Storage::unlock(dir, vault, pin, MARK)
}

fn consecutive(dir: &Path) -> u32 {
    PinState::load(dir).consecutive
}

fn wrong_pin(outcome: Result<Opening, StorageError>) -> (u32, u64) {
    match outcome {
        Ok(Opening::WrongPin { failures, wait_ms }) => (failures, wait_ms),
        Ok(Opening::Opened(_)) => panic!("a wrong PIN opened the vault"),
        Ok(Opening::Delayed { wait_ms }) => panic!("delayed by {wait_ms} ms instead of a verdict"),
        Err(e) => panic!("an error instead of a verdict: {e}"),
    }
}

fn delayed(outcome: Result<Opening, StorageError>) -> u64 {
    match outcome {
        Ok(Opening::Delayed { wait_ms }) => wait_ms,
        Ok(Opening::Opened(_)) => panic!("opened during a delay"),
        Ok(Opening::WrongPin { .. }) => panic!("the PIN was checked during a delay"),
        Err(e) => panic!("an error instead of a delay: {e}"),
    }
}

fn opens(outcome: Result<Opening, StorageError>) -> Storage {
    match outcome {
        Ok(Opening::Opened(s)) => *s,
        Ok(Opening::WrongPin { .. }) => panic!("the right PIN was refused"),
        Ok(Opening::Delayed { wait_ms }) => panic!("delayed by {wait_ms} ms"),
        Err(e) => panic!("the right PIN did not open: {e}"),
    }
}

/// Bytes 6..10 of the counter file: failures in a row.
fn consecutive_in(snapshot: &[u8]) -> u32 {
    u32::from_be_bytes(snapshot[6..10].try_into().unwrap())
}

// ─── It opens ────────────────────────────────────────────────────────────────

#[test]
fn the_right_pin_opens_and_the_data_is_there() {
    let dir = temp();
    let vault = TestVault::empty();
    let identity = Identity::generate().unwrap();
    {
        let store = create(dir.path(), &vault);
        assert!(store.created_now());
        store.save_identity(&identity).unwrap();
    }
    let store = opens(attempt(dir.path(), &vault, PIN));
    assert!(!store.created_now());
    let back = store
        .load_identity()
        .unwrap()
        .expect("the identity survived");
    assert_eq!(back.public().fingerprint(), identity.public().fingerprint());
}

#[test]
fn a_wrong_pin_is_refused_and_a_right_one_resets_the_count() {
    let dir = temp();
    let vault = TestVault::empty();
    create(dir.path(), &vault);

    assert_eq!(wrong_pin(attempt(dir.path(), &vault, WRONG)), (1, 0));
    assert_eq!(wrong_pin(attempt(dir.path(), &vault, WRONG)), (2, 0));
    assert_eq!(consecutive(dir.path()), 2);

    opens(attempt(dir.path(), &vault, PIN));
    assert_eq!(consecutive(dir.path()), 0);
    assert_eq!(
        PinState::load(dir.path()).total,
        2,
        "the total keeps the failures"
    );
}

// ─── It counts before it knows ───────────────────────────────────────────────

/// Cutting power at the moment the verdict is known must not leave the attempt uncounted.
/// The test key records what the counter file held when the hardware was asked.
#[test]
fn the_attempt_is_on_disk_before_the_hardware_is_asked() {
    let dir = temp();
    let vault = TestVault::empty();
    create(dir.path(), &vault);
    vault.watch_file(dir.path().join(PIN_STATE_FILE));

    wrong_pin(attempt(dir.path(), &vault, WRONG));
    opens(attempt(dir.path(), &vault, PIN));

    let snaps = vault.snapshots();
    assert_eq!(snaps.len(), 2, "one PIN chain per attempt");
    let first = snaps[0].as_ref().expect("the counter existed");
    let second = snaps[1].as_ref().expect("the counter existed");
    assert_eq!(
        consecutive_in(first),
        1,
        "the wrong attempt was not counted in advance"
    );
    assert_eq!(
        consecutive_in(second),
        2,
        "the right attempt was not counted in advance"
    );
}

#[test]
fn a_transient_failure_in_the_chain_costs_no_attempt() {
    let dir = temp();
    let vault = TestVault::empty();
    create(dir.path(), &vault);

    // In an unlock the chain calls go: key check value, then the PIN chain.
    for behaviour in [Behaviour::Transient, Behaviour::Internal] {
        vault.fail_chain_call(1, behaviour);
        let err = attempt(dir.path(), &vault, PIN)
            .err()
            .expect("it opened although the hardware failed");
        assert!(
            !matches!(err, StorageError::KeyGone),
            "{behaviour:?} became key loss"
        );
        assert!(err.is_retryable());
        assert_eq!(consecutive(dir.path()), 0, "{behaviour:?} cost an attempt");
    }
    opens(attempt(dir.path(), &vault, PIN));
}

#[test]
fn a_key_gone_mid_chain_is_key_gone_and_costs_no_attempt() {
    let dir = temp();
    let vault = TestVault::empty();
    create(dir.path(), &vault);
    vault.fail_chain_call(1, Behaviour::Gone);
    let err = attempt(dir.path(), &vault, PIN).err().expect("it opened");
    assert!(matches!(err, StorageError::KeyGone));
    assert_eq!(consecutive(dir.path()), 0);
}

// ─── It is never fooled into destroying data ─────────────────────────────────

#[test]
fn a_missing_key_is_key_gone_and_no_new_key_is_made() {
    let dir = temp();
    let vault = TestVault::empty();
    create(dir.path(), &vault);
    vault.forget_key();

    let err = attempt(dir.path(), &vault, PIN)
        .err()
        .expect("it opened without a key");
    assert!(matches!(err, StorageError::KeyGone), "got {err}");
    assert!(!vault.has_key(), "a new key was created over existing data");
    let creations = vault
        .log()
        .iter()
        .filter(|c| **c == Call::EnsureKey { allow_create: true })
        .count();
    assert_eq!(
        creations, 1,
        "a key may be created only once, at the very start"
    );
    assert_eq!(consecutive(dir.path()), 0);
}

/// A reissued key must not look like a wrong PIN: the owner would collect delays for a
/// PIN that is right, and never learn that the key is not the one.
#[test]
fn a_different_key_is_not_a_wrong_pin() {
    let dir = temp();
    let vault = TestVault::empty();
    create(dir.path(), &vault);
    vault.replace_key();

    let err = attempt(dir.path(), &vault, PIN)
        .err()
        .expect("a foreign key opened it");
    assert!(matches!(err, StorageError::KeyMismatch), "got {err}");
    assert!(!err.is_retryable());
    assert_eq!(consecutive(dir.path()), 0, "a foreign key cost an attempt");
}

// ─── Delays ──────────────────────────────────────────────────────────────────

#[test]
fn five_attempts_are_free_then_the_delays_start() {
    let dir = temp();
    let vault = TestVault::empty();
    create(dir.path(), &vault);

    for n in 1..=FREE_ATTEMPTS {
        assert_eq!(wrong_pin(attempt(dir.path(), &vault, WRONG)), (n, 0));
    }
    assert_eq!(wrong_pin(attempt(dir.path(), &vault, WRONG)), (6, 30_000));

    // Even the right PIN has to wait, and it is not checked while it waits.
    assert_eq!(delayed(attempt(dir.path(), &vault, PIN)), 30_000);
    vault.advance_ms(29_999);
    assert_eq!(delayed(attempt(dir.path(), &vault, PIN)), 1);
    vault.advance_ms(1);
    opens(attempt(dir.path(), &vault, PIN));
}

#[test]
fn a_delayed_attempt_does_not_reach_the_hardware() {
    let dir = temp();
    let vault = TestVault::empty();
    create(dir.path(), &vault);
    for _ in 0..6 {
        wrong_pin(attempt(dir.path(), &vault, WRONG));
    }
    let chains_before = vault
        .log()
        .iter()
        .filter(|c| matches!(c, Call::Chain { rounds } if *rounds > 1))
        .count();
    delayed(attempt(dir.path(), &vault, PIN));
    let chains_after = vault
        .log()
        .iter()
        .filter(|c| matches!(c, Call::Chain { rounds } if *rounds > 1))
        .count();
    assert_eq!(
        chains_before, chains_after,
        "a guess was computed during a delay"
    );
}

#[test]
fn a_reboot_restarts_the_delay_in_full() {
    let dir = temp();
    let vault = TestVault::empty();
    create(dir.path(), &vault);
    for _ in 0..6 {
        wrong_pin(attempt(dir.path(), &vault, WRONG));
    }
    vault.advance_ms(20_000);
    vault.reboot();
    assert_eq!(delayed(attempt(dir.path(), &vault, PIN)), 30_000);
    vault.advance_ms(30_000);
    opens(attempt(dir.path(), &vault, PIN));
}

#[test]
fn a_lost_counter_starts_with_a_delay() {
    let dir = temp();
    let vault = TestVault::empty();
    create(dir.path(), &vault);
    std::fs::remove_file(dir.path().join(PIN_STATE_FILE)).unwrap();
    assert_eq!(delayed(attempt(dir.path(), &vault, PIN)), 30_000);
}

// ─── Refusals before anything is counted ─────────────────────────────────────

#[test]
fn an_impossible_pin_is_refused_and_not_counted() {
    let dir = temp();
    let vault = TestVault::empty();
    create(dir.path(), &vault);
    for pin in [&b"12345"[..], b"12345678901234567", b"1234567a", b""] {
        let err = attempt(dir.path(), &vault, pin)
            .err()
            .expect("an impossible PIN was tried");
        assert!(matches!(err, StorageError::BadPin), "{pin:?}: {err}");
    }
    assert_eq!(consecutive(dir.path()), 0);
}

#[test]
fn a_short_pin_cannot_be_set() {
    let dir = temp();
    let vault = TestVault::empty();
    let err = Storage::create(
        dir.path(),
        &vault,
        b"12345",
        KdfParams::fast_for_tests(),
        MARK,
    )
    .expect_err("a five-digit PIN was accepted");
    assert!(matches!(err, StorageError::BadPin));
    assert_eq!(presence(dir.path()).unwrap(), Presence::Absent);
}

/// A header asking for 2³² rounds or gigabytes of Argon2 memory would hang the app.
#[test]
fn a_planted_header_is_refused_before_anything_is_computed() {
    for (at, value) in [(8usize, u32::MAX), (8, 0), (12, u32::MAX), (16, 1_000)] {
        let dir = temp();
        let vault = TestVault::empty();
        create(dir.path(), &vault);
        let path = dir.path().join(WRAPPER_FILE);
        let mut raw = std::fs::read(&path).unwrap();
        raw[at..at + 4].copy_from_slice(&value.to_be_bytes());
        std::fs::write(&path, &raw).unwrap();

        let err = attempt(dir.path(), &vault, PIN)
            .err()
            .expect("a planted header was used");
        assert!(matches!(err, StorageError::Wrapper(_)), "byte {at}: {err}");
        assert_eq!(consecutive(dir.path()), 0);
    }
}

#[test]
fn a_vault_is_never_created_over_an_existing_one() {
    let dir = temp();
    let vault = TestVault::empty();
    create(dir.path(), &vault);
    let before = std::fs::read(dir.path().join(WRAPPER_FILE)).unwrap();
    assert!(Storage::create(dir.path(), &vault, PIN, KdfParams::fast_for_tests(), MARK).is_err());
    assert_eq!(
        std::fs::read(dir.path().join(WRAPPER_FILE)).unwrap(),
        before
    );
}

// ─── Builds before the PIN ───────────────────────────────────────────────────

#[test]
fn legacy_data_is_recognised_and_cleared_only_on_request() {
    let dir = temp();
    let vault = TestVault::empty();
    let mut legacy = b"APVLT1".to_vec();
    legacy.extend_from_slice(&[1, 1]);
    legacy.extend_from_slice(&[0x5a; 40]);
    std::fs::write(dir.path().join(WRAPPER_FILE), &legacy).unwrap();

    assert_eq!(presence(dir.path()).unwrap(), Presence::Legacy);
    let err = attempt(dir.path(), &vault, PIN)
        .err()
        .expect("legacy data opened");
    assert!(matches!(err, StorageError::Legacy));
    assert!(!err.is_retryable());
    assert!(
        Storage::create(dir.path(), &vault, PIN, KdfParams::fast_for_tests(), MARK).is_err(),
        "a new vault was made over legacy data without the owner's say"
    );
    assert!(
        dir.path().join(WRAPPER_FILE).exists(),
        "legacy data was removed on its own"
    );

    Storage::wipe(dir.path(), &vault).unwrap();
    assert_eq!(presence(dir.path()).unwrap(), Presence::Absent);
    create(dir.path(), &vault);
}

// ─── Erasure and self-check helpers ──────────────────────────────────────────

#[test]
fn wipe_destroys_the_key_and_the_counter() {
    let dir = temp();
    let vault = TestVault::empty();
    create(dir.path(), &vault);
    wrong_pin(attempt(dir.path(), &vault, WRONG));
    Storage::wipe(dir.path(), &vault).unwrap();
    assert!(!vault.has_key());
    assert!(!dir.path().join(PIN_STATE_FILE).exists());
    assert!(!dir.path().join(WRAPPER_FILE).exists());
    assert_eq!(presence(dir.path()).unwrap(), Presence::Absent);
}

#[test]
fn a_random_candidate_is_rejected_without_counting() {
    let dir = temp();
    let vault = TestVault::empty();
    create(dir.path(), &vault);
    let (rejected, _) = wrapper::random_candidate_rejected(dir.path(), &vault).unwrap();
    assert!(rejected);
    assert_eq!(consecutive(dir.path()), 0);
    assert_eq!(PinState::load(dir.path()).total, 0);
}

#[test]
fn calibration_stays_within_bounds() {
    let vault = TestVault::empty();
    vault.ensure_key(true).expect("the fake creates its key");
    let tee = SecurityLevel::from_raw(SecurityLevel::TRUSTED_ENVIRONMENT);
    let small = wrapper::calibrate(&vault, tee, 64).unwrap();
    assert_eq!(small.rounds, 64, "the fast fake must hit the ceiling");
    let big = wrapper::calibrate(&vault, tee, 20_000).unwrap();
    assert!(
        (128..=20_000).contains(&big.rounds),
        "rounds {}",
        big.rounds
    );
    assert!(big.per_op_ns > 0);
}

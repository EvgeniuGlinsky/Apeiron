//! The whole storage, against a test hardware key.
//!
//! There is only one on-device check, and everything that can be found out here must be
//! found out here. What is left for the phone is exactly what the development machine
//! lacks: the real Keystore.
//!
//! The project convention holds: for every "it works" test there are several
//! "it cannot be fooled" ones.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::path::Path;

use apeiron_core::vodozemac::olm::{Account, OlmMessage};
use apeiron_core::{Chat, Identity, PrekeyBundle, Sigchain};
use apeiron_store::testing::{Behaviour, TestVault};
use apeiron_store::wrapper::{presence, WRAPPER_FILE};
use apeiron_store::{KdfParams, Opening, Presence, Storage, StorageError, DATABASE_FILE};

const PIN: &[u8] = b"24681357";
const MARK: [u8; 8] = [1; 8];

fn temp() -> tempfile::TempDir {
    tempfile::tempdir().expect("the directory is created")
}

/// Creates the storage with [`PIN`] on first use, unlocks it afterwards.
fn open(dir: &Path, vault: &TestVault) -> Storage {
    match presence(dir).expect("the directory is readable") {
        Presence::Absent => Storage::create(dir, vault, PIN, KdfParams::fast_for_tests(), MARK)
            .expect("the storage is created"),
        _ => match Storage::unlock(dir, vault, PIN, MARK).expect("the storage unlocks") {
            Opening::Opened(store) => *store,
            _ => panic!("the right PIN was refused"),
        },
    }
}

/// Unlocks with [`PIN`] and reports whether the vault actually opened.
fn opened(dir: &Path, vault: &TestVault) -> bool {
    matches!(
        Storage::unlock(dir, vault, PIN, MARK),
        Ok(Opening::Opened(_))
    )
}

/// A ready, verified prekey bundle.
fn bundle(identity: &Identity, account: &mut Account) -> PrekeyBundle {
    let bytes = PrekeyBundle::create(identity, account)
        .expect("the bundle is assembled")
        .to_bytes();
    PrekeyBundle::parse(&bytes)
        .expect("the bundle parses")
        .verify()
        .expect("the signature is valid")
}

// ─── What it was all for ─────────────────────────────────────────────────────

#[test]
fn identity_survives_reopening() {
    let dir = temp();
    let vault = TestVault::empty();

    let original = Identity::generate().expect("the OS provides randomness");
    let fingerprint = original.public().fingerprint();
    {
        let store = open(dir.path(), &vault);
        assert!(store.created_now(), "the first launch must be the first");
        assert!(store.load_identity().unwrap().is_none());
        store.save_identity(&original).unwrap();
    }

    let store = open(dir.path(), &vault);
    assert!(!store.created_now(), "the second launch posed as the first");
    let restored = store.load_identity().unwrap().expect("identity present");
    assert_eq!(
        restored.public().fingerprint(),
        fingerprint,
        "after the restart the fingerprint changed, so the identity is a different one"
    );
}

#[test]
fn device_account_survives_reopening() {
    let dir = temp();
    let vault = TestVault::empty();

    let account = Account::new();
    let device_key = account.identity_keys().curve25519;
    {
        let store = open(dir.path(), &vault);
        store.save_account(&account).unwrap();
    }

    let store = open(dir.path(), &vault);
    let restored = store.load_account().unwrap().expect("account present");
    assert_eq!(
        restored.identity_keys().curve25519,
        device_key,
        "after the restart the device key changed: all conversations are broken"
    );
}

/// The main property: a message encrypted before the database was closed is read after
/// it is opened again.
#[test]
fn a_conversation_survives_reopening() {
    let dir = temp();
    let vault = TestVault::empty();

    let alice = Identity::generate().unwrap();
    let mut alice_account = Account::new();
    let bob = Identity::generate().unwrap();
    let mut bob_account = Account::new();

    let bob_bundle = bundle(&bob, &mut bob_account);
    let alice_bundle = bundle(&alice, &mut alice_account);

    let mut chat = Chat::initiate(&alice_account, &bob_bundle).unwrap();
    let first = chat.encrypt("before restart").unwrap();

    {
        let store = open(dir.path(), &vault);
        let contact = store.save_contact(&bob.public(), "Bob").unwrap();
        store
            .save_chat_and_account(contact, &chat, &alice_account)
            .unwrap();
    }
    drop(chat);

    let store = open(dir.path(), &vault);
    let contact = store
        .find_contact(&bob.public())
        .unwrap()
        .expect("contact present");
    assert_eq!(contact.name, "Bob");
    let chats = store.load_chats(contact.id).unwrap();
    assert_eq!(chats.len(), 1, "the conversation was lost");
    assert_eq!(
        chats[0].peer().to_bytes(),
        bob.public().to_bytes(),
        "after loading the peer became someone else"
    );

    // Bob reads what Alice encrypted before the restart.
    let pre_key = match &first {
        OlmMessage::PreKey(m) => m.clone(),
        OlmMessage::Normal(_) => panic!("the first message must be a pre-key one"),
    };
    let (_, text) = Chat::accept(&mut bob_account, &alice_bundle, &pre_key).unwrap();
    assert_eq!(text, "before restart");
}

#[test]
fn sigchain_survives_reopening_and_is_reverified() {
    let dir = temp();
    let vault = TestVault::empty();

    let root = Identity::generate().unwrap();
    let chain = Sigchain::create(&root).unwrap();
    let length = chain.len();
    {
        let store = open(dir.path(), &vault);
        store.save_sigchain(&chain).unwrap();
    }

    let store = open(dir.path(), &vault);
    let restored = store.load_sigchain().unwrap().expect("sigchain present");
    assert_eq!(restored.len(), length);
    restored.verify().expect("the sigchain verifies");
}

#[test]
fn meta_survives_reopening() {
    let dir = temp();
    let vault = TestVault::empty();
    {
        let store = open(dir.path(), &vault);
        assert!(store.meta_get("first launch").unwrap().is_none());
        store.meta_set("first launch", b"1730000000").unwrap();
    }
    let store = open(dir.path(), &vault);
    assert_eq!(
        store.meta_get("first launch").unwrap().as_deref(),
        Some(&b"1730000000"[..])
    );
}

// ─── It cannot be fooled ─────────────────────────────────────────────────────

/// The most valuable property in the whole storage.
///
/// The wrapper is on disk, but the key is not in the secure module. This is what
/// regularly happens in the field: a firmware update, removal of the screen lock,
/// restoring data from a backup without the keys. Creating a new key here would mean
/// destroying the owner's conversations irrecoverably, so instead of "first
/// launch" what must come is "key gone".
#[test]
fn a_missing_key_with_the_wrapper_present_is_never_a_fresh_start() {
    let dir = temp();
    let vault = TestVault::empty();
    {
        let store = open(dir.path(), &vault);
        store.save_identity(&Identity::generate().unwrap()).unwrap();
    }
    assert!(dir.path().join(WRAPPER_FILE).exists());

    vault.forget_key();

    let err = Storage::unlock(dir.path(), &vault, PIN, MARK)
        .err()
        .expect("it opened, though there is no key");
    assert!(
        matches!(err, StorageError::KeyGone),
        "instead of \"key gone\" got: {err}"
    );
    assert!(!err.is_retryable());
    assert!(
        !vault.has_key(),
        "with the key missing a new one was created: the conversations are destroyed"
    );
}

/// A transient failure has no right to turn into "key gone".
///
/// There is nothing else between this test and destruction of the owner's conversations.
/// `setUnlockedDeviceRequired` fails on an unlocked device if
/// it was unlocked with weak biometrics: a confirmed firmware defect.
#[test]
fn transient_failure_never_maps_to_gone() {
    let dir = temp();
    let vault = TestVault::empty();
    {
        let store = open(dir.path(), &vault);
        store.save_identity(&Identity::generate().unwrap()).unwrap();
    }

    for behaviour in [Behaviour::Transient, Behaviour::Internal] {
        vault.set_behaviour(behaviour);
        let err = Storage::unlock(dir.path(), &vault, PIN, MARK)
            .err()
            .expect("it opened despite a failure");
        assert!(
            !matches!(err, StorageError::KeyGone),
            "{behaviour:?} passed off as key loss: {err}"
        );
        assert!(
            err.is_retryable(),
            "{behaviour:?} declared unrecoverable: {err}"
        );
    }

    // And the data is intact after that.
    vault.set_behaviour(Behaviour::Normal);
    let store = open(dir.path(), &vault);
    assert!(store.load_identity().unwrap().is_some());
}

#[test]
fn a_flipped_byte_in_the_wrapper_is_rejected() {
    let dir = temp();
    let vault = TestVault::empty();
    {
        let _ = open(dir.path(), &vault);
    }

    let path = dir.path().join(WRAPPER_FILE);
    let mut raw = std::fs::read(&path).unwrap();
    let last = raw.len() - 1;
    raw[last] ^= 0xff;
    std::fs::write(&path, &raw).unwrap();

    // A damaged sealed part cannot be told from a wrong PIN — both fail authentication —
    // and it must not be taken for key loss either: the data is intact.
    match Storage::unlock(dir.path(), &vault, PIN, MARK) {
        Ok(Opening::Opened(_)) => panic!("a substituted wrapper passed"),
        Err(StorageError::KeyGone) => {
            panic!("file corruption passed off as key loss, while the data is intact")
        }
        _ => {}
    }
}

#[test]
fn a_tampered_wrapper_header_is_rejected() {
    let dir = temp();
    let vault = TestVault::empty();
    {
        let _ = open(dir.path(), &vault);
    }

    // The protection level byte lies in the clear: the eighth one. By tweaking it an adversary
    // would change the label on the screen without touching anything else. The whole header
    // is authenticated data of the sealed key, and here this is caught.
    let path = dir.path().join(WRAPPER_FILE);
    let mut raw = std::fs::read(&path).unwrap();
    assert_eq!(raw[7], 1, "the fake declares TEE");
    raw[7] = 2;
    std::fs::write(&path, &raw).unwrap();

    assert!(
        !opened(dir.path(), &vault),
        "a tweaked protection level passed as the real one"
    );
}

#[test]
fn a_foreign_file_is_not_taken_for_a_wrapper() {
    let dir = temp();
    let vault = TestVault::empty();
    std::fs::write(dir.path().join(WRAPPER_FILE), b"not apeiron at all").unwrap();
    assert!(Storage::unlock(dir.path(), &vault, PIN, MARK).is_err());
}

/// The list of peers must not be readable from the database file without the key.
///
/// If the public key were stored in the clear (for convenient lookup), the file itself
/// would tell whom the person corresponds with. That is exactly what the database was meant
/// to close off, so lookup goes by an opaque tag.
#[test]
fn nothing_secret_is_readable_from_the_database_file() {
    let dir = temp();
    let vault = TestVault::empty();

    let me = Identity::generate().unwrap();
    let secret = me.export_secret();
    let peer = Identity::generate().unwrap();
    let peer_public = peer.public().to_bytes();

    {
        let store = open(dir.path(), &vault);
        store.save_identity(&me).unwrap();
        store.save_contact(&peer.public(), "Notable name").unwrap();
        store.meta_set("note", "secret".as_bytes()).unwrap();
    }

    let mut blob = std::fs::read(dir.path().join(DATABASE_FILE)).unwrap();
    for suffix in ["-wal", "-shm"] {
        let extra = dir.path().join(format!("{DATABASE_FILE}{suffix}"));
        if extra.exists() {
            blob.extend_from_slice(&std::fs::read(&extra).unwrap());
        }
    }

    assert!(
        !contains(&blob, secret.as_bytes()),
        "the identity secret lies in the database file in the clear"
    );
    assert!(
        !contains(&blob, &peer_public),
        "the peer's public key lies in the database file in the clear"
    );
    assert!(
        !contains(&blob, "Notable name".as_bytes()),
        "the peer's name lies in the database file in the clear"
    );
    assert!(
        !contains(&blob, "secret".as_bytes()),
        "an internal value lies in the database file in the clear"
    );
    // Control: the search itself works, and "nothing found" is not because it cannot
    // search.
    assert!(contains(&blob, b"SQLite format 3"));
}

/// After erasure the old ciphertext cannot be read by anything, even if a copy was
/// taken. This is R-005: the key is destroyed, not the data.
#[test]
fn wipe_makes_the_old_ciphertext_unreadable() {
    let dir = temp();
    let vault = TestVault::empty();
    {
        let store = open(dir.path(), &vault);
        store.save_identity(&Identity::generate().unwrap()).unwrap();
    }

    // The copy is taken before erasure, as an adversary would take it.
    let copy = std::fs::read(dir.path().join(DATABASE_FILE)).unwrap();
    let wrapper_copy = std::fs::read(dir.path().join(WRAPPER_FILE)).unwrap();

    Storage::wipe(dir.path(), &vault).unwrap();
    assert!(!vault.has_key(), "the key survived erasure");
    assert!(!dir.path().join(WRAPPER_FILE).exists());
    assert!(!dir.path().join(DATABASE_FILE).exists());

    // Put the copy back in place: without the key it is useless.
    std::fs::write(dir.path().join(DATABASE_FILE), &copy).unwrap();
    std::fs::write(dir.path().join(WRAPPER_FILE), &wrapper_copy).unwrap();
    let err = Storage::unlock(dir.path(), &vault, PIN, MARK)
        .err()
        .expect("the copy opened after erasure");
    assert!(matches!(err, StorageError::KeyGone));
}

/// After erasure one can start over, and it is a different identity.
#[test]
fn a_fresh_start_after_wipe_is_a_different_identity() {
    let dir = temp();
    let vault = TestVault::empty();

    let first = Identity::generate().unwrap();
    {
        let store = open(dir.path(), &vault);
        store.save_identity(&first).unwrap();
    }

    Storage::wipe(dir.path(), &vault).unwrap();

    let store = open(dir.path(), &vault);
    assert!(store.created_now());
    assert!(
        store.load_identity().unwrap().is_none(),
        "after erasure the previous identity was found"
    );
}

#[test]
fn a_database_from_a_newer_schema_is_refused() {
    let dir = temp();
    let vault = TestVault::empty();
    {
        let _ = open(dir.path(), &vault);
    }

    let conn = rusqlite::Connection::open(dir.path().join(DATABASE_FILE)).unwrap();
    conn.execute("UPDATE schema_version SET version = 99 WHERE id = 1", [])
        .unwrap();
    drop(conn);

    let err = Storage::unlock(dir.path(), &vault, PIN, MARK)
        .err()
        .expect("a newer database was read");
    assert!(
        matches!(
            err,
            StorageError::SchemaTooNew {
                found: 99,
                known: _
            }
        ),
        "instead of a schema version refusal got: {err}"
    );
}

#[test]
fn a_record_moved_to_another_row_is_rejected() {
    let dir = temp();
    let vault = TestVault::empty();

    let first = Identity::generate().unwrap();
    let second = Identity::generate().unwrap();
    {
        let store = open(dir.path(), &vault);
        store.save_contact(&first.public(), "first").unwrap();
        store.save_contact(&second.public(), "second").unwrap();
    }

    // Swap the sealed content of two rows without touching the tags.
    let conn = rusqlite::Connection::open(dir.path().join(DATABASE_FILE)).unwrap();
    let a: Vec<u8> = conn
        .query_row("SELECT sealed FROM contacts WHERE id = 1", [], |r| r.get(0))
        .unwrap();
    let b: Vec<u8> = conn
        .query_row("SELECT sealed FROM contacts WHERE id = 2", [], |r| r.get(0))
        .unwrap();
    conn.execute(
        "UPDATE contacts SET sealed = ?1 WHERE id = 1",
        rusqlite::params![b],
    )
    .unwrap();
    conn.execute(
        "UPDATE contacts SET sealed = ?1 WHERE id = 2",
        rusqlite::params![a],
    )
    .unwrap();
    drop(conn);

    let store = open(dir.path(), &vault);
    assert!(
        store.contacts().is_err(),
        "swapped records were read as their own"
    );
}

// ─── Self-check ──────────────────────────────────────────────────────────────

/// On the first launch the self-check must say "nothing to check", not
/// report success.
///
/// A green line where nothing was checked is exactly the false
/// confidence the self-check was undertaken against.
#[test]
fn the_self_check_does_not_claim_success_on_the_first_run() {
    let dir = temp();
    let vault = TestVault::empty();
    let store = open(dir.path(), &vault);
    store.save_identity(&Identity::generate().unwrap()).unwrap();
    store.save_account(&Account::new()).unwrap();

    let checks = apeiron_store::selfcheck::run_with_mark(&store, "process-A");
    let survived = find(&checks, "conversation readable after the restart");
    assert!(
        !survived.passed,
        "on the first launch the state was declared to have survived a restart"
    );
    assert!(survived.detail.contains("Force stop"));
}

/// The most important property of the self-check: it does not turn green without a real
/// restart.
///
/// Opening the check screen a second time in the same session is not a restart. Without this
/// protection everything would turn green without proving anything, and a green mark where
/// nothing was checked is more harmful than no check at all: people rely on it.
#[test]
fn the_self_check_stays_red_within_the_same_process() {
    let dir = temp();
    let vault = TestVault::empty();
    let store = open(dir.path(), &vault);
    store.save_identity(&Identity::generate().unwrap()).unwrap();
    store.save_account(&Account::new()).unwrap();

    let _ = apeiron_store::selfcheck::run_with_mark(&store, "process-A");
    // The same process, a second run: the state will match, but it cannot be counted.
    let checks = apeiron_store::selfcheck::run_with_mark(&store, "process-A");

    assert!(!find(&checks, "the probe was planted by another process").passed);
    for name in [
        "identity survived the restart",
        "device account is the same",
        "conversation readable after the restart",
    ] {
        let check = find(&checks, name);
        assert!(
            !check.passed,
            "\"{name}\" counted without a restart: {}",
            check.detail
        );
        assert!(check.detail.contains("this same process"));
    }
}

/// And on the second one it must pass in full.
#[test]
fn the_self_check_passes_after_a_real_reopen() {
    let dir = temp();
    let vault = TestVault::empty();
    {
        let store = open(dir.path(), &vault);
        store.save_identity(&Identity::generate().unwrap()).unwrap();
        store.save_account(&Account::new()).unwrap();
        let _ = apeiron_store::selfcheck::run_with_mark(&store, "process-A");
    }

    // A different mark = a different process, i.e. a real restart.
    let store = open(dir.path(), &vault);
    let checks = apeiron_store::selfcheck::run_with_mark(&store, "process-B");
    let failed: Vec<_> = checks
        .iter()
        .filter(|c| !c.passed)
        .map(|c| format!("{}: {}", c.name, c.detail))
        .collect();
    assert!(
        failed.is_empty(),
        "after a real restart these did not pass: {failed:#?}"
    );
    assert_eq!(
        find(&checks, "check runs").detail,
        "2",
        "runs are not counted"
    );
}

/// The proof of a restart is not lost by running the check again in the new process.
///
/// On the phone the check screen runs the self-check more than once per launch, and the
/// report that gets sent is the last one. Before this was fixed, the second run saw the
/// mark written by the first and turned red although the restart had been proven.
#[test]
fn the_proof_of_a_restart_survives_a_second_run_in_the_new_process() {
    let dir = temp();
    let vault = TestVault::empty();
    {
        let store = open(dir.path(), &vault);
        store.save_identity(&Identity::generate().unwrap()).unwrap();
        store.save_account(&Account::new()).unwrap();
        let _ = apeiron_store::selfcheck::run_with_mark(&store, "process-A");
    }

    let store = open(dir.path(), &vault);
    for run in 1..=3 {
        let checks = apeiron_store::selfcheck::run_with_mark(&store, "process-B");
        let failed: Vec<_> = checks
            .iter()
            .filter(|c| !c.passed)
            .map(|c| format!("{}: {}", c.name, c.detail))
            .collect();
        assert!(
            failed.is_empty(),
            "run {run} in the new process lost the proof: {failed:#?}"
        );
    }

    // And a third process still sees the probe the first one planted.
    let checks = apeiron_store::selfcheck::run_with_mark(&store, "process-C");
    assert!(checks.iter().all(|c| c.passed));
}

/// A probe planted anew inside a process is that process's own, whatever was proven
/// before in it.
///
/// Guards against keeping "restart proven" in memory: after the storage is reset within
/// the same process, such a memory would outlive the probe it was about and turn the new
/// probe, planted by this very process, green.
#[test]
fn a_probe_planted_anew_in_the_same_process_is_not_counted() {
    let vault = TestVault::empty();
    let old = temp();
    {
        let store = open(old.path(), &vault);
        store.save_identity(&Identity::generate().unwrap()).unwrap();
        store.save_account(&Account::new()).unwrap();
        let _ = apeiron_store::selfcheck::run_with_mark(&store, "process-A");
        let proven = apeiron_store::selfcheck::run_with_mark(&store, "process-B");
        assert!(find(&proven, "the probe was planted by another process").passed);
    }

    // The same process B starts over with an empty storage.
    let reset = temp();
    let vault = TestVault::empty();
    let store = open(reset.path(), &vault);
    store.save_identity(&Identity::generate().unwrap()).unwrap();
    store.save_account(&Account::new()).unwrap();
    for _ in 0..2 {
        let checks = apeiron_store::selfcheck::run_with_mark(&store, "process-B");
        assert!(!find(&checks, "the probe was planted by another process").passed);
        assert!(!find(&checks, "conversation readable after the restart").passed);
    }
}

/// The self-check has no right to touch production data.
#[test]
fn the_self_check_touches_nothing_it_checks() {
    let dir = temp();
    let vault = TestVault::empty();
    let me = Identity::generate().unwrap();
    let peer = Identity::generate().unwrap();

    let store = open(dir.path(), &vault);
    store.save_identity(&me).unwrap();
    store.save_account(&Account::new()).unwrap();
    let contact = store.save_contact(&peer.public(), "Peer").unwrap();

    let _ = apeiron_store::selfcheck::run_with_mark(&store, "process-A");
    let _ = apeiron_store::selfcheck::run_with_mark(&store, "process-B");

    assert_eq!(
        store
            .load_identity()
            .unwrap()
            .unwrap()
            .public()
            .fingerprint(),
        me.public().fingerprint()
    );
    let found = store.find_contact(&peer.public()).unwrap().unwrap();
    assert_eq!(found.id, contact);
    assert_eq!(found.name, "Peer");
    assert!(vault.has_key(), "the self-check destroyed the key");
}

fn find<'a>(
    checks: &'a [apeiron_store::selfcheck::Check],
    name: &str,
) -> &'a apeiron_store::selfcheck::Check {
    checks
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("the report has no line \"{name}\""))
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

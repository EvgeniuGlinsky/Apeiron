//! Schema v2: the migration from v1 on real records, and the new tables
//! (`docs/transport.md` §9). The rule of `docs/storage.md`: a migration brings the test that
//! the data survived it.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::path::Path;

use apeiron_core::vodozemac::olm::Account;
use apeiron_core::{Chat, Identity, PrekeyBundle, SecretKey, Sigchain};
use apeiron_store::testing::TestVault;
use apeiron_store::wrapper::presence;
use apeiron_store::{KdfParams, Opening, Presence, Storage, StorageError, DATABASE_FILE};

const PIN: &[u8] = b"24681357";
const MARK: [u8; 8] = [2; 8];

fn open(dir: &Path, vault: &TestVault) -> Storage {
    match presence(dir).unwrap() {
        Presence::Absent => {
            Storage::create(dir, vault, PIN, KdfParams::fast_for_tests(), MARK).unwrap()
        }
        _ => match Storage::unlock(dir, vault, PIN, MARK).unwrap() {
            Opening::Opened(store) => *store,
            _ => panic!("the right PIN was refused"),
        },
    }
}

fn chat_with(peer: &Identity) -> Chat {
    let mut peer_account = Account::new();
    let bundle = PrekeyBundle::parse(
        &PrekeyBundle::create(peer, &mut peer_account)
            .unwrap()
            .to_bytes(),
    )
    .unwrap()
    .verify()
    .unwrap();
    Chat::initiate(&Account::new(), &bundle).unwrap()
}

fn raw(dir: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open(dir.join(DATABASE_FILE)).unwrap()
}

/// Turns a database written by this code into exactly what schema v1 left on the phones:
/// the tables of v2 gone, version 1. The records themselves are byte-for-byte what v1 wrote
/// (`record::a_record_sealed_by_schema_v1_still_opens`).
fn downgrade_to_v1(dir: &Path) {
    raw(dir)
        .execute_batch(
            "DROP TABLE messages; DROP TABLE pair_state; DROP TABLE outbox;
             DROP TABLE invitations; UPDATE schema_version SET version = 1 WHERE id = 1;",
        )
        .unwrap();
}

fn version(dir: &Path) -> u16 {
    raw(dir)
        .query_row("SELECT version FROM schema_version WHERE id = 1", [], |r| {
            r.get(0)
        })
        .unwrap()
}

#[test]
fn every_record_of_v1_survives_the_migration() {
    let dir = tempfile::tempdir().unwrap();
    let vault = TestVault::empty();
    let me = Identity::generate().unwrap();
    let peer = Identity::generate().unwrap();
    let account = Account::new();
    let chain = Sigchain::create(&me).unwrap();
    let chat = chat_with(&peer);

    let contact = {
        let store = open(dir.path(), &vault);
        store.save_identity(&me).unwrap();
        store.save_account(&account).unwrap();
        store.save_sigchain(&chain).unwrap();
        let contact = store.save_contact(&peer.public(), "Peer").unwrap();
        store.save_chat(contact, &chat).unwrap();
        store.meta_set("first launch", b"yes").unwrap();
        contact
    };
    downgrade_to_v1(dir.path());
    assert_eq!(version(dir.path()), 1);

    let store = open(dir.path(), &vault);
    assert_eq!(version(dir.path()), 2, "not migrated");
    assert_eq!(
        store
            .load_identity()
            .unwrap()
            .unwrap()
            .public()
            .fingerprint(),
        me.public().fingerprint()
    );
    assert_eq!(
        store.load_account().unwrap().unwrap().curve25519_key(),
        account.curve25519_key()
    );
    assert_eq!(store.load_sigchain().unwrap().unwrap().len(), chain.len());
    let found = store.find_contact(&peer.public()).unwrap().unwrap();
    assert_eq!((found.id, found.name.as_str()), (contact, "Peer"));
    assert_eq!(
        store.load_chats(contact).unwrap()[0].session_id(),
        chat.session_id()
    );
    assert_eq!(
        store.meta_get("first launch").unwrap().unwrap(),
        b"yes".to_vec()
    );

    // The self-check says so, as it will on the phone after the update.
    let checks = apeiron_store::selfcheck::run_with_mark(&store, "after the migration");
    let schema = checks
        .iter()
        .find(|c| c.name == "database schema")
        .expect("a line about the schema");
    assert!(schema.passed, "{}", schema.detail);

    // And the new tables work at once.
    let ids = store
        .commit_conversation(contact, &chat, b"pair", &[b"first".to_vec()], &[])
        .unwrap();
    assert_eq!(ids.len(), 1);
    assert_eq!(&*store.messages(contact, None, 10).unwrap()[0].1, b"first");
}

/// The round of a conversation is all or nothing: a failure in the middle leaves no half.
#[test]
fn a_conversation_commit_is_all_or_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let vault = TestVault::empty();
    let store = open(dir.path(), &vault);
    let peer = Identity::generate().unwrap();
    let contact = store.save_contact(&peer.public(), "Peer").unwrap();
    let chat = chat_with(&peer);

    // Changing a message that does not exist fails — after the session, the pair state and a
    // new message have already been written inside the transaction.
    let result = store.commit_conversation(
        contact,
        &chat,
        b"pair state",
        &[b"new".to_vec()],
        &[(999, b"changed".to_vec())],
    );
    assert!(matches!(result, Err(StorageError::NotFound(_))));
    assert!(store.load_pair_state(contact).unwrap().is_none());
    assert!(store.messages(contact, None, 10).unwrap().is_empty());
    assert!(store.load_chats(contact).unwrap().is_empty());
}

#[test]
fn history_comes_a_page_at_a_time_newest_first() {
    let dir = tempfile::tempdir().unwrap();
    let vault = TestVault::empty();
    let store = open(dir.path(), &vault);
    let peer = Identity::generate().unwrap();
    let contact = store.save_contact(&peer.public(), "Peer").unwrap();
    let chat = chat_with(&peer);

    let texts: Vec<Vec<u8>> = (0..5).map(|i| format!("m{i}").into_bytes()).collect();
    let ids = store
        .commit_conversation(contact, &chat, b"p", &texts, &[])
        .unwrap();

    let page = store.messages(contact, None, 2).unwrap();
    assert_eq!(
        page.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        vec![ids[4], ids[3]]
    );
    let older = store.messages(contact, Some(ids[3]), 2).unwrap();
    assert_eq!(&*older[0].1, b"m2");
    assert_eq!(&*older[1].1, b"m1");

    // A message changed in place keeps its place.
    store
        .commit_conversation(contact, &chat, b"p", &[], &[(ids[0], b"m0, read".to_vec())])
        .unwrap();
    let all = store.messages(contact, None, 10).unwrap();
    assert_eq!(&*all[4].1, b"m0, read");
}

/// A record moved to another row does not open there.
#[test]
fn messages_are_bound_to_their_place() {
    let dir = tempfile::tempdir().unwrap();
    let vault = TestVault::empty();
    let store = open(dir.path(), &vault);
    let peer = Identity::generate().unwrap();
    let contact = store.save_contact(&peer.public(), "Peer").unwrap();
    let ids = store
        .commit_conversation(
            contact,
            &chat_with(&peer),
            b"p",
            &[b"a".to_vec(), b"b".to_vec()],
            &[],
        )
        .unwrap();
    let conn = raw(dir.path());
    let first: Vec<u8> = conn
        .query_row("SELECT sealed FROM messages WHERE id = ?1", [ids[0]], |r| {
            r.get(0)
        })
        .unwrap();
    conn.execute(
        "UPDATE messages SET sealed = ?1 WHERE id = ?2",
        rusqlite::params![first, ids[1]],
    )
    .unwrap();
    drop(conn);
    assert!(store.messages(contact, None, 10).is_err());
}

/// The outbox opens with the background key and nothing else, and says nothing about whom
/// its items are for.
#[test]
fn the_outbox_is_for_the_background_key_only() {
    let dir = tempfile::tempdir().unwrap();
    let vault = TestVault::empty();
    let store = open(dir.path(), &vault);
    let background = SecretKey::generate().unwrap();
    let items = vec![vec![1u8; 1004], vec![2u8; 1004]];

    store.replace_outbox(&background, &items).unwrap();
    assert_eq!(store.outbox(&background).unwrap(), items);
    assert!(store.outbox(&SecretKey::generate().unwrap()).is_err());

    store.replace_outbox(&background, &items[1..]).unwrap();
    assert_eq!(store.outbox(&background).unwrap(), items[1..].to_vec());

    let columns: Vec<String> = {
        let conn = raw(dir.path());
        let mut stmt = conn
            .prepare("SELECT name FROM pragma_table_info('outbox')")
            .unwrap();
        let names = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        names
    };
    assert_eq!(columns, vec!["id".to_string(), "sealed".to_string()]);
}

/// An introduction is one transaction: the contact, its session, pair state, first message and
/// the account whose one-time key was spent.
#[test]
fn an_introduction_is_stored_whole() {
    let dir = tempfile::tempdir().unwrap();
    let vault = TestVault::empty();
    let store = open(dir.path(), &vault);
    let peer = Identity::generate().unwrap();
    let account = Account::new();
    let chat = chat_with(&peer);

    let (contact, ids) = store
        .introduce(
            &peer.public(),
            "Peer",
            &chat,
            &account,
            b"pair",
            &[b"hello".to_vec()],
        )
        .unwrap();
    assert_eq!(
        store.find_contact(&peer.public()).unwrap().unwrap().id,
        contact
    );
    assert_eq!(
        store.load_chats(contact).unwrap()[0].session_id(),
        chat.session_id()
    );
    assert_eq!(&*store.load_pair_state(contact).unwrap().unwrap(), b"pair");
    assert_eq!(store.messages(contact, None, 10).unwrap()[0].0, ids[0]);
    assert_eq!(
        store.load_account().unwrap().unwrap().curve25519_key(),
        account.curve25519_key()
    );
}

#[test]
fn invitations_are_kept_until_forgotten() {
    let dir = tempfile::tempdir().unwrap();
    let vault = TestVault::empty();
    let store = open(dir.path(), &vault);
    let a = store.save_invitation(b"invitation a").unwrap();
    let b = store.save_invitation(b"invitation b").unwrap();
    let all = store.invitations().unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(&*all[0].1, b"invitation a");
    store.delete_invitation(a).unwrap();
    let left = store.invitations().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].0, b);
}

/// Nothing written into the new tables lies in the file in the clear.
#[test]
fn nothing_new_is_in_the_file_in_the_clear() {
    let dir = tempfile::tempdir().unwrap();
    let vault = TestVault::empty();
    let text = "секретный текст сообщения".as_bytes();
    let pair = b"PAIR-STATE-MARKER-0123456789";
    let invitation = b"INVITATION-MARKER-0123456789";
    {
        let store = open(dir.path(), &vault);
        let peer = Identity::generate().unwrap();
        let contact = store.save_contact(&peer.public(), "Peer").unwrap();
        store
            .commit_conversation(contact, &chat_with(&peer), pair, &[text.to_vec()], &[])
            .unwrap();
        store.save_invitation(invitation).unwrap();
        store
            .replace_outbox(
                &SecretKey::generate().unwrap(),
                &[b"OUTBOX-MARKER-01".to_vec()],
            )
            .unwrap();
    }
    let mut bytes = std::fs::read(dir.path().join(DATABASE_FILE)).unwrap();
    // The write-ahead log counts too: that is where fresh writes are.
    if let Ok(wal) = std::fs::read(dir.path().join(format!("{DATABASE_FILE}-wal"))) {
        bytes.extend(wal);
    }
    for needle in [
        text,
        pair.as_slice(),
        invitation.as_slice(),
        b"OUTBOX-MARKER-01".as_slice(),
    ] {
        assert!(
            !bytes.windows(needle.len()).any(|w| w == needle),
            "found in the clear: {}",
            String::from_utf8_lossy(needle)
        );
    }
}

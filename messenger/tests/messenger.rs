//! Two people, two databases, one fake DHT: the service end to end (`docs/transport.md` §4–§8).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::cell::Cell;

use apeiron_core::vodozemac::olm::Account;
use apeiron_core::Identity;
use apeiron_messenger::{
    accept_invitation, contacts, create_invitation, history, invitations, poll_invitations,
    set_read_receipts, verification, ContactStatus, Conversation, MessageKind, MessengerError,
    Status, StoreAccess, Update,
};
use apeiron_store::testing::TestVault;
use apeiron_store::{KdfParams, Opening, Storage};
use apeiron_transport::dht::FakeDht;

/// 24.09.2026, 12:00 UTC.
const T0: u64 = 1_790_251_200;
const PIN: &[u8] = b"13572468";

struct Person {
    dir: tempfile::TempDir,
    vault: TestVault,
    store: Storage,
    id: Identity,
}

fn person() -> Person {
    let dir = tempfile::tempdir().unwrap();
    let vault = TestVault::empty();
    let store =
        Storage::create(dir.path(), &vault, PIN, KdfParams::fast_for_tests(), [3; 8]).unwrap();
    let id = Identity::generate().unwrap();
    store.save_identity(&id).unwrap();
    store.save_account(&Account::new()).unwrap();
    Person {
        dir,
        vault,
        store,
        id,
    }
}

impl Person {
    /// Closes the database and opens it again, as a restart does.
    fn reopen(&mut self) {
        match Storage::unlock(self.dir.path(), &self.vault, PIN, [4; 8]).unwrap() {
            Opening::Opened(store) => self.store = *store,
            _ => panic!("the right PIN was refused"),
        }
    }

    fn conversation(&self, contact: i64) -> Conversation {
        Conversation::load(&self.store, &self.id, contact).unwrap()
    }

    fn only_contact(&self) -> i64 {
        let all = contacts(&self.store, &self.id).unwrap();
        assert_eq!(all.len(), 1);
        all[0].contact.id
    }

    fn round(&self, contact: i64, dht: &FakeDht, now: u64) -> Vec<Update> {
        self.conversation(contact)
            .round(&self.store, dht, now)
            .unwrap()
    }

    fn history(&self, contact: i64) -> Vec<apeiron_messenger::MessageRecord> {
        let mut page: Vec<_> = history(&self.store, contact, None, 100)
            .unwrap()
            .into_iter()
            .map(|(_, m)| m)
            .collect();
        page.reverse();
        page
    }

    fn status_of(&self, contact: i64, text: &str) -> Status {
        match self
            .history(contact)
            .into_iter()
            .find(|m| m.text.as_str() == text)
            .expect("the message is in the history")
            .kind
        {
            MessageKind::Mine(s) => s,
            other => panic!("not mine: {other:?}"),
        }
    }
}

/// A introduces B and they talk; the history on both sides says what happened.
fn introduce(a: &Person, b: &Person, dht: &FakeDht) -> (i64, i64) {
    let invitation = create_invitation(&a.store, &a.id, "Боб", T0).unwrap();
    let b_side =
        accept_invitation(&b.store, &b.id, &invitation.text, "Алиса", "привет", T0).unwrap();
    b.round(b_side, dht, T0 + 1);
    let updates = poll_invitations(&a.store, &a.id, dht, T0 + 2).unwrap();
    assert!(updates.contains(&Update::InvitationGone {
        invitation: invitation.id
    }));
    (a.only_contact(), b_side)
}

#[test]
fn an_introduction_and_a_conversation() {
    let dht = FakeDht::new();
    let a = person();
    let b = person();
    let (a_side, b_side) = introduce(&a, &b, &dht);

    // A has the contact under the name it gave, with B's first text, and no invitation left.
    let seen = contacts(&a.store, &a.id).unwrap();
    assert_eq!(seen[0].contact.name, "Боб");
    assert_eq!(seen[0].status, ContactStatus::Live);
    assert!(!seen[0].contact.verified);
    assert_eq!(a.history(a_side)[0].text.as_str(), "привет");
    assert!(invitations(&a.store).unwrap().is_empty());

    // B waits until A's first state; then the first text is delivered.
    assert_eq!(
        contacts(&b.store, &b.id).unwrap()[0].status,
        ContactStatus::Waiting
    );
    a.round(a_side, &dht, T0 + 3);
    b.round(b_side, &dht, T0 + 4);
    assert_eq!(
        contacts(&b.store, &b.id).unwrap()[0].status,
        ContactStatus::Live
    );
    assert_eq!(b.status_of(b_side, "привет"), Status::Delivered);

    // A message: queued, then on the network, then delivered.
    a.conversation(a_side)
        .send(&a.store, "как дела?", T0 + 5)
        .unwrap();
    assert_eq!(a.status_of(a_side, "как дела?"), Status::Queued);
    a.round(a_side, &dht, T0 + 6);
    assert_eq!(a.status_of(a_side, "как дела?"), Status::Sent);
    b.round(b_side, &dht, T0 + 7);
    a.round(a_side, &dht, T0 + 8);
    assert_eq!(a.status_of(a_side, "как дела?"), Status::Delivered);
    let got: Vec<String> = b
        .history(b_side)
        .iter()
        .filter(|m| m.kind == MessageKind::Theirs)
        .map(|m| m.text.to_string())
        .collect();
    assert_eq!(got, vec!["как дела?".to_string()]);

    assert!(matches!(
        a.conversation(a_side).send(&a.store, "  ", T0 + 9),
        Err(MessengerError::Empty)
    ));
}

/// A store whose `n`-th access fails, as a crash or a locked vault would.
struct FailingAt<'a> {
    store: &'a Storage,
    n: Cell<usize>,
}

impl StoreAccess for FailingAt<'_> {
    fn with<R>(
        &self,
        f: impl FnOnce(&Storage) -> Result<R, MessengerError>,
    ) -> Result<R, MessengerError> {
        let n = self.n.get();
        self.n.set(n.wrapping_sub(1));
        if n == 1 {
            return Err(MessengerError::Locked);
        }
        f(self.store)
    }
}

/// §6: nothing that acknowledges is put before what it acknowledges is stored. A round whose
/// first commit fails puts nothing; loaded again, the message is received exactly once.
#[test]
fn nothing_is_acknowledged_before_it_is_stored() {
    let dht = FakeDht::new();
    let a = person();
    let b = person();
    let (a_side, b_side) = introduce(&a, &b, &dht);
    a.round(a_side, &dht, T0 + 3);
    b.round(b_side, &dht, T0 + 4);

    a.conversation(a_side)
        .send(&a.store, "важное", T0 + 5)
        .unwrap();
    a.round(a_side, &dht, T0 + 6);

    let failing = FailingAt {
        store: &b.store,
        n: Cell::new(1),
    };
    let mut broken = b.conversation(b_side);
    assert!(broken.round(&failing, &dht, T0 + 7).is_err());
    a.round(a_side, &dht, T0 + 8);
    assert_eq!(
        a.status_of(a_side, "важное"),
        Status::Sent,
        "acknowledged what B never stored"
    );
    // The broken owner refuses to go on; a fresh one does the work, once.
    assert!(matches!(
        broken.send(&b.store, "x", T0 + 9),
        Err(MessengerError::Reload)
    ));
    b.round(b_side, &dht, T0 + 10);
    a.round(a_side, &dht, T0 + 11);
    assert_eq!(a.status_of(a_side, "важное"), Status::Delivered);
    let received = b
        .history(b_side)
        .iter()
        .filter(|m| m.text.as_str() == "важное")
        .count();
    assert_eq!(received, 1);
}

/// Two owners of one conversation: the second writes while the first holds an older copy. The
/// first is refused instead of writing its old session over the new one.
#[test]
fn a_second_owner_is_caught() {
    let dht = FakeDht::new();
    let a = person();
    let b = person();
    let (a_side, b_side) = introduce(&a, &b, &dht);
    let mut first = a.conversation(a_side);
    let mut second = a.conversation(a_side);
    second.send(&a.store, "из второго", T0 + 3).unwrap();
    assert!(matches!(
        first.send(&a.store, "из первого", T0 + 4),
        Err(MessengerError::Reload)
    ));
    a.conversation(a_side)
        .send(&a.store, "из первого", T0 + 5)
        .unwrap();
    for t in 6..10 {
        a.round(a_side, &dht, T0 + t);
        b.round(b_side, &dht, T0 + t);
    }
    let got: Vec<String> = b
        .history(b_side)
        .iter()
        .filter(|m| m.kind == MessageKind::Theirs)
        .map(|m| m.text.to_string())
        .collect();
    assert_eq!(
        got,
        vec!["из второго".to_string(), "из первого".to_string()]
    );
}

/// The invitation's one-time key is stored with it: a restart between inviting and the reply
/// changes nothing.
#[test]
fn an_invitation_survives_a_restart() {
    let dht = FakeDht::new();
    let mut a = person();
    let b = person();
    let invitation = create_invitation(&a.store, &a.id, "B", T0).unwrap();
    a.reopen();
    let b_side = accept_invitation(&b.store, &b.id, &invitation.text, "A", "", T0).unwrap();
    b.round(b_side, &dht, T0 + 1);
    poll_invitations(&a.store, &a.id, &dht, T0 + 2).unwrap();
    assert_eq!(contacts(&a.store, &a.id).unwrap().len(), 1);
    assert!(
        a.history(a.only_contact()).is_empty(),
        "an empty first text is no message"
    );
}

#[test]
fn accepting_the_same_invitation_twice_is_refused() {
    let a = person();
    let b = person();
    let invitation = create_invitation(&a.store, &a.id, "B", T0).unwrap();
    accept_invitation(&b.store, &b.id, &invitation.text, "A", "hi", T0).unwrap();
    assert!(matches!(
        accept_invitation(&b.store, &b.id, &invitation.text, "A", "hi", T0),
        Err(MessengerError::AlreadyContact)
    ));
}

/// Both invite each other and both accept. The lower identity's invitation wins on both sides,
/// even when that side's session has become live before the other reply is opened.
#[test]
fn crossed_invitations_meet_on_one_session() {
    let dht = FakeDht::new();
    let (low, high) = {
        let (x, y) = (person(), person());
        if x.id.public().to_bytes() < y.id.public().to_bytes() {
            (x, y)
        } else {
            (y, x)
        }
    };
    let from_low = create_invitation(&low.store, &low.id, "high", T0).unwrap();
    let from_high = create_invitation(&high.store, &high.id, "low", T0).unwrap();
    let low_side = accept_invitation(&low.store, &low.id, &from_high.text, "high", "", T0).unwrap();
    let high_side =
        accept_invitation(&high.store, &high.id, &from_low.text, "low", "", T0).unwrap();
    low.round(low_side, &dht, T0 + 1);
    high.round(high_side, &dht, T0 + 1);

    // The lower side opens the reply to its invitation first: it wins, so it switches.
    poll_invitations(&low.store, &low.id, &dht, T0 + 2).unwrap();
    low.round(low_side, &dht, T0 + 3);
    // The higher side's session (from accepting the lower's invitation) is the winner, and has
    // just become live; the reply to its own invitation must not replace it.
    high.round(high_side, &dht, T0 + 4);
    poll_invitations(&high.store, &high.id, &dht, T0 + 5).unwrap();
    assert!(invitations(&high.store).unwrap().is_empty());

    let low_contact = low.only_contact();
    let high_contact = high.only_contact();
    low.conversation(low_contact)
        .send(&low.store, "до тебя", T0 + 6)
        .unwrap();
    high.conversation(high_contact)
        .send(&high.store, "и до тебя", T0 + 6)
        .unwrap();
    for t in 7..11 {
        low.round(low_contact, &dht, T0 + t);
        high.round(high_contact, &dht, T0 + t);
    }
    assert_eq!(low.status_of(low_contact, "до тебя"), Status::Delivered);
    assert_eq!(high.status_of(high_contact, "и до тебя"), Status::Delivered);
    same_number_on_both_sides(&low, low_contact, &high, high_contact);
}

/// Both sides see one safety number, and each holds the other's real fingerprint: what the
/// verification screen shows, through the path it takes.
fn same_number_on_both_sides(a: &Person, a_side: i64, b: &Person, b_side: i64) {
    let on_a = verification(&a.store, &a.id, a_side).unwrap();
    let on_b = verification(&b.store, &b.id, b_side).unwrap();
    assert_eq!(on_a.safety_number, on_b.safety_number);
    assert_eq!(on_a.their_fingerprint, on_b.my_fingerprint);
    assert_eq!(on_b.their_fingerprint, on_a.my_fingerprint);
    assert_ne!(on_a.safety_number, on_a.my_fingerprint);
}

#[test]
fn the_safety_number_is_one_on_both_sides() {
    let dht = FakeDht::new();
    let a = person();
    let b = person();
    let (a_side, b_side) = introduce(&a, &b, &dht);
    same_number_on_both_sides(&a, a_side, &b, b_side);
}

/// The list counts the peer's entries after what was shown, and shows the newest; the chat,
/// once shown, clears the count.
#[test]
fn unread_counts_until_the_chat_is_shown() {
    let dht = FakeDht::new();
    let a = person();
    let b = person();
    let (a_side, b_side) = introduce(&a, &b, &dht);
    a.round(a_side, &dht, T0 + 3);
    b.round(b_side, &dht, T0 + 4);
    for text in ["раз", "два"] {
        b.conversation(b_side).send(&b.store, text, T0 + 5).unwrap();
    }
    b.round(b_side, &dht, T0 + 6);
    a.round(a_side, &dht, T0 + 7);

    let seen = &contacts(&a.store, &a.id).unwrap()[0];
    assert_eq!(seen.unread, 3, "the first text and two more");
    let (newest, last) = seen.last.as_ref().unwrap();
    assert_eq!(last.text.as_str(), "два");
    assert!(a.conversation(a_side).mark_read(&a.store, *newest).unwrap());
    assert_eq!(contacts(&a.store, &a.id).unwrap()[0].unread, 0);
}

/// B is shown A's message and A sees it read — only while both send receipts: off on either
/// side, it stays delivered.
#[test]
fn a_read_receipt_reaches_the_sender_only_when_both_send_them() {
    let dht = FakeDht::new();
    let a = person();
    let b = person();
    let (a_side, b_side) = introduce(&a, &b, &dht);
    a.round(a_side, &dht, T0 + 3);
    b.round(b_side, &dht, T0 + 4);
    let read_by_b = |text: &str, t: u64| {
        a.conversation(a_side).send(&a.store, text, t).unwrap();
        a.round(a_side, &dht, t + 1);
        b.round(b_side, &dht, t + 2);
        let newest = history(&b.store, b_side, None, 1).unwrap()[0].0;
        b.conversation(b_side).mark_read(&b.store, newest).unwrap();
        b.round(b_side, &dht, t + 3);
        a.round(a_side, &dht, t + 4);
        a.status_of(a_side, text)
    };
    assert_eq!(read_by_b("прочти", T0 + 10), Status::Read);
    set_read_receipts(&b.store, false).unwrap();
    assert_eq!(read_by_b("и это", T0 + 20), Status::Delivered);
    set_read_receipts(&b.store, true).unwrap();
    set_read_receipts(&a.store, false).unwrap();
    assert_eq!(read_by_b("и третье", T0 + 30), Status::Delivered);
}

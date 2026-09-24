//! Two real Olm sessions talking through a DHT that loses, forgets and reorders
//! (`docs/transport.md` §4–§6, and the review of §12).
//!
//! Each scenario of the review that could lose a message or lie about it is a test here.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use apeiron_core::vodozemac::olm::{Account, OlmMessage};
use apeiron_core::{Chat, Identity, PrekeyBundle};
use apeiron_transport::address::{day_of, DirectionKey};
use apeiron_transport::dht::{Dht, FakeDht};
use apeiron_transport::engine::{receive_round, round, send_round};
use apeiron_transport::envelope::{ITEM_BYTES, MAX_OLM_BYTES};
use apeiron_transport::pair::{
    Event, LostWhy, Pair, GIVE_UP_AFTER_S, NORMAL_TEXT_BYTES, PREKEY_TEXT_BYTES, REPUT_EVERY_S,
};
use apeiron_transport::TransportError;

/// 24.09.2026, 12:00 UTC: away from midnight, so one state address a day.
const T0: u64 = 1_790_251_200;

struct Side {
    id: Identity,
    account: Account,
    chat: Chat,
    pair: Pair,
}

impl Side {
    fn round(&mut self, dht: &FakeDht, now: u64) -> Vec<Event> {
        round(&mut self.pair, &mut self.chat, dht, now).unwrap()
    }

    fn send(&mut self, text: &str, now: u64) -> u64 {
        self.pair.send(&mut self.chat, text, now).unwrap()
    }

    fn state_key(&self, peer: &Side, day: u32) -> [u8; 32] {
        let secret = self.id.pair_secret(&peer.id.public()).unwrap();
        DirectionKey::new(
            &secret,
            &self.id.public(),
            &peer.id.public(),
            &self.chat.session_id(),
        )
        .unwrap()
        .state(day)
        .unwrap()
        .public_key()
    }

    fn message_key(&self, peer: &Side, index: u64) -> [u8; 32] {
        let secret = self.id.pair_secret(&peer.id.public()).unwrap();
        DirectionKey::new(
            &secret,
            &self.id.public(),
            &peer.id.public(),
            &self.chat.session_id(),
        )
        .unwrap()
        .message(index)
        .unwrap()
        .public_key()
    }
}

/// A and B with a session, as after an introduction: B has accepted A's first message (the
/// inbox of §8 carries it), A has not heard from B yet.
fn pair_up() -> (Side, Side) {
    let a = Identity::generate().unwrap();
    let b = Identity::generate().unwrap();
    let mut account_a = Account::new();
    let mut account_b = Account::new();
    let bundle = |id: &Identity, acc: &mut Account| {
        PrekeyBundle::parse(&PrekeyBundle::create(id, acc).unwrap().to_bytes())
            .unwrap()
            .verify()
            .unwrap()
    };
    let bundle_a = bundle(&a, &mut account_a);
    let bundle_b = bundle(&b, &mut account_b);

    let mut chat_a = Chat::initiate(&account_a, &bundle_b).unwrap();
    let OlmMessage::PreKey(hello) = chat_a.encrypt("hello").unwrap() else {
        panic!("the first message must be a pre-key message");
    };
    let (chat_b, text) = Chat::accept(&mut account_b, &bundle_a, &hello).unwrap();
    assert_eq!(text, "hello");
    assert_eq!(chat_a.session_id(), chat_b.session_id());

    let sid = chat_a.session_id();
    let pair_a = Pair::new(&a, &b.public(), &sid).unwrap();
    let pair_b = Pair::new(&b, &a.public(), &sid).unwrap();
    (
        Side {
            id: a,
            account: account_a,
            chat: chat_a,
            pair: pair_a,
        },
        Side {
            id: b,
            account: account_b,
            chat: chat_b,
            pair: pair_b,
        },
    )
}

fn received(events: &[Event]) -> Vec<(u64, String)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Received { first, text } => Some((*first, text.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn a_message_arrives_and_is_acknowledged() {
    let dht = FakeDht::new();
    let (mut a, mut b) = pair_up();

    let id = a.send("привет", T0);
    a.round(&dht, T0);
    assert_eq!(
        received(&b.round(&dht, T0 + 1)),
        vec![(id, "привет".into())]
    );
    assert_eq!(
        a.pair.waiting(),
        1,
        "not acknowledged before B's state is read"
    );

    let events = a.round(&dht, T0 + 2);
    assert!(events.contains(&Event::Delivered { first: id }));
    assert_eq!(a.pair.waiting(), 0);
}

#[test]
fn a_long_cyrillic_text_arrives_whole() {
    let dht = FakeDht::new();
    let (mut a, mut b) = pair_up();
    let text = "Длинное сообщение о том, что ничего не пропадает. ".repeat(60);
    assert!(text.len() > 5 * PREKEY_TEXT_BYTES);

    let id = a.send(&text, T0);
    a.round(&dht, T0);
    assert_eq!(received(&b.round(&dht, T0 + 1)), vec![(id, text)]);
    assert!(a
        .round(&dht, T0 + 2)
        .contains(&Event::Delivered { first: id }));
}

#[test]
fn a_text_longer_than_32_parts_is_refused_not_cut() {
    let (mut a, _) = pair_up();
    let text = "x".repeat(32 * PREKEY_TEXT_BYTES + 1);
    assert!(matches!(
        a.pair.send(&mut a.chat, &text, T0),
        Err(TransportError::TooLong { parts: 33 })
    ));
    assert_eq!(a.pair.waiting(), 0, "nothing half-sent");
}

#[test]
fn a_lost_put_is_repaired_by_the_reput() {
    let dht = FakeDht::new();
    let (mut a, mut b) = pair_up();

    let id = a.send("first", T0);
    dht.lose_next_puts(1); // the part; the state goes through
    a.round(&dht, T0);
    assert!(received(&b.round(&dht, T0 + 1)).is_empty());

    // Within the hour nothing is put again; after it, the part is.
    a.round(&dht, T0 + 10);
    assert!(received(&b.round(&dht, T0 + 11)).is_empty());
    a.round(&dht, T0 + REPUT_EVERY_S);
    assert_eq!(
        received(&b.round(&dht, T0 + REPUT_EVERY_S + 1)),
        vec![(id, "first".into())]
    );
}

#[test]
fn expired_items_come_back_with_the_reput() {
    let dht = FakeDht::new();
    let (mut a, mut b) = pair_up();

    let id = a.send("still here", T0);
    a.round(&dht, T0);
    dht.expire_all();
    assert!(received(&b.round(&dht, T0 + 60)).is_empty());

    a.round(&dht, T0 + REPUT_EVERY_S);
    assert_eq!(
        received(&b.round(&dht, T0 + REPUT_EVERY_S + 1)),
        vec![(id, "still here".into())]
    );
}

/// Review, critical 1(a): the receiver away for longer than the sender re-puts. Before, the
/// receiver asked for index 0 forever and never saw the later messages; the sender said
/// "not delivered" and the receiver said nothing at all.
#[test]
fn a_week_away_is_told_not_silently_dropped() {
    let dht = FakeDht::new();
    let (mut a, mut b) = pair_up();

    let lost = a.send("sent while B was away", T0);
    a.round(&dht, T0);

    let later = T0 + GIVE_UP_AFTER_S;
    let events = a.round(&dht, later);
    assert!(events.contains(&Event::NotDelivered { first: lost }));
    dht.expire_all(); // the DHT forgot it long ago

    let next = a.send("after the week", later + 1);
    a.round(&dht, later + 1);

    let events = b.round(&dht, later + 2);
    assert!(events.contains(&Event::Lost {
        from: lost,
        to: next,
        why: LostWhy::GivenUp
    }));
    assert_eq!(received(&events), vec![(next, "after the week".into())]);
}

/// Review, critical 1(b) and major 3: the receiver's acknowledgement is lost. Before, it was
/// never repeated and the sender re-put for a week, then said "not delivered" about a message
/// that had been read.
#[test]
fn a_lost_acknowledgement_is_repaired() {
    let dht = FakeDht::new();
    let (mut a, mut b) = pair_up();

    let id = a.send("read, but the ack is lost", T0);
    a.round(&dht, T0);
    // B reads, and its state never reaches the DHT.
    let receiving = receive_round(&mut b.pair, &mut b.chat, &dht, T0 + 1).unwrap();
    assert_eq!(received(&receiving).len(), 1);
    dht.lose_next_puts(1);
    send_round(&mut b.pair, &dht, T0 + 1).unwrap();
    assert!(!a
        .round(&dht, T0 + 2)
        .contains(&Event::Delivered { first: id }));

    // The state is re-put like everything else; the re-put part is not a new message.
    let events_b = b.round(&dht, T0 + REPUT_EVERY_S + 1);
    assert!(received(&events_b).is_empty(), "a re-put part read twice");
    let events_a = a.round(&dht, T0 + REPUT_EVERY_S + 2);
    assert!(events_a.contains(&Event::Delivered { first: id }));
}

#[test]
fn duplicates_never_reach_olm() {
    let dht = FakeDht::new();
    let (mut a, mut b) = pair_up();

    let id = a.send("once", T0);
    a.round(&dht, T0);
    let key = a.message_key(&b, id);
    let value = dht.stored(&key).unwrap().value;
    assert_eq!(received(&b.round(&dht, T0 + 1)).len(), 1);

    assert!(
        !b.pair.on_part(id, &value).unwrap(),
        "a duplicate taken as new"
    );
    for t in 2..5 {
        assert!(received(&b.round(&dht, T0 + t)).is_empty());
    }
}

#[test]
fn both_sides_send_at_once() {
    let dht = FakeDht::new();
    let (mut a, mut b) = pair_up();

    let from_a = a.send("from A", T0);
    let from_b = b.send("from B", T0);
    let mut got_a = Vec::new();
    let mut got_b = Vec::new();
    for t in 0..3 {
        got_a.extend(received(&a.round(&dht, T0 + t)));
        got_b.extend(received(&b.round(&dht, T0 + t)));
    }
    assert_eq!(got_a, vec![(from_b, "from B".into())]);
    assert_eq!(got_b, vec![(from_a, "from A".into())]);
    assert!(
        !a.chat.sends_prekey_messages(),
        "A has heard from B: its messages are normal now"
    );
}

/// Review, major 4: Olm keeps 40 skipped keys per chain. Fifty messages that piled up while the
/// receiver was away all arrive, because they are decrypted in the order they were encrypted.
#[test]
fn fifty_messages_while_away_all_arrive_in_order() {
    let dht = FakeDht::new();
    let (mut a, mut b) = pair_up();

    let sent: Vec<(u64, String)> = (0..50)
        .map(|i| {
            let text = format!("message {i}");
            (a.send(&text, T0 + i), text)
        })
        .collect();
    a.round(&dht, T0 + 60);

    let got = received(&b.round(&dht, T0 + 120));
    assert_eq!(got, sent);
}

/// Review, major 5: an address that already holds something else is not used.
#[test]
fn a_squatted_address_is_not_used() {
    let dht = FakeDht::new();
    let (mut a, mut b) = pair_up();

    let key = a.message_key(&b, 0);
    dht.plant(key, 9, vec![0; ITEM_BYTES]);
    let id = a.send("into a taken place", T0);
    let events = a.round(&dht, T0);
    assert!(events.contains(&Event::Squatted { first: id }));
    assert_eq!(dht.stored(&key).unwrap().seq, 9, "put over the squatter");

    // B learns from A's state that index 0 is gone and moves on.
    let next = a.send("next", T0 + 1);
    a.round(&dht, T0 + 1);
    let events = b.round(&dht, T0 + 2);
    assert!(events.contains(&Event::Lost {
        from: id,
        to: next,
        why: LostWhy::GivenUp
    }));
    assert_eq!(received(&events), vec![(next, "next".into())]);
}

/// §3: the DHT sees one length, and not the device key an Olm pre-key message carries.
#[test]
fn the_dht_sees_one_length_and_no_device_key() {
    let dht = FakeDht::new();
    let (mut a, b) = pair_up();
    assert!(a.chat.sends_prekey_messages());

    let id = a.send("a pre-key message", T0);
    a.round(&dht, T0);
    let device_key = a.account.curve25519_key().to_bytes();
    for key in [a.message_key(&b, id), a.state_key(&b, day_of(T0))] {
        let value = dht.stored(&key).unwrap().value;
        assert_eq!(value.len(), ITEM_BYTES);
        assert!(!value.windows(32).any(|w| w == device_key));
    }
}

/// §5: the state items signed when the vault locks serve the days after it, so a reader who
/// locks the phone at once still acknowledges.
#[test]
fn state_signed_at_lock_serves_the_days_ahead() {
    let dht = FakeDht::new();
    let (mut a, mut b) = pair_up();

    let id = a.send("read, then locked", T0);
    a.round(&dht, T0);
    let events = receive_round(&mut b.pair, &mut b.chat, &dht, T0 + 1).unwrap();
    assert_eq!(received(&events).len(), 1);
    // Locked before its own round: only what the background job puts, days later.
    let background = b.pair.items_for_lock(T0 + 1).unwrap();

    let three_days = T0 + 3 * 86_400;
    let today_key = b.state_key(&a, day_of(three_days));
    let item = background
        .iter()
        .find(|i| i.key == today_key)
        .expect("a state item for that day");
    dht.put(item).unwrap();

    assert!(a
        .round(&dht, three_days)
        .contains(&Event::Delivered { first: id }));
}

#[test]
fn nothing_is_put_again_before_it_is_due() {
    let dht = FakeDht::new();
    let (mut a, mut b) = pair_up();

    a.send("once", T0);
    a.round(&dht, T0);
    b.round(&dht, T0 + 1);
    a.round(&dht, T0 + 2);
    let puts = dht.puts();
    for t in 3..60 {
        send_round(&mut a.pair, &dht, T0 + t).unwrap();
    }
    assert_eq!(dht.puts(), puts, "put again within the hour");
}

/// A value at the peer's address that does not open with the pair's keys changes nothing.
#[test]
fn foreign_values_are_ignored() {
    let dht = FakeDht::new();
    let (a, mut b) = pair_up();

    dht.plant(a.state_key(&b, day_of(T0)), 1, vec![7; ITEM_BYTES]);
    dht.plant(a.message_key(&b, 0), 1, vec![7; ITEM_BYTES]);
    let events = b.round(&dht, T0);
    assert!(events.is_empty(), "{events:?}");
    assert_eq!(b.pair.next_recv(), 0);
}

/// The text budgets are the largest that fit: at the start of a session (pre-key messages) and
/// after it (normal ones), including far into a chain, where the index takes more bytes.
#[test]
fn budgets_fit_the_envelope() {
    let (mut a, mut b) = pair_up();
    let len = |chat: &mut Chat, n: usize| chat.encrypt(&"x".repeat(n)).unwrap().to_parts().1.len();

    assert!(a.chat.sends_prekey_messages());
    assert!(len(&mut a.chat, PREKEY_TEXT_BYTES) <= MAX_OLM_BYTES);
    assert!(
        len(&mut a.chat, PREKEY_TEXT_BYTES + 16) > MAX_OLM_BYTES,
        "budget not tight"
    );

    // B answers, A reads: from now on A's messages are normal.
    let reply = b.chat.encrypt("reply").unwrap();
    a.chat.decrypt(&reply).unwrap();
    assert!(!a.chat.sends_prekey_messages());
    assert!(len(&mut a.chat, NORMAL_TEXT_BYTES) <= MAX_OLM_BYTES);
    assert!(
        len(&mut a.chat, NORMAL_TEXT_BYTES + 16) > MAX_OLM_BYTES,
        "budget not tight"
    );
    for _ in 0..20_000 {
        a.chat.encrypt("").unwrap();
    }
    assert!(len(&mut a.chat, NORMAL_TEXT_BYTES) <= MAX_OLM_BYTES);
}

/// Stores both sides and restores them from the bytes, as the app does on every launch.
fn store_and_restore(side: &mut Side, peer: &Side) {
    let pair = side.pair.to_bytes().unwrap();
    let chat = side.chat.pickle().unwrap();
    side.chat = Chat::from_pickle(&chat).unwrap();
    side.pair = Pair::restore(&side.id, &peer.id.public(), &side.chat.session_id(), &pair).unwrap();
}

/// §9: the pair survives being stored at any moment — here with a long message half assembled
/// on the receiving side (its first part decrypted, the second lost for now) and parts waiting
/// behind the gap — and the conversation goes on as if nothing had happened.
#[test]
fn a_pair_survives_being_stored_mid_conversation() {
    let dht = FakeDht::new();
    let (mut a, mut b) = pair_up();

    let text = "Длинное сообщение, которое прервётся на середине. ".repeat(30);
    let id = a.send(&text, T0);
    let hole = a.message_key(&b, id + 1);
    dht.black_hole(hole);
    a.round(&dht, T0);
    assert!(received(&b.round(&dht, T0 + 1)).is_empty());

    store_and_restore(&mut a, &b);
    store_and_restore(&mut b, &a);

    dht.heal(&hole);
    a.round(&dht, T0 + REPUT_EVERY_S);
    let got = received(&b.round(&dht, T0 + REPUT_EVERY_S + 1));
    assert_eq!(got, vec![(id, text)]);

    store_and_restore(&mut a, &b);
    assert!(a
        .round(&dht, T0 + REPUT_EVERY_S + 2)
        .contains(&Event::Delivered { first: id }));

    // And both ways after it.
    let from_b = b.send("ответ после восстановления", T0 + REPUT_EVERY_S + 3);
    b.round(&dht, T0 + REPUT_EVERY_S + 3);
    store_and_restore(&mut a, &b);
    assert_eq!(
        received(&a.round(&dht, T0 + REPUT_EVERY_S + 4)),
        vec![(from_b, "ответ после восстановления".into())]
    );
}

#[test]
fn a_damaged_pair_state_is_refused() {
    let dht = FakeDht::new();
    let (mut a, b) = pair_up();
    a.send("something to store", T0);
    a.round(&dht, T0);
    let bytes = a.pair.to_bytes().unwrap();
    let sid = a.chat.session_id();

    assert!(Pair::restore(&a.id, &b.id.public(), &sid, &bytes).is_ok());
    for cut in [0, 1, bytes.len() / 2, bytes.len() - 1] {
        assert!(
            Pair::restore(&a.id, &b.id.public(), &sid, &bytes[..cut]).is_err(),
            "restored from {cut} of {} bytes",
            bytes.len()
        );
    }
    let mut longer = bytes.to_vec();
    longer.push(0);
    assert!(Pair::restore(&a.id, &b.id.public(), &sid, &longer).is_err());
    let mut other_format = bytes.to_vec();
    other_format[0] = 9;
    assert!(Pair::restore(&a.id, &b.id.public(), &sid, &other_format).is_err());
    // Restored for another session, it would derive that session's addresses silently.
    assert!(matches!(
        Pair::restore(&a.id, &b.id.public(), "another session", &bytes),
        Err(TransportError::Corrupt(_))
    ));
}

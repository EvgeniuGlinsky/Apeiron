//! Getting to know each other through an invitation's one-time inbox (`docs/transport.md` §8).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use apeiron_core::vodozemac::olm::Account;
use apeiron_core::Identity;
use apeiron_transport::address::day_of;
use apeiron_transport::dht::{Dht, FakeDht};
use apeiron_transport::engine::round;
use apeiron_transport::envelope::ITEM_BYTES;
use apeiron_transport::invite::{accept, open_reply, Invitation, MAX_FIRST_TEXT_BYTES};
use apeiron_transport::pair::Event;
use apeiron_transport::TransportError;

const T0: u64 = 1_790_251_200;

fn today() -> u32 {
    day_of(T0)
}

struct Person {
    id: Identity,
    account: Account,
}

fn person() -> Person {
    Person {
        id: Identity::generate().unwrap(),
        account: Account::new(),
    }
}

#[test]
fn an_introduction_through_the_inbox_and_then_a_conversation() {
    let dht = FakeDht::new();
    let mut a = person();
    let mut b = person();

    // A sends the text of an invitation through some other messenger; it adds line breaks.
    let invitation = Invitation::create(&a.id, &mut a.account, today()).unwrap();
    let text = invitation.to_text();
    let wrapped = format!("  {}\n{}  ", &text[..40], &text[40..]);
    let received = Invitation::from_text(&wrapped).unwrap();
    assert_eq!(received.inviter(), &a.id.public());

    // B accepts and says hello; its first round puts the reply into the inbox.
    let accepted = accept(&b.id, &mut b.account, &received, "здравствуй", today()).unwrap();
    let (mut b_chat, mut b_pair) = (accepted.chat, accepted.pair);
    assert!(!b_pair.peer_seen(), "waiting until A answers");
    round(&mut b_pair, &mut b_chat, &dht, T0).unwrap();

    // A finds the reply and opens it.
    let value = dht
        .get_first(&invitation.inbox_key().unwrap())
        .unwrap()
        .expect("the reply is in the inbox")
        .value;
    let joined = open_reply(&a.id, &mut a.account, &invitation, &value).unwrap();
    assert_eq!(joined.peer, b.id.public());
    assert_eq!(joined.first_text, "здравствуй");
    let (mut a_chat, mut a_pair) = (joined.chat, joined.pair);
    assert_eq!(a_chat.session_id(), b_chat.session_id());

    // A's first round publishes its state: that is B's acknowledgement.
    round(&mut a_pair, &mut a_chat, &dht, T0 + 1).unwrap();
    let events = round(&mut b_pair, &mut b_chat, &dht, T0 + 2).unwrap();
    assert!(events.contains(&Event::Accepted), "{events:?}");
    assert!(b_pair.peer_seen());

    // And from here on it is an ordinary conversation, both ways.
    let from_a = a_pair.send(&mut a_chat, "рад знакомству", T0 + 3).unwrap();
    let from_b = b_pair.send(&mut b_chat, "взаимно", T0 + 3).unwrap();
    let mut got_a = Vec::new();
    let mut got_b = Vec::new();
    for t in 4..8 {
        got_a.extend(round(&mut a_pair, &mut a_chat, &dht, T0 + t).unwrap());
        got_b.extend(round(&mut b_pair, &mut b_chat, &dht, T0 + t).unwrap());
    }
    assert!(got_b.contains(&Event::Received {
        first: from_a,
        text: "рад знакомству".into()
    }));
    assert!(got_a.contains(&Event::Received {
        first: from_b,
        text: "взаимно".into()
    }));
}

#[test]
fn the_reply_is_re_put_until_the_inviter_answers() {
    let dht = FakeDht::new();
    let mut a = person();
    let mut b = person();
    let invitation = Invitation::create(&a.id, &mut a.account, today()).unwrap();
    let mut accepted = accept(&b.id, &mut b.account, &invitation, "hi", today()).unwrap();

    round(&mut accepted.pair, &mut accepted.chat, &dht, T0).unwrap();
    dht.expire_all(); // A was away for hours; the DHT forgot the reply
    round(&mut accepted.pair, &mut accepted.chat, &dht, T0 + 3_600).unwrap();

    let value = dht
        .get_first(&invitation.inbox_key().unwrap())
        .unwrap()
        .expect("put again")
        .value;
    assert!(open_reply(&a.id, &mut a.account, &invitation, &value).is_ok());
}

#[test]
fn an_expired_invitation_is_refused() {
    let mut a = person();
    let mut b = person();
    let invitation = Invitation::create(&a.id, &mut a.account, today()).unwrap();
    let later = invitation.expires() + 1;
    assert!(matches!(
        accept(&b.id, &mut b.account, &invitation, "hi", later),
        Err(TransportError::Invitation(_))
    ));
}

#[test]
fn ones_own_invitation_is_refused() {
    let mut a = person();
    let invitation = Invitation::create(&a.id, &mut a.account, today()).unwrap();
    let mut second = Account::new();
    assert!(accept(&a.id, &mut second, &invitation, "hi", today()).is_err());
}

#[test]
fn a_tampered_invitation_does_not_verify() {
    let mut a = person();
    let invitation = Invitation::create(&a.id, &mut a.account, today()).unwrap();
    let mut bytes = invitation.to_bytes();
    bytes[10] ^= 1; // inside the bundle, under its signature
    assert!(Invitation::from_bytes(&bytes).is_err());
    assert!(Invitation::from_text("hello: not an invitation").is_err());
    assert!(Invitation::from_bytes(&bytes[1..]).is_err());
}

#[test]
fn a_first_text_too_long_is_refused_not_cut() {
    let mut a = person();
    let mut b = person();
    let invitation = Invitation::create(&a.id, &mut a.account, today()).unwrap();
    let longest = "x".repeat(MAX_FIRST_TEXT_BYTES);
    assert!(accept(&b.id, &mut b.account, &invitation, &longest, today()).is_ok());
    let mut c = person();
    let too_long = "x".repeat(MAX_FIRST_TEXT_BYTES + 1);
    assert!(accept(&c.id, &mut c.account, &invitation, &too_long, today()).is_err());
}

/// Whoever holds one invitation's secret must not spend another invitation's key: the reply is
/// refused unless its Olm message uses the one-time key of the invitation it answers.
#[test]
fn a_reply_with_another_invitations_key_is_refused() {
    let mut a = person();
    let mut mallory = person();
    let first = Invitation::create(&a.id, &mut a.account, today()).unwrap();
    let second = Invitation::create(&a.id, &mut a.account, today()).unwrap();

    // Mallory knows the second invitation's secret, and builds a reply that opens in its inbox
    // but uses the first invitation's bundle and one-time key.
    let mut mixed = second.to_bytes();
    mixed[1..225].copy_from_slice(&first.to_bytes()[1..225]);
    let mixed = Invitation::from_bytes(&mixed).unwrap();
    let forged = accept(&mallory.id, &mut mallory.account, &mixed, "hi", today()).unwrap();

    let dht = FakeDht::new();
    let (mut chat, mut pair) = (forged.chat, forged.pair);
    round(&mut pair, &mut chat, &dht, T0).unwrap();
    let value = dht.stored(&second.inbox_key().unwrap()).unwrap().value;
    assert!(matches!(
        open_reply(&a.id, &mut a.account, &second, &value),
        Err(TransportError::Invitation(_))
    ));
}

/// Someone else answered first — whoever else saw the invitation, or someone it was forwarded
/// to. The reply cannot land (the first one stays), and B is told instead of waiting forever.
#[test]
fn a_taken_inbox_is_reported() {
    let dht = FakeDht::new();
    let mut a = person();
    let mut b = person();
    let invitation = Invitation::create(&a.id, &mut a.account, today()).unwrap();
    dht.plant(invitation.inbox_key().unwrap(), 1, vec![9; ITEM_BYTES]);

    let mut accepted = accept(&b.id, &mut b.account, &invitation, "hi", today()).unwrap();
    let events = round(&mut accepted.pair, &mut accepted.chat, &dht, T0).unwrap();
    assert!(events.contains(&Event::InvitationTaken), "{events:?}");
    assert_eq!(
        dht.stored(&invitation.inbox_key().unwrap()).unwrap().value,
        vec![9; ITEM_BYTES]
    );
}

/// §8: whoever read the invitation can find the reply, but not open it — and so does not
/// learn who answered.
#[test]
fn the_inbox_secret_alone_does_not_open_the_reply() {
    let dht = FakeDht::new();
    let mut a = person();
    let mut b = person();
    let invitation = Invitation::create(&a.id, &mut a.account, today()).unwrap();
    let mut accepted = accept(&b.id, &mut b.account, &invitation, "hi", today()).unwrap();
    round(&mut accepted.pair, &mut accepted.chat, &dht, T0).unwrap();
    let value = dht.stored(&invitation.inbox_key().unwrap()).unwrap().value;
    assert_eq!(value.len(), ITEM_BYTES);

    // B's identity is nowhere in it.
    let b_identity = b.id.public().to_bytes();
    assert!(!value
        .windows(32)
        .any(|w| w == &b_identity[..32] || w == &b_identity[32..]));

    // Someone with the invitation (the secret) but not A's identity opens nothing.
    let mut eve = person();
    assert!(matches!(
        open_reply(&eve.id, &mut eve.account, &invitation, &value),
        Err(TransportError::NotOurs)
    ));
}

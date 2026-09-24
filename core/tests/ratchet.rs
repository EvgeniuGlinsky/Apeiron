//! Checks of pairwise conversation, by section of the reference `radio-mesh-demo/s07_ratchet.py`.
//!
//! The reference is written in pure Python and describes the required behavior:
//! an exchange with direction changes (section C), a channel with losses and reordering
//! (D), forward secrecy (E), the intermediary attack (F). Here the same is
//! required of our Rust implementation.
//!
//! The tests are integration tests on purpose: they see only the core's public API. If
//! something cannot be done from outside, it cannot be done in the application either.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use apeiron_core::vodozemac::olm::{Account, OlmMessage, SessionConfig, SessionCreationError};
use apeiron_core::vodozemac::Curve25519PublicKey;
use apeiron_core::{pickle_account, unpickle_account, Chat, Identity, PrekeyBundle};

/// A participant: a long-term identity plus a device with Olm keys.
struct Party {
    identity: Identity,
    account: Account,
}

impl Party {
    fn new() -> Self {
        Self {
            identity: Identity::generate().expect("the OS provides randomness"),
            account: Account::new(),
        }
    }

    /// A prekey bundle to hand to the other side.
    fn bundle(&mut self) -> Vec<u8> {
        PrekeyBundle::create(&self.identity, &mut self.account)
            .expect("the bundle is assembled")
            .to_bytes()
            .to_vec()
    }
}

/// Parses and verifies a bundle, as the application is obliged to do.
fn accept_bundle(bytes: &[u8]) -> PrekeyBundle {
    PrekeyBundle::parse(bytes)
        .expect("the bundle parses")
        .verify()
        .expect("the signature is valid")
}

fn prekey_of(message: &OlmMessage) -> &apeiron_core::vodozemac::olm::PreKeyMessage {
    match message {
        OlmMessage::PreKey(m) => m,
        OlmMessage::Normal(_) => panic!("a session-establishment message was expected"),
    }
}

// ─── Section C. Exchange with direction changes ──────────────────────────────

#[test]
fn exchange_with_direction_changes() {
    let mut alice = Party::new();
    let mut bob = Party::new();

    let bob_bundle = accept_bundle(&bob.bundle());
    let alice_bundle = accept_bundle(&alice.bundle());

    let mut a_chat = Chat::initiate(&alice.account, &bob_bundle).expect("session is created");
    let first = a_chat.encrypt("greetings").expect("encrypted");

    let (mut b_chat, text) =
        Chat::accept(&mut bob.account, &alice_bundle, prekey_of(&first)).expect("session accepted");
    assert_eq!(text, "greetings");

    // The session identifier is identical on both sides: they are in one conversation.
    assert_eq!(a_chat.session_id(), b_chat.session_id());

    // Five turnarounds in a row: each turnaround advances the DH ratchet.
    for round in 0..5 {
        let from_bob = b_chat
            .encrypt(&format!("reply {round}"))
            .expect("encrypted");
        assert_eq!(
            a_chat.decrypt(&from_bob).expect("decrypted"),
            format!("reply {round}")
        );

        let from_alice = b_chat_reply(&mut a_chat, round);
        assert_eq!(
            b_chat.decrypt(&from_alice).expect("decrypted"),
            format!("query {round}")
        );
    }

    // Each side sees the peer's identity: the one shown on the
    // verification screen.
    assert_eq!(
        a_chat.peer().fingerprint(),
        bob.identity.public().fingerprint()
    );
    assert_eq!(
        b_chat.peer().fingerprint(),
        alice.identity.public().fingerprint()
    );
}

fn b_chat_reply(chat: &mut Chat, round: usize) -> OlmMessage {
    chat.encrypt(&format!("query {round}")).expect("encrypted")
}

// ─── Section D. Out-of-order receive ─────────────────────────────────────────

/// Two hundred messages back to front.
///
/// This is exactly the case `decrypt_batch` was written for: after a day
/// offline the relay queue hands over everything at once and in arbitrary order.
#[test]
fn batch_survives_reverse_order() {
    const COUNT: usize = 200;
    let (mut a_chat, mut b_chat) = established_pair();

    let mut sent = Vec::with_capacity(COUNT);
    let mut texts = Vec::with_capacity(COUNT);
    for i in 0..COUNT {
        let text = format!("message no. {i}");
        sent.push(a_chat.encrypt(&text).expect("encrypted"));
        texts.push(text);
    }

    // The batch arrives back to front.
    let mut reversed = sent.clone();
    reversed.reverse();

    let results = b_chat.decrypt_batch(&reversed);
    assert_eq!(results.len(), COUNT);

    let lost = results.iter().filter(|r| r.is_err()).count();
    assert_eq!(
        lost, 0,
        "when sorted into chain order, not a single message may be lost"
    );

    for (position, result) in results.iter().enumerate() {
        let expected = &texts[COUNT - 1 - position];
        assert_eq!(result.as_ref().expect("read"), expected);
    }
}

/// The control experiment: the same thing without sorting into order.
///
/// Needed so that `decrypt_batch` does not look like an excessive precaution. If
/// someone one day decides sorting is unnecessary, this test will show the price.
#[test]
fn naive_reverse_order_loses_messages() {
    const COUNT: usize = 200;
    let (mut a_chat, mut b_chat) = established_pair();

    let mut sent = Vec::with_capacity(COUNT);
    for i in 0..COUNT {
        sent.push(
            a_chat
                .encrypt(&format!("message no. {i}"))
                .expect("encrypted"),
        );
    }
    sent.reverse();

    let lost = sent
        .iter()
        .map(|m| b_chat.decrypt(m))
        .filter(|r| r.is_err())
        .count();

    assert!(
        lost > 100,
        "naive back-to-front decryption must lose messages; lost {lost}"
    );
}

/// A channel with losses and reordering: the model of section D of the reference.
#[test]
fn lossy_and_shuffled_channel() {
    const COUNT: usize = 40;
    let (mut a_chat, mut b_chat) = established_pair();

    let mut sent = Vec::with_capacity(COUNT);
    for i in 0..COUNT {
        sent.push((
            i,
            a_chat
                .encrypt(&format!("message no. {i}"))
                .expect("encrypted"),
        ));
    }

    // A deterministic "network": drop every third, reorder the rest by a
    // simple reversible rule. Randomness is not needed here; what is needed is
    // reproducibility: a failed test must fail again.
    let delivered: Vec<(usize, OlmMessage)> =
        sent.into_iter().filter(|(i, _)| i % 3 != 0).collect();
    let mut shuffled = delivered.clone();
    shuffled.reverse();
    shuffled.rotate_left(7);

    let messages: Vec<OlmMessage> = shuffled.iter().map(|(_, m)| m.clone()).collect();
    let results = b_chat.decrypt_batch(&messages);

    for ((original, _), result) in shuffled.iter().zip(results.iter()) {
        assert_eq!(
            result.as_ref().expect("what was delivered is readable"),
            &format!("message no. {original}")
        );
    }

    // What was lost in the network stays lost, and that is fine: recovering
    // it is the job of the delivery layer, not of the ratchet.
    assert_eq!(results.iter().filter(|r| r.is_err()).count(), 0);
}

// ─── Section E. Forward secrecy ──────────────────────────────────────────────

/// The key of a used message is destroyed.
///
/// Forward secrecy in operational form: state captured **now**
/// does not read what was read earlier. It is also protection against replay: the same
/// message will not pass a second time.
#[test]
fn used_message_key_is_gone() {
    let (mut a_chat, mut b_chat) = established_pair();

    let first = a_chat.encrypt("first").expect("encrypted");
    assert_eq!(b_chat.decrypt(&first).expect("is read"), "first");

    for i in 0..10 {
        let m = a_chat.encrypt(&format!("more {i}")).expect("encrypted");
        b_chat.decrypt(&m).expect("is read");
    }

    let again = b_chat.decrypt(&first);
    let err = again.expect_err("reading the same message again is impossible");
    assert!(
        err.is_lost_forever(),
        "the error must speak of irrecoverable key loss, not of breakage: {err}"
    );
}

// ─── Section F. The intermediary ─────────────────────────────────────────────

/// Without fingerprint verification the intermediary wins completely.
///
/// This is not a warning in the documentation but an executable fact: both sides see
/// properly working encryption, the signatures are valid, and both are corresponding
/// with Mallory. The only thing that gives him away is the mismatch of the safety number.
#[test]
fn mitm_succeeds_without_fingerprint_check_and_fails_with_it() {
    let mut alice = Party::new();
    let mut bob = Party::new();
    let mut mallory = Party::new();

    // Mallory intercepts the bundle exchange and slips each side his own.
    // He keeps Alice's and Bob's real bundles for himself.
    let alice_gets = accept_bundle(&mallory.bundle()); // Alice thinks this is Bob
    let bob_gets = accept_bundle(&mallory.bundle()); // Bob thinks this is Alice
    let alice_real = accept_bundle(&alice.bundle());
    let bob_real = accept_bundle(&bob.bundle());

    // The signatures **are valid**: Mallory signed his keys with his own identity.
    // Cryptography is not broken anywhere; the trust model is.
    let mut alice_chat = Chat::initiate(&alice.account, &alice_gets).expect("session is created");
    let first = alice_chat.encrypt("secret").expect("encrypted");

    let (_, intercepted) =
        Chat::accept(&mut mallory.account, &alice_real, prekey_of(&first)).expect("Mallory reads");
    assert_eq!(intercepted, "secret", "the intermediary reads plaintext");

    // And forwards it on to Bob, now over his own session, with Bob's real bundle.
    let mut mallory_with_bob =
        Chat::initiate(&mallory.account, &bob_real).expect("session is created");
    let forwarded = mallory_with_bob.encrypt(&intercepted).expect("encrypted");

    let (bob_chat, seen_by_bob) =
        Chat::accept(&mut bob.account, &bob_gets, prekey_of(&forwarded)).expect("Bob accepts");
    assert_eq!(
        seen_by_bob, "secret",
        "Bob sees the text and suspects nothing"
    );

    // And now verification. Alice and Bob read their safety numbers to each other aloud.
    let alice_sees = alice.identity.public().safety_number(alice_chat.peer());
    let bob_sees = bob.identity.public().safety_number(bob_chat.peer());

    assert_ne!(
        alice_sees, bob_sees,
        "the safety numbers must differ, otherwise the intermediary is indistinguishable"
    );

    // For comparison: the honest case, where the numbers match.
    let honest_alice = alice
        .identity
        .public()
        .safety_number(&bob.identity.public());
    let honest_bob = bob
        .identity
        .public()
        .safety_number(&alice.identity.public());
    assert_eq!(honest_alice, honest_bob);
}

// ─── Prekey bundle checks ────────────────────────────────────────────────────

#[test]
fn tampered_bundle_is_rejected() {
    let mut bob = Party::new();
    let bytes = bob.bundle();

    // Flip one bit in the device key: the signature stops matching.
    for position in [64usize, 80, 100, 130] {
        let mut broken = bytes.clone();
        broken[position] ^= 0b0000_0001;
        // Some of the bytes are the keys themselves; such bundles may not parse at all,
        // and that is a rejection too.
        if let Ok(unverified) = PrekeyBundle::parse(&broken) {
            assert!(
                unverified.verify().is_err(),
                "a substituted byte {position} must break verification"
            );
        }
    }
}

#[test]
fn bundle_of_wrong_length_is_rejected() {
    let mut bob = Party::new();
    let bytes = bob.bundle();
    assert!(PrekeyBundle::parse(&bytes[..bytes.len() - 1]).is_err());
    assert!(PrekeyBundle::parse(&[]).is_err());
}

#[test]
fn each_bundle_carries_a_fresh_one_time_key() {
    let mut bob = Party::new();
    let first = accept_bundle(&bob.bundle());
    let second = accept_bundle(&bob.bundle());
    assert_ne!(
        first.one_time_key().to_bytes(),
        second.one_time_key().to_bytes(),
        "a one-time key is one-time for a reason"
    );
}

/// A zero public key must be rejected.
///
/// The February 2026 finding against vodozemac: all-zero keys were accepted,
/// giving a predictable zero shared secret. Fixed in 0.10.0.
/// We check this with **our own** test rather than taking it on trust: if the dependency
/// ever slides back, it will become visible here.
#[test]
fn zero_public_key_is_rejected() {
    let mut bob = Party::new();
    let alice = Party::new();
    let bundle = accept_bundle(&bob.bundle());

    let zero = Curve25519PublicKey::from_bytes([0u8; 32]);

    let by_one_time = alice.account.create_outbound_session(
        SessionConfig::version_1(),
        bundle.device_curve_key(),
        zero,
    );
    assert!(
        matches!(by_one_time, Err(SessionCreationError::NonContributoryKey)),
        "a zero one-time key must be rejected"
    );

    let by_identity = alice.account.create_outbound_session(
        SessionConfig::version_1(),
        zero,
        bundle.one_time_key(),
    );
    assert!(
        matches!(by_identity, Err(SessionCreationError::NonContributoryKey)),
        "a zero device key must be rejected"
    );
}

// ─── Common ──────────────────────────────────────────────────────────────────

/// A pair with an already established session: Alice wrote, Bob accepted.
fn established_pair() -> (Chat, Chat) {
    let mut alice = Party::new();
    let mut bob = Party::new();

    let bob_bundle = accept_bundle(&bob.bundle());
    let alice_bundle = accept_bundle(&alice.bundle());

    let mut a_chat = Chat::initiate(&alice.account, &bob_bundle).expect("session is created");
    let hello = a_chat.encrypt("start").expect("encrypted");
    let (b_chat, text) =
        Chat::accept(&mut bob.account, &alice_bundle, prekey_of(&hello)).expect("session accepted");
    assert_eq!(text, "start");

    (a_chat, b_chat)
}

// ─── Storage: state must survive a restart ───────────────────────────────────

/// The identity survives export and import.
///
/// Without this an application restart would mean a new fingerprint, and hence
/// a safety number read aloud anew with every peer.
#[test]
fn identity_survives_export_and_import() {
    let original = Identity::generate().expect("the OS provides randomness");
    let exported = original.export_secret();
    let restored =
        Identity::from_secret_bytes(exported.as_bytes()).expect("our own secret is readable");

    assert_eq!(
        original.public().to_bytes(),
        restored.public().to_bytes(),
        "after import a different identity came out"
    );
    assert_eq!(
        original.public().fingerprint(),
        restored.public().fingerprint()
    );

    // The key is the same, not "similar": a signature of the restored identity must
    // verify against the original one.
    let message = b"apeiron";
    let signature = restored.sign(message);
    original
        .public()
        .verify(message, &signature)
        .expect("the signature did not match");
}

#[test]
fn secret_of_wrong_length_is_rejected() {
    let identity = Identity::generate().expect("the OS provides randomness");
    let exported = identity.export_secret();
    assert!(Identity::from_secret_bytes(&exported.as_bytes()[..63]).is_err());
    assert!(Identity::from_secret_bytes(&[]).is_err());
}

/// A conversation survives a restart: a message encrypted before saving
/// is read after loading.
///
/// This is exactly what the storage was undertaken for. If the ratchet state
/// is lost, the peers diverge forever, and there is nothing to fix it with.
#[test]
fn chat_survives_pickle_and_unpickle() {
    let mut alice = Party::new();
    let mut bob = Party::new();

    let bob_bundle = accept_bundle(&bob.bundle());
    let alice_bundle = accept_bundle(&alice.bundle());

    let mut a_chat = Chat::initiate(&alice.account, &bob_bundle).expect("session is created");
    let first = a_chat.encrypt("before restart").expect("encrypted");

    // Both sides go to disk and come back from there.
    let saved_chat = a_chat.pickle().expect("the state is saved");
    let saved_account = pickle_account(&bob.account).expect("the account is saved");
    drop(a_chat);

    let restored_chat = Chat::from_pickle(&saved_chat).expect("the state is read");
    let mut restored_account = unpickle_account(&saved_account).expect("the account is read");

    assert_eq!(
        restored_chat.peer().to_bytes(),
        bob.identity.public().to_bytes(),
        "after loading the peer became someone else"
    );

    let (_, text) = Chat::accept(&mut restored_account, &alice_bundle, prekey_of(&first))
        .expect("a message encrypted before saving could not be read after loading");
    assert_eq!(text, "before restart");
}

/// The conversation continues after both sides are saved and loaded.
///
/// Separate from the previous test: there the start of the session was checked, here that
/// the ratchet did not get out of step, i.e. that the state itself was saved, not a part of it.
#[test]
fn conversation_continues_after_reload() {
    let mut alice = Party::new();
    let mut bob = Party::new();

    let bob_bundle = accept_bundle(&bob.bundle());
    let alice_bundle = accept_bundle(&alice.bundle());

    let mut a_chat = Chat::initiate(&alice.account, &bob_bundle).expect("session is created");
    let first = a_chat.encrypt("one").expect("encrypted");
    let (mut b_chat, text) =
        Chat::accept(&mut bob.account, &alice_bundle, prekey_of(&first)).expect("session accepted");
    assert_eq!(text, "one");

    let reply = b_chat.encrypt("two").expect("encrypted");
    assert_eq!(a_chat.decrypt(&reply).expect("is read"), "two");

    // Both states go to disk.
    let saved_a = a_chat.pickle().expect("is saved");
    let saved_b = b_chat.pickle().expect("is saved");
    drop(a_chat);
    drop(b_chat);

    let mut a_chat = Chat::from_pickle(&saved_a).expect("is read");
    let mut b_chat = Chat::from_pickle(&saved_b).expect("is read");

    let third = a_chat.encrypt("three").expect("encrypted");
    assert_eq!(
        b_chat.decrypt(&third).expect("is read after restart"),
        "three"
    );
    let fourth = b_chat.encrypt("four").expect("encrypted");
    assert_eq!(
        a_chat.decrypt(&fourth).expect("is read after restart"),
        "four"
    );
}

#[test]
fn damaged_chat_state_is_not_swallowed() {
    let alice = Party::new();
    let mut bob = Party::new();
    let bob_bundle = accept_bundle(&bob.bundle());
    let chat = Chat::initiate(&alice.account, &bob_bundle).expect("session is created");

    let mut saved = chat.pickle().expect("the state is saved");
    let last = saved.len() - 1;
    saved[last] ^= 0xff;

    assert!(
        Chat::from_pickle(&saved).is_err(),
        "a damaged state passed as intact"
    );
    assert!(
        Chat::from_pickle(&saved[..10]).is_err(),
        "a truncated state passed as intact"
    );
}

#[test]
fn damaged_account_state_is_not_swallowed() {
    let account = Account::new();
    let mut saved = pickle_account(&account).expect("the account is saved");
    saved[5] ^= 0xff;
    assert!(unpickle_account(&saved).is_err());
    assert!(unpickle_account(&[]).is_err());
}

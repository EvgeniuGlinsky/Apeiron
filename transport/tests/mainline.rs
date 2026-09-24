//! The transport over the real DHT protocol, on a local test network: real nodes, real KRPC
//! over UDP, real BEP 44 storage and signature checks — only on localhost.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use apeiron_core::vodozemac::olm::{Account, OlmMessage};
use apeiron_core::{Chat, Identity, PrekeyBundle};
use apeiron_transport::engine::round;
use apeiron_transport::mainline_dht::MainlineDht;
use apeiron_transport::pair::{Event, Pair};

const T0: u64 = 1_790_251_200;

#[test]
fn two_sides_talk_through_real_dht_nodes() {
    let Ok(testnet) = mainline::Testnet::builder(12).build() else {
        return; // no UDP on this machine: nothing to test here
    };
    // Each side has its own node, as two phones would.
    let (Ok(node_a), Ok(node_b)) = (
        MainlineDht::local(&testnet.bootstrap),
        MainlineDht::local(&testnet.bootstrap),
    ) else {
        panic!("a node of the local test network did not bootstrap");
    };

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
    let (mut chat_b, _) = Chat::accept(&mut account_b, &bundle_a, &hello).unwrap();
    let sid = chat_a.session_id();
    let mut pair_a = Pair::new(&a, &b.public(), &sid).unwrap();
    let mut pair_b = Pair::new(&b, &a.public(), &sid).unwrap();

    let text = "через настоящие узлы DHT";
    let id = pair_a.send(&mut chat_a, text, T0).unwrap();
    round(&mut pair_a, &mut chat_a, &node_a, T0).unwrap();

    let got = round(&mut pair_b, &mut chat_b, &node_b, T0 + 1).unwrap();
    assert!(
        got.contains(&Event::Received {
            first: id,
            text: text.to_string()
        }),
        "{got:?}"
    );

    let back = round(&mut pair_a, &mut chat_a, &node_a, T0 + 2).unwrap();
    assert!(back.contains(&Event::Delivered { first: id }), "{back:?}");
}

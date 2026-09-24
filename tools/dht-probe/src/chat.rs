//! `dht-probe chat`: the two sides of a conversation, each on its own node, talking through the
//! real Mainline DHT (`docs/transport.md` §10, step 2).
//!
//! Every message goes A → B and its answer B → A, and three times are measured: the put, the
//! time until the other side has read it, and the time until the sender sees it acknowledged.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use apeiron_core::vodozemac::olm::{Account, OlmMessage};
use apeiron_core::{Chat, Identity, PrekeyBundle};
use apeiron_transport::engine::round;
use apeiron_transport::mainline_dht::MainlineDht;
use apeiron_transport::pair::{Event, Pair};

/// How long one side waits for something before giving up on it.
const PATIENCE: Duration = Duration::from_secs(120);

/// Pause between rounds while waiting, as an open chat would poll.
const POLL: Duration = Duration::from_secs(3);

struct Side {
    name: &'static str,
    chat: Chat,
    pair: Pair,
    node: MainlineDht,
}

fn unix_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn ms(d: Duration) -> u128 {
    d.as_millis()
}

impl Side {
    fn round(&mut self) -> Result<Vec<Event>, String> {
        round(&mut self.pair, &mut self.chat, &self.node, unix_s()).map_err(|e| e.to_string())
    }

    /// Rounds until `wanted` shows up among the events, or patience runs out.
    fn wait_for(&mut self, wanted: impl Fn(&Event) -> bool) -> Result<Option<Duration>, String> {
        let started = Instant::now();
        while started.elapsed() < PATIENCE {
            if self.round()?.iter().any(&wanted) {
                return Ok(Some(started.elapsed()));
            }
            std::thread::sleep(POLL);
        }
        Ok(None)
    }
}

fn session() -> Result<(Identity, Identity, Chat, Chat), String> {
    let a = Identity::generate().map_err(|e| e.to_string())?;
    let b = Identity::generate().map_err(|e| e.to_string())?;
    let mut account_a = Account::new();
    let mut account_b = Account::new();
    let bundle = |id: &Identity, acc: &mut Account| {
        PrekeyBundle::create(id, acc)
            .map_err(|e| e.to_string())
            .and_then(|b| PrekeyBundle::parse(&b.to_bytes()).map_err(|e| e.to_string()))
            .and_then(|b| b.verify().map_err(|e| e.to_string()))
    };
    let bundle_a = bundle(&a, &mut account_a)?;
    let bundle_b = bundle(&b, &mut account_b)?;
    let mut chat_a = Chat::initiate(&account_a, &bundle_b).map_err(|e| e.to_string())?;
    // What the inbox of an invitation carries (§8): A's first message, accepted by B.
    let OlmMessage::PreKey(hello) = chat_a.encrypt("hello").map_err(|e| e.to_string())? else {
        return Err("the first message is not a pre-key message".to_string());
    };
    let (chat_b, _) = Chat::accept(&mut account_b, &bundle_a, &hello).map_err(|e| e.to_string())?;
    Ok((a, b, chat_a, chat_b))
}

fn node(name: &str, log: &dyn Fn(&str)) -> Result<MainlineDht, String> {
    let started = Instant::now();
    let node = MainlineDht::bootstrap().map_err(|e| format!("{name}: {e}"))?;
    log(&format!(
        "[chat] {name}: own node bootstrapped in {} ms",
        ms(started.elapsed())
    ));
    Ok(node)
}

/// One message from `from` to `to`, and its acknowledgement back.
fn exchange(from: &mut Side, to: &mut Side, text: &str, log: &dyn Fn(&str)) -> Result<(), String> {
    let first = from
        .pair
        .send(&mut from.chat, text, unix_s())
        .map_err(|e| e.to_string())?;
    let started = Instant::now();
    from.round()?;
    let put = started.elapsed();

    let read = to.wait_for(
        |e| matches!(e, Event::Received { first: f, text: t } if *f == first && t == text),
    )?;
    let acked = from.wait_for(|e| matches!(e, Event::Delivered { first: f } if *f == first))?;
    log(&format!(
        "[chat] {} → {} #{first} ({} bytes): put {} ms; read {}; acknowledged {}",
        from.name,
        to.name,
        text.len(),
        ms(put),
        read.map_or("NOT within the patience".to_string(), |d| format!(
            "{} ms later",
            ms(d)
        )),
        acked.map_or("NOT within the patience".to_string(), |d| format!(
            "{} ms after that",
            ms(d)
        )),
    ));
    Ok(())
}

pub fn run(messages: u32, log: &dyn Fn(&str)) -> Result<(), String> {
    let (a, b, chat_a, chat_b) = session()?;
    let sid = chat_a.session_id();
    let mut side_a = Side {
        name: "A",
        pair: Pair::new(&a, &b.public(), &sid).map_err(|e| e.to_string())?,
        chat: chat_a,
        node: node("A", log)?,
    };
    let mut side_b = Side {
        name: "B",
        pair: Pair::new(&b, &a.public(), &sid).map_err(|e| e.to_string())?,
        chat: chat_b,
        node: node("B", log)?,
    };
    log(&format!(
        "[chat] {messages} exchanges through the real DHT, each side on its own node"
    ));
    for i in 0..messages {
        exchange(
            &mut side_a,
            &mut side_b,
            &format!("сообщение {i} — через настоящую DHT, без серверов"),
            log,
        )?;
        exchange(&mut side_b, &mut side_a, &format!("ответ {i}"), log)?;
    }
    log("[chat] done");
    Ok(())
}

//! One round of sending and one of receiving, against any [`Dht`].
//!
//! The caller owns the contact: it loads the [`Pair`] and the `Chat`, runs a round, and commits
//! both — with what the round reported — in one transaction.

use apeiron_core::Chat;

use crate::dht::Dht;
use crate::pair::{Event, Pair};
use crate::TransportError;

/// A whole round: receive, then send. In this order, so that what arrived is acknowledged in
/// the same round — the other way round the acknowledgement waits for the next one.
pub fn round(
    pair: &mut Pair,
    chat: &mut Chat,
    dht: &impl Dht,
    now: u64,
) -> Result<Vec<Event>, TransportError> {
    let mut events = receive_round(pair, chat, dht, now)?;
    events.extend(send_round(pair, dht, now)?);
    Ok(events)
}

/// Puts what is due. An address used for the first time is asked for first: if it already
/// holds something else, the schedule is reused or known to someone else, and the message is
/// not put (`docs/transport.md` §2).
pub fn send_round(pair: &mut Pair, dht: &impl Dht, now: u64) -> Result<Vec<Event>, TransportError> {
    let due = pair.due(now)?;
    let mut events = due.events;
    for item in due.items {
        // A message found squatted a moment ago takes its other parts with it.
        if !pair.is_pending(&item.key) {
            continue;
        }
        if due.first_puts.contains(&item.key) {
            match dht.get_first(&item.key) {
                Ok(Some(found)) if found.value != item.value => {
                    events.extend(pair.squatted(&item.key));
                    continue;
                }
                Ok(_) => {}
                // Unknown whether the address is free: not now.
                Err(_) => continue,
            }
        }
        if dht.put(&item).is_ok() {
            pair.mark_put(&item.key, now);
        }
    }
    Ok(events)
}

/// Asks for the peer's state, fetches what it announces, decrypts what can be decrypted.
pub fn receive_round(
    pair: &mut Pair,
    chat: &mut Chat,
    dht: &impl Dht,
    now: u64,
) -> Result<Vec<Event>, TransportError> {
    let mut events = Vec::new();
    for (key, day) in pair.peer_state_keys(now)? {
        if let Ok(Some(found)) = dht.get_latest(&key) {
            // A value that does not open with the pair's keys is not the peer's: ignored.
            if let Ok(e) = pair.on_peer_state(day, &found.value) {
                events.extend(e);
            }
        }
    }
    let wanted = pair.wanted()?;
    let keys: Vec<[u8; 32]> = wanted.iter().map(|(_, k)| *k).collect();
    for ((index, _), got) in wanted.iter().zip(dht.get_first_many(&keys)) {
        if let Ok(Some(found)) = got {
            // The same: something at the address that is not the peer's part is ignored.
            let _ = pair.on_part(*index, &found.value);
        }
    }
    events.extend(pair.receive(chat));
    Ok(events)
}

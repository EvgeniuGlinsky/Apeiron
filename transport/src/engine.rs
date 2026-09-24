//! One round of sending and one of receiving, against any [`Dht`].
//!
//! The caller owns the contact: it loads the [`Pair`] and the `Chat`, runs a round, and commits
//! both — with what the round reported — in one transaction.
//!
//! **A caller that stores the conversation must not use [`round`] as it is.** Its send half
//! puts a state item acknowledging what the receive half has just decrypted, before anything
//! is on disk: a crash in between and the peer stops re-putting messages this side never kept,
//! and the item's `seq` is taken by a value the restored pair does not know. Such a caller runs
//! [`receive_round`], then [`Pair::due`], commits, and only then [`put_due`]
//! (`apeiron-messenger` does). [`round`] is for callers that keep nothing: tests, `dht-probe`.

use std::collections::HashSet;

use apeiron_core::Chat;

use crate::dht::Dht;
use crate::item::SignedItem;
use crate::pair::{Due, Event, Pair};
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
    Ok(put_due(pair, dht, due, now))
}

/// Puts what [`Pair::due`] chose, asking first for every address used for the first time.
/// Returns the events `due` reported and those of the puts.
pub fn put_due(pair: &mut Pair, dht: &impl Dht, due: Due, now: u64) -> Vec<Event> {
    let mut events = due.events;

    // The addresses used for the first time are asked for all at once, then everything is put
    // all at once: one after another, a round took ten seconds and more.
    let fresh: Vec<&SignedItem> = due
        .items
        .iter()
        .filter(|i| due.first_puts.contains(&i.key))
        .collect();
    let keys: Vec<[u8; 32]> = fresh.iter().map(|i| i.key).collect();
    let mut not_now = HashSet::new();
    for (item, found) in fresh.iter().zip(dht.get_first_many(&keys)) {
        match found {
            Ok(Some(f)) if f.value != item.value => events.extend(pair.squatted(&item.key)),
            // Free, or holding exactly this item (put before a crash): go ahead.
            Ok(_) => {}
            // Unknown whether the address is free: not now.
            Err(_) => {
                not_now.insert(item.key);
            }
        }
    }

    // A message found squatted takes its other parts with it.
    let to_put: Vec<SignedItem> = due
        .items
        .into_iter()
        .filter(|i| pair.is_pending(&i.key) && !not_now.contains(&i.key))
        .collect();
    for (item, result) in to_put.iter().zip(dht.put_many(&to_put)) {
        if result.is_ok() {
            events.extend(pair.mark_put(&item.key, now));
        }
    }
    events
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

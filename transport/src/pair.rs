//! The transport state of one conversation (`docs/transport.md` §4–§6).
//!
//! Pure: the time is passed in, the DHT is not touched here, and every change is made to this
//! value, which the caller commits together with the Olm session in one transaction. One owner
//! per contact calls these methods one at a time — two load-modify-save cycles on one `Chat`
//! would reuse an Olm message key.
//!
//! **On an error nothing here has changed, but the `Chat` passed in may have advanced.** The
//! caller must then drop it and load the committed one, never save it.

use std::collections::{BTreeMap, BTreeSet};

use apeiron_core::vodozemac::olm::OlmMessage;
use apeiron_core::{Chat, Identity, PublicIdentity};

use crate::address::{day_of, DirectionKey};
use crate::envelope::{Body, Part, State, MAX_OLM_BYTES, MAX_PARTS};
use crate::item::{open_value, SignedItem};
use crate::TransportError;

mod persist;

/// Re-put an unacknowledged item this often (BEP 44 recommends hourly; survival ≥ 4 h measured).
pub const REPUT_EVERY_S: u64 = 3_600;

/// Stop re-putting after this long, and say "not delivered".
pub const GIVE_UP_AFTER_S: u64 = 7 * 86_400;

/// State items signed when the vault locks: today and the days after it.
pub const STATE_DAYS_AHEAD: u32 = 7;

/// Days after an invitation's expiry during which the reply is still re-put. The inviter
/// still opens a reply on the day after the expiry (its clock may lag), so the one who
/// accepted keeps it there until then.
pub const INTRO_GRACE_DAYS: u32 = 1;

/// Bytes of UTF-8 per part in a normal Olm message: the largest length whose V1 encoding fits
/// into [`MAX_OLM_BYTES`] (checked by `budgets_fit_the_envelope`).
pub const NORMAL_TEXT_BYTES: usize = 799;

/// The same for a pre-key message, which carries about a hundred bytes of session set-up.
pub const PREKEY_TEXT_BYTES: usize = 687;

/// How far above `next_recv` the state reports what has arrived.
const WINDOW: u64 = 64;

/// At most this many parts are asked for in one round.
const MAX_FETCH: u64 = 64;

/// Something the person should learn about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A whole message from the peer, identified by the index of its first part.
    Received { first: u64, text: String },
    /// The peer's indices `from..to` will never be read.
    Lost { from: u64, to: u64, why: LostWhy },
    /// The peer has every part of my message `first`.
    Delivered { first: u64 },
    /// My message `first` was given up: not delivered.
    NotDelivered { first: u64 },
    /// An address I was about to use already held something else: the schedule is reused or
    /// known to someone else. The message is not delivered.
    Squatted { first: u64 },
    /// Every part of my message `first` has reached the DHT at least once.
    Sent { first: u64 },
    /// My reply to the invitation has reached the DHT at least once.
    IntroSent,
    /// The inviter has taken my reply to its invitation: the contact is no longer "waiting".
    /// Can come after [`Event::NotAccepted`], if the inviter opened the reply on its last day.
    Accepted,
    /// Someone answered the invitation before me — whoever else saw it, or someone it was
    /// forwarded to. This contact will not be established.
    InvitationTaken,
    /// The invitation expired and the inviter never answered: the reply is not re-put any
    /// more. The pair still listens, in case the inviter took it at the last moment.
    NotAccepted,
}

/// Where the reply to an invitation stands, for the one who accepted it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntroStatus {
    /// No reply of mine: I made the invitation, or the inviter has already taken the reply.
    None,
    /// Re-put until the inviter answers.
    Waiting,
    /// Someone else answered first.
    Taken,
    /// The invitation expired unanswered.
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LostWhy {
    /// The peer stopped re-putting them before they arrived.
    GivenUp,
    /// They arrived but could not be decrypted.
    Undecryptable,
}

#[derive(Debug, Clone)]
struct Outgoing {
    item: SignedItem,
    /// First index of the message this part belongs to.
    message: u64,
    made_at: u64,
    put_at: Option<u64>,
}

#[derive(Debug, Clone)]
struct CurrentState {
    day: u32,
    state: State,
    item: SignedItem,
    put_at: Option<u64>,
}

#[derive(Debug, Clone)]
struct Assembly {
    first: u64,
    parts: u8,
    texts: Vec<String>,
    /// A part failed to decrypt: the rest are decrypted to keep Olm in step, and dropped.
    broken: bool,
}

/// Items to put now, and what happened while choosing them.
#[derive(Debug, Default)]
pub struct Due {
    pub items: Vec<SignedItem>,
    /// Keys among `items` that have never been put: ask for them first (§2).
    pub first_puts: BTreeSet<[u8; 32]>,
    pub events: Vec<Event>,
}

pub struct Pair {
    /// The Olm session the addresses are derived for; stored, and checked on restore.
    session_id: String,
    to_peer: DirectionKey,
    from_peer: DirectionKey,
    next_send: u64,
    outbox: BTreeMap<u64, Outgoing>,
    next_recv: u64,
    inbound: BTreeMap<u64, Part>,
    peer: State,
    state_seq: BTreeMap<u32, i64>,
    current: Option<CurrentState>,
    assembling: Option<Assembly>,
    /// Losses up to here have been reported.
    lost_to: u64,
    /// Whether a state of the peer has ever been read: for the one who accepted an invitation,
    /// that is the inviter's acknowledgement (`docs/transport.md` §8).
    peer_seen: bool,
    /// The reply put into an invitation's inbox, re-put until the peer is seen.
    intro: IntroState,
}

#[derive(Debug, Clone)]
struct Intro {
    item: SignedItem,
    put_at: Option<u64>,
    /// The invitation's expiry day.
    expires: u32,
}

#[derive(Debug, Clone)]
enum IntroState {
    None,
    Waiting(Intro),
    Taken,
    Expired { expires: u32 },
}

impl Pair {
    /// The transport of the conversation `session_id` between `me` and `peer`.
    pub fn new(
        me: &Identity,
        peer: &PublicIdentity,
        session_id: &str,
    ) -> Result<Self, TransportError> {
        let secret = me.pair_secret(peer)?;
        let mine = me.public();
        Ok(Self {
            session_id: session_id.to_string(),
            to_peer: DirectionKey::new(&secret, &mine, peer, session_id)?,
            from_peer: DirectionKey::new(&secret, peer, &mine, session_id)?,
            next_send: 0,
            outbox: BTreeMap::new(),
            next_recv: 0,
            inbound: BTreeMap::new(),
            peer: State::default(),
            state_seq: BTreeMap::new(),
            current: None,
            assembling: None,
            lost_to: 0,
            peer_seen: false,
            intro: IntroState::None,
        })
    }

    /// The reply to an invitation expiring on day `expires`, to be re-put with everything else
    /// until the peer answers — or until [`INTRO_GRACE_DAYS`] after the expiry.
    pub fn with_intro(mut self, reply: SignedItem, expires: u32) -> Self {
        self.intro = IntroState::Waiting(Intro {
            item: reply,
            put_at: None,
            expires,
        });
        self
    }

    /// Whether a state of the peer has ever been read. For the one who accepted an invitation:
    /// whether the inviter has taken the reply — until then the contact is "waiting".
    pub fn peer_seen(&self) -> bool {
        self.peer_seen
    }

    /// The Olm session this pair belongs to.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Where my reply to an invitation stands.
    pub fn intro_status(&self) -> IntroStatus {
        match self.intro {
            IntroState::None => IntroStatus::None,
            IntroState::Waiting(_) => IntroStatus::Waiting,
            IntroState::Taken => IntroStatus::Taken,
            IntroState::Expired { .. } => IntroStatus::Expired,
        }
    }

    /// Whether nothing will ever come of this pair: someone else took the invitation, or it
    /// expired unanswered and the inviter stayed silent for as long as a message is re-put.
    pub fn is_closed(&self, now: u64) -> bool {
        match self.intro {
            IntroState::Taken => true,
            IntroState::Expired { expires } => {
                let give_up_days = u32::try_from(GIVE_UP_AFTER_S / 86_400).unwrap_or(u32::MAX);
                day_of(now)
                    > expires
                        .saturating_add(INTRO_GRACE_DAYS)
                        .saturating_add(give_up_days)
            }
            _ => false,
        }
    }

    // ─── Sending ─────────────────────────────────────────────────────────────

    /// Encrypts `text` into parts and signs them. Returns the index of the first part, which
    /// identifies the message. Nothing is put here: see [`Pair::due`].
    pub fn send(&mut self, chat: &mut Chat, text: &str, now: u64) -> Result<u64, TransportError> {
        let budget = if chat.sends_prekey_messages() {
            PREKEY_TEXT_BYTES
        } else {
            NORMAL_TEXT_BYTES
        };
        let chunks = split(text, budget);
        let parts = u8::try_from(chunks.len())
            .ok()
            .filter(|&p| p <= MAX_PARTS)
            .ok_or(TransportError::TooLong {
                parts: chunks.len(),
            })?;

        let first = self.next_send;
        let mut staged = Vec::with_capacity(chunks.len());
        for (k, chunk) in chunks.iter().enumerate() {
            let (olm_type, olm) = chat.encrypt(chunk)?.to_parts();
            if olm.len() > MAX_OLM_BYTES {
                return Err(TransportError::Internal(
                    "an Olm message outgrew its budget",
                ));
            }
            let part = u8::try_from(k).map_err(|_| TransportError::Internal("part number"))?;
            let index = first
                .checked_add(u64::from(part))
                .ok_or(TransportError::Internal("index overflow"))?;
            let body = Body::Part(Part {
                part,
                parts,
                olm_type: u8::try_from(olm_type)
                    .map_err(|_| TransportError::Internal("Olm type"))?,
                olm,
            });
            let item = SignedItem::seal(&self.to_peer.message(index)?, 1, &body)?;
            staged.push((index, item));
        }

        for (index, item) in staged {
            self.outbox.insert(
                index,
                Outgoing {
                    item,
                    message: first,
                    made_at: now,
                    put_at: None,
                },
            );
        }
        self.next_send = first.saturating_add(u64::from(parts));
        Ok(first)
    }

    /// What to put now: parts never put, re-puts that are due, and the state item. Messages
    /// older than [`GIVE_UP_AFTER_S`] are given up here.
    pub fn due(&mut self, now: u64) -> Result<Due, TransportError> {
        let mut due = Due::default();

        let expired: Vec<u64> = self
            .outbox
            .iter()
            .filter(|(_, o)| o.made_at.saturating_add(GIVE_UP_AFTER_S) <= now)
            .map(|(_, o)| o.message)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        for message in expired {
            self.outbox.retain(|_, o| o.message != message);
            due.events.push(Event::NotDelivered { first: message });
        }

        for o in self.outbox.values() {
            match o.put_at {
                None => {
                    due.first_puts.insert(o.item.key);
                    due.items.push(o.item.clone());
                }
                Some(t) if t.saturating_add(REPUT_EVERY_S) <= now => due.items.push(o.item.clone()),
                Some(_) => {}
            }
        }

        let current = self.current_state(now)?;
        match current.put_at {
            None => due.items.push(current.item.clone()),
            Some(t) if t.saturating_add(REPUT_EVERY_S) <= now => {
                due.items.push(current.item.clone())
            }
            Some(_) => {}
        }

        if let IntroState::Waiting(intro) = &self.intro {
            if day_of(now) > intro.expires.saturating_add(INTRO_GRACE_DAYS) {
                self.intro = IntroState::Expired {
                    expires: intro.expires,
                };
                due.events.push(Event::NotAccepted);
            }
        }
        if let IntroState::Waiting(intro) = &self.intro {
            match intro.put_at {
                None => {
                    due.first_puts.insert(intro.item.key);
                    due.items.push(intro.item.clone());
                }
                Some(t) if t.saturating_add(REPUT_EVERY_S) <= now => {
                    due.items.push(intro.item.clone())
                }
                Some(_) => {}
            }
        }
        Ok(due)
    }

    /// The item with `key` reached the DHT at `now`. Reports a message whose last part not yet
    /// put has just been put for the first time, and the reply's first put.
    pub fn mark_put(&mut self, key: &[u8; 32], now: u64) -> Option<Event> {
        let mut first_put_of = None;
        for o in self.outbox.values_mut() {
            if &o.item.key == key {
                if o.put_at.is_none() {
                    first_put_of = Some(o.message);
                }
                o.put_at = Some(now);
            }
        }
        if let Some(c) = self.current.as_mut() {
            if &c.item.key == key {
                c.put_at = Some(now);
            }
        }
        if let IntroState::Waiting(i) = &mut self.intro {
            if &i.item.key == key {
                let first = i.put_at.is_none();
                i.put_at = Some(now);
                if first {
                    return Some(Event::IntroSent);
                }
            }
        }
        let message = first_put_of?;
        self.outbox
            .values()
            .filter(|o| o.message == message)
            .all(|o| o.put_at.is_some())
            .then_some(Event::Sent { first: message })
    }

    fn intro_key(&self) -> Option<&[u8; 32]> {
        match &self.intro {
            IntroState::Waiting(i) => Some(&i.item.key),
            _ => None,
        }
    }

    /// Whether the item with `key` is still one to put.
    pub fn is_pending(&self, key: &[u8; 32]) -> bool {
        self.outbox.values().any(|o| &o.item.key == key)
            || self.current.as_ref().is_some_and(|c| &c.item.key == key)
            || self.intro_key() == Some(key)
    }

    /// The address `key`, about to be used for the first time, already holds something else.
    /// Its message is not put and is reported.
    pub fn squatted(&mut self, key: &[u8; 32]) -> Option<Event> {
        if self.intro_key() == Some(key) {
            // Someone answered this invitation first: whoever else saw its secret, or someone
            // it was forwarded to. Our reply would never land (BEP 44 keeps the first).
            self.intro = IntroState::Taken;
            return Some(Event::InvitationTaken);
        }
        let message = self
            .outbox
            .values()
            .find(|o| &o.item.key == key)
            .map(|o| o.message)?;
        self.outbox.retain(|_, o| o.message != message);
        Some(Event::Squatted { first: message })
    }

    /// State items for today and the next days, with the values of this moment, for the
    /// background job to put while the vault is locked (§5).
    pub fn items_for_lock(&mut self, now: u64) -> Result<Vec<SignedItem>, TransportError> {
        let today = day_of(now);
        let mut out = vec![self.current_state(now)?.item.clone()];
        let state = self.state();
        for d in 1..STATE_DAYS_AHEAD {
            let day = today.saturating_add(d);
            let seq = self.next_state_seq(day, today);
            out.push(SignedItem::seal(
                &self.to_peer.state(day)?,
                seq,
                &Body::State(state),
            )?);
        }
        out.extend(self.outbox.values().map(|o| o.item.clone()));
        if let IntroState::Waiting(i) = &self.intro {
            out.push(i.item.clone());
        }
        Ok(out)
    }

    /// The next `seq` for the state address of `day`. The counter of every day is kept until
    /// that day is two days in the past — counted from `today`, not from `day`, or signing the
    /// days ahead at lock would forget today's counter and send `seq` backwards.
    fn next_state_seq(&mut self, day: u32, today: u32) -> i64 {
        let seq = self
            .state_seq
            .get(&day)
            .copied()
            .unwrap_or(0)
            .saturating_add(1);
        self.state_seq.insert(day, seq);
        let keep_from = today.saturating_sub(2);
        self.state_seq.retain(|&d, _| d >= keep_from);
        seq
    }

    /// The state item for today, made anew when the state changed or the day did.
    fn current_state(&mut self, now: u64) -> Result<&CurrentState, TransportError> {
        let day = day_of(now);
        let state = self.state();
        let stale = match &self.current {
            Some(c) => c.day != day || c.state != state,
            None => true,
        };
        if stale {
            let seq = self.next_state_seq(day, day);
            let item = SignedItem::seal(&self.to_peer.state(day)?, seq, &Body::State(state))?;
            self.current = Some(CurrentState {
                day,
                state,
                item,
                put_at: None,
            });
        }
        self.current
            .as_ref()
            .ok_or(TransportError::Internal("no current state"))
    }

    /// What this side tells the peer.
    pub fn state(&self) -> State {
        let mut recv_bits = 0u64;
        for &index in self.inbound.keys() {
            let offset = index.saturating_sub(self.next_recv);
            if (1..=WINDOW).contains(&offset) {
                recv_bits |= 1u64 << (offset - 1);
            }
        }
        State {
            next_recv: self.next_recv,
            recv_bits,
            next_send: self.next_send,
            send_floor: self.send_floor(),
        }
    }

    fn send_floor(&self) -> u64 {
        self.outbox.keys().next().copied().unwrap_or(self.next_send)
    }

    // ─── Receiving ───────────────────────────────────────────────────────────

    /// The peer's state addresses to ask this round: today's, and around UTC midnight also
    /// the neighbouring day's, since the two clocks need not agree.
    pub fn peer_state_keys(&self, now: u64) -> Result<Vec<([u8; 32], u32)>, TransportError> {
        let day = day_of(now);
        let into_day = now % 86_400;
        let mut days = vec![day];
        if into_day < 3_600 {
            days.push(day.saturating_sub(1));
        }
        if into_day >= 86_400 - 3_600 {
            days.push(day.saturating_add(1));
        }
        days.into_iter()
            .map(|d| Ok((self.from_peer.state(d)?.public_key(), d)))
            .collect()
    }

    /// A state item found at the peer's state address of `day`.
    pub fn on_peer_state(&mut self, day: u32, value: &[u8]) -> Result<Vec<Event>, TransportError> {
        let Body::State(s) = open_value(&self.from_peer.state(day)?, value)? else {
            return Err(TransportError::NotOurs);
        };
        // Every field only grows in truth; an older item (yesterday's, a stale re-put) must not
        // take anything back.
        let recv_bits = match s.next_recv.cmp(&self.peer.next_recv) {
            std::cmp::Ordering::Greater => s.recv_bits,
            std::cmp::Ordering::Equal => s.recv_bits | self.peer.recv_bits,
            std::cmp::Ordering::Less => self.peer.recv_bits,
        };
        self.peer = State {
            // The peer cannot have more of mine than I have sent.
            next_recv: s.next_recv.max(self.peer.next_recv).min(self.next_send),
            recv_bits,
            next_send: s.next_send.max(self.peer.next_send),
            send_floor: s.send_floor.max(self.peer.send_floor),
        };
        let mut events = Vec::new();
        if !self.peer_seen {
            self.peer_seen = true;
            // The inviter has taken the reply: it need not be re-put any more. Also after the
            // expiry: the inviter may have opened it on its last day.
            if matches!(
                self.intro,
                IntroState::Waiting(_) | IntroState::Expired { .. }
            ) {
                self.intro = IntroState::None;
                events.push(Event::Accepted);
            }
        }
        events.extend(self.take_acknowledged());
        Ok(events)
    }

    /// Removes what the peer has; reports messages all of whose parts it has.
    fn take_acknowledged(&mut self) -> Vec<Event> {
        let peer = self.peer;
        let has = |index: u64| {
            index < peer.next_recv || {
                let offset = index.saturating_sub(peer.next_recv);
                (1..=WINDOW).contains(&offset) && peer.recv_bits & (1u64 << (offset - 1)) != 0
            }
        };
        let touched: BTreeSet<u64> = self
            .outbox
            .iter()
            .filter(|(&i, _)| has(i))
            .map(|(_, o)| o.message)
            .collect();
        self.outbox.retain(|&i, _| !has(i));
        touched
            .into_iter()
            .filter(|m| !self.outbox.values().any(|o| o.message == *m))
            .map(|first| Event::Delivered { first })
            .collect()
    }

    /// The peer's parts to ask for now, with their addresses.
    ///
    /// Until the peer's state has been read once, `next_recv` itself is asked for blindly. After
    /// that the state is trusted: asking for an address nobody has put yet waits for the whole
    /// DHT query, seconds of every idle round, for nothing.
    pub fn wanted(&self) -> Result<Vec<(u64, [u8; 32])>, TransportError> {
        let probe = if self.peer_seen {
            self.next_recv
        } else {
            self.next_recv.saturating_add(1)
        };
        let end = self
            .peer
            .next_send
            .max(probe)
            .min(self.next_recv.saturating_add(MAX_FETCH));
        (self.next_recv..end)
            .filter(|i| !self.inbound.contains_key(i))
            .map(|i| Ok((i, self.from_peer.message(i)?.public_key())))
            .collect()
    }

    /// A value found at the address of the peer's part `index`. Returns whether it was new.
    /// Duplicates are dropped here, by index, before Olm could ever see them.
    pub fn on_part(&mut self, index: u64, value: &[u8]) -> Result<bool, TransportError> {
        if index < self.next_recv || self.inbound.contains_key(&index) {
            return Ok(false);
        }
        let Body::Part(part) = open_value(&self.from_peer.message(index)?, value)? else {
            return Err(TransportError::NotOurs);
        };
        self.inbound.insert(index, part);
        Ok(true)
    }

    /// Decrypts what can be decrypted: strictly in index order, from `next_recv`. A gap is
    /// waited for until the peer's floor passes it; then it is reported lost and skipped.
    pub fn receive(&mut self, chat: &mut Chat) -> Vec<Event> {
        let mut events = Vec::new();
        loop {
            let index = self.next_recv;
            if let Some(part) = self.inbound.remove(&index) {
                self.take_part(chat, index, part, &mut events);
                self.next_recv = index.saturating_add(1);
                continue;
            }
            if index < self.peer.send_floor {
                // Gone for good: the peer gave up on everything below its floor. Up to the next
                // part that did arrive, or to the floor.
                let to = self
                    .inbound
                    .keys()
                    .next()
                    .copied()
                    .unwrap_or(self.peer.send_floor)
                    .min(self.peer.send_floor);
                let from = self.assembling.take().map_or(index, |a| a.first);
                report_lost(&mut events, &mut self.lost_to, from, to, LostWhy::GivenUp);
                self.next_recv = to;
                continue;
            }
            break;
        }
        events
    }

    fn take_part(&mut self, chat: &mut Chat, index: u64, part: Part, events: &mut Vec<Event>) {
        let first = index.saturating_sub(u64::from(part.part));
        let end = first.saturating_add(u64::from(part.parts));

        // Olm sees every part, in order, even of a message that cannot be shown: skipping one
        // would leave a skipped key behind for nothing.
        let text = OlmMessage::from_parts(usize::from(part.olm_type), &part.olm)
            .map_err(|_| ())
            .and_then(|m| chat.decrypt(&m).map_err(|_| ()));

        let continues = matches!(&self.assembling, Some(a) if a.first == first);
        if !continues {
            // Another message was unfinished: the rest of it is gone.
            if let Some(old) = self.assembling.take() {
                report_lost(
                    events,
                    &mut self.lost_to,
                    old.first,
                    first,
                    LostWhy::GivenUp,
                );
            }
            // The beginning of this one never arrived.
            if part.part != 0 {
                report_lost(events, &mut self.lost_to, first, end, LostWhy::GivenUp);
            }
            self.assembling = Some(Assembly {
                first,
                parts: part.parts,
                texts: Vec::new(),
                broken: part.part != 0,
            });
        }

        let Some(a) = self.assembling.as_mut() else {
            return;
        };
        match text {
            Ok(t) => {
                if !a.broken {
                    a.texts.push(t);
                }
            }
            Err(()) => {
                if !a.broken {
                    a.broken = true;
                    report_lost(
                        events,
                        &mut self.lost_to,
                        first,
                        end,
                        LostWhy::Undecryptable,
                    );
                }
            }
        }
        if part.part.saturating_add(1) >= a.parts {
            if let Some(done) = self.assembling.take() {
                if !done.broken {
                    events.push(Event::Received {
                        first: done.first,
                        text: done.texts.concat(),
                    });
                }
            }
        }
    }

    /// How many of my messages' parts wait for an acknowledgement.
    pub fn waiting(&self) -> usize {
        self.outbox.len()
    }

    /// How many of the peer's indices have been dealt with.
    pub fn next_recv(&self) -> u64 {
        self.next_recv
    }
}

/// Reports the peer's indices `from..to` as lost, except what an earlier report already
/// covered: one loss is told once.
fn report_lost(events: &mut Vec<Event>, lost_to: &mut u64, from: u64, to: u64, why: LostWhy) {
    let from = from.max(*lost_to);
    if from < to {
        events.push(Event::Lost { from, to, why });
        *lost_to = to;
    }
}

/// Cuts `text` into pieces of at most `budget` bytes of UTF-8, on character boundaries. An
/// empty text is one empty piece.
fn split(text: &str, budget: usize) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while rest.len() > budget {
        let mut cut = budget;
        while cut > 0 && !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        let (head, tail) = rest.split_at(cut);
        out.push(head);
        rest = tail;
    }
    out.push(rest);
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn split_keeps_characters_whole_and_everything() {
        let text = "ж".repeat(10) + "abc";
        for budget in [3, 4, 5, 7] {
            let parts = split(&text, budget);
            assert_eq!(parts.concat(), text);
            assert!(parts.iter().all(|p| p.len() <= budget));
        }
        assert_eq!(split("", 10), vec![""]);
        assert_eq!(split("abc", 3), vec!["abc"]);
    }
}

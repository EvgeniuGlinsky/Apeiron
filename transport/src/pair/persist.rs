//! The pair's state as bytes, for the `pair_state` table (`docs/transport.md` §9).
//!
//! A versioned binary layout, written and read field by field with every length checked. The
//! direction keys are **not** in it: they are derived again from the two identities and the
//! session id when the pair is restored, so the stored state holds no secret of its own and
//! cannot be paired with keys it was not made with.
//!
//! What it does hold is sensitive all the same — the parts of an unfinished message, already
//! decrypted — so the bytes come in [`Zeroizing`] and the storage seals them under the PIN key.

use std::collections::BTreeMap;

use apeiron_core::{Identity, PublicIdentity};
use zeroize::Zeroizing;

use super::{Assembly, CurrentState, Intro, IntroState, Outgoing, Pair};
use crate::address::DirectionKey;
use crate::envelope::{Part, State};
use crate::item::SignedItem;
use crate::TransportError;

const FORMAT: u8 = 1;

struct Writer(Zeroizing<Vec<u8>>);

impl Writer {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    fn i64(&mut self, v: i64) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    fn bytes(&mut self, v: &[u8]) -> Result<(), TransportError> {
        self.u32(u32::try_from(v.len()).map_err(|_| TransportError::Internal("too long"))?);
        self.0.extend_from_slice(v);
        Ok(())
    }
    fn count(&mut self, n: usize) -> Result<(), TransportError> {
        self.u32(u32::try_from(n).map_err(|_| TransportError::Internal("too many"))?);
        Ok(())
    }
    fn when(&mut self, v: Option<u64>) {
        match v {
            Some(t) => {
                self.u8(1);
                self.u64(t);
            }
            None => self.u8(0),
        }
    }
    fn state(&mut self, s: &State) {
        for v in [s.next_recv, s.recv_bits, s.next_send, s.send_floor] {
            self.u64(v);
        }
    }
    fn item(&mut self, i: &SignedItem) -> Result<(), TransportError> {
        self.bytes(&i.to_bytes())
    }
}

struct Reader<'a> {
    buf: &'a [u8],
}

fn corrupt(what: &'static str) -> TransportError {
    TransportError::Corrupt(what)
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], TransportError> {
        let (head, rest) = self
            .buf
            .split_at_checked(n)
            .ok_or_else(|| corrupt("truncated"))?;
        self.buf = rest;
        Ok(head)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], TransportError> {
        self.take(N)?.try_into().map_err(|_| corrupt("truncated"))
    }
    fn u8(&mut self) -> Result<u8, TransportError> {
        let [b] = self.array::<1>()?;
        Ok(b)
    }
    fn u16(&mut self) -> Result<u16, TransportError> {
        Ok(u16::from_be_bytes(self.array()?))
    }
    fn u32(&mut self) -> Result<u32, TransportError> {
        Ok(u32::from_be_bytes(self.array()?))
    }
    fn u64(&mut self) -> Result<u64, TransportError> {
        Ok(u64::from_be_bytes(self.array()?))
    }
    fn i64(&mut self) -> Result<i64, TransportError> {
        Ok(i64::from_be_bytes(self.array()?))
    }
    fn flag(&mut self) -> Result<bool, TransportError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(corrupt("flag")),
        }
    }
    fn bytes(&mut self) -> Result<&'a [u8], TransportError> {
        let n = usize::try_from(self.u32()?).map_err(|_| corrupt("length"))?;
        self.take(n)
    }
    fn count(&mut self) -> Result<usize, TransportError> {
        let n = usize::try_from(self.u32()?).map_err(|_| corrupt("count"))?;
        // Every entry takes at least one byte: a count beyond what is left is a lie.
        if n > self.buf.len() {
            return Err(corrupt("count beyond the data"));
        }
        Ok(n)
    }
    fn when(&mut self) -> Result<Option<u64>, TransportError> {
        Ok(if self.flag()? {
            Some(self.u64()?)
        } else {
            None
        })
    }
    fn state(&mut self) -> Result<State, TransportError> {
        Ok(State {
            next_recv: self.u64()?,
            recv_bits: self.u64()?,
            next_send: self.u64()?,
            send_floor: self.u64()?,
        })
    }
    fn item(&mut self) -> Result<SignedItem, TransportError> {
        SignedItem::from_bytes(self.bytes()?)
    }
}

impl Pair {
    /// Everything but the keys, for the storage to seal.
    pub fn to_bytes(&self) -> Result<Zeroizing<Vec<u8>>, TransportError> {
        let mut w = Writer(Zeroizing::new(Vec::new()));
        w.u8(FORMAT);
        w.bytes(self.session_id.as_bytes())?;
        w.u64(self.next_send);
        w.u64(self.next_recv);
        w.u64(self.lost_to);
        w.u8(u8::from(self.peer_seen));
        w.state(&self.peer);

        w.count(self.state_seq.len())?;
        for (&day, &seq) in &self.state_seq {
            w.u32(day);
            w.i64(seq);
        }

        match &self.current {
            Some(c) => {
                w.u8(1);
                w.u32(c.day);
                w.state(&c.state);
                w.item(&c.item)?;
                w.when(c.put_at);
            }
            None => w.u8(0),
        }

        w.count(self.outbox.len())?;
        for (&index, o) in &self.outbox {
            w.u64(index);
            w.u64(o.message);
            w.u64(o.made_at);
            w.when(o.put_at);
            w.item(&o.item)?;
        }

        w.count(self.inbound.len())?;
        for (&index, p) in &self.inbound {
            w.u64(index);
            w.u8(p.part);
            w.u8(p.parts);
            w.u8(p.olm_type);
            w.bytes(&p.olm)?;
        }

        match &self.assembling {
            Some(a) => {
                w.u8(1);
                w.u64(a.first);
                w.u8(a.parts);
                w.u8(u8::from(a.broken));
                w.count(a.texts.len())?;
                for t in &a.texts {
                    w.bytes(t.as_bytes())?;
                }
            }
            None => w.u8(0),
        }

        match &self.intro {
            IntroState::None => w.u8(0),
            IntroState::Waiting(i) => {
                w.u8(1);
                w.item(&i.item)?;
                w.when(i.put_at);
                w.u32(i.expires);
            }
            IntroState::Taken => w.u8(2),
            IntroState::Expired { expires } => {
                w.u8(3);
                w.u32(*expires);
            }
        }
        w.u16(0x0a0e); // end mark: a cut record does not end here by chance
        Ok(w.0)
    }

    /// The pair of `me` and `peer` in conversation `session_id`, as [`Pair::to_bytes`] left it.
    /// The keys are derived anew; nothing in `bytes` can stand in for them.
    pub fn restore(
        me: &Identity,
        peer: &PublicIdentity,
        session_id: &str,
        bytes: &[u8],
    ) -> Result<Self, TransportError> {
        let secret = me.pair_secret(peer)?;
        let mine = me.public();
        let mut r = Reader { buf: bytes };
        if r.u8()? != FORMAT {
            return Err(corrupt("unknown format"));
        }
        // Keys derived for another session would silently read and write someone else's
        // addresses: the state belongs to the session it was stored with, or to none.
        if r.bytes()? != session_id.as_bytes() {
            return Err(corrupt("the state of another session"));
        }
        let next_send = r.u64()?;
        let next_recv = r.u64()?;
        let lost_to = r.u64()?;
        let peer_seen = r.flag()?;
        let peer_state = r.state()?;

        let mut state_seq = BTreeMap::new();
        for _ in 0..r.count()? {
            state_seq.insert(r.u32()?, r.i64()?);
        }

        let current = if r.flag()? {
            Some(CurrentState {
                day: r.u32()?,
                state: r.state()?,
                item: r.item()?,
                put_at: r.when()?,
            })
        } else {
            None
        };

        let mut outbox = BTreeMap::new();
        for _ in 0..r.count()? {
            let index = r.u64()?;
            let message = r.u64()?;
            let made_at = r.u64()?;
            let put_at = r.when()?;
            let item = r.item()?;
            outbox.insert(
                index,
                Outgoing {
                    item,
                    message,
                    made_at,
                    put_at,
                },
            );
        }

        let mut inbound = BTreeMap::new();
        for _ in 0..r.count()? {
            let index = r.u64()?;
            let part = Part {
                part: r.u8()?,
                parts: r.u8()?,
                olm_type: r.u8()?,
                olm: r.bytes()?.to_vec(),
            };
            inbound.insert(index, part);
        }

        let assembling = if r.flag()? {
            let first = r.u64()?;
            let parts = r.u8()?;
            let broken = r.flag()?;
            let mut texts = Vec::new();
            for _ in 0..r.count()? {
                texts.push(String::from_utf8(r.bytes()?.to_vec()).map_err(|_| corrupt("text"))?);
            }
            Some(Assembly {
                first,
                parts,
                texts,
                broken,
            })
        } else {
            None
        };

        let intro = match r.u8()? {
            0 => IntroState::None,
            1 => IntroState::Waiting(Intro {
                item: r.item()?,
                put_at: r.when()?,
                expires: r.u32()?,
            }),
            2 => IntroState::Taken,
            3 => IntroState::Expired { expires: r.u32()? },
            _ => return Err(corrupt("reply state")),
        };
        if r.u16()? != 0x0a0e || !r.buf.is_empty() {
            return Err(corrupt("does not end where it should"));
        }

        Ok(Self {
            session_id: session_id.to_string(),
            to_peer: DirectionKey::new(&secret, &mine, peer, session_id)?,
            from_peer: DirectionKey::new(&secret, peer, &mine, session_id)?,
            next_send,
            outbox,
            next_recv,
            inbound,
            peer: peer_state,
            state_seq,
            current,
            assembling,
            lost_to,
            peer_seen,
            intro,
        })
    }
}

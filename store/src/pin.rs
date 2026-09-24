//! Attempt counter and delays for the PIN (R-011).
//!
//! # What this protects against, and what it does not
//!
//! Against someone who holds the unlocked phone and can only use the app's own screen,
//! the counter and the delays are the whole defence: after five free attempts every
//! further one costs 30 s, 1 min, 5 min, 15 min, and then an hour each.
//!
//! Against someone with root on the phone the counter is worthless, and this module does
//! not pretend otherwise: the file can simply be put back. What holds there is the cost of
//! one attempt — the hardware chain in `wrapper` — not this file.
//!
//! # Why the attempt is counted before the verdict
//!
//! The counter is increased and flushed to disk **before** the PIN is checked. Otherwise
//! cutting power at the moment the verdict is known leaves the attempt uncounted — the way
//! the iOS passcode counter was once bypassed. A transient hardware failure that happens
//! before the chain produced its result is not a verdict, and only then is the previous
//! state put back (see [`PinState::restore_after_transient`]).
//!
//! # Clock
//!
//! Delays are measured on the boot clock (`elapsedRealtime`), not on wall time: changing
//! the system time does not shorten them. After a reboot the delay starts over from the
//! first reading in the new boot — never shorter, and never longer than one step. A reboot
//! also costs the holder the unlocked state of the phone.

use std::path::Path;

pub use apeiron_platform::BootClock;

use crate::fsutil::write_atomically;
use crate::StorageError;

/// File name of the counter, next to the wrapper file.
pub const PIN_STATE_FILE: &str = "pin.state";

/// Attempts that cost nothing.
pub const FREE_ATTEMPTS: u32 = 5;

/// Identifies the file. Six bytes, so that a stray file does not parse.
const MAGIC: &[u8; 6] = b"APPIN1";

/// `magic ‖ consecutive (4) ‖ total (4) ‖ has_anchor (1) ‖ boot (8) ‖ elapsed (8) ‖ writer (8)`.
const FILE_BYTES: usize = 6 + 4 + 4 + 1 + 8 + 8 + 8;

/// Delay before the next attempt after `consecutive` failures in a row.
pub fn delay_ms(consecutive: u32) -> u64 {
    match consecutive {
        0..=FREE_ATTEMPTS => 0,
        6 => 30_000,
        7 => 60_000,
        8 => 5 * 60_000,
        9 => 15 * 60_000,
        _ => 60 * 60_000,
    }
}

/// What the counter file says.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PinState {
    /// Failures since the last successful unlock.
    pub consecutive: u32,
    /// Failures over the whole life of the vault. Only for the report.
    pub total: u32,
    /// Where the current delay started. `None` when there is nothing to wait for, or when
    /// the start is not known yet (then the full delay is counted from now).
    pub anchor: Option<BootClock>,
    /// Random mark of the process that wrote the file. Lets the self-check tell "the count
    /// survived a restart" from "this same process remembers it".
    pub writer: [u8; 8],
}

/// Whether an attempt may be made now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gate {
    /// Go ahead.
    Open,
    /// Wait this long. If `reanchored` is set, the caller must save it before answering:
    /// the delay has just been (re)started at this reading.
    Wait {
        remaining_ms: u64,
        reanchored: Option<PinState>,
    },
}

impl PinState {
    /// Reads the counter.
    ///
    /// A missing or unreadable file while a vault exists is not "zero failures": nobody
    /// but the app writes here, so its absence means loss or tampering, and the answer is
    /// the first step of the delay rather than a fresh start.
    pub fn load(dir: &Path) -> Self {
        match std::fs::read(dir.join(PIN_STATE_FILE)) {
            Ok(raw) => Self::from_bytes(&raw).unwrap_or_else(Self::lost),
            Err(_) => Self::lost(),
        }
    }

    /// The state assumed when the file is gone.
    pub fn lost() -> Self {
        Self {
            consecutive: FREE_ATTEMPTS + 1,
            total: 0,
            anchor: None,
            writer: [0; 8],
        }
    }

    /// Writes the counter atomically and flushes it.
    pub fn save(&self, dir: &Path) -> Result<(), StorageError> {
        write_atomically(dir, &dir.join(PIN_STATE_FILE), &self.to_bytes())
    }

    /// Whether an attempt may be made at `now`.
    pub fn gate(&self, now: BootClock) -> Gate {
        let delay = delay_ms(self.consecutive);
        if delay == 0 {
            return Gate::Open;
        }
        match self.anchor {
            Some(a) if same_boot(a, now) => {
                let passed = u64::try_from(now.elapsed_ms - a.elapsed_ms).unwrap_or(0);
                if passed >= delay {
                    Gate::Open
                } else {
                    Gate::Wait {
                        remaining_ms: delay - passed,
                        reanchored: None,
                    }
                }
            }
            // No anchor yet, or the phone rebooted: the full delay starts now. Never
            // longer than one step, whatever the uptime was before.
            _ => Gate::Wait {
                remaining_ms: delay,
                reanchored: Some(Self {
                    anchor: Some(now),
                    ..self.clone()
                }),
            },
        }
    }

    /// The state to write **before** checking the PIN: the attempt counted as a failure.
    pub fn begin_attempt(&self, now: BootClock, writer: [u8; 8]) -> Self {
        Self {
            consecutive: self.consecutive.saturating_add(1),
            total: self.total.saturating_add(1),
            anchor: Some(now),
            writer,
        }
    }

    /// The state after a successful unlock.
    pub fn after_success(&self, writer: [u8; 8]) -> Self {
        Self {
            consecutive: 0,
            total: self.total.saturating_sub(1),
            anchor: None,
            writer,
        }
    }

    /// The state to put back when the hardware failed before producing a result.
    ///
    /// Allowed only when the chain did **not** finish: after that the attempt is a verdict,
    /// whatever happens next.
    pub fn restore_after_transient(previous: &Self) -> Self {
        previous.clone()
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(FILE_BYTES);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&self.consecutive.to_be_bytes());
        out.extend_from_slice(&self.total.to_be_bytes());
        let anchor = self.anchor.unwrap_or(BootClock {
            boot_count: 0,
            elapsed_ms: 0,
        });
        out.push(u8::from(self.anchor.is_some()));
        out.extend_from_slice(&anchor.boot_count.to_be_bytes());
        out.extend_from_slice(&anchor.elapsed_ms.to_be_bytes());
        out.extend_from_slice(&self.writer);
        out
    }

    fn from_bytes(raw: &[u8]) -> Option<Self> {
        if raw.len() != FILE_BYTES || raw.get(..6)? != MAGIC.as_slice() {
            return None;
        }
        let u32_at = |at: usize| raw.get(at..at + 4)?.try_into().ok().map(u32::from_be_bytes);
        let i64_at = |at: usize| raw.get(at..at + 8)?.try_into().ok().map(i64::from_be_bytes);
        let has_anchor = *raw.get(14)? == 1;
        Some(Self {
            consecutive: u32_at(6)?,
            total: u32_at(10)?,
            anchor: has_anchor.then_some(BootClock {
                boot_count: i64_at(15)?,
                elapsed_ms: i64_at(23)?,
            }),
            writer: raw.get(31..39)?.try_into().ok()?,
        })
    }
}

/// Whether two readings belong to the same boot.
///
/// The elapsed time going backwards is the primary sign of a reboot; the boot counter is
/// the second one and is used only when both readings have it.
fn same_boot(a: BootClock, b: BootClock) -> bool {
    if b.elapsed_ms < a.elapsed_ms {
        return false;
    }
    if a.boot_count >= 0 && b.boot_count >= 0 {
        return a.boot_count == b.boot_count;
    }
    true
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    const MARK: [u8; 8] = [7; 8];

    fn at(boot: i64, ms: i64) -> BootClock {
        BootClock {
            boot_count: boot,
            elapsed_ms: ms,
        }
    }

    fn failed(n: u32, anchor: BootClock) -> PinState {
        PinState {
            consecutive: n,
            total: n,
            anchor: Some(anchor),
            writer: MARK,
        }
    }

    #[test]
    fn five_attempts_are_free() {
        for n in 0..=FREE_ATTEMPTS {
            assert_eq!(failed(n, at(1, 0)).gate(at(1, 1)), Gate::Open, "n = {n}");
        }
    }

    #[test]
    fn the_sixth_waits_thirty_seconds() {
        let s = failed(6, at(1, 1_000));
        assert_eq!(
            s.gate(at(1, 11_000)),
            Gate::Wait {
                remaining_ms: 20_000,
                reanchored: None
            }
        );
        assert_eq!(s.gate(at(1, 31_000)), Gate::Open);
    }

    #[test]
    fn schedule_grows_and_stops_at_an_hour() {
        let steps: Vec<u64> = (6..=12).map(delay_ms).collect();
        assert_eq!(
            steps,
            [30_000, 60_000, 300_000, 900_000, 3_600_000, 3_600_000, 3_600_000]
        );
    }

    #[test]
    fn reboot_restarts_the_delay_but_never_lengthens_it() {
        // Anchor late in a long uptime; after reboot the clock is small again.
        let s = failed(10, at(4, 40 * 24 * 3_600_000));
        match s.gate(at(5, 60_000)) {
            Gate::Wait {
                remaining_ms,
                reanchored: Some(new),
            } => {
                assert_eq!(remaining_ms, delay_ms(10));
                assert_eq!(new.anchor, Some(at(5, 60_000)));
            }
            other => panic!("expected a restarted delay, got {other:?}"),
        }
    }

    #[test]
    fn reboot_is_seen_without_a_boot_counter() {
        let s = failed(6, at(-1, 90_000));
        assert!(matches!(
            s.gate(at(-1, 5_000)),
            Gate::Wait {
                reanchored: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn moving_wall_time_does_not_help() {
        // Only the boot clock is consulted; there is no wall time to move.
        let s = failed(8, at(1, 0));
        assert!(matches!(s.gate(at(1, 299_999)), Gate::Wait { .. }));
    }

    #[test]
    fn attempt_is_counted_before_the_verdict_and_success_resets() {
        let s = PinState::default();
        let during = s.begin_attempt(at(1, 10), MARK);
        assert_eq!(during.consecutive, 1);
        assert_eq!(during.total, 1);
        let after = during.after_success(MARK);
        assert_eq!(after.consecutive, 0);
        assert_eq!(after.total, 0, "a successful attempt is not a failure");
        assert_eq!(after.anchor, None);
    }

    #[test]
    fn lost_file_is_not_a_fresh_start() {
        let dir = tempfile::tempdir().expect("temp dir");
        let s = PinState::load(dir.path());
        assert_eq!(s, PinState::lost());
        assert!(matches!(s.gate(at(1, 0)), Gate::Wait { .. }));
    }

    #[test]
    fn corrupt_file_is_treated_as_lost() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join(PIN_STATE_FILE), b"APPIN1 garbage").expect("write");
        assert_eq!(PinState::load(dir.path()), PinState::lost());
    }

    #[test]
    fn round_trip_through_the_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let s = failed(7, at(3, 123_456));
        s.save(dir.path()).expect("save");
        assert_eq!(PinState::load(dir.path()), s);
        let none = PinState::default();
        none.save(dir.path()).expect("save");
        assert_eq!(PinState::load(dir.path()), none);
    }
}

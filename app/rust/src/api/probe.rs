//! Bridge to the DHT measurement (R-012): can the DHT hold envelopes from this phone?
//!
//! Test data only — random seeds and values derived from them. Nothing here is secret,
//! nothing touches the vault, and the report is meant to be copied and sent as is.
//!
//! Every step appends its report to a log file next to the seeds, and the screen shows the
//! log, not its own memory: a measurement that spans hours must survive leaving the screen
//! and stopping the app.

use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use apeiron_transport::probe::{self, GetOutcome, PutOutcome, Seed};

use crate::paths::storage_dir;

/// Seeds put by this phone, one per line: `seed unix-seconds`.
const SEEDS_FILE: &str = "probe-seeds.txt";

/// Reports of every step, oldest first.
const LOG_FILE: &str = "probe-log.txt";

/// The log keeps its newest part only. It is copied into a message as a whole, so it has to
/// stay short enough to paste.
const LOG_LIMIT: usize = 16 * 1024;

fn unix_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `2026-09-24 10:28:50 UTC` from unix seconds, without a date library.
fn utc(unix: u64) -> String {
    let days = i64::try_from(unix / 86_400).unwrap_or(0);
    let secs = unix % 86_400;
    // Days to a civil date (H. Hinnant's algorithm), valid for any date after 1970.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC",
        secs / 3_600,
        secs % 3_600 / 60,
        secs % 60
    )
}

fn log_path() -> Result<PathBuf, String> {
    storage_dir().map(|d| d.join(LOG_FILE))
}

/// Appends one step's report to the log, keeping its newest `LOG_LIMIT` bytes, and returns
/// the report. A log that cannot be written is said so in the report, not hidden.
fn record(started: Instant, at: u64, report: String) -> String {
    let mut block = format!("[{}] {}", utc(at), report.trim_end());
    block.push_str(&format!(" (took {} s)\n\n", started.elapsed().as_secs()));
    let saved = log_path().and_then(|path| {
        let previous = std::fs::read_to_string(&path).unwrap_or_default();
        std::fs::write(&path, newest(previous + &block, LOG_LIMIT)).map_err(|e| e.to_string())
    });
    if let Err(e) = saved {
        block.push_str(&format!("could not save the log: {e}\n\n"));
    }
    block
}

/// The newest part of `text` no longer than `limit` bytes, cut at a line start.
fn newest(text: String, limit: usize) -> String {
    let Some(cut) = text.len().checked_sub(limit).filter(|&c| c > 0) else {
        return text;
    };
    // The first line start at or after `cut`. A newline is one byte and a whole character,
    // so the position right after it is always a character boundary.
    let from = cut.saturating_sub(1);
    let start = text
        .as_bytes()
        .get(from..)
        .and_then(|tail| tail.iter().position(|&b| b == b'\n'))
        .map(|nl| from.saturating_add(nl).saturating_add(1));
    start
        .and_then(|s| text.get(s..))
        .unwrap_or_default()
        .to_string()
}

/// The log of every step so far, oldest first. Empty if nothing was measured yet.
#[flutter_rust_bridge::frb]
pub fn dht_probe_log() -> String {
    log_path()
        .and_then(|p| std::fs::read_to_string(p).map_err(|e| e.to_string()))
        .unwrap_or_default()
}

/// Forgets the log. The seeds stay: "check own" still finds what was put.
#[flutter_rust_bridge::frb]
pub fn dht_probe_clear_log() -> String {
    match log_path().and_then(|p| match std::fs::remove_file(p) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }) {
        Ok(()) => String::new(),
        Err(e) => format!("could not clear the log: {e}"),
    }
}

/// Puts `count` probe items with random seeds and remembers them on this phone.
///
/// The report lists the seeds: the desktop fetches the same items from them hours later.
/// Items are put in parallel — one put waits for the whole query, several seconds.
#[flutter_rust_bridge::frb]
pub fn dht_probe_put(count: u32) -> String {
    let started = Instant::now();
    let at = unix_s();
    record(started, at, put_report(count, at))
}

fn put_report(count: u32, at: u64) -> String {
    let count = count.clamp(1, 32);
    let mut report = String::from("DHT probe: put from this phone\n");
    let seeds: Vec<Seed> = match (0..count).map(|_| Seed::random()).collect() {
        Ok(s) => s,
        Err(e) => return format!("{report}{e}\n"),
    };
    let (dht, boot_ms) = match probe::node() {
        Ok(v) => v,
        Err(e) => return format!("{report}{e}\n"),
    };
    let outcomes: Vec<PutOutcome> = std::thread::scope(|scope| {
        let handles: Vec<_> = seeds
            .iter()
            .map(|seed| {
                let dht = dht.clone();
                scope.spawn(move || probe::put(&dht, seed))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| {
                h.join().unwrap_or(PutOutcome {
                    ok: false,
                    ms: 0,
                    error: "the put thread died".to_string(),
                })
            })
            .collect()
    });
    report.push_str(&format!(
        "{}; bootstrap {boot_ms} ms\n",
        probe::summarize_puts(&outcomes)
    ));

    let mut saved = String::new();
    for (seed, outcome) in seeds.iter().zip(&outcomes) {
        if outcome.ok {
            saved.push_str(&format!("{} {at}\n", seed.to_hex()));
        }
    }
    match storage_dir() {
        Ok(dir) => {
            let path = dir.join(SEEDS_FILE);
            let previous = std::fs::read_to_string(&path).unwrap_or_default();
            if let Err(e) = std::fs::write(&path, previous + &saved) {
                report.push_str(&format!("could not remember the seeds: {e}\n"));
            }
        }
        Err(e) => report.push_str(&format!("could not remember the seeds: {e}\n")),
    }
    report.push_str("seeds (for the desktop):\n");
    report.push_str(&saved);
    report
}

/// Fetches the items this phone put earlier, all at once, and lists each with its age.
#[flutter_rust_bridge::frb]
pub fn dht_probe_get_own() -> String {
    let started = Instant::now();
    let at = unix_s();
    record(started, at, own_report(at))
}

fn own_report(now: u64) -> String {
    let mut report = String::from("DHT probe: this phone's own items\n");
    let text = match storage_dir()
        .and_then(|d| std::fs::read_to_string(d.join(SEEDS_FILE)).map_err(|e| e.to_string()))
    {
        Ok(t) => t,
        Err(e) => return format!("{report}nothing put yet ({e})\n"),
    };
    let entries: Vec<(Seed, u64)> = text
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let seed = Seed::from_hex(parts.next()?).ok()?;
            let at = parts.next()?.parse::<u64>().ok()?;
            Some((seed, at))
        })
        .collect();
    if entries.is_empty() {
        return format!("{report}nothing put yet (no seeds in the file)\n");
    }
    let (dht, boot_ms) = match probe::node() {
        Ok(v) => v,
        Err(e) => return format!("{report}{e}\n"),
    };
    let seeds: Vec<Seed> = entries.iter().map(|(s, _)| *s).collect();
    let outcomes = probe::get_many(&dht, &seeds);
    let ages: Vec<u64> = entries
        .iter()
        .map(|(_, at)| now.saturating_sub(*at) / 60)
        .collect();
    report.push_str(&format!(
        "age {} to {} min: {}; bootstrap {boot_ms} ms\n",
        ages.iter().min().copied().unwrap_or(0),
        ages.iter().max().copied().unwrap_or(0),
        probe::summarize_gets(&outcomes)
    ));
    report.push_str("seeds (for the desktop):\n");
    for (((seed, at), age), outcome) in entries.iter().zip(&ages).zip(&outcomes) {
        report.push_str(&format!(
            "{} {at} age {age} min: {}\n",
            seed.to_hex(),
            verdict(outcome)
        ));
    }
    report
}

/// One item's result in a word, for the per-seed list.
fn verdict(o: &GetOutcome) -> String {
    if o.died {
        "lookup thread died".to_string()
    } else if o.found {
        format!("found, first response {} ms", o.first_ms.unwrap_or(0))
    } else if o.wrong {
        "a different value".to_string()
    } else {
        format!("not found ({} responses)", o.responses)
    }
}

/// Fetches the desktop's items for each of `days` — public seeds, so nothing had to be sent
/// here. One node for all of them and every lookup at once: bootstrapping twice and looking
/// up one by one made this take minutes.
#[flutter_rust_bridge::frb]
pub fn dht_probe_get_public(days: Vec<String>, count: u32) -> String {
    let started = Instant::now();
    let at = unix_s();
    record(started, at, public_report(&days, count))
}

fn public_report(days: &[String], count: u32) -> String {
    let count = count.clamp(1, 64);
    let mut report = String::from("DHT probe: the desktop's public items\n");
    let (dht, boot_ms) = match probe::node() {
        Ok(v) => v,
        Err(e) => return format!("{report}{e}\n"),
    };
    let seeds: Vec<Seed> = days
        .iter()
        .flat_map(|day| (0..count).map(move |i| Seed::public(day, i)))
        .collect();
    let outcomes = probe::get_many(&dht, &seeds);
    let per_day = usize::try_from(count).unwrap_or(usize::MAX);
    for (day, chunk) in days.iter().zip(outcomes.chunks(per_day)) {
        report.push_str(&format!("{day}: {}\n", probe::summarize_gets(chunk)));
    }
    report.push_str(&format!("bootstrap {boot_ms} ms\n"));
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_formats_known_moments() {
        assert_eq!(utc(0), "1970-01-01 00:00:00 UTC");
        // The phone's put in the check of 24 September 2026.
        assert_eq!(utc(1_790_245_730), "2026-09-24 10:28:50 UTC");
        // A leap day and the last second of a year.
        assert_eq!(utc(1_709_164_800), "2024-02-29 00:00:00 UTC");
        assert_eq!(utc(1_735_689_599), "2024-12-31 23:59:59 UTC");
    }

    #[test]
    fn the_log_keeps_its_newest_lines() {
        let text = "one\ntwo\nthree\n".to_string();
        assert_eq!(newest(text.clone(), 100), text);
        assert_eq!(newest(text.clone(), 7), "three\n");
        assert_eq!(newest(text, 3), "");
        // A cut that falls exactly on a line start keeps that line.
        assert_eq!(newest("ab\ncd\n".to_string(), 3), "cd\n");
        assert_eq!(newest("ab\ncd\n".to_string(), 6), "ab\ncd\n");
        // Never cut inside a character.
        let text = "ж\nжж\n".to_string();
        assert_eq!(newest(text, 4), "");
    }
}

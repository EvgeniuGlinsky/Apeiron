//! Bridge to the DHT measurement (R-012): can the DHT hold envelopes from this phone?
//!
//! Test data only — random seeds and values derived from them. Nothing here is secret,
//! nothing touches the vault, and the report is meant to be copied and sent as is.

use std::time::{SystemTime, UNIX_EPOCH};

use apeiron_transport::probe::{self, PutOutcome, Seed};

use crate::paths::storage_dir;

/// Seeds put by this phone, one per line: `seed unix-seconds`.
const SEEDS_FILE: &str = "probe-seeds.txt";

fn unix_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Puts `count` probe items with random seeds and remembers them on this phone.
///
/// The report lists the seeds: the desktop fetches the same items from them hours later.
/// Items are put in parallel — one put waits for the whole query, several seconds.
#[flutter_rust_bridge::frb]
pub fn dht_probe_put(count: u32) -> String {
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

    let now = unix_s();
    let mut saved = String::new();
    for (seed, outcome) in seeds.iter().zip(&outcomes) {
        if outcome.ok {
            saved.push_str(&format!("{} {now}\n", seed.to_hex()));
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

/// Fetches the items this phone put earlier, and says how old they are.
#[flutter_rust_bridge::frb]
pub fn dht_probe_get_own() -> String {
    let mut report = String::from("DHT probe: this phone's own items\n");
    let text = match storage_dir()
        .and_then(|d| std::fs::read_to_string(d.join(SEEDS_FILE)).map_err(|e| e.to_string()))
    {
        Ok(t) => t,
        Err(e) => return format!("{report}nothing put yet ({e})\n"),
    };
    let now = unix_s();
    let entries: Vec<(Seed, u64)> = text
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let seed = Seed::from_hex(parts.next()?).ok()?;
            let at = parts.next()?.parse::<u64>().ok()?;
            Some((seed, at))
        })
        .collect();
    let (dht, boot_ms) = match probe::node() {
        Ok(v) => v,
        Err(e) => return format!("{report}{e}\n"),
    };
    let outcomes: Vec<_> = entries.iter().map(|(s, _)| probe::get(&dht, s)).collect();
    let oldest = entries
        .iter()
        .map(|(_, at)| now.saturating_sub(*at))
        .max()
        .unwrap_or(0);
    let newest = entries
        .iter()
        .map(|(_, at)| now.saturating_sub(*at))
        .min()
        .unwrap_or(0);
    report.push_str(&format!(
        "age {} to {} min: {}; bootstrap {boot_ms} ms\n",
        newest / 60,
        oldest / 60,
        probe::summarize_gets(&outcomes)
    ));
    report
}

/// Fetches the desktop's items for `day` — public seeds, so nothing had to be sent here.
#[flutter_rust_bridge::frb]
pub fn dht_probe_get_public(day: String, count: u32) -> String {
    let count = count.clamp(1, 64);
    let seeds: Vec<Seed> = (0..count).map(|i| Seed::public(&day, i)).collect();
    let mut report = format!("DHT probe: the desktop's public items of {day}\n");
    let (dht, boot_ms) = match probe::node() {
        Ok(v) => v,
        Err(e) => return format!("{report}{e}\n"),
    };
    let outcomes: Vec<_> = seeds.iter().map(|s| probe::get(&dht, s)).collect();
    report.push_str(&format!(
        "{}; bootstrap {boot_ms} ms\n",
        probe::summarize_gets(&outcomes)
    ));
    report
}

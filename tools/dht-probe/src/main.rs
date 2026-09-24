//! Desktop side of the go/no-go measurement: can Mainline DHT hold envelopes?
//!
//! Every read uses a fresh node (new id, new port, new routing table), so a hit means
//! "the network has it", not "our own node remembered it".
//!
//! Usage:
//!   dht-probe run [--items N] [--dir D] [--schedule 0,5,30,60,120,240,480] [--public DAY]
//!       put N items (random seeds, or the public seeds of DAY that a phone can fetch
//!       without anything being sent to it), then read them back on the schedule;
//!   dht-probe get --file F [--dir D]
//!       read the items whose seeds (64 hex characters each) appear anywhere in F —
//!       for example a report pasted from the phone.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use apeiron_transport::probe::{self, Seed};

struct Args {
    cmd: String,
    items: u32,
    dir: PathBuf,
    file: Option<PathBuf>,
    public: Option<String>,
    schedule_min: Vec<u64>,
}

fn parse_args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let cmd = it.next().ok_or("expected a command: run | get")?;
    let mut args = Args {
        cmd,
        items: 24,
        dir: PathBuf::from("dht-probe-out"),
        file: None,
        public: None,
        schedule_min: vec![0, 5, 30, 60, 120, 240, 480],
    };
    while let Some(flag) = it.next() {
        let value = it.next().ok_or(format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--items" => args.items = value.parse().map_err(|_| "bad --items")?,
            "--dir" => args.dir = PathBuf::from(value),
            "--file" => args.file = Some(PathBuf::from(value)),
            "--public" => args.public = Some(value),
            "--schedule" => {
                args.schedule_min = value
                    .split(',')
                    .map(|s| s.trim().parse::<u64>().map_err(|_| "bad --schedule"))
                    .collect::<Result<_, _>>()?
            }
            other => return Err(format!("unknown flag {other}")),
        }
    }
    Ok(args)
}

fn unix_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn log(dir: &Path, line: &str) {
    println!("{line}");
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("summary.txt"))
    {
        let _ = writeln!(f, "{line}");
    }
}

/// Every run of 64 hex characters in `text` is taken as a seed.
fn seeds_in(text: &str) -> Vec<Seed> {
    text.split(|c: char| !c.is_ascii_hexdigit())
        .filter(|w| w.len() == 64)
        .filter_map(|w| Seed::from_hex(w).ok())
        .collect()
}

fn read_round(dir: &Path, seeds: &[Seed], what: &str) {
    match probe::node() {
        Ok((dht, boot_ms)) => {
            let outcomes = probe::get_many(&dht, seeds);
            log(
                dir,
                &format!(
                    "[get] {what}: {}; bootstrap {boot_ms} ms",
                    probe::summarize_gets(&outcomes)
                ),
            );
        }
        Err(e) => log(dir, &format!("[get] {what}: {e}")),
    }
}

fn main() {
    if let Err(e) = real_main() {
        eprintln!("dht-probe: {e}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<(), String> {
    let args = parse_args()?;
    fs::create_dir_all(&args.dir).map_err(|e| e.to_string())?;
    match args.cmd.as_str() {
        "get" => {
            let file = args.file.as_ref().ok_or("get needs --file")?;
            let text = fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
            let seeds = seeds_in(&text);
            if seeds.is_empty() {
                return Err(format!("no seeds in {}", file.display()));
            }
            read_round(
                &args.dir,
                &seeds,
                &format!("{} seeds from {}", seeds.len(), file.display()),
            );
        }
        "run" => {
            let seeds: Vec<Seed> = match &args.public {
                Some(day) => (0..args.items).map(|i| Seed::public(day, i)).collect(),
                None => (0..args.items)
                    .map(|_| Seed::random())
                    .collect::<Result<_, _>>()?,
            };
            let list: String = seeds.iter().map(|s| s.to_hex() + "\n").collect();
            fs::write(args.dir.join("seeds.txt"), list).map_err(|e| e.to_string())?;
            log(
                &args.dir,
                &format!(
                    "[run] started at unix {} s, {} items{}, schedule {:?} min",
                    unix_s(),
                    seeds.len(),
                    args.public
                        .as_ref()
                        .map(|d| format!(" (public seeds of {d})"))
                        .unwrap_or_default(),
                    args.schedule_min
                ),
            );
            let (dht, boot_ms) = probe::node()?;
            let puts: Vec<_> = seeds.iter().map(|s| probe::put(&dht, s)).collect();
            log(
                &args.dir,
                &format!(
                    "[put] {}; bootstrap {boot_ms} ms",
                    probe::summarize_puts(&puts)
                ),
            );
            drop(dht);
            let stored: Vec<Seed> = seeds
                .iter()
                .zip(&puts)
                .filter(|(_, p)| p.ok)
                .map(|(s, _)| *s)
                .collect();
            let t0 = Instant::now();
            for offset in &args.schedule_min {
                let due = Duration::from_secs(offset * 60);
                if let Some(wait) = due.checked_sub(t0.elapsed()) {
                    std::thread::sleep(wait);
                }
                read_round(&args.dir, &stored, &format!("age ~{offset} min"));
            }
            log(&args.dir, "[run] done");
        }
        other => return Err(format!("unknown command {other}")),
    }
    Ok(())
}

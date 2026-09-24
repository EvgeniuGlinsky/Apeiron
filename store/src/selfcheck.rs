//! Self-check of the storage on a live device.
//!
//! It exists because there is only one check on the phone. Everything that can be found
//! out on the development machine has been found out by tests; what is moved here is
//! exactly what cannot be found out on the development machine: the behavior of the
//! **real** Keystore, and that the state really survived a real restart of the real app.
//!
//! The result is a list of "passed / DID NOT PASS" lines that can be photographed
//! and sent in full.
//!
//! # What is not here
//!
//! Nothing destructive. The substitution checks work with in-memory copies, and
//! erasure is not checked at all: it destroys the owner's data, and invoking it
//! quietly, under the guise of a check, is not allowed. It has a separate deliberate
//! action.

use std::sync::OnceLock;

use apeiron_core::vodozemac::olm::Account;
use apeiron_core::{random_bytes, Chat, Identity, PrekeyBundle};
use apeiron_platform::HardwareKey;

use crate::pin::PinState;
use crate::record::{open_record, seal_record, Table};
use crate::{Storage, StorageError};

/// Keys of the internal records by which the previous launch is recognized.
const META_FINGERPRINT: &str = "самопроверка/отпечаток";
const META_DEVICE_KEY: &str = "самопроверка/ключ устройства";
const META_PROBE_CHAT: &str = "самопроверка/проба переписки";
const META_PROBE_CIPHERTEXT: &str = "самопроверка/проба шифртекста";
const META_PROBE_BUNDLE: &str = "самопроверка/проба пакета";
const META_PROBE_ACCOUNT: &str = "самопроверка/проба аккаунта";
const META_RUNS: &str = "самопроверка/число прогонов";
const META_SEED_PROCESS: &str = "самопроверка/процесс, заложивший пробу";

/// The text of the probe message. Stored encrypted between launches.
const PROBE_TEXT: &str = "это сообщение зашифровано до перезапуска";

/// How to really restart the app. Swiping it away from the recents list does not kill the
/// process on Samsung, and a check that asks for a restart it cannot get stays red forever.
const HOW_TO_RESTART: &str =
    "Остановите приложение: Настройки → Приложения → Apeiron → «Остановить» (смахивание из недавних процесс не завершает), откройте заново и повторите";

/// A random mark of **this** process.
///
/// Needed because otherwise the self-check cannot tell "the app was restarted"
/// from "the check screen was opened a second time". In the second case all the lines
/// about state survival would turn green without proving anything, and a green
/// mark where nothing was checked is exactly the false confidence
/// the self-check was undertaken against.
///
/// A random number, not a process identifier: the system reuses those, and a
/// coincidence, however unlikely, would give a false answer in exactly the direction
/// where that is not allowed.
static PROCESS_MARK: OnceLock<String> = OnceLock::new();

fn process_mark() -> &'static str {
    PROCESS_MARK.get_or_init(|| match random_bytes::<16>() {
        Ok(bytes) => hex::encode(bytes),
        // The OS refusing randomness is no reason to crash here: the mark becomes empty,
        // the comparison will never match, and the check will honestly say "not proven".
        Err(_) => String::new(),
    })
}

/// One line of the report.
#[derive(Debug, Clone)]
pub struct Check {
    /// What was checked.
    pub name: String,
    /// Whether it passed.
    pub passed: bool,
    /// Detail: what makes sense to read with one's own eyes.
    pub detail: String,
}

impl Check {
    fn ok(name: &str, detail: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            passed: true,
            detail: detail.into(),
        }
    }

    fn failed(name: &str, detail: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            passed: false,
            detail: detail.into(),
        }
    }
}

/// Runs the self-check.
///
/// On the first call it seeds what will be needed on the next launch, and
/// honestly says so: proving that the state survived a restart is impossible on the first
/// launch, and pretending the check passed is not allowed.
pub fn run(storage: &Storage) -> Vec<Check> {
    run_with_mark(storage, process_mark())
}

/// The same, but with a given process mark.
///
/// Exists for checks on the development machine: a test lives in one process, and
/// without this the property "there will be no green until the app is restarted"
/// could not be checked at all, i.e. the most important property here would rest
/// on a word of honor.
pub fn run_with_mark(storage: &Storage, mark: &str) -> Vec<Check> {
    let mut out = Vec::new();

    let runs = bump_runs(storage);
    let fresh = is_fresh_process(storage, mark);

    out.push(Check::ok("прогонов проверки", format!("{runs}")));
    out.push(Check {
        name: "закладку делал другой процесс".to_string(),
        passed: fresh,
        detail: if fresh {
            "да — значит строкам про выживание состояния можно верить".to_string()
        } else {
            format!("НЕТ: пробу заложил этот же процесс, перезапуск не проверен ничем. {HOW_TO_RESTART}")
        },
    });

    let mut planted = false;
    out.push(check_level(storage));
    out.push(check_identity(storage, fresh, &mut planted));
    out.push(check_account(storage, fresh, &mut planted));
    out.push(check_conversation(storage, fresh, &mut planted));
    out.push(check_record_binding(storage));
    out.push(check_damaged_record(storage));

    // The mark names the process that planted the probe, so it moves only when this run
    // planted something. It is written last: up to this point the checks above were reading
    // it. Rewriting it on every run made the second run in the same process see its own
    // mark and lose the proof the first run had.
    //
    // No proof is kept in memory instead, on purpose: if the storage were reset within this
    // process, a remembered "restart proven" would outlive the probe it was about, and the
    // probe planted anew by this very process would then turn green. Here that cannot
    // happen: planting anew moves the mark to this process.
    let has_mark = matches!(storage.meta_get(META_SEED_PROCESS), Ok(Some(_)));
    if planted || !has_mark {
        let _ = storage.meta_set(META_SEED_PROCESS, mark.as_bytes());
    }

    out
}

/// Whether the probe was seeded by another process rather than this one.
fn is_fresh_process(storage: &Storage, mark: &str) -> bool {
    match storage.meta_get(META_SEED_PROCESS) {
        Ok(Some(saved)) => !mark.is_empty() && String::from_utf8_lossy(&saved) != mark,
        // No mark yet: so this is the very first run, and there is nothing to prove.
        _ => false,
    }
}

/// Counts runs and at the same time checks that internal records get written at all.
///
/// Runs specifically, not launches: by itself this number proves
/// nothing, and passing it off as the number of launches would be a lie.
/// [`is_fresh_process`] is responsible for the restart.
fn bump_runs(storage: &Storage) -> u64 {
    let previous = storage
        .meta_get(META_RUNS)
        .ok()
        .flatten()
        .and_then(|b| String::from_utf8(b).ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let now = previous.saturating_add(1);
    let _ = storage.meta_set(META_RUNS, now.to_string().as_bytes());
    now
}

fn check_level(storage: &Storage) -> Check {
    let level = storage.security_level();
    let at_creation = storage.level_at_creation();
    let mut detail = format!("система сообщает: {} ({})", level.name(), level.raw());
    if level.raw() != at_creation.raw() {
        detail.push_str(&format!(
            "; при создании было {} ({})",
            at_creation.name(),
            at_creation.raw()
        ));
    }
    // The word "reports" here is not a softening but precision: for symmetric keys
    // there is no attestation, and KeyInfo is the framework's self-report in our own
    // process. It is not a check and must not be called one.
    Check {
        name: "где лежит ключ".to_string(),
        passed: level.is_hardware(),
        detail,
    }
}

fn check_identity(storage: &Storage, fresh: bool, planted: &mut bool) -> Check {
    let name = "личность пережила перезапуск";
    let identity = match storage.load_identity() {
        Ok(Some(id)) => id,
        Ok(None) => return Check::failed(name, "личности в хранилище нет"),
        Err(e) => return Check::failed(name, e.to_string()),
    };
    let current = identity.public().fingerprint();

    match storage.meta_get(META_FINGERPRINT) {
        Ok(Some(saved)) => {
            let saved = String::from_utf8_lossy(&saved).into_owned();
            if saved == current {
                verified(name, fresh, format!("отпечаток тот же: {current}"))
            } else {
                Check::failed(
                    name,
                    format!("было {saved}, стало {current} — это другая личность"),
                )
            }
        }
        Ok(None) => {
            *planted = true;
            let _ = storage.meta_set(META_FINGERPRINT, current.as_bytes());
            not_yet(name, format!("запомнен отпечаток {current}"))
        }
        Err(e) => Check::failed(name, e.to_string()),
    }
}

fn check_account(storage: &Storage, fresh: bool, planted: &mut bool) -> Check {
    let name = "аккаунт устройства тот же";
    let account = match storage.load_account() {
        Ok(Some(a)) => a,
        Ok(None) => return Check::failed(name, "аккаунта в хранилище нет"),
        Err(e) => return Check::failed(name, e.to_string()),
    };
    let current = account.identity_keys().curve25519.to_base64();

    match storage.meta_get(META_DEVICE_KEY) {
        Ok(Some(saved)) => {
            let saved = String::from_utf8_lossy(&saved).into_owned();
            if saved == current {
                verified(name, fresh, "ключ устройства не сменился")
            } else {
                Check::failed(
                    name,
                    "ключ устройства сменился — все существующие переписки порваны",
                )
            }
        }
        Ok(None) => {
            *planted = true;
            let _ = storage.meta_set(META_DEVICE_KEY, current.as_bytes());
            not_yet(name, "ключ устройства запомнен")
        }
        Err(e) => Check::failed(name, e.to_string()),
    }
}

/// The main check: a message encrypted during the previous launch is read during this one.
///
/// It is done with a real pair of Olm accounts and a real ratchet. If the
/// ratchet state is lost between launches, the peers diverge
/// forever, and there is nothing to fix it with; that is why it is exactly this that must
/// be checked, not "the file is in place".
fn check_conversation(storage: &Storage, fresh: bool, planted: &mut bool) -> Check {
    let name = "переписка читается после перезапуска";

    let saved_chat = storage.meta_get(META_PROBE_CHAT).ok().flatten();
    let saved_ct = storage.meta_get(META_PROBE_CIPHERTEXT).ok().flatten();
    let saved_bundle = storage.meta_get(META_PROBE_BUNDLE).ok().flatten();
    let saved_account = storage.meta_get(META_PROBE_ACCOUNT).ok().flatten();

    match (saved_chat, saved_ct, saved_bundle, saved_account) {
        (Some(_), Some(ct), Some(bundle), Some(account)) => match replay(&ct, &bundle, &account) {
            Ok(text) if text == PROBE_TEXT => {
                verified(name, fresh, "расшифровано ровно то, что было зашифровано")
            }
            Ok(text) => Check::failed(name, format!("расшифровалось другое: {text}")),
            Err(e) => Check::failed(name, e.to_string()),
        },
        _ => match {
            *planted = true;
            seed_probe(storage)
        } {
            Ok(()) => not_yet(name, "проба заложена"),
            Err(e) => Check::failed(name, e.to_string()),
        },
    }
}

/// Seeds the probe: two identities, a prekey bundle, a session and one message.
fn seed_probe(storage: &Storage) -> Result<(), StorageError> {
    let sender = Identity::generate()?;
    let receiver = Identity::generate()?;
    let mut sender_account = Account::new();
    let mut receiver_account = Account::new();

    let bundle_bytes = PrekeyBundle::create(&receiver, &mut receiver_account)
        .map_err(|e| StorageError::Wrapper(e.to_string()))?
        .to_bytes();
    let bundle = PrekeyBundle::parse(&bundle_bytes)
        .and_then(|b| b.verify())
        .map_err(|e| StorageError::Wrapper(e.to_string()))?;

    // The sender's bundle is assembled from THE SAME account that is later used to
    // create the session. Otherwise the device key in the message will not match the key
    // in the bundle, and receiving will fail; that is exactly as it should be, it is a check
    // against substitution, not nitpicking.
    let sender_bundle = PrekeyBundle::create(&sender, &mut sender_account)
        .map_err(|e| StorageError::Wrapper(e.to_string()))?
        .to_bytes();

    let mut chat = Chat::initiate(&sender_account, &bundle)?;
    let message = chat.encrypt(PROBE_TEXT)?;

    let ciphertext = match message {
        apeiron_core::vodozemac::olm::OlmMessage::PreKey(m) => m.to_base64(),
        apeiron_core::vodozemac::olm::OlmMessage::Normal(_) => {
            return Err(StorageError::Wrapper(
                "первое сообщение обязано быть сообщением установления сессии".to_string(),
            ))
        }
    };

    storage.meta_set(META_PROBE_CHAT, &chat.pickle()?)?;
    storage.meta_set(META_PROBE_CIPHERTEXT, ciphertext.as_bytes())?;
    storage.meta_set(META_PROBE_BUNDLE, &sender_bundle)?;
    storage.meta_set(
        META_PROBE_ACCOUNT,
        &apeiron_core::pickle_account(&receiver_account)?,
    )?;
    Ok(())
}

/// Decrypts the probe with the state that survived the restart.
fn replay(ciphertext: &[u8], bundle: &[u8], account: &[u8]) -> Result<String, StorageError> {
    let mut receiver_account = apeiron_core::unpickle_account(account)?;
    let sender_bundle = PrekeyBundle::parse(bundle)
        .and_then(|b| b.verify())
        .map_err(|e| StorageError::Wrapper(e.to_string()))?;

    let text = String::from_utf8_lossy(ciphertext).into_owned();
    let message = apeiron_core::vodozemac::olm::PreKeyMessage::from_base64(&text)
        .map_err(|e| StorageError::Wrapper(e.to_string()))?;

    let (_, decrypted) = Chat::accept(&mut receiver_account, &sender_bundle, &message)?;
    Ok(decrypted)
}

/// A record moved to someone else's place is not readable.
///
/// Checked on probe bytes; production records are not touched.
fn check_record_binding(storage: &Storage) -> Check {
    let name = "запись нельзя переложить на чужое место";
    let key = storage.probe_key();
    let sealed = match seal_record(key, Table::Meta, 1, b"probe") {
        Ok(v) => v,
        Err(e) => return Check::failed(name, e.to_string()),
    };
    let moved = open_record(key, Table::Meta, 2, &sealed).is_err();
    let other_table = open_record(key, Table::Contact, 1, &sealed).is_err();
    let own_place = open_record(key, Table::Meta, 1, &sealed).is_ok();

    if moved && other_table && own_place {
        Check::ok(name, "чужой номер и чужая таблица отвергнуты")
    } else {
        Check::failed(
            name,
            format!("своё место: {own_place}, чужой номер отвергнут: {moved}, чужая таблица отвергнута: {other_table}"),
        )
    }
}

/// A damaged record is rejected, not half-read.
fn check_damaged_record(storage: &Storage) -> Check {
    let name = "повреждённая запись отвергается";
    let key = storage.probe_key();
    let mut sealed = match seal_record(key, Table::Meta, 1, b"probe") {
        Ok(v) => v,
        Err(e) => return Check::failed(name, e.to_string()),
    };
    let Some(last) = sealed.last_mut() else {
        return Check::failed(name, "пустая запись");
    };
    *last ^= 0x01;

    if open_record(key, Table::Meta, 1, &sealed).is_err() {
        Check::ok(name, "перевёрнутый бит замечен")
    } else {
        Check::failed(name, "перевёрнутый бит прошёл как исправная запись")
    }
}

/// Nothing to check yet: the seed has just been planted.
///
/// Not marked as passed. A green line where nothing was checked is
/// exactly the false confidence the whole self-check was undertaken
/// against.
fn not_yet(name: &str, detail: impl Into<String>) -> Check {
    Check {
        name: name.to_string(),
        passed: false,
        detail: format!("проверять нечего, {}. {HOW_TO_RESTART}", detail.into()),
    }
}

/// The state matched, but this can be counted only if the seed was planted by another
/// process.
///
/// Otherwise all that is proven is that the database is readable by the same app that
/// has just written it, and that is not at all what is being checked.
fn verified(name: &str, fresh: bool, detail: impl Into<String>) -> Check {
    let detail = detail.into();
    if fresh {
        Check::ok(name, detail)
    } else {
        Check {
            name: name.to_string(),
            passed: false,
            detail: format!(
                "{detail}, но закладку делал этот же процесс — перезапуск не проверен. {HOW_TO_RESTART}"
            ),
        }
    }
}

// ─── The PIN (R-011) ─────────────────────────────────────────────────────────

/// Safety factor on the attacker's speed. Measured here, one operation goes through
/// keystore2, its database and the HAL; someone with root can talk to the secure hardware
/// directly and skip most of that. Five to twenty times faster is plausible, so the
/// estimate is divided by ten and called what it is: an estimate.
const ATTACKER_SPEEDUP: u64 = 10;

/// Threads used to see whether the secure hardware serves several operations at once.
const PARALLEL_THREADS: u64 = 4;

/// Checks of the PIN that only the real phone can answer.
///
/// `failures_in_this_process` is how many wrong PINs this very process has seen: if the
/// counter on disk holds more, it survived a restart.
pub fn pin_checks<H: HardwareKey + Sync>(
    storage: &Storage,
    hw: &H,
    failures_in_this_process: u32,
) -> Vec<Check> {
    let mut out = Vec::new();
    let dir = storage.dir();

    let params = match crate::wrapper::params(dir) {
        Ok(Some(p)) => p,
        Ok(None) => {
            out.push(Check::failed("пин: хранилище", "хранилище без пина"));
            return out;
        }
        Err(e) => {
            out.push(Check::failed("пин: хранилище", e.to_string()));
            return out;
        }
    };
    out.push(Check::ok(
        "пин: параметры",
        format!(
            "цепочка {} операций в железе, Argon2id {} МиБ × {} прохода",
            params.rounds,
            params.argon_kib / 1024,
            params.argon_t
        ),
    ));

    let t = storage.timing();
    out.push(Check::ok(
        "пин: разблокировка в этом запуске",
        format!("Argon2id {} мс, цепочка {} мс", t.argon_ms, t.chain_ms),
    ));

    let calibration = match crate::wrapper::calibrate(hw, storage.security_level(), u32::MAX) {
        Ok(c) => c,
        Err(e) => {
            out.push(Check::failed("пин: замер операции", e.to_string()));
            return out;
        }
    };
    out.push(Check::ok(
        "пин: одна операция в железе",
        format!(
            "{} мкс (лучшая из трёх пачек по 64)",
            calibration.per_op_ns / 1_000
        ),
    ));

    let parallel = parallel_speedup(hw, calibration.per_op_ns);
    out.push(Check::ok(
        "пин: параллельность железа",
        match parallel {
            Some(x) => format!(
                "{PARALLEL_THREADS} потока дают ×{:.1} к одному",
                x as f64 / 100.0
            ),
            None => "замерить не удалось".to_string(),
        },
    ));

    out.push(estimate(params.rounds, calibration.per_op_ns, parallel));

    match crate::wrapper::random_candidate_rejected(dir, hw) {
        Ok((true, _)) => out.push(Check::ok(
            "пин: случайный неверный пин отвергнут",
            "проверено без счётчика: кандидат рождён внутри и наружу не выходит",
        )),
        Ok((false, _)) => out.push(Check::failed(
            "пин: случайный неверный пин отвергнут",
            "НЕТ: случайный пин открыл хранилище",
        )),
        Err(e) => out.push(Check::failed(
            "пин: случайный неверный пин отвергнут",
            e.to_string(),
        )),
    }

    let state = PinState::load(dir);
    let survived = state.total > failures_in_this_process;
    out.push(Check {
        name: "пин: счётчик пережил перезапуск".to_string(),
        passed: survived,
        detail: if survived {
            format!(
                "неудач всего {}, из них в этом процессе {failures_in_this_process}: остальные записал прежний процесс",
                state.total
            )
        } else {
            format!(
                "неудач всего {}, все в этом процессе. Введите неверный пин и повторите после перезапуска. {HOW_TO_RESTART}",
                state.total
            )
        },
    });

    out.push(match hw.boot_clock() {
        Ok(c) if c.elapsed_ms > 0 => Check::ok(
            "пин: часы загрузки",
            format!(
                "BOOT_COUNT {}, elapsedRealtime {} с{}",
                c.boot_count,
                c.elapsed_ms / 1_000,
                if c.boot_count < 0 {
                    " (счётчика загрузок нет: перезагрузка узнаётся только по часам)"
                } else {
                    ""
                }
            ),
        ),
        Ok(c) => Check::failed("пин: часы загрузки", format!("странные часы: {c:?}")),
        Err(e) => Check::failed("пин: часы загрузки", e.to_string()),
    });

    out
}

/// How much faster `PARALLEL_THREADS` threads get through operations than one, in
/// hundredths (250 = ×2.5). `None` if it could not be measured.
fn parallel_speedup<H: HardwareKey + Sync>(hw: &H, per_op_ns: u64) -> Option<u64> {
    const ROUNDS: u32 = 64;
    let probe = random_bytes::<32>().ok()?;
    let started = std::time::Instant::now();
    let all_ok = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..PARALLEL_THREADS)
            .map(|_| scope.spawn(|| hw.hmac_chain(&probe, ROUNDS).is_ok()))
            .collect();
        handles.into_iter().all(|h| h.join().unwrap_or(false))
    });
    if !all_ok {
        return None;
    }
    let wall_ns = u64::try_from(started.elapsed().as_nanos()).ok()?.max(1);
    let serial_ns = per_op_ns
        .checked_mul(u64::from(ROUNDS))?
        .checked_mul(PARALLEL_THREADS)?;
    serial_ns.checked_mul(100)?.checked_div(wall_ns)
}

/// The honest estimate of guessing the PIN by someone with root on this very phone.
fn estimate(rounds: u32, per_op_ns: u64, parallel: Option<u64>) -> Check {
    let speedup_x100 = parallel.unwrap_or(100).max(100);
    // Nanoseconds per guess for the attacker: the chain, faster by the safety factor and
    // by the parallelism measured here. Argon2id is not counted: they can precompute it.
    let guess_ns = u64::from(rounds)
        .saturating_mul(per_op_ns)
        .saturating_mul(100)
        / ATTACKER_SPEEDUP
        / speedup_x100;
    // On average half of the space is searched.
    let days = |digits: u32| -> f64 {
        let half = 10f64.powi(i32::try_from(digits).unwrap_or(0)) / 2.0;
        half * guess_ns as f64 / 1e9 / 86_400.0
    };
    Check::ok(
        "пин: оценка перебора с root на этом телефоне",
        format!(
            "в среднем 6 цифр ≈ {:.1} сут, 8 цифр ≈ {:.0} сут, 10 цифр ≈ {:.0} лет. \
             Оценка с запасом ×{ATTACKER_SPEEDUP}: в обход фреймворка железо может отвечать быстрее. \
             Если ключ извлекут из железа — эти числа не действуют, остаётся Argon2id и длина пина",
            days(6),
            days(8),
            days(10) / 365.0
        ),
    )
}

//! Самопроверка хранилища на живом устройстве.
//!
//! Существует затем, что проверка на телефоне одна. Всё, что можно выяснить на
//! рабочей машине, выяснено тестами; сюда вынесено ровно то, что на рабочей
//! машине не выяснишь, — поведение **настоящего** Keystore и то, что состояние
//! действительно пережило настоящий перезапуск настоящего приложения.
//!
//! Результат — список строк «прошло / НЕ ПРОШЛО», который можно сфотографировать
//! и прислать целиком.
//!
//! # Чего здесь нет
//!
//! Ничего разрушающего. Проверки на подмену работают с копиями в памяти, а
//! стирание не проверяется вовсе: оно уничтожает данные владельца, и вызывать
//! его втихую, под видом проверки, нельзя. Для него отдельное осознанное
//! действие.

use std::sync::OnceLock;

use apeiron_core::vodozemac::olm::Account;
use apeiron_core::{random_bytes, Chat, Identity, PrekeyBundle};

use crate::record::{open_record, seal_record, Table};
use crate::{Storage, StorageError};

/// Ключи служебных записей, по которым узнаётся прошлый запуск.
const META_FINGERPRINT: &str = "самопроверка/отпечаток";
const META_DEVICE_KEY: &str = "самопроверка/ключ устройства";
const META_PROBE_CHAT: &str = "самопроверка/проба переписки";
const META_PROBE_CIPHERTEXT: &str = "самопроверка/проба шифртекста";
const META_PROBE_BUNDLE: &str = "самопроверка/проба пакета";
const META_PROBE_ACCOUNT: &str = "самопроверка/проба аккаунта";
const META_RUNS: &str = "самопроверка/число прогонов";
const META_SEED_PROCESS: &str = "самопроверка/процесс, заложивший пробу";

/// Текст пробного сообщения. Хранится зашифрованным между запусками.
const PROBE_TEXT: &str = "это сообщение зашифровано до перезапуска";

/// Случайная метка **этого** процесса.
///
/// Нужна затем, что иначе самопроверка не отличает «приложение перезапустили»
/// от «экран проверки открыли второй раз». Во втором случае все строки про
/// выживание состояния стали бы зелёными, ничего не доказав, — а зелёная
/// отметка там, где ничего не проверялось, это ровно та ложная уверенность,
/// против которой самопроверка и затевалась.
///
/// Случайное число, а не идентификатор процесса: тот система переиспользует, и
/// совпадение, пусть и маловероятное, дало бы ложный ответ именно в ту сторону,
/// в которую нельзя.
static PROCESS_MARK: OnceLock<String> = OnceLock::new();

fn process_mark() -> &'static str {
    PROCESS_MARK.get_or_init(|| match random_bytes::<16>() {
        Ok(bytes) => hex::encode(bytes),
        // Отказ ОС в случайности здесь не повод падать: метка станет пустой,
        // сравнение никогда не сойдётся, и проверка честно скажет «не доказано».
        Err(_) => String::new(),
    })
}

/// Одна строка отчёта.
#[derive(Debug, Clone)]
pub struct Check {
    /// Что проверялось.
    pub name: String,
    /// Прошло ли.
    pub passed: bool,
    /// Подробность — то, что имеет смысл прочитать глазами.
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

/// Прогоняет самопроверку.
///
/// При первом вызове засевает то, что понадобится на следующем запуске, и
/// честно пишет об этом: доказать, что состояние пережило перезапуск, на первом
/// запуске невозможно, и делать вид, что проверка прошла, нельзя.
pub fn run(storage: &Storage) -> Vec<Check> {
    run_with_mark(storage, process_mark())
}

/// То же, но с заданной меткой процесса.
///
/// Существует ради проверок на рабочей машине: тест живёт в одном процессе, и
/// без этого свойство «зелёного не будет, пока приложение не перезапустили»
/// проверить было бы нечем — то есть самое важное здесь свойство осталось бы
/// на честном слове.
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
            concat!(
                "НЕТ: пробу заложил этот же процесс. Убейте приложение из ",
                "списка недавних и откройте заново — иначе перезапуск не ",
                "проверен ничем"
            )
            .to_string()
        },
    });

    out.push(check_level(storage));
    out.push(check_identity(storage, fresh));
    out.push(check_account(storage, fresh));
    out.push(check_conversation(storage, fresh));
    out.push(check_record_binding(storage));
    out.push(check_damaged_record(storage));

    // Метка пишется последней: до этого момента её читали проверки выше.
    let _ = storage.meta_set(META_SEED_PROCESS, mark.as_bytes());

    out
}

/// Заложена ли проба другим процессом, а не этим же.
fn is_fresh_process(storage: &Storage, mark: &str) -> bool {
    match storage.meta_get(META_SEED_PROCESS) {
        Ok(Some(saved)) => !mark.is_empty() && String::from_utf8_lossy(&saved) != mark,
        // Метки ещё нет — значит это первый прогон вообще, и доказывать нечего.
        _ => false,
    }
}

/// Считает прогоны и заодно проверяет, что служебные записи вообще пишутся.
///
/// Именно прогоны, а не запуски: само по себе это число ничего не
/// доказывает, и выдавать его за число запусков было бы враньём. За
/// перезапуск отвечает [`is_fresh_process`].
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
    // Слово «сообщает» здесь не смягчение, а точность: для симметричных ключей
    // аттестации не существует, и KeyInfo — самоотчёт фреймворка в нашем же
    // процессе. Проверкой это не является и называться так не должно.
    Check {
        name: "где лежит ключ".to_string(),
        passed: level.is_hardware(),
        detail,
    }
}

fn check_identity(storage: &Storage, fresh: bool) -> Check {
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
            let _ = storage.meta_set(META_FINGERPRINT, current.as_bytes());
            not_yet(name, format!("запомнен отпечаток {current}"))
        }
        Err(e) => Check::failed(name, e.to_string()),
    }
}

fn check_account(storage: &Storage, fresh: bool) -> Check {
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
            let _ = storage.meta_set(META_DEVICE_KEY, current.as_bytes());
            not_yet(name, "ключ устройства запомнен")
        }
        Err(e) => Check::failed(name, e.to_string()),
    }
}

/// Главная проверка: сообщение, зашифрованное в прошлый запуск, читается в этот.
///
/// Делается настоящей парой аккаунтов Olm и настоящим храповиком. Если
/// состояние храповика теряется между запусками, собеседники расходятся
/// навсегда, и починить это нечем — поэтому проверять надо именно его, а не
/// «файл на месте».
fn check_conversation(storage: &Storage, fresh: bool) -> Check {
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
        _ => match seed_probe(storage) {
            Ok(()) => not_yet(name, "проба заложена"),
            Err(e) => Check::failed(name, e.to_string()),
        },
    }
}

/// Закладывает пробу: две личности, пакет пред-ключей, сессия и одно сообщение.
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

    // Пакет отправителя собирается из ТОГО ЖЕ аккаунта, которым потом
    // создаётся сессия. Иначе ключ устройства в сообщении не сойдётся с ключом
    // в пакете, и приём откажет — ровно так и должно быть, это проверка от
    // подмены, а не придирка.
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

/// Расшифровывает пробу тем состоянием, которое пережило перезапуск.
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

/// Запись, переложенная на чужое место, не читается.
///
/// Проверяется на пробных байтах, боевые записи не трогаются.
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

/// Повреждённая запись отвергается, а не читается наполовину.
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

/// Проверять ещё нечего: закладка только что сделана.
///
/// Не помечается пройденным. Зелёная строка там, где ничего не проверялось, —
/// это ровно та ложная уверенность, против которой затевалась вся
/// самопроверка.
fn not_yet(name: &str, detail: impl Into<String>) -> Check {
    Check {
        name: name.to_string(),
        passed: false,
        detail: format!(
            "проверять нечего, {}. Убейте приложение из списка недавних, откройте заново и повторите",
            detail.into()
        ),
    }
}

/// Состояние сошлось — но зачесть это можно, только если закладку делал другой
/// процесс.
///
/// Иначе доказано лишь то, что база читается тем же приложением, которое её
/// только что записало, а проверяется совсем не это.
fn verified(name: &str, fresh: bool, detail: impl Into<String>) -> Check {
    let detail = detail.into();
    if fresh {
        Check::ok(name, detail)
    } else {
        Check {
            name: name.to_string(),
            passed: false,
            detail: format!(
                "{detail}, но закладку делал этот же процесс — перезапуск не проверен. Убейте приложение и повторите"
            ),
        }
    }
}

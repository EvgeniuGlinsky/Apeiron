//! Хранилище целиком, против подставного аппаратного ключа.
//!
//! Проверка на устройстве одна, и всё, что можно выяснить здесь, обязано быть
//! выяснено здесь. На телефон остаётся ровно то, чего на рабочей машине нет:
//! настоящий Keystore.
//!
//! Соглашение проекта сохраняется: на один тест «работает» приходится несколько
//! «не даёт себя обмануть».

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::path::Path;

use apeiron_core::vodozemac::olm::{Account, OlmMessage};
use apeiron_core::{Chat, Identity, PrekeyBundle, Sigchain};
use apeiron_platform::KeyWrapper;
use apeiron_store::testing::{Behaviour, TestVault};
use apeiron_store::{wrapper::WRAPPER_FILE, Storage, StorageError, DATABASE_FILE};

fn temp() -> tempfile::TempDir {
    tempfile::tempdir().expect("каталог создаётся")
}

fn open(dir: &Path, vault: &TestVault) -> Storage {
    Storage::open(dir, vault).expect("хранилище открывается")
}

/// Готовый проверенный пакет пред-ключей.
fn bundle(identity: &Identity, account: &mut Account) -> PrekeyBundle {
    let bytes = PrekeyBundle::create(identity, account)
        .expect("пакет собирается")
        .to_bytes();
    PrekeyBundle::parse(&bytes)
        .expect("пакет разбирается")
        .verify()
        .expect("подпись верна")
}

// ─── То, ради чего всё затевалось ────────────────────────────────────────────

#[test]
fn identity_survives_reopening() {
    let dir = temp();
    let vault = TestVault::empty();

    let original = Identity::generate().expect("ОС отдаёт случайность");
    let fingerprint = original.public().fingerprint();
    {
        let store = open(dir.path(), &vault);
        assert!(store.created_now(), "первый запуск обязан быть первым");
        assert!(store.load_identity().unwrap().is_none());
        store.save_identity(&original).unwrap();
    }

    let store = open(dir.path(), &vault);
    assert!(!store.created_now(), "второй запуск выдал себя за первый");
    let restored = store.load_identity().unwrap().expect("личность на месте");
    assert_eq!(
        restored.public().fingerprint(),
        fingerprint,
        "после перезапуска отпечаток изменился — значит, личность другая"
    );
}

#[test]
fn device_account_survives_reopening() {
    let dir = temp();
    let vault = TestVault::empty();

    let account = Account::new();
    let device_key = account.identity_keys().curve25519;
    {
        let store = open(dir.path(), &vault);
        store.save_account(&account).unwrap();
    }

    let store = open(dir.path(), &vault);
    let restored = store.load_account().unwrap().expect("аккаунт на месте");
    assert_eq!(
        restored.identity_keys().curve25519,
        device_key,
        "после перезапуска сменился ключ устройства — все переписки порваны"
    );
}

/// Главное свойство: сообщение, зашифрованное до закрытия базы, читается после
/// её открытия заново.
#[test]
fn a_conversation_survives_reopening() {
    let dir = temp();
    let vault = TestVault::empty();

    let alice = Identity::generate().unwrap();
    let mut alice_account = Account::new();
    let bob = Identity::generate().unwrap();
    let mut bob_account = Account::new();

    let bob_bundle = bundle(&bob, &mut bob_account);
    let alice_bundle = bundle(&alice, &mut alice_account);

    let mut chat = Chat::initiate(&alice_account, &bob_bundle).unwrap();
    let first = chat.encrypt("до перезапуска").unwrap();

    {
        let store = open(dir.path(), &vault);
        let contact = store.save_contact(&bob.public(), "Боб").unwrap();
        store
            .save_chat_and_account(contact, &chat, &alice_account)
            .unwrap();
    }
    drop(chat);

    let store = open(dir.path(), &vault);
    let contact = store
        .find_contact(&bob.public())
        .unwrap()
        .expect("контакт на месте");
    assert_eq!(contact.name, "Боб");
    let chats = store.load_chats(contact.id).unwrap();
    assert_eq!(chats.len(), 1, "переписка потерялась");
    assert_eq!(
        chats[0].peer().to_bytes(),
        bob.public().to_bytes(),
        "после загрузки собеседник стал другим"
    );

    // Боб читает то, что Алиса зашифровала до перезапуска.
    let pre_key = match &first {
        OlmMessage::PreKey(m) => m.clone(),
        OlmMessage::Normal(_) => panic!("первое сообщение обязано быть pre-key"),
    };
    let (_, text) = Chat::accept(&mut bob_account, &alice_bundle, &pre_key).unwrap();
    assert_eq!(text, "до перезапуска");
}

#[test]
fn sigchain_survives_reopening_and_is_reverified() {
    let dir = temp();
    let vault = TestVault::empty();

    let root = Identity::generate().unwrap();
    let chain = Sigchain::create(&root).unwrap();
    let length = chain.len();
    {
        let store = open(dir.path(), &vault);
        store.save_sigchain(&chain).unwrap();
    }

    let store = open(dir.path(), &vault);
    let restored = store.load_sigchain().unwrap().expect("журнал на месте");
    assert_eq!(restored.len(), length);
    restored.verify().expect("журнал проверяется");
}

#[test]
fn meta_survives_reopening() {
    let dir = temp();
    let vault = TestVault::empty();
    {
        let store = open(dir.path(), &vault);
        assert!(store.meta_get("первый запуск").unwrap().is_none());
        store.meta_set("первый запуск", b"1730000000").unwrap();
    }
    let store = open(dir.path(), &vault);
    assert_eq!(
        store.meta_get("первый запуск").unwrap().as_deref(),
        Some(&b"1730000000"[..])
    );
}

// ─── Не даёт себя обмануть ───────────────────────────────────────────────────

/// Самое дорогое свойство во всём хранилище.
///
/// Обёртка на диске есть, а ключа в защищённом модуле нет. Так выглядит то, что
/// в поле случается регулярно: обновление прошивки, снятие блокировки экрана,
/// восстановление данных из бэкапа без ключей. Создать новый ключ здесь значило
/// бы уничтожить переписку владельца безвозвратно, поэтому вместо «первого
/// запуска» обязано прийти «ключ исчез».
#[test]
fn a_missing_key_with_the_wrapper_present_is_never_a_fresh_start() {
    let dir = temp();
    let vault = TestVault::empty();
    {
        let store = open(dir.path(), &vault);
        store.save_identity(&Identity::generate().unwrap()).unwrap();
    }
    assert!(dir.path().join(WRAPPER_FILE).exists());

    vault.forget_key();

    let err = Storage::open(dir.path(), &vault).expect_err("открылось, хотя ключа нет");
    assert!(
        matches!(err, StorageError::KeyGone),
        "вместо «ключ исчез» пришло: {err}"
    );
    assert!(!err.is_retryable());
    assert!(
        !vault.has_key(),
        "при отсутствующем ключе был создан новый — переписка уничтожена"
    );
}

/// Преходящий отказ не имеет права превратиться в «ключ исчез».
///
/// Между этим тестом и уничтожением переписки владельца нет ничего другого.
/// `setUnlockedDeviceRequired` отказывает на разблокированном устройстве, если
/// его разблокировали слабой биометрией, — подтверждённый дефект прошивок.
#[test]
fn transient_failure_never_maps_to_gone() {
    let dir = temp();
    let vault = TestVault::empty();
    {
        let store = open(dir.path(), &vault);
        store.save_identity(&Identity::generate().unwrap()).unwrap();
    }

    for behaviour in [Behaviour::Transient, Behaviour::Internal] {
        vault.set_behaviour(behaviour);
        let err = Storage::open(dir.path(), &vault).expect_err("открылось при отказе");
        assert!(
            !matches!(err, StorageError::KeyGone),
            "{behaviour:?} выдан за потерю ключа: {err}"
        );
        assert!(
            err.is_retryable(),
            "{behaviour:?} объявлен невосстановимым: {err}"
        );
    }

    // И данные после этого целы.
    vault.set_behaviour(Behaviour::Normal);
    let store = open(dir.path(), &vault);
    assert!(store.load_identity().unwrap().is_some());
}

#[test]
fn a_flipped_byte_in_the_wrapper_is_rejected() {
    let dir = temp();
    let vault = TestVault::empty();
    {
        let _ = open(dir.path(), &vault);
    }

    let path = dir.path().join(WRAPPER_FILE);
    let mut raw = std::fs::read(&path).unwrap();
    let last = raw.len() - 1;
    raw[last] ^= 0xff;
    std::fs::write(&path, &raw).unwrap();

    let err = Storage::open(dir.path(), &vault).expect_err("подменённая обёртка прошла");
    assert!(
        !matches!(err, StorageError::KeyGone),
        "повреждение файла выдано за потерю ключа: данные-то целы"
    );
}

#[test]
fn a_tampered_wrapper_header_is_rejected() {
    let dir = temp();
    let vault = TestVault::empty();
    {
        let _ = open(dir.path(), &vault);
    }

    // Байт уровня защиты лежит открытым — восьмой. Подправив его, противник
    // поменял бы надпись на экране, не трогая больше ничего. Заголовок повторён
    // внутри запечатанного, и здесь это ловится.
    let path = dir.path().join(WRAPPER_FILE);
    let mut raw = std::fs::read(&path).unwrap();
    // Было 2 (StrongBox) — ставим 0 (программный): именно такую подмену и
    // хотел бы сделать противник, чтобы экран соврал в успокоительную сторону.
    assert_eq!(raw[7], 2, "подстава объявляет StrongBox");
    raw[7] = 0;
    std::fs::write(&path, &raw).unwrap();

    assert!(
        Storage::open(dir.path(), &vault).is_err(),
        "подправленный уровень защиты прошёл как настоящий"
    );
}

#[test]
fn a_foreign_file_is_not_taken_for_a_wrapper() {
    let dir = temp();
    let vault = TestVault::empty();
    std::fs::write(dir.path().join(WRAPPER_FILE), b"not apeiron at all").unwrap();
    assert!(Storage::open(dir.path(), &vault).is_err());
}

/// Список собеседников не должен читаться из файла базы без ключа.
///
/// Если хранить публичный ключ открытым — ради удобного поиска, — то файл сам
/// расскажет, с кем человек переписывается. Это ровно то, что база и должна
/// была закрыть, поэтому поиск идёт по непрозрачной метке.
#[test]
fn nothing_secret_is_readable_from_the_database_file() {
    let dir = temp();
    let vault = TestVault::empty();

    let me = Identity::generate().unwrap();
    let secret = me.export_secret();
    let peer = Identity::generate().unwrap();
    let peer_public = peer.public().to_bytes();

    {
        let store = open(dir.path(), &vault);
        store.save_identity(&me).unwrap();
        store.save_contact(&peer.public(), "Приметное имя").unwrap();
        store.meta_set("заметка", "тайна".as_bytes()).unwrap();
    }

    let mut blob = std::fs::read(dir.path().join(DATABASE_FILE)).unwrap();
    for suffix in ["-wal", "-shm"] {
        let extra = dir.path().join(format!("{DATABASE_FILE}{suffix}"));
        if extra.exists() {
            blob.extend_from_slice(&std::fs::read(&extra).unwrap());
        }
    }

    assert!(
        !contains(&blob, secret.as_bytes()),
        "секрет личности лежит в файле базы открытым"
    );
    assert!(
        !contains(&blob, &peer_public),
        "публичный ключ собеседника лежит в файле базы открытым"
    );
    assert!(
        !contains(&blob, "Приметное имя".as_bytes()),
        "имя собеседника лежит в файле базы открытым"
    );
    assert!(
        !contains(&blob, "тайна".as_bytes()),
        "служебное значение лежит в файле базы открытым"
    );
    // Контроль: сам поиск работает, и «ничего не нашлось» не оттого, что искать
    // не умеет.
    assert!(contains(&blob, b"SQLite format 3"));
}

/// После стирания старый шифротекст не читается ничем — даже если копию успели
/// снять. Это R-005: уничтожается ключ, а не данные.
#[test]
fn wipe_makes_the_old_ciphertext_unreadable() {
    let dir = temp();
    let vault = TestVault::empty();
    {
        let store = open(dir.path(), &vault);
        store.save_identity(&Identity::generate().unwrap()).unwrap();
    }

    // Копия снята до стирания — как её снял бы противник.
    let copy = std::fs::read(dir.path().join(DATABASE_FILE)).unwrap();
    let wrapper_copy = std::fs::read(dir.path().join(WRAPPER_FILE)).unwrap();

    Storage::wipe(dir.path(), &vault).unwrap();
    assert!(!vault.has_key(), "ключ пережил стирание");
    assert!(!dir.path().join(WRAPPER_FILE).exists());
    assert!(!dir.path().join(DATABASE_FILE).exists());

    // Возвращаем копию на место: без ключа она бесполезна.
    std::fs::write(dir.path().join(DATABASE_FILE), &copy).unwrap();
    std::fs::write(dir.path().join(WRAPPER_FILE), &wrapper_copy).unwrap();
    let err = Storage::open(dir.path(), &vault).expect_err("копия открылась после стирания");
    assert!(matches!(err, StorageError::KeyGone));
}

/// После стирания можно начать заново, и это уже другая личность.
#[test]
fn a_fresh_start_after_wipe_is_a_different_identity() {
    let dir = temp();
    let vault = TestVault::empty();

    let first = Identity::generate().unwrap();
    {
        let store = open(dir.path(), &vault);
        store.save_identity(&first).unwrap();
    }

    Storage::wipe(dir.path(), &vault).unwrap();

    let store = open(dir.path(), &vault);
    assert!(store.created_now());
    assert!(
        store.load_identity().unwrap().is_none(),
        "после стирания нашлась прежняя личность"
    );
}

#[test]
fn a_database_from_a_newer_schema_is_refused() {
    let dir = temp();
    let vault = TestVault::empty();
    {
        let _ = open(dir.path(), &vault);
    }

    let conn = rusqlite::Connection::open(dir.path().join(DATABASE_FILE)).unwrap();
    conn.execute("UPDATE schema_version SET version = 99 WHERE id = 1", [])
        .unwrap();
    drop(conn);

    let err = Storage::open(dir.path(), &vault).expect_err("база новее прочиталась");
    assert!(
        matches!(
            err,
            StorageError::SchemaTooNew {
                found: 99,
                known: _
            }
        ),
        "вместо отказа по версии схемы пришло: {err}"
    );
}

#[test]
fn a_record_moved_to_another_row_is_rejected() {
    let dir = temp();
    let vault = TestVault::empty();

    let first = Identity::generate().unwrap();
    let second = Identity::generate().unwrap();
    {
        let store = open(dir.path(), &vault);
        store.save_contact(&first.public(), "первый").unwrap();
        store.save_contact(&second.public(), "второй").unwrap();
    }

    // Переставляем запечатанное содержимое двух строк местами, не трогая метки.
    let conn = rusqlite::Connection::open(dir.path().join(DATABASE_FILE)).unwrap();
    let a: Vec<u8> = conn
        .query_row("SELECT sealed FROM contacts WHERE id = 1", [], |r| r.get(0))
        .unwrap();
    let b: Vec<u8> = conn
        .query_row("SELECT sealed FROM contacts WHERE id = 2", [], |r| r.get(0))
        .unwrap();
    conn.execute(
        "UPDATE contacts SET sealed = ?1 WHERE id = 1",
        rusqlite::params![b],
    )
    .unwrap();
    conn.execute(
        "UPDATE contacts SET sealed = ?1 WHERE id = 2",
        rusqlite::params![a],
    )
    .unwrap();
    drop(conn);

    let store = open(dir.path(), &vault);
    assert!(
        store.contacts().is_err(),
        "переставленные записи прочитались как свои"
    );
}

#[test]
fn the_vault_refuses_oversized_payloads() {
    // Предел на входе KEK — не придирка: StrongBox медленнее TEE в десятки раз,
    // и мегабайт через него шифруется порядка пятнадцати секунд.
    let vault = TestVault::empty();
    vault.ensure_key(true).unwrap();
    assert!(vault.wrap(&[0u8; 65]).is_err());
    assert!(vault.wrap(&[]).is_err());
    assert!(vault.wrap(&[0u8; 34]).is_ok());
}

// ─── Самопроверка ────────────────────────────────────────────────────────────

/// На первом запуске самопроверка обязана говорить «проверить нечего», а не
/// рапортовать успехом.
///
/// Зелёная строка там, где ничего не проверялось, — ровно та ложная
/// уверенность, против которой самопроверка и затевалась.
#[test]
fn the_self_check_does_not_claim_success_on_the_first_run() {
    let dir = temp();
    let vault = TestVault::empty();
    let store = open(dir.path(), &vault);
    store.save_identity(&Identity::generate().unwrap()).unwrap();
    store.save_account(&Account::new()).unwrap();

    let checks = apeiron_store::selfcheck::run(&store);
    let survived = find(&checks, "переписка читается после перезапуска");
    assert!(
        !survived.passed,
        "на первом запуске объявлено, что состояние пережило перезапуск"
    );
    assert!(survived.detail.contains("Перезапустите"));
}

/// А на втором — обязана пройти целиком.
#[test]
fn the_self_check_passes_after_a_real_reopen() {
    let dir = temp();
    let vault = TestVault::empty();
    {
        let store = open(dir.path(), &vault);
        store.save_identity(&Identity::generate().unwrap()).unwrap();
        store.save_account(&Account::new()).unwrap();
        let _ = apeiron_store::selfcheck::run(&store);
    }

    let store = open(dir.path(), &vault);
    let checks = apeiron_store::selfcheck::run(&store);
    let failed: Vec<_> = checks
        .iter()
        .filter(|c| !c.passed)
        .map(|c| format!("{}: {}", c.name, c.detail))
        .collect();
    assert!(
        failed.is_empty(),
        "после настоящего перезапуска не прошло: {failed:#?}"
    );
    assert_eq!(
        find(&checks, "запусков с этим хранилищем").detail,
        "2",
        "запуски не считаются"
    );
}

/// Самопроверка не имеет права трогать боевые данные.
#[test]
fn the_self_check_touches_nothing_it_checks() {
    let dir = temp();
    let vault = TestVault::empty();
    let me = Identity::generate().unwrap();
    let peer = Identity::generate().unwrap();

    let store = open(dir.path(), &vault);
    store.save_identity(&me).unwrap();
    store.save_account(&Account::new()).unwrap();
    let contact = store.save_contact(&peer.public(), "Собеседник").unwrap();

    let _ = apeiron_store::selfcheck::run(&store);
    let _ = apeiron_store::selfcheck::run(&store);

    assert_eq!(
        store
            .load_identity()
            .unwrap()
            .unwrap()
            .public()
            .fingerprint(),
        me.public().fingerprint()
    );
    let found = store.find_contact(&peer.public()).unwrap().unwrap();
    assert_eq!(found.id, contact);
    assert_eq!(found.name, "Собеседник");
    assert!(vault.has_key(), "самопроверка уничтожила ключ");
}

fn find<'a>(
    checks: &'a [apeiron_store::selfcheck::Check],
    name: &str,
) -> &'a apeiron_store::selfcheck::Check {
    checks
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("в отчёте нет строки «{name}»"))
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

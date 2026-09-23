//! Проверка парной переписки — по разделам эталона `radio-mesh-demo/s07_ratchet.py`.
//!
//! Эталон написан на чистом Python и служит описанием требуемого поведения:
//! обмен со сменой направления (раздел C), канал с потерями и перестановками
//! (D), прямая секретность (E), атака посредника (F). Здесь то же самое
//! требуется от нашей реализации на Rust.
//!
//! Тесты интеграционные намеренно: им доступен только публичный API ядра. Если
//! что-то нельзя сделать снаружи — этого нельзя сделать и в приложении.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use apeiron_core::vodozemac::olm::{Account, OlmMessage, SessionConfig, SessionCreationError};
use apeiron_core::vodozemac::Curve25519PublicKey;
use apeiron_core::{Chat, Identity, PrekeyBundle};

/// Участник: долговременная личность плюс устройство с ключами Olm.
struct Party {
    identity: Identity,
    account: Account,
}

impl Party {
    fn new() -> Self {
        Self {
            identity: Identity::generate().expect("ОС отдаёт случайность"),
            account: Account::new(),
        }
    }

    /// Пакет пред-ключей для передачи другой стороне.
    fn bundle(&mut self) -> Vec<u8> {
        PrekeyBundle::create(&self.identity, &mut self.account)
            .expect("пакет собирается")
            .to_bytes()
            .to_vec()
    }
}

/// Разбирает и проверяет пакет — как это обязано делать приложение.
fn accept_bundle(bytes: &[u8]) -> PrekeyBundle {
    PrekeyBundle::parse(bytes)
        .expect("пакет разбирается")
        .verify()
        .expect("подпись верна")
}

fn prekey_of(message: &OlmMessage) -> &apeiron_core::vodozemac::olm::PreKeyMessage {
    match message {
        OlmMessage::PreKey(m) => m,
        OlmMessage::Normal(_) => panic!("ожидалось сообщение установления сессии"),
    }
}

// ─── Раздел C. Обмен со сменой направления ───────────────────────────────────

#[test]
fn exchange_with_direction_changes() {
    let mut alice = Party::new();
    let mut bob = Party::new();

    let bob_bundle = accept_bundle(&bob.bundle());
    let alice_bundle = accept_bundle(&alice.bundle());

    let mut a_chat = Chat::initiate(&alice.account, &bob_bundle).expect("сессия создаётся");
    let first = a_chat.encrypt("здравствуй").expect("шифруется");

    let (mut b_chat, text) =
        Chat::accept(&mut bob.account, &alice_bundle, prekey_of(&first)).expect("сессия принята");
    assert_eq!(text, "здравствуй");

    // Идентификатор сессии совпадает у обеих сторон — они в одной переписке.
    assert_eq!(a_chat.session_id(), b_chat.session_id());

    // Пять разворотов подряд: каждый разворот двигает DH-храповик.
    for round in 0..5 {
        let from_bob = b_chat
            .encrypt(&format!("ответ {round}"))
            .expect("шифруется");
        assert_eq!(
            a_chat.decrypt(&from_bob).expect("расшифровывается"),
            format!("ответ {round}")
        );

        let from_alice = b_chat_reply(&mut a_chat, round);
        assert_eq!(
            b_chat.decrypt(&from_alice).expect("расшифровывается"),
            format!("вопрос {round}")
        );
    }

    // Каждая сторона видит личность собеседника — ту, что показывается на
    // экране сверки.
    assert_eq!(
        a_chat.peer().fingerprint(),
        bob.identity.public().fingerprint()
    );
    assert_eq!(
        b_chat.peer().fingerprint(),
        alice.identity.public().fingerprint()
    );
}

fn b_chat_reply(chat: &mut Chat, round: usize) -> OlmMessage {
    chat.encrypt(&format!("вопрос {round}")).expect("шифруется")
}

// ─── Раздел D. Приём вне порядка ─────────────────────────────────────────────

/// Двести сообщений задом наперёд.
///
/// Это и есть случай, ради которого написан `decrypt_batch`: после суток
/// offline очередь ретранслятора отдаёт всё разом и в произвольном порядке.
#[test]
fn batch_survives_reverse_order() {
    const COUNT: usize = 200;
    let (mut a_chat, mut b_chat) = established_pair();

    let mut sent = Vec::with_capacity(COUNT);
    let mut texts = Vec::with_capacity(COUNT);
    for i in 0..COUNT {
        let text = format!("сообщение {i}");
        sent.push(a_chat.encrypt(&text).expect("шифруется"));
        texts.push(text);
    }

    // Пачка приходит задом наперёд.
    let mut reversed = sent.clone();
    reversed.reverse();

    let results = b_chat.decrypt_batch(&reversed);
    assert_eq!(results.len(), COUNT);

    let lost = results.iter().filter(|r| r.is_err()).count();
    assert_eq!(
        lost, 0,
        "при разложении по порядку не должно теряться ничего"
    );

    for (position, result) in results.iter().enumerate() {
        let expected = &texts[COUNT - 1 - position];
        assert_eq!(result.as_ref().expect("прочитано"), expected);
    }
}

/// Контрольный опыт: то же самое без разложения по порядку.
///
/// Нужен, чтобы `decrypt_batch` не выглядел лишней предосторожностью. Если
/// однажды кто-то решит, что сортировка не нужна, этот тест покажет цену.
#[test]
fn naive_reverse_order_loses_messages() {
    const COUNT: usize = 200;
    let (mut a_chat, mut b_chat) = established_pair();

    let mut sent = Vec::with_capacity(COUNT);
    for i in 0..COUNT {
        sent.push(
            a_chat
                .encrypt(&format!("сообщение {i}"))
                .expect("шифруется"),
        );
    }
    sent.reverse();

    let lost = sent
        .iter()
        .map(|m| b_chat.decrypt(m))
        .filter(|r| r.is_err())
        .count();

    assert!(
        lost > 100,
        "наивная расшифровка задом наперёд обязана терять сообщения, потеряно {lost}"
    );
}

/// Канал с потерями и перестановками — модель раздела D эталона.
#[test]
fn lossy_and_shuffled_channel() {
    const COUNT: usize = 40;
    let (mut a_chat, mut b_chat) = established_pair();

    let mut sent = Vec::with_capacity(COUNT);
    for i in 0..COUNT {
        sent.push((
            i,
            a_chat
                .encrypt(&format!("сообщение {i}"))
                .expect("шифруется"),
        ));
    }

    // Детерминированная «сеть»: теряем каждое третье, остальное переставляем
    // простым обратимым правилом. Случайность здесь не нужна — нужна
    // воспроизводимость: упавший тест должен падать снова.
    let delivered: Vec<(usize, OlmMessage)> =
        sent.into_iter().filter(|(i, _)| i % 3 != 0).collect();
    let mut shuffled = delivered.clone();
    shuffled.reverse();
    shuffled.rotate_left(7);

    let messages: Vec<OlmMessage> = shuffled.iter().map(|(_, m)| m.clone()).collect();
    let results = b_chat.decrypt_batch(&messages);

    for ((original, _), result) in shuffled.iter().zip(results.iter()) {
        assert_eq!(
            result.as_ref().expect("доставленное читается"),
            &format!("сообщение {original}")
        );
    }

    // Потерянное в сети остаётся потерянным — и это нормально: восстанавливать
    // его должен слой доставки, а не храповик.
    assert_eq!(results.iter().filter(|r| r.is_err()).count(), 0);
}

// ─── Раздел E. Прямая секретность ────────────────────────────────────────────

/// Ключ использованного сообщения уничтожается.
///
/// Прямая секретность в операционном виде: состояние, захваченное **сейчас**,
/// не читает то, что прочитано раньше. Заодно это защита от повтора — то же
/// сообщение второй раз не пройдёт.
#[test]
fn used_message_key_is_gone() {
    let (mut a_chat, mut b_chat) = established_pair();

    let first = a_chat.encrypt("первое").expect("шифруется");
    assert_eq!(b_chat.decrypt(&first).expect("читается"), "первое");

    for i in 0..10 {
        let m = a_chat.encrypt(&format!("ещё {i}")).expect("шифруется");
        b_chat.decrypt(&m).expect("читается");
    }

    let again = b_chat.decrypt(&first);
    let err = again.expect_err("повторное чтение того же сообщения невозможно");
    assert!(
        err.is_lost_forever(),
        "ошибка должна говорить о безвозвратной потере ключа, а не о поломке: {err}"
    );
}

// ─── Раздел F. Посредник ─────────────────────────────────────────────────────

/// Без сверки отпечатков посредник побеждает полностью.
///
/// Это не предупреждение в документации, а исполняемый факт: обе стороны видят
/// исправно работающее шифрование, подписи верны, и обе переписываются
/// с Мэллори. Единственное, что его выдаёт, — расхождение числа сверки.
#[test]
fn mitm_succeeds_without_fingerprint_check_and_fails_with_it() {
    let mut alice = Party::new();
    let mut bob = Party::new();
    let mut mallory = Party::new();

    // Мэллори перехватывает обмен пакетами и подсовывает каждой стороне свой.
    // Настоящие пакеты Алисы и Боба он оставляет себе.
    let alice_gets = accept_bundle(&mallory.bundle()); // Алиса думает, что это Боб
    let bob_gets = accept_bundle(&mallory.bundle()); // Боб думает, что это Алиса
    let alice_real = accept_bundle(&alice.bundle());
    let bob_real = accept_bundle(&bob.bundle());

    // Подписи **верны**: Мэллори подписал свои ключи своей личностью.
    // Криптография не нарушена ни в одном месте — нарушена модель доверия.
    let mut alice_chat = Chat::initiate(&alice.account, &alice_gets).expect("сессия создаётся");
    let first = alice_chat.encrypt("секрет").expect("шифруется");

    let (_, intercepted) =
        Chat::accept(&mut mallory.account, &alice_real, prekey_of(&first)).expect("Мэллори читает");
    assert_eq!(intercepted, "секрет", "посредник читает открытый текст");

    // И пересылает дальше Бобу — уже своей сессией, настоящим пакетом Боба.
    let mut mallory_with_bob =
        Chat::initiate(&mallory.account, &bob_real).expect("сессия создаётся");
    let forwarded = mallory_with_bob.encrypt(&intercepted).expect("шифруется");

    let (bob_chat, seen_by_bob) =
        Chat::accept(&mut bob.account, &bob_gets, prekey_of(&forwarded)).expect("Боб принимает");
    assert_eq!(
        seen_by_bob, "секрет",
        "Боб видит текст и ничего не подозревает"
    );

    // А теперь сверка. Алиса и Боб читают друг другу числа сверки вслух.
    let alice_sees = alice.identity.public().safety_number(alice_chat.peer());
    let bob_sees = bob.identity.public().safety_number(bob_chat.peer());

    assert_ne!(
        alice_sees, bob_sees,
        "числа сверки обязаны разойтись — иначе посредник неотличим"
    );

    // Для сравнения: честный случай, где числа совпадают.
    let honest_alice = alice
        .identity
        .public()
        .safety_number(&bob.identity.public());
    let honest_bob = bob
        .identity
        .public()
        .safety_number(&alice.identity.public());
    assert_eq!(honest_alice, honest_bob);
}

// ─── Проверки пакета пред-ключей ─────────────────────────────────────────────

#[test]
fn tampered_bundle_is_rejected() {
    let mut bob = Party::new();
    let bytes = bob.bundle();

    // Меняем один бит в ключе устройства — подпись перестаёт сходиться.
    for position in [64usize, 80, 100, 130] {
        let mut broken = bytes.clone();
        broken[position] ^= 0b0000_0001;
        // Часть байтов — сами ключи; такие пакеты могут не разобраться вовсе,
        // и это тоже отказ.
        if let Ok(unverified) = PrekeyBundle::parse(&broken) {
            assert!(
                unverified.verify().is_err(),
                "подменённый байт {position} обязан ломать проверку"
            );
        }
    }
}

#[test]
fn bundle_of_wrong_length_is_rejected() {
    let mut bob = Party::new();
    let bytes = bob.bundle();
    assert!(PrekeyBundle::parse(&bytes[..bytes.len() - 1]).is_err());
    assert!(PrekeyBundle::parse(&[]).is_err());
}

#[test]
fn each_bundle_carries_a_fresh_one_time_key() {
    let mut bob = Party::new();
    let first = accept_bundle(&bob.bundle());
    let second = accept_bundle(&bob.bundle());
    assert_ne!(
        first.one_time_key().to_bytes(),
        second.one_time_key().to_bytes(),
        "одноразовый ключ на то и одноразовый"
    );
}

/// Нулевой публичный ключ обязан отвергаться.
///
/// Замечание февраля 2026 года к vodozemac: принимались полностью нулевые
/// ключи, дающие предсказуемый нулевой общий секрет. Исправлено в 0.10.0.
/// Проверяем это **своим** тестом, а не на слово: если однажды зависимость
/// поедет назад, здесь станет видно.
#[test]
fn zero_public_key_is_rejected() {
    let mut bob = Party::new();
    let alice = Party::new();
    let bundle = accept_bundle(&bob.bundle());

    let zero = Curve25519PublicKey::from_bytes([0u8; 32]);

    let by_one_time = alice.account.create_outbound_session(
        SessionConfig::version_1(),
        bundle.device_curve_key(),
        zero,
    );
    assert!(
        matches!(by_one_time, Err(SessionCreationError::NonContributoryKey)),
        "нулевой одноразовый ключ обязан отвергаться"
    );

    let by_identity = alice.account.create_outbound_session(
        SessionConfig::version_1(),
        zero,
        bundle.one_time_key(),
    );
    assert!(
        matches!(by_identity, Err(SessionCreationError::NonContributoryKey)),
        "нулевой ключ устройства обязан отвергаться"
    );
}

// ─── Общее ───────────────────────────────────────────────────────────────────

/// Пара с уже установленной сессией: Алиса написала, Боб принял.
fn established_pair() -> (Chat, Chat) {
    let mut alice = Party::new();
    let mut bob = Party::new();

    let bob_bundle = accept_bundle(&bob.bundle());
    let alice_bundle = accept_bundle(&alice.bundle());

    let mut a_chat = Chat::initiate(&alice.account, &bob_bundle).expect("сессия создаётся");
    let hello = a_chat.encrypt("начало").expect("шифруется");
    let (b_chat, text) =
        Chat::accept(&mut bob.account, &alice_bundle, prekey_of(&hello)).expect("сессия принята");
    assert_eq!(text, "начало");

    (a_chat, b_chat)
}

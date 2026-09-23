//! Проверка журнала личности.
//!
//! Ценность журнала не в том, что он принимает правильные записи, а в том, что
//! отвергает неправильные. Поэтому здесь на один тест «работает» приходится
//! десяток «не даёт себя обмануть».

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use apeiron_core::vodozemac::olm::Account;
use apeiron_core::{EntryBody, Identity, Sigchain, SigchainError};

fn identity() -> Identity {
    Identity::generate().expect("ОС отдаёт случайность")
}

/// Устройство: пара ключей Olm, как у настоящего телефона.
fn device() -> (Account, [u8; 32], [u8; 32]) {
    let account = Account::new();
    let ed = *account.ed25519_key().as_bytes();
    let curve = account.curve25519_key().to_bytes();
    (account, ed, curve)
}

fn add(ed: [u8; 32], curve: [u8; 32]) -> EntryBody {
    EntryBody::AddDevice { ed, curve }
}

// ─── Как должно быть ─────────────────────────────────────────────────────────

#[test]
fn fresh_chain_verifies() {
    let root = identity();
    let chain = Sigchain::create(&root).unwrap();
    let state = chain.verify().unwrap();

    assert_eq!(state.root().fingerprint(), root.public().fingerprint());
    assert_eq!(state.active_devices().count(), 0);
    assert_eq!(chain.len(), 1);
}

#[test]
fn devices_are_added_and_revoked() {
    let root = identity();
    let (_, ed1, curve1) = device();
    let (_, ed2, curve2) = device();

    let mut chain = Sigchain::create(&root).unwrap();
    chain.append(&root, add(ed1, curve1)).unwrap();
    chain.append(&root, add(ed2, curve2)).unwrap();

    assert_eq!(chain.verify().unwrap().active_devices().count(), 2);

    chain
        .append(&root, EntryBody::RevokeDevice { ed: ed1 })
        .unwrap();

    let state = chain.verify().unwrap();
    assert_eq!(state.active_devices().count(), 1);
    assert!(!state.is_active(&ed1), "отозванное устройство не действует");
    assert!(state.is_known(&ed1), "но из журнала оно не исчезает");
    assert!(state.is_active(&ed2));
}

/// Устройство добавляет другое устройство.
///
/// Без этого второй телефон можно было бы завести только с первого, а потеряв
/// его — уже никак.
#[test]
fn an_active_device_may_sign() {
    let root = identity();
    let (account1, ed1, curve1) = device();
    let (_, ed2, curve2) = device();

    let mut chain = Sigchain::create(&root).unwrap();
    chain.append(&root, add(ed1, curve1)).unwrap();
    chain.append(&account1, add(ed2, curve2)).unwrap();

    assert_eq!(chain.verify().unwrap().active_devices().count(), 2);
}

#[test]
fn serialisation_roundtrip() {
    let root = identity();
    let (_, ed, curve) = device();

    let mut chain = Sigchain::create(&root).unwrap();
    chain.append(&root, add(ed, curve)).unwrap();

    let bytes = chain.to_bytes();
    let parsed = Sigchain::parse(&bytes).unwrap();

    assert_eq!(parsed.len(), chain.len());
    assert_eq!(parsed.to_bytes(), bytes);
    assert!(parsed.verify().unwrap().is_active(&ed));
}

// ─── Чего быть не должно ─────────────────────────────────────────────────────

#[test]
fn foreign_identity_cannot_append() {
    let root = identity();
    let stranger = identity();
    let (_, ed, curve) = device();

    let mut chain = Sigchain::create(&root).unwrap();
    let err = chain.append(&stranger, add(ed, curve)).unwrap_err();

    assert!(matches!(err, SigchainError::UnknownSigner(_)), "{err}");
    assert_eq!(chain.len(), 1, "отвергнутая запись не должна оставаться");
}

#[test]
fn revoked_device_cannot_sign() {
    let root = identity();
    let (account, ed, curve) = device();
    let (_, other_ed, other_curve) = device();

    let mut chain = Sigchain::create(&root).unwrap();
    chain.append(&root, add(ed, curve)).unwrap();
    chain.append(&root, EntryBody::RevokeDevice { ed }).unwrap();

    let err = chain
        .append(&account, add(other_ed, other_curve))
        .unwrap_err();
    assert!(matches!(err, SigchainError::RevokedSigner(_)), "{err}");
}

#[test]
fn device_cannot_be_added_twice() {
    let root = identity();
    let (_, ed, curve) = device();

    let mut chain = Sigchain::create(&root).unwrap();
    chain.append(&root, add(ed, curve)).unwrap();

    let err = chain.append(&root, add(ed, curve)).unwrap_err();
    assert!(matches!(err, SigchainError::DuplicateDevice(_)), "{err}");
}

/// Отзыв необратим.
///
/// Иначе противник, получивший корневой ключ на минуту, мог бы вернуть
/// отозванное устройство, и отзыв перестал бы что-либо значить.
#[test]
fn revoked_device_cannot_be_added_again() {
    let root = identity();
    let (_, ed, curve) = device();

    let mut chain = Sigchain::create(&root).unwrap();
    chain.append(&root, add(ed, curve)).unwrap();
    chain.append(&root, EntryBody::RevokeDevice { ed }).unwrap();

    let err = chain.append(&root, add(ed, curve)).unwrap_err();
    assert!(matches!(err, SigchainError::DuplicateDevice(_)), "{err}");
}

#[test]
fn unknown_device_cannot_be_revoked() {
    let root = identity();
    let (_, ed, _) = device();

    let mut chain = Sigchain::create(&root).unwrap();
    let err = chain
        .append(&root, EntryBody::RevokeDevice { ed })
        .unwrap_err();
    assert!(matches!(err, SigchainError::UnknownDevice(_)), "{err}");
}

#[test]
fn device_cannot_be_revoked_twice() {
    let root = identity();
    let (_, ed, curve) = device();

    let mut chain = Sigchain::create(&root).unwrap();
    chain.append(&root, add(ed, curve)).unwrap();
    chain.append(&root, EntryBody::RevokeDevice { ed }).unwrap();

    let err = chain
        .append(&root, EntryBody::RevokeDevice { ed })
        .unwrap_err();
    assert!(matches!(err, SigchainError::AlreadyRevoked(_)), "{err}");
}

/// Любая правка байта ломает журнал.
#[test]
fn any_altered_byte_breaks_the_chain() {
    let bytes = sample_chain().to_bytes();

    // Проходим по всей длине с шагом, чтобы задеть все поля: номер, ссылку,
    // ключ подписавшего, тело, подпись.
    let mut checked = 0;
    for position in (0..bytes.len()).step_by(7) {
        let mut broken = bytes.clone();
        broken[position] ^= 0b0000_0001;
        if broken == bytes {
            continue;
        }
        checked += 1;

        let verdict = Sigchain::parse(&broken).and_then(|c| c.verify().map(|_| ()));
        assert!(
            verdict.is_err(),
            "правка байта {position} осталась незамеченной"
        );
    }
    assert!(checked > 20, "проверено слишком мало позиций: {checked}");
}

/// Изъятие записи обязано быть видно.
///
/// Это и есть причина, по которой каждая запись ссылается на хеш предыдущей:
/// подпись защищает запись по отдельности, а от утаивания записи — например,
/// отзыва устройства — защищает только ссылка.
#[test]
fn a_removed_entry_is_noticed() {
    let root = identity();
    let (_, ed1, curve1) = device();
    let (_, ed2, curve2) = device();

    let mut chain = Sigchain::create(&root).unwrap();
    let after_genesis = chain.to_bytes().len();

    chain.append(&root, add(ed1, curve1)).unwrap();
    let after_first = chain.to_bytes().len();

    chain.append(&root, add(ed2, curve2)).unwrap();
    let bytes = chain.to_bytes();

    // Выкидываем среднюю запись — «я не добавлял это устройство».
    let mut without_middle = Vec::new();
    without_middle.extend_from_slice(&bytes[..after_genesis]);
    without_middle.extend_from_slice(&bytes[after_first..]);

    let verdict = Sigchain::parse(&without_middle).and_then(|c| c.verify().map(|_| ()));
    let err = verdict.unwrap_err();
    assert!(
        matches!(
            err,
            SigchainError::BrokenLink(_) | SigchainError::OutOfOrder { .. }
        ),
        "{err}"
    );
}

#[test]
fn reordered_entries_are_noticed() {
    let root = identity();
    let (_, ed1, curve1) = device();
    let (_, ed2, curve2) = device();

    let mut chain = Sigchain::create(&root).unwrap();
    let a = chain.to_bytes().len();
    chain.append(&root, add(ed1, curve1)).unwrap();
    let b = chain.to_bytes().len();
    chain.append(&root, add(ed2, curve2)).unwrap();
    let bytes = chain.to_bytes();

    // Меняем местами вторую и третью записи.
    let mut swapped = Vec::new();
    swapped.extend_from_slice(&bytes[..a]);
    swapped.extend_from_slice(&bytes[b..]);
    swapped.extend_from_slice(&bytes[a..b]);

    let verdict = Sigchain::parse(&swapped).and_then(|c| c.verify().map(|_| ()));
    assert!(verdict.is_err(), "перестановка записей обязана ловиться");
}

#[test]
fn truncated_bytes_are_rejected() {
    let bytes = sample_chain().to_bytes();
    for cut in [1usize, 10, 40, 100] {
        if cut >= bytes.len() {
            continue;
        }
        let verdict = Sigchain::parse(&bytes[..bytes.len() - cut]);
        assert!(
            verdict.is_err(),
            "обрезанный на {cut} байт журнал обязан отвергаться"
        );
    }
    assert!(matches!(Sigchain::parse(&[]), Err(SigchainError::Empty)));
}

#[test]
fn a_second_genesis_is_rejected() {
    let root = identity();
    let mut chain = Sigchain::create(&root).unwrap();
    let err = chain
        .append(
            &root,
            EntryBody::Genesis {
                root: root.public(),
            },
        )
        .unwrap_err();
    assert!(matches!(err, SigchainError::RepeatedGenesis(_)), "{err}");
}

/// Чужой журнал, приклеенный к нашему началу, не должен проходить.
#[test]
fn entries_from_another_chain_do_not_graft() {
    let mine = sample_chain().to_bytes();
    let theirs = sample_chain().to_bytes();

    let mut grafted = mine.clone();
    grafted.extend_from_slice(&theirs);

    let verdict = Sigchain::parse(&grafted).and_then(|c| c.verify().map(|_| ()));
    assert!(verdict.is_err(), "склейка двух журналов обязана ловиться");
}

fn sample_chain() -> Sigchain {
    let root = identity();
    let (_, ed, curve) = device();
    let mut chain = Sigchain::create(&root).unwrap();
    chain.append(&root, add(ed, curve)).unwrap();
    chain
}

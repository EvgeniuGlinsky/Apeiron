//! Официальные тест-векторы стандартов.
//!
//! Критерий готовности этапа 2 требует прогнать против нашей реализации те же
//! векторы, что проходит эталон `radio-mesh-demo/s07_ratchet.py` (раздел A).
//! Векторы здесь те же самые — сверены с этим файлом, а он, в свою очередь,
//! с текстами RFC.
//!
//! # Зачем это, если примитивы чужие
//!
//! Именно потому, что чужие. Мы не пишем криптографию сами, но отвечаем за
//! выбор реализаций, и «крейт популярный» — не проверка. Проверка вот эта:
//! стандарт говорит, что при таком входе выход должен быть таким, и он такой.
//!
//! Тест ловит не ошибку в самих крейтах (её нашли бы раньше нас), а нашу
//! собственную: неверный порядок аргументов, перепутанные соль и метку,
//! подмену версии при обновлении зависимости. То есть ровно тот класс ошибок,
//! который не виден на глаз и не проявляется до самого взлома.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;

fn unhex(s: &str) -> Vec<u8> {
    hex::decode(s).expect("вектор записан шестнадцатерично")
}

fn array32(s: &str) -> [u8; 32] {
    unhex(s).try_into().expect("32 байта")
}

// ─── RFC 7748. X25519 ────────────────────────────────────────────────────────

/// Скалярное умножение, RFC 7748 п. 5.2.
#[test]
fn rfc7748_scalar_multiplication() {
    let k = array32("a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4");
    let u = array32("e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c");
    let expected = array32("c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552");

    assert_eq!(x25519_dalek::x25519(k, u), expected);
}

/// Обмен ключами, RFC 7748 п. 6.1: открытые ключи и общий секрет.
#[test]
fn rfc7748_key_exchange() {
    let alice_secret = array32("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a");
    let alice_public = array32("8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a");
    let bob_secret = array32("5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb");
    let bob_public = array32("de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f");
    let shared = array32("4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742");

    let base = x25519_dalek::X25519_BASEPOINT_BYTES;
    assert_eq!(x25519_dalek::x25519(alice_secret, base), alice_public);
    assert_eq!(x25519_dalek::x25519(bob_secret, base), bob_public);

    // Обе стороны приходят к одному секрету — тому самому, что в стандарте.
    assert_eq!(x25519_dalek::x25519(alice_secret, bob_public), shared);
    assert_eq!(x25519_dalek::x25519(bob_secret, alice_public), shared);
}

// ─── RFC 5869. HKDF-SHA256 ───────────────────────────────────────────────────

/// Test Case 1 из приложения A: и промежуточный PRK, и итоговый материал.
#[test]
fn rfc5869_test_case_1() {
    let ikm = vec![0x0b; 22];
    let salt = unhex("000102030405060708090a0b0c");
    let info = unhex("f0f1f2f3f4f5f6f7f8f9");
    let expected_prk = unhex("077709362c2e32df0ddc3f0dc47bba6390b6c73bb50f9c3122ec844ad7c2b3e5");
    let expected_okm = unhex(
        "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865",
    );

    let (prk, hk) = Hkdf::<Sha256>::extract(Some(&salt), &ikm);
    assert_eq!(prk.as_slice(), expected_prk.as_slice(), "PRK");

    let mut okm = vec![0u8; expected_okm.len()];
    hk.expand(&info, &mut okm).expect("42 байта выводятся");
    assert_eq!(okm, expected_okm, "OKM");
}

// ─── RFC 8439. ChaCha20-Poly1305 ─────────────────────────────────────────────

/// AEAD, RFC 8439 п. 2.8.2 — шифртекст, метка и отказ при подделке.
#[test]
fn rfc8439_aead() {
    let key: Vec<u8> = (0x80u8..0xA0).collect();
    let nonce_bytes = unhex("070000004041424344454647");
    let aad = unhex("50515253c0c1c2c3c4c5c6c7");
    let plaintext = "Ladies and Gentlemen of the class of '99: If I could offer you \
         only one tip for the future, sunscreen would be it."
        .as_bytes();

    let expected_ciphertext = unhex(
        "d31a8d34648e60db7b86afbc53ef7ec2a4aded51296e08fea9e2b5a736ee62d6\
         3dbea45e8ca9671282fafb69da92728b1a71de0a9e060b2905d6a5b67ecd3b36\
         92ddbd7f2d778b8c9803aee328091b58fab324e4fad675945585808b4831d7bc\
         3ff4def08e4b7a9de576d26586cec64b6116",
    );
    let expected_tag = unhex("1ae10b594f09e26a7e902ecbd0600691");

    let cipher = ChaCha20Poly1305::new_from_slice(&key).expect("ключ 32 байта");
    let nonce = Nonce::try_from(nonce_bytes.as_slice()).expect("12 байт");

    let sealed = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .expect("шифруется");

    // Крейт отдаёт шифртекст и метку одним куском, стандарт приводит их
    // по отдельности.
    let (ciphertext, tag) = sealed.split_at(sealed.len() - 16);
    assert_eq!(ciphertext, expected_ciphertext.as_slice(), "шифртекст");
    assert_eq!(tag, expected_tag.as_slice(), "метка аутентичности");

    let opened = cipher
        .decrypt(
            &nonce,
            Payload {
                msg: &sealed,
                aad: &aad,
            },
        )
        .expect("расшифровывается обратно");
    assert_eq!(opened, plaintext);

    // Подделка обязана отвергаться — в этом весь смысл аутентифицированного
    // шифрования. Шифрование без проверки подлинности запрещено прямо (§18).
    let mut forged = sealed.clone();
    forged[5] ^= 1;
    assert!(
        cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: &forged,
                    aad: &aad
                }
            )
            .is_err(),
        "подделка шифртекста обязана отвергаться"
    );

    // Подмена незашифрованной, но защищённой части — тоже.
    let mut other_aad = aad.clone();
    other_aad[0] ^= 1;
    assert!(
        cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: &sealed,
                    aad: &other_aad
                }
            )
            .is_err(),
        "подмена связанных данных обязана отвергаться"
    );
}

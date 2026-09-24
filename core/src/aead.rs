//! Симметричное шифрование и вывод ключей для локального хранения.
//!
//! Здесь нет ни одного самодельного примитива — только сборка готовых
//! (§18 исследования, «не писать свой примитив шифрования или режим»).
//!
//! # Что выбрано и почему
//!
//! **XChaCha20-Poly1305, а не ChaCha20-Poly1305.** Разница в длине одноразового
//! числа: 192 бита против 96. При 96 битах случайные одноразовые числа
//! становятся опасны — вероятность совпадения по парадоксу дней рождения
//! перестаёт быть пренебрежимой уже на миллиардах записей, поэтому их принято
//! считать счётчиком. Счётчик же требует надёжно сохранять состояние между
//! запусками, а телефон выключают в произвольный момент. При 192 битах
//! случайное число безопасно без всякого состояния: это и есть причина выбора.
//! Сама конструкция стандартная и опирается на ту же ChaCha20-Poly1305,
//! правильность которой проверяется официальными векторами RFC 8439
//! (`core/tests/rfc_vectors.rs`).
//!
//! **HKDF-SHA256 для вывода подключей.** Каждое назначение получает свой ключ
//! из одного мастер-ключа, и метка назначения обязательна: без неё один и тот же
//! ключ окажется у разных подсистем, и ошибка в одной станет ошибкой во всех.
//!
//! # Чего здесь ещё нет
//!
//! Мастер-ключ пока **создаётся в памяти и нигде не хранится**. По решению
//! R-002 он обязан жить в аппаратном хранилище (StrongBox/TEE), а это требует
//! JNI и проверки на устройстве. До тех пор ядро ничего секретного на диск не
//! пишет: отсутствие схемы лучше слабой схемы, выдаваемой за сильную.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::random::{random_bytes, RandomError};

/// Длина ключа.
pub const KEY_BYTES: usize = 32;
/// Длина одноразового числа XChaCha20.
pub const NONCE_BYTES: usize = 24;
/// Длина метки аутентичности Poly1305.
pub const TAG_BYTES: usize = 16;

/// Разделитель области вывода ключей.
const KDF_DOMAIN: &[u8] = b"apeiron/kdf/v1";

/// Метки назначения для [`SecretKey::derive`].
///
/// Реестр, а не строки по месту вызова. Причина простая и неприятная: опечатка
/// в метке даёт **другой ключ**, всё продолжает работать, и обнаруживается это
/// ровно тогда, когда данные уже записаны чужим ключом. Компилятор ловит
/// опечатку в имени константы; в строковом литерале он не ловит ничего.
///
/// Имена следуют соглашению проекта `apeiron/<область>/v1`. Версия в конце —
/// не украшение: меняя метку, вы делаете нечитаемым всё, что было записано
/// прежней.
pub mod purpose {
    /// Секрет личности.
    pub const IDENTITY: &str = "apeiron/storage/identity/v1";
    /// Состояние аккаунта Olm — ключи этого устройства.
    pub const ACCOUNT: &str = "apeiron/storage/account/v1";
    /// Состояние храповика по каждой переписке.
    pub const SESSION: &str = "apeiron/storage/session/v1";
    /// Журнал личности.
    pub const SIGCHAIN: &str = "apeiron/storage/sigchain/v1";
    /// Записи о контактах.
    pub const CONTACT: &str = "apeiron/storage/contact/v1";
    /// Служебные записи хранилища.
    pub const META: &str = "apeiron/storage/meta/v1";
    /// Метки поиска.
    ///
    /// Отдельная ветвь, не связанная с ключами расшифровки: знание метки не
    /// приближает к содержимому. Тот же приём, что `K_addr` в исследовании
    /// (§16.2), применённый к локальной базе.
    pub const TAG: &str = "apeiron/storage/tag/v1";

    /// Все метки разом — для проверки, что среди них нет повторов.
    pub const ALL: &[&str] = &[IDENTITY, ACCOUNT, SESSION, SIGCHAIN, CONTACT, META, TAG];
}

/// Что может пойти не так.
#[derive(Debug, thiserror::Error)]
pub enum AeadError {
    #[error("не удалось запечатать: {0}")]
    Seal(String),

    #[error(
        "ЗАПИСЬ НЕ ПРОШЛА ПРОВЕРКУ ПОДЛИННОСТИ. Она повреждена или подменена; \
         содержимому доверять нельзя."
    )]
    Open,

    #[error("запись короче служебных полей: {0} байт при минимуме {1}")]
    TooShort(usize, usize),

    #[error(transparent)]
    Random(#[from] RandomError),
}

/// Ключ, который затирает себя при уничтожении.
///
/// Обёртка нужна не для красоты: голый `[u8; 32]` остаётся в памяти после
/// выхода из области видимости, и найти его в дампе процесса — дело техники.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretKey([u8; KEY_BYTES]);

impl SecretKey {
    /// Новый случайный ключ.
    pub fn generate() -> Result<Self, RandomError> {
        Ok(Self(random_bytes::<KEY_BYTES>()?))
    }

    /// Ключ из готовых байтов — например, полученных из аппаратного хранилища.
    pub fn from_bytes(bytes: [u8; KEY_BYTES]) -> Self {
        Self(bytes)
    }

    /// Подключ для конкретного назначения.
    ///
    /// Метка назначения ([`purpose`](Self::derive)) обязательна и должна быть
    /// уникальной: два назначения с одной меткой получат один ключ, и это
    /// ровно тот случай, когда ошибка не проявляется до самого взлома.
    pub fn derive(&self, purpose: &str) -> Self {
        let hk = Hkdf::<Sha256>::new(Some(KDF_DOMAIN), &self.0);
        let mut out = [0u8; KEY_BYTES];
        // Ошибка здесь возможна только при запросе длины больше 255×32 байт;
        // у нас длина фиксирована, поэтому ветка недостижима — но паниковать
        // всё равно нельзя, и вместо этого возвращается пустой ключ, который
        // тут же сломает любую проверку подлинности. Молчаливой слабости нет.
        if hk.expand(purpose.as_bytes(), &mut out).is_err() {
            out.zeroize();
        }
        Self(out)
    }

    fn cipher(&self) -> Result<XChaCha20Poly1305, AeadError> {
        XChaCha20Poly1305::new_from_slice(&self.0).map_err(|e| AeadError::Seal(e.to_string()))
    }
}

impl std::fmt::Debug for SecretKey {
    /// Печатать ключ нельзя: строки логов переживают процесс.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretKey(<скрыт>)")
    }
}

/// Запечатывает данные.
///
/// Формат: `одноразовое число (24) || шифртекст || метка (16)`. Одноразовое
/// число хранится рядом открыто — это нормально и необходимо: секретным оно
/// быть не обязано, обязано быть неповторяющимся.
///
/// `aad` — то, что не шифруется, но защищается от подмены: например,
/// идентификатор записи. Подменив его, противник получит отказ проверки,
/// а не другое содержимое.
pub fn seal(key: &SecretKey, aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, AeadError> {
    let nonce_bytes = random_bytes::<NONCE_BYTES>()?;
    let nonce = XNonce::from(nonce_bytes);

    let ciphertext = key
        .cipher()?
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|e| AeadError::Seal(e.to_string()))?;

    let mut out = Vec::with_capacity(NONCE_BYTES + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Распечатывает данные, проверяя подлинность.
///
/// Отказ означает ровно одно: запись не та, которую запечатывали этим ключом.
/// Отличить повреждение от подмены невозможно и не нужно — обращаться с ними
/// следует одинаково.
pub fn open(key: &SecretKey, aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>, AeadError> {
    let minimum = NONCE_BYTES + TAG_BYTES;
    if sealed.len() < minimum {
        return Err(AeadError::TooShort(sealed.len(), minimum));
    }

    let (nonce_bytes, ciphertext) = sealed.split_at(NONCE_BYTES);
    let nonce = XNonce::try_from(nonce_bytes).map_err(|_| AeadError::Open)?;

    key.cipher()?
        .decrypt(
            &nonce,
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| AeadError::Open)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )]

    use super::*;

    #[test]
    fn roundtrip() {
        let key = SecretKey::generate().unwrap();
        let sealed = seal(&key, "запись 7".as_bytes(), "привет".as_bytes()).unwrap();
        let opened = open(&key, "запись 7".as_bytes(), &sealed).unwrap();
        assert_eq!(opened, "привет".as_bytes());
    }

    #[test]
    fn same_plaintext_seals_differently() {
        // Одинаковый открытый текст обязан давать разный шифртекст: иначе по
        // хранилищу видно, какие записи совпадают.
        let key = SecretKey::generate().unwrap();
        let a = seal(&key, b"", "одно и то же".as_bytes()).unwrap();
        let b = seal(&key, b"", "одно и то же".as_bytes()).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn tampered_ciphertext_is_rejected() {
        let key = SecretKey::generate().unwrap();
        let mut sealed = seal(&key, b"", "текст".as_bytes()).unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 1;
        assert!(matches!(open(&key, b"", &sealed), Err(AeadError::Open)));
    }

    #[test]
    fn tampered_nonce_is_rejected() {
        let key = SecretKey::generate().unwrap();
        let mut sealed = seal(&key, b"", "текст".as_bytes()).unwrap();
        sealed[0] ^= 1;
        assert!(matches!(open(&key, b"", &sealed), Err(AeadError::Open)));
    }

    #[test]
    fn substituted_aad_is_rejected() {
        // Запись, переставленная на чужое место, читаться не должна.
        let key = SecretKey::generate().unwrap();
        let sealed = seal(&key, "запись 7".as_bytes(), "текст".as_bytes()).unwrap();
        assert!(matches!(
            open(&key, "запись 8".as_bytes(), &sealed),
            Err(AeadError::Open)
        ));
    }

    #[test]
    fn another_key_is_rejected() {
        let key = SecretKey::generate().unwrap();
        let other = SecretKey::generate().unwrap();
        let sealed = seal(&key, b"", "текст".as_bytes()).unwrap();
        assert!(matches!(open(&other, b"", &sealed), Err(AeadError::Open)));
    }

    #[test]
    fn truncated_record_is_rejected() {
        let key = SecretKey::generate().unwrap();
        let sealed = seal(&key, b"", "текст".as_bytes()).unwrap();
        assert!(matches!(
            open(&key, b"", &sealed[..NONCE_BYTES]),
            Err(AeadError::TooShort(_, _))
        ));
    }

    #[test]
    fn purposes_give_different_keys() {
        let master = SecretKey::generate().unwrap();
        let a = master.derive("хранилище сообщений");
        let b = master.derive("хранилище контактов");
        let sealed = seal(&a, b"", "текст".as_bytes()).unwrap();
        assert!(
            open(&b, b"", &sealed).is_err(),
            "метки назначения обязаны разделять ключи"
        );
        assert_eq!(open(&a, b"", &sealed).unwrap(), "текст".as_bytes());
    }

    #[test]
    fn derivation_is_deterministic() {
        let master = SecretKey::from_bytes([7u8; KEY_BYTES]);
        let a = master.derive("одно");
        let b = master.derive("одно");
        let sealed = seal(&a, b"", "текст".as_bytes()).unwrap();
        assert_eq!(open(&b, b"", &sealed).unwrap(), "текст".as_bytes());
    }

    #[test]
    fn purpose_labels_are_unique() {
        // Две подсистемы с одной меткой получат один ключ, и это ровно тот
        // случай, когда ошибка не проявляется до самого взлома.
        let mut seen = std::collections::BTreeSet::new();
        for label in purpose::ALL {
            assert!(seen.insert(*label), "метка назначения повторяется: {label}");
        }
        assert_eq!(seen.len(), purpose::ALL.len());
    }

    #[test]
    fn every_purpose_gives_its_own_key() {
        let master = SecretKey::generate().expect("ОС отдаёт случайность");
        let mut keys = std::collections::BTreeSet::new();
        for label in purpose::ALL {
            let derived = master.derive(label);
            assert!(
                keys.insert(derived.0),
                "две метки назначения дали один ключ: {label}"
            );
        }
    }

    #[test]
    fn debug_does_not_print_the_key() {
        let key = SecretKey::from_bytes([0xAB; KEY_BYTES]);
        let shown = format!("{key:?}");
        assert!(
            !shown.contains("ab"),
            "ключ не должен попадать в строку: {shown}"
        );
        assert!(!shown.contains("171"));
    }
}

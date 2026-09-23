//! Долговременная личность устройства.
//!
//! Две независимые пары ключей:
//!   * Ed25519 — подпись (журнал ключей, подтверждение авторства);
//!   * X25519  — согласование ключей (вход в двойной храповик).
//!
//! Они не выводятся друг из друга намеренно: связывание одного типа ключа с другим —
//! источник тонких ошибок, а связь между ними и так устанавливается подписанным
//! журналом личности (sigchain).
//!
//! Секретные части не покидают Rust — решение R-004 в `docs/threat-log.md`.
//! `SigningKey` и `StaticSecret` затирают себя при уничтожении (feature `zeroize`),
//! поэтому собственный `Drop` здесь не нужен и намеренно не пишется.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey as X25519Public, StaticSecret};

/// Групп в отпечатке, читаемом вслух при сверке.
pub const FINGERPRINT_GROUPS: usize = 6;
/// Цифр в каждой группе.
pub const FINGERPRINT_DIGITS_PER_GROUP: usize = 5;

/// Байт в сериализованной публичной личности: 32 (Ed25519) + 32 (X25519).
pub const PUBLIC_IDENTITY_BYTES: usize = 64;

/// Разделитель области для хеша отпечатка. Меняя его, вы меняете все отпечатки:
/// это осознанно ломающее изменение, требующее повторной сверки пользователями.
const FINGERPRINT_DOMAIN: &[u8] = b"apeiron/fingerprint/v1";
const SAFETY_NUMBER_DOMAIN: &[u8] = b"apeiron/safety-number/v1";

#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    #[error("неверная длина: ожидалось {expected} байт, получено {got}")]
    Length { expected: usize, got: usize },
    #[error("байты не образуют корректный ключ Ed25519")]
    MalformedVerifyingKey,
    #[error("подпись не проходит проверку")]
    BadSignature,
}

/// Публичная часть личности. Передаётся свободно, секретов не содержит.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicIdentity {
    verifying: VerifyingKey,
    agreement: X25519Public,
}

impl PublicIdentity {
    pub fn verifying_key(&self) -> &VerifyingKey {
        &self.verifying
    }

    pub fn agreement_key(&self) -> &X25519Public {
        &self.agreement
    }

    pub fn to_bytes(&self) -> [u8; PUBLIC_IDENTITY_BYTES] {
        let mut out = [0u8; PUBLIC_IDENTITY_BYTES];
        out[..32].copy_from_slice(self.verifying.as_bytes());
        out[32..].copy_from_slice(self.agreement.as_bytes());
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, IdentityError> {
        if bytes.len() != PUBLIC_IDENTITY_BYTES {
            return Err(IdentityError::Length {
                expected: PUBLIC_IDENTITY_BYTES,
                got: bytes.len(),
            });
        }
        let (v, a) = bytes.split_at(32);
        let varr: [u8; 32] = v.try_into().map_err(|_| IdentityError::Length {
            expected: 32,
            got: v.len(),
        })?;
        let aarr: [u8; 32] = a.try_into().map_err(|_| IdentityError::Length {
            expected: 32,
            got: a.len(),
        })?;
        let verifying =
            VerifyingKey::from_bytes(&varr).map_err(|_| IdentityError::MalformedVerifyingKey)?;
        Ok(Self {
            verifying,
            agreement: X25519Public::from(aarr),
        })
    }

    /// Отпечаток одной личности: 30 цифр шестью группами.
    ///
    /// Показывается в профиле. Для сверки с собеседником используйте
    /// [`PublicIdentity::safety_number`] — она защищает от подмены обеих сторон сразу.
    pub fn fingerprint(&self) -> String {
        let mut h = Sha256::new();
        h.update(FINGERPRINT_DOMAIN);
        h.update(self.to_bytes());
        digits_from_hash(&h.finalize())
    }

    /// Число сверки для пары собеседников.
    ///
    /// Симметрично: обе стороны получают одну и ту же строку независимо от того, кто
    /// кого добавил. Это то, что сравнивают голосом или через QR перед началом переписки.
    /// Без этой сверки стойкий шифр полностью побеждается активным посредником —
    /// см. демонстрацию в `radio-mesh-demo/s07_ratchet.py`, раздел F.
    pub fn safety_number(&self, other: &PublicIdentity) -> String {
        let a = self.to_bytes();
        let b = other.to_bytes();
        // Упорядочиваем, чтобы результат не зависел от того, кто считает.
        let (first, second) = if a <= b { (&a, &b) } else { (&b, &a) };
        let mut h = Sha256::new();
        h.update(SAFETY_NUMBER_DOMAIN);
        h.update(first);
        h.update(second);
        digits_from_hash(&h.finalize())
    }

    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), IdentityError> {
        self.verifying
            .verify(message, signature)
            .map_err(|_| IdentityError::BadSignature)
    }
}

/// Секретная личность. Не сериализуется наружу и не пересекает границу FFI.
pub struct Identity {
    signing: SigningKey,
    agreement: StaticSecret,
}

impl Identity {
    /// Новая личность из системного источника случайности.
    pub fn generate() -> Self {
        Self {
            signing: SigningKey::generate(&mut OsRng),
            agreement: StaticSecret::random_from_rng(OsRng),
        }
    }

    pub fn public(&self) -> PublicIdentity {
        PublicIdentity {
            verifying: self.signing.verifying_key(),
            agreement: X25519Public::from(&self.agreement),
        }
    }

    pub fn sign(&self, message: &[u8]) -> Signature {
        self.signing.sign(message)
    }

    /// Общий секрет по Диффи — Хеллману. Результат — сырьё для HKDF, а не ключ:
    /// использовать напрямую нельзя.
    pub fn diffie_hellman(&self, peer: &X25519Public) -> x25519_dalek::SharedSecret {
        self.agreement.diffie_hellman(peer)
    }
}

impl std::fmt::Debug for Identity {
    /// Намеренно не печатает секретные части: строки логов переживают процесс.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Identity")
            .field("public", &self.public().fingerprint())
            .finish_non_exhaustive()
    }
}

/// Превращает хеш в читаемые вслух цифры: шесть групп по пять.
fn digits_from_hash(hash: &[u8]) -> String {
    hash.chunks_exact(5)
        .take(FINGERPRINT_GROUPS)
        .map(|chunk| {
            let v = chunk.iter().fold(0u64, |acc, b| (acc << 8) | u64::from(*b));
            format!("{:0width$}", v % 100_000, width = FINGERPRINT_DIGITS_PER_GROUP)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_identity_roundtrip() {
        let id = Identity::generate();
        let pub_a = id.public();
        let bytes = pub_a.to_bytes();
        let pub_b = PublicIdentity::from_bytes(&bytes).expect("свои же байты должны разбираться");
        assert_eq!(pub_a, pub_b);
    }

    #[test]
    fn from_bytes_rejects_wrong_length() {
        assert!(PublicIdentity::from_bytes(&[0u8; 63]).is_err());
        assert!(PublicIdentity::from_bytes(&[0u8; 65]).is_err());
    }

    #[test]
    fn signature_verifies_and_tampering_is_caught() {
        let id = Identity::generate();
        let pubkey = id.public();
        let msg = b"road to myworld";
        let sig = id.sign(msg);
        assert!(pubkey.verify(msg, &sig).is_ok());
        assert!(pubkey.verify(b"road to myworld!", &sig).is_err());
    }

    #[test]
    fn diffie_hellman_agrees_both_ways() {
        let a = Identity::generate();
        let b = Identity::generate();
        let ab = a.diffie_hellman(b.public().agreement_key());
        let ba = b.diffie_hellman(a.public().agreement_key());
        assert_eq!(ab.as_bytes(), ba.as_bytes());
    }

    #[test]
    fn fingerprint_shape_is_stable() {
        let fp = Identity::generate().public().fingerprint();
        let groups: Vec<&str> = fp.split(' ').collect();
        assert_eq!(groups.len(), FINGERPRINT_GROUPS);
        for g in groups {
            assert_eq!(g.len(), FINGERPRINT_DIGITS_PER_GROUP);
            assert!(g.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn safety_number_is_symmetric() {
        let a = Identity::generate().public();
        let b = Identity::generate().public();
        assert_eq!(a.safety_number(&b), b.safety_number(&a));
    }

    #[test]
    fn safety_number_changes_if_a_key_is_swapped() {
        // Это и есть обнаружение посредника: подменённый ключ даёт другое число сверки.
        let a = Identity::generate().public();
        let b = Identity::generate().public();
        let impostor = Identity::generate().public();
        assert_ne!(a.safety_number(&b), a.safety_number(&impostor));
    }

    #[test]
    fn fingerprint_differs_from_safety_number() {
        // Разделение областей хеширования должно давать разные значения.
        let a = Identity::generate().public();
        assert_ne!(a.fingerprint(), a.safety_number(&a));
    }
}

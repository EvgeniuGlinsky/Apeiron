//! Пакет пред-ключей: то, чем начинается переписка.
//!
//! Чтобы написать первым тому, кого сейчас нет в сети, нужен его ключ. Пакет
//! пред-ключей — это набор публичных ключей устройства, который лежит там, где
//! его можно взять: у слепого ретранслятора, в QR-коде, на бумаге.
//!
//! # Что именно здесь защищается
//!
//! Двойной храповик Olm даёт стойкость шифрования, но **ничего не говорит о
//! том, чей ключ вы взяли**. Подмена пакета посередине полностью побеждает
//! стойкую криптографию: обе стороны видят «шифрование работает», а читает
//! третий (см. `s07_ratchet.py`, раздел F, и тест `mitm_*` ниже).
//!
//! Поэтому пакет **подписан долговременной личностью** ([`Identity`]), и
//! проверка подписи не может быть забыта по невнимательности: разобранный из
//! байтов пакет имеет тип [`UnverifiedPrekeyBundle`], а построить сессию можно
//! только из [`PrekeyBundle`], который иначе как через [`UnverifiedPrekeyBundle::verify`]
//! не получить. Забыть проверку нельзя — код без неё не скомпилируется.
//!
//! Подпись при этом **не отвечает на вопрос, чья это личность**. На него
//! отвечает только сверка отпечатка голосом или лично — см.
//! [`PublicIdentity::safety_number`].

use ed25519_dalek::{Signature, SignatureError};
use vodozemac::{olm::Account, Curve25519PublicKey, Ed25519PublicKey, KeyError};

use crate::identity::{Identity, IdentityError, PublicIdentity, PUBLIC_IDENTITY_BYTES};

/// Разделитель области подписи пакета.
///
/// Меняя его, вы делаете старые пакеты непроверяемыми — это осознанно ломающее
/// изменение. Разделитель нужен, чтобы подпись пакета нельзя было предъявить
/// как подпись чего-то другого: одна и та же личность подписывает и пакеты,
/// и записи журнала, и области у них обязаны не пересекаться.
const PREKEY_DOMAIN: &[u8] = b"apeiron/prekey-bundle/v1";

/// Длина ключа Curve25519 и Ed25519 в байтах.
const KEY_BYTES: usize = 32;
/// Длина подписи Ed25519 в байтах.
const SIGNATURE_BYTES: usize = 64;

/// Длина сериализованного пакета.
pub const PREKEY_BUNDLE_BYTES: usize = PUBLIC_IDENTITY_BYTES + KEY_BYTES * 3 + SIGNATURE_BYTES;

/// Что может пойти не так с пакетом пред-ключей.
#[derive(Debug, thiserror::Error)]
pub enum PrekeyError {
    #[error("пакет пред-ключей: ожидалось {expected} байт, получено {got}")]
    Length { expected: usize, got: usize },

    #[error("пакет пред-ключей: личность не разбирается: {0}")]
    Identity(#[from] IdentityError),

    #[error("пакет пред-ключей: ключ не разбирается: {0}")]
    Key(#[from] KeyError),

    #[error("пакет пред-ключей: подпись не разбирается")]
    MalformedSignature,

    #[error(
        "ПОДПИСЬ ПАКЕТА НЕВЕРНА. Ключи в нём не принадлежат заявленной личности — \
         либо пакет подменён по дороге, либо повреждён. Переписку начинать нельзя."
    )]
    BadSignature,

    #[error("у устройства не осталось неизрасходованных одноразовых ключей")]
    NoOneTimeKeys,
}

/// Пакет пред-ключей, **подпись которого ещё не проверена**.
///
/// Единственное, что с ним можно сделать полезного, — вызвать
/// [`UnverifiedPrekeyBundle::verify`]. Это не вежливое пожелание, а устройство
/// типов: [`PrekeyBundle`] нельзя собрать иначе.
#[derive(Debug, Clone)]
pub struct UnverifiedPrekeyBundle {
    inner: PrekeyBundle,
}

/// Проверенный пакет пред-ключей.
///
/// «Проверенный» здесь значит ровно одно: ключи в нём подписаны той личностью,
/// которая в нём же и указана. Что эта личность принадлежит тому человеку,
/// которого вы имеете в виду, проверяется **только** сверкой отпечатка.
#[derive(Debug, Clone)]
pub struct PrekeyBundle {
    identity: PublicIdentity,
    device_curve: Curve25519PublicKey,
    device_ed: Ed25519PublicKey,
    one_time: Curve25519PublicKey,
    signature: Signature,
}

impl PrekeyBundle {
    /// Собирает пакет для своего устройства, забирая один одноразовый ключ.
    ///
    /// Одноразовые ключи для того и одноразовые: каждый выданный пакет должен
    /// нести свой. Повторное использование ключа ослабляет начальное
    /// согласование до отсутствия прямой секретности на первом сообщении.
    pub fn create(identity: &Identity, account: &mut Account) -> Result<Self, PrekeyError> {
        // Спрашивать надо именно про **невыданные** ключи: `one_time_keys()`
        // отдаёт только их, а `stored_one_time_key_count()` считает и уже
        // выданные. Перепутать легко, и тогда второй пакет собрать не выйдет
        // вовсе — ровно на этом и споткнулись в первой версии.
        if account.one_time_keys().is_empty() {
            account.generate_one_time_keys(1);
        }

        let one_time = *account
            .one_time_keys()
            .values()
            .next()
            .ok_or(PrekeyError::NoOneTimeKeys)?;

        // Ключ считается выданным сразу: иначе при следующем вызове мы отдадим
        // тот же самый, а одноразовый ключ на то и одноразовый.
        account.mark_keys_as_published();

        let device_curve = account.curve25519_key();
        let device_ed = account.ed25519_key();
        let public = identity.public();

        let signature = identity.sign(&signed_payload(
            &public,
            &device_curve,
            &device_ed,
            &one_time,
        ));

        Ok(Self {
            identity: public,
            device_curve,
            device_ed,
            one_time,
            signature,
        })
    }

    /// Долговременная личность, которой подписан пакет.
    pub fn identity(&self) -> &PublicIdentity {
        &self.identity
    }

    /// Ключ устройства для согласования (Curve25519).
    pub fn device_curve_key(&self) -> Curve25519PublicKey {
        self.device_curve
    }

    /// Подписной ключ устройства (Ed25519).
    pub fn device_ed_key(&self) -> Ed25519PublicKey {
        self.device_ed
    }

    /// Одноразовый ключ.
    pub fn one_time_key(&self) -> Curve25519PublicKey {
        self.one_time
    }

    /// Сериализация в канонический вид — он же то, что подписывается.
    pub fn to_bytes(&self) -> [u8; PREKEY_BUNDLE_BYTES] {
        let mut out = [0u8; PREKEY_BUNDLE_BYTES];
        let mut at = 0;
        let mut put = |src: &[u8]| {
            let end = at + src.len();
            if let Some(slot) = out.get_mut(at..end) {
                slot.copy_from_slice(src);
            }
            at = end;
        };
        put(&self.identity.to_bytes());
        put(&self.device_curve.to_bytes());
        put(self.device_ed.as_bytes());
        put(&self.one_time.to_bytes());
        put(&self.signature.to_bytes());
        out
    }

    /// Разбор пакета. Подпись **не проверяется** — для этого есть
    /// [`UnverifiedPrekeyBundle::verify`], и обойти её нельзя.
    pub fn parse(bytes: &[u8]) -> Result<UnverifiedPrekeyBundle, PrekeyError> {
        if bytes.len() != PREKEY_BUNDLE_BYTES {
            return Err(PrekeyError::Length {
                expected: PREKEY_BUNDLE_BYTES,
                got: bytes.len(),
            });
        }

        let mut at = 0;
        let mut take = |n: usize| -> &[u8] {
            let slice = bytes.get(at..at + n).unwrap_or(&[]);
            at += n;
            slice
        };

        let identity = PublicIdentity::from_bytes(take(PUBLIC_IDENTITY_BYTES))?;
        let device_curve = Curve25519PublicKey::from_slice(take(KEY_BYTES))?;
        let device_ed = ed25519_from_slice(take(KEY_BYTES))?;
        let one_time = Curve25519PublicKey::from_slice(take(KEY_BYTES))?;
        let signature = Signature::from_slice(take(SIGNATURE_BYTES))
            .map_err(|_: SignatureError| PrekeyError::MalformedSignature)?;

        Ok(UnverifiedPrekeyBundle {
            inner: Self {
                identity,
                device_curve,
                device_ed,
                one_time,
                signature,
            },
        })
    }
}

impl UnverifiedPrekeyBundle {
    /// Личность, **которая заявлена** в пакете. Смотреть на неё до проверки
    /// подписи можно только чтобы решить, стоит ли вообще возиться.
    pub fn claimed_identity(&self) -> &PublicIdentity {
        &self.inner.identity
    }

    /// Проверяет подпись и отдаёт пакет, пригодный к работе.
    pub fn verify(self) -> Result<PrekeyBundle, PrekeyError> {
        let payload = signed_payload(
            &self.inner.identity,
            &self.inner.device_curve,
            &self.inner.device_ed,
            &self.inner.one_time,
        );
        self.inner
            .identity
            .verify(&payload, &self.inner.signature)
            .map_err(|_| PrekeyError::BadSignature)?;
        Ok(self.inner)
    }
}

/// То, что подписывается: разделитель области и все ключи пакета подряд.
///
/// Включать в подпись саму личность обязательно: иначе подпись, снятую с одного
/// пакета, можно было бы предъявить в пакете с другой личностью.
fn signed_payload(
    identity: &PublicIdentity,
    device_curve: &Curve25519PublicKey,
    device_ed: &Ed25519PublicKey,
    one_time: &Curve25519PublicKey,
) -> Vec<u8> {
    let mut payload =
        Vec::with_capacity(PREKEY_DOMAIN.len() + PUBLIC_IDENTITY_BYTES + KEY_BYTES * 3);
    payload.extend_from_slice(PREKEY_DOMAIN);
    payload.extend_from_slice(&identity.to_bytes());
    payload.extend_from_slice(&device_curve.to_bytes());
    payload.extend_from_slice(device_ed.as_bytes());
    payload.extend_from_slice(&one_time.to_bytes());
    payload
}

fn ed25519_from_slice(bytes: &[u8]) -> Result<Ed25519PublicKey, PrekeyError> {
    let array: [u8; KEY_BYTES] = bytes.try_into().map_err(|_| PrekeyError::Length {
        expected: KEY_BYTES,
        got: bytes.len(),
    })?;
    Ok(Ed25519PublicKey::from_slice(&array)?)
}

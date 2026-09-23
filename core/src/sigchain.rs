//! Журнал личности: подписанная цепочка записей, доступная только на дозапись.
//!
//! Это и есть «блокчейн» проекта — и намеренно самый скучный из возможных.
//! Ни сети согласия, ни майнинга, ни монеты: одна личность ведёт свой
//! собственный журнал, каждая запись ссылается на хеш предыдущей и подписана.
//! Больше ничего для задачи не нужно, а всё лишнее — это лишний код рядом
//! с ключами.
//!
//! # Что журнал отвечает
//!
//! На вопрос «какие устройства сейчас принадлежат этой личности». Появление
//! нового телефона, отзыв потерянного — события, которые собеседник обязан
//! увидеть и проверить сам, не спрашивая ничей сервер.
//!
//! # Что журнал НЕ отвечает
//!
//! На вопрос «чья это личность». Цепочка внутренне непротиворечива у кого
//! угодно: любой может завести свою и подписать в ней что хочет. Связь
//! с человеком даёт **только** сверка отпечатка голосом или лично.
//!
//! # Почему запись ссылается на хеш предыдущей
//!
//! Чтобы нельзя было **изъять** запись. Подпись защищает каждую запись по
//! отдельности, но набор подписанных записей можно предъявить не полностью —
//! например, утаить отзыв устройства. Ссылка на хеш делает такую выборку
//! видимой: цепочка просто не сойдётся.

use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use crate::identity::{Identity, IdentityError, PublicIdentity, PUBLIC_IDENTITY_BYTES};

/// Разделитель области для подписи записи.
const SIGN_DOMAIN: &[u8] = b"apeiron/sigchain/entry/v1";
/// Разделитель области для хеша записи.
const HASH_DOMAIN: &[u8] = b"apeiron/sigchain/link/v1";

const KEY_BYTES: usize = 32;
const HASH_BYTES: usize = 32;
const SIGNATURE_BYTES: usize = 64;

/// Метки видов записей в сериализованном виде.
const TAG_GENESIS: u8 = 1;
const TAG_ADD_DEVICE: u8 = 2;
const TAG_REVOKE_DEVICE: u8 = 3;

/// Что может пойти не так с журналом.
#[derive(Debug, thiserror::Error)]
pub enum SigchainError {
    #[error("журнал пуст")]
    Empty,

    #[error("запись {0}: первой обязана быть запись о рождении личности")]
    MissingGenesis(u64),

    #[error("запись {0}: рождение личности может быть только первой записью")]
    RepeatedGenesis(u64),

    #[error("запись {index}: номер {got}, ожидался {expected} — журнал переставлен или неполон")]
    OutOfOrder {
        index: usize,
        got: u64,
        expected: u64,
    },

    #[error(
        "запись {0}: ссылка на предыдущую не сходится. Из журнала что-то изъято \
         или подменено — например, отзыв устройства."
    )]
    BrokenLink(u64),

    #[error("запись {0}: подписана ключом, который этой личности не принадлежит")]
    UnknownSigner(u64),

    #[error("запись {0}: подписана отозванным устройством")]
    RevokedSigner(u64),

    #[error("ЗАПИСЬ {0}: ПОДПИСЬ НЕВЕРНА. Журналу доверять нельзя.")]
    BadSignature(u64),

    #[error("запись {0}: устройство уже есть в журнале")]
    DuplicateDevice(u64),

    #[error("запись {0}: отзыв устройства, которого в журнале нет")]
    UnknownDevice(u64),

    #[error("запись {0}: устройство уже отозвано")]
    AlreadyRevoked(u64),

    #[error("журнал оборван на записи {0}")]
    Truncated(usize),

    #[error("запись {0}: неизвестный вид записи {1}")]
    UnknownTag(usize, u8),

    #[error(transparent)]
    Identity(#[from] IdentityError),

    #[error("ключ в записи не разбирается")]
    MalformedKey,
}

/// Кто имеет право подписать запись журнала.
///
/// Подписывать может как корневая личность, так и уже добавленное активное
/// устройство: иначе добавить второй телефон можно было бы только с первого,
/// а потеряв его — уже никак.
pub trait ChainSigner {
    /// Публичный ключ подписи, 32 байта.
    fn signer_key(&self) -> [u8; KEY_BYTES];
    /// Подпись, 64 байта.
    fn sign_entry(&self, payload: &[u8]) -> [u8; SIGNATURE_BYTES];
}

impl ChainSigner for Identity {
    fn signer_key(&self) -> [u8; KEY_BYTES] {
        self.public().verifying_key().to_bytes()
    }

    fn sign_entry(&self, payload: &[u8]) -> [u8; SIGNATURE_BYTES] {
        self.sign(payload).to_bytes()
    }
}

impl ChainSigner for vodozemac::olm::Account {
    fn signer_key(&self) -> [u8; KEY_BYTES] {
        *self.ed25519_key().as_bytes()
    }

    fn sign_entry(&self, payload: &[u8]) -> [u8; SIGNATURE_BYTES] {
        self.sign(payload).to_bytes()
    }
}

/// Содержание записи.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryBody {
    /// Рождение личности. Всегда первая и единственная в своём роде.
    Genesis { root: PublicIdentity },
    /// Устройство добавлено.
    AddDevice {
        ed: [u8; KEY_BYTES],
        curve: [u8; KEY_BYTES],
    },
    /// Устройство отозвано. Необратимо: вернуть его можно только новым ключом.
    RevokeDevice { ed: [u8; KEY_BYTES] },
}

impl EntryBody {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Genesis { root } => {
                out.push(TAG_GENESIS);
                out.extend_from_slice(&root.to_bytes());
            }
            Self::AddDevice { ed, curve } => {
                out.push(TAG_ADD_DEVICE);
                out.extend_from_slice(ed);
                out.extend_from_slice(curve);
            }
            Self::RevokeDevice { ed } => {
                out.push(TAG_REVOKE_DEVICE);
                out.extend_from_slice(ed);
            }
        }
    }
}

/// Одна запись журнала.
#[derive(Debug, Clone)]
pub struct Entry {
    seq: u64,
    prev: [u8; HASH_BYTES],
    signer: [u8; KEY_BYTES],
    body: EntryBody,
    signature: [u8; SIGNATURE_BYTES],
}

impl Entry {
    /// Байты, которые подписываются.
    fn signed_bytes(
        seq: u64,
        prev: &[u8; HASH_BYTES],
        signer: &[u8; KEY_BYTES],
        body: &EntryBody,
    ) -> Vec<u8> {
        let mut out = Vec::with_capacity(SIGN_DOMAIN.len() + 8 + HASH_BYTES + KEY_BYTES + 1 + 64);
        out.extend_from_slice(SIGN_DOMAIN);
        out.extend_from_slice(&seq.to_be_bytes());
        out.extend_from_slice(prev);
        out.extend_from_slice(signer);
        body.encode(&mut out);
        out
    }

    /// Полное содержимое записи, включая подпись, — то, что уходит в поток.
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.seq.to_be_bytes());
        out.extend_from_slice(&self.prev);
        out.extend_from_slice(&self.signer);
        self.body.encode(out);
        out.extend_from_slice(&self.signature);
    }

    /// Хеш записи, на который сошлётся следующая.
    ///
    /// Подпись входит в хеш намеренно: тогда подмена подписи тоже рвёт цепочку,
    /// а не остаётся локальной поломкой одной записи.
    fn hash(&self) -> [u8; HASH_BYTES] {
        let mut encoded = Vec::new();
        self.encode(&mut encoded);

        let mut hasher = Sha256::new();
        hasher.update(HASH_DOMAIN);
        hasher.update(&encoded);
        hasher.finalize().into()
    }
}

/// Состояние личности, восстановленное из проверенного журнала.
#[derive(Debug, Clone)]
pub struct ChainState {
    root: PublicIdentity,
    devices: BTreeMap<[u8; KEY_BYTES], Device>,
}

/// Устройство в журнале.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub curve: [u8; KEY_BYTES],
    pub revoked: bool,
}

impl ChainState {
    /// Корневая личность — та, чей отпечаток сверяют голосом.
    pub fn root(&self) -> &PublicIdentity {
        &self.root
    }

    /// Действующие устройства.
    pub fn active_devices(&self) -> impl Iterator<Item = (&[u8; KEY_BYTES], &Device)> {
        self.devices.iter().filter(|(_, d)| !d.revoked)
    }

    /// Признаётся ли устройство действующим.
    pub fn is_active(&self, ed: &[u8; KEY_BYTES]) -> bool {
        self.devices.get(ed).is_some_and(|d| !d.revoked)
    }

    /// Известно ли устройство вообще — в том числе отозванное.
    pub fn is_known(&self, ed: &[u8; KEY_BYTES]) -> bool {
        self.devices.contains_key(ed)
    }
}

/// Журнал личности.
///
/// Прочитать из него состояние можно **только** через [`Sigchain::verify`] —
/// другого способа нет намеренно: список устройств, взятый из непроверенного
/// журнала, хуже отсутствия списка.
#[derive(Debug, Clone)]
pub struct Sigchain {
    entries: Vec<Entry>,
}

impl Sigchain {
    /// Заводит журнал: первая запись — рождение личности, подписанное ею самой.
    pub fn create(root: &Identity) -> Result<Self, SigchainError> {
        let body = EntryBody::Genesis {
            root: root.public(),
        };
        let mut chain = Self {
            entries: Vec::new(),
        };
        chain.push(root, body)?;
        Ok(chain)
    }

    /// Дописывает запись. Журнал после дозаписи обязан остаться проверяемым —
    /// иначе запись не добавляется вовсе.
    pub fn append(
        &mut self,
        signer: &impl ChainSigner,
        body: EntryBody,
    ) -> Result<(), SigchainError> {
        self.push(signer, body)
    }

    fn push(&mut self, signer: &impl ChainSigner, body: EntryBody) -> Result<(), SigchainError> {
        let seq = self.entries.len() as u64;
        let prev = match self.entries.last() {
            Some(last) => last.hash(),
            None => [0u8; HASH_BYTES],
        };
        let signer_key = signer.signer_key();
        let payload = Entry::signed_bytes(seq, &prev, &signer_key, &body);
        let signature = signer.sign_entry(&payload);

        self.entries.push(Entry {
            seq,
            prev,
            signer: signer_key,
            body,
            signature,
        });

        // Проверяем целиком: дешевле, чем повторять правила в двух местах,
        // и не даёт создать журнал, который сам же не примешь.
        match self.verify() {
            Ok(_) => Ok(()),
            Err(e) => {
                self.entries.pop();
                Err(e)
            }
        }
    }

    /// Проверяет журнал целиком и восстанавливает состояние.
    pub fn verify(&self) -> Result<ChainState, SigchainError> {
        let first = self.entries.first().ok_or(SigchainError::Empty)?;

        let EntryBody::Genesis { root } = &first.body else {
            return Err(SigchainError::MissingGenesis(first.seq));
        };
        let mut state = ChainState {
            root: root.clone(),
            devices: BTreeMap::new(),
        };
        let root_key = state.root.verifying_key().to_bytes();

        let mut expected_prev = [0u8; HASH_BYTES];

        for (index, entry) in self.entries.iter().enumerate() {
            let expected_seq = index as u64;
            if entry.seq != expected_seq {
                return Err(SigchainError::OutOfOrder {
                    index,
                    got: entry.seq,
                    expected: expected_seq,
                });
            }
            if entry.prev != expected_prev {
                return Err(SigchainError::BrokenLink(entry.seq));
            }

            // Кто имел право подписывать на этот момент.
            let signer_allowed = entry.signer == root_key
                || match state.devices.get(&entry.signer) {
                    Some(device) if device.revoked => {
                        return Err(SigchainError::RevokedSigner(entry.seq))
                    }
                    Some(_) => true,
                    None => false,
                };
            if !signer_allowed {
                return Err(SigchainError::UnknownSigner(entry.seq));
            }

            verify_signature(entry)?;

            match &entry.body {
                EntryBody::Genesis { .. } => {
                    if index != 0 {
                        return Err(SigchainError::RepeatedGenesis(entry.seq));
                    }
                    if entry.signer != root_key {
                        return Err(SigchainError::UnknownSigner(entry.seq));
                    }
                }
                EntryBody::AddDevice { ed, curve } => {
                    // Повторное добавление запрещено и для отозванных: иначе
                    // отзыв обратим, а он обязан быть окончательным.
                    if state.devices.contains_key(ed) {
                        return Err(SigchainError::DuplicateDevice(entry.seq));
                    }
                    state.devices.insert(
                        *ed,
                        Device {
                            curve: *curve,
                            revoked: false,
                        },
                    );
                }
                EntryBody::RevokeDevice { ed } => match state.devices.get_mut(ed) {
                    None => return Err(SigchainError::UnknownDevice(entry.seq)),
                    Some(device) if device.revoked => {
                        return Err(SigchainError::AlreadyRevoked(entry.seq))
                    }
                    Some(device) => device.revoked = true,
                },
            }

            expected_prev = entry.hash();
        }

        Ok(state)
    }

    /// Сериализация журнала целиком.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for entry in &self.entries {
            entry.encode(&mut out);
        }
        out
    }

    /// Разбор журнала. Проверку выполняет [`Sigchain::verify`] — разбор ничего
    /// не подтверждает.
    pub fn parse(bytes: &[u8]) -> Result<Self, SigchainError> {
        let mut entries = Vec::new();
        let mut at = 0usize;

        while at < bytes.len() {
            let index = entries.len();
            let seq_end = at + 8;
            let prev_end = seq_end + HASH_BYTES;
            let signer_end = prev_end + KEY_BYTES;
            let tag_end = signer_end + 1;

            let seq_bytes: [u8; 8] = bytes
                .get(at..seq_end)
                .and_then(|s| s.try_into().ok())
                .ok_or(SigchainError::Truncated(index))?;
            let prev: [u8; HASH_BYTES] = bytes
                .get(seq_end..prev_end)
                .and_then(|s| s.try_into().ok())
                .ok_or(SigchainError::Truncated(index))?;
            let signer: [u8; KEY_BYTES] = bytes
                .get(prev_end..signer_end)
                .and_then(|s| s.try_into().ok())
                .ok_or(SigchainError::Truncated(index))?;
            let tag = *bytes
                .get(signer_end..tag_end)
                .and_then(|s| s.first())
                .ok_or(SigchainError::Truncated(index))?;

            let (body, body_end) = match tag {
                TAG_GENESIS => {
                    let end = tag_end + PUBLIC_IDENTITY_BYTES;
                    let slice = bytes
                        .get(tag_end..end)
                        .ok_or(SigchainError::Truncated(index))?;
                    (
                        EntryBody::Genesis {
                            root: PublicIdentity::from_bytes(slice)?,
                        },
                        end,
                    )
                }
                TAG_ADD_DEVICE => {
                    let end = tag_end + KEY_BYTES * 2;
                    let ed: [u8; KEY_BYTES] = bytes
                        .get(tag_end..tag_end + KEY_BYTES)
                        .and_then(|s| s.try_into().ok())
                        .ok_or(SigchainError::Truncated(index))?;
                    let curve: [u8; KEY_BYTES] = bytes
                        .get(tag_end + KEY_BYTES..end)
                        .and_then(|s| s.try_into().ok())
                        .ok_or(SigchainError::Truncated(index))?;
                    (EntryBody::AddDevice { ed, curve }, end)
                }
                TAG_REVOKE_DEVICE => {
                    let end = tag_end + KEY_BYTES;
                    let ed: [u8; KEY_BYTES] = bytes
                        .get(tag_end..end)
                        .and_then(|s| s.try_into().ok())
                        .ok_or(SigchainError::Truncated(index))?;
                    (EntryBody::RevokeDevice { ed }, end)
                }
                other => return Err(SigchainError::UnknownTag(index, other)),
            };

            let signature_end = body_end + SIGNATURE_BYTES;
            let signature: [u8; SIGNATURE_BYTES] = bytes
                .get(body_end..signature_end)
                .and_then(|s| s.try_into().ok())
                .ok_or(SigchainError::Truncated(index))?;

            entries.push(Entry {
                seq: u64::from_be_bytes(seq_bytes),
                prev,
                signer,
                body,
                signature,
            });
            at = signature_end;
        }

        if entries.is_empty() {
            return Err(SigchainError::Empty);
        }
        Ok(Self { entries })
    }

    /// Число записей.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Проверка подписи записи.
///
/// Используется **строгая** проверка Ed25519. Обычная допускает подписи в
/// неканонической записи и точки малого порядка: на стойкость это почти не
/// влияет, но означает, что одна и та же запись может иметь несколько
/// различающихся подписей. В журнале, где запись хешируется вместе с подписью,
/// это превратилось бы в две разные «одинаковые» цепочки.
fn verify_signature(entry: &Entry) -> Result<(), SigchainError> {
    let key = VerifyingKey::from_bytes(&entry.signer).map_err(|_| SigchainError::MalformedKey)?;
    let signature = Signature::from_bytes(&entry.signature);
    let payload = Entry::signed_bytes(entry.seq, &entry.prev, &entry.signer, &entry.body);

    key.verify_strict(&payload, &signature)
        .map_err(|_| SigchainError::BadSignature(entry.seq))
}

//! Парная переписка: двойной храповик поверх проверенного пакета пред-ключей.
//!
//! Храповик не свой — [`vodozemac`], реализация Olm от Matrix.org, прошедшая
//! независимый аудит. Своих криптографических примитивов в проекте нет и не
//! будет (§18 исследования). Здесь — только то, что этой реализации не хватает
//! для нашей архитектуры.
//!
//! # Чего не хватает: приём пачкой
//!
//! Olm хранит не больше 40 ключей пропущенных сообщений на цепочку приёма
//! (`MAX_MESSAGE_KEYS`) и отказывается от разрыва больше 2000
//! (`MAX_MESSAGE_GAP`). Для Matrix этого достаточно: там сервер отдаёт
//! сообщения примерно в порядке отправки.
//!
//! У нас не так. Архитектура прямо предполагает доставку через слепой
//! ретранслятор с окном ожидания до суток: устройство было offline, потом
//! включилось и забрало всё разом — в том порядке, в каком очередь отдала.
//! Сто сообщений, пришедших задом наперёд, при наивной расшифровке означают
//! шестьдесят **навсегда потерянных**: расшифровав сотое, храповик прокрутится
//! вперёд и выбросит ключи, которые не поместились в сорок.
//!
//! Спасает то, что **порядок читается до расшифровки**: в заголовке каждого
//! сообщения Olm открыто лежат ключ храповика и номер в цепочке. Значит пачку
//! можно разложить по порядку и расшифровать по возрастанию номера — тогда
//! пропусков не возникает вовсе. Это делает [`Chat::decrypt_batch`], и
//! разница на двухстах сообщениях — между «всё прочитано» и «половина
//! потеряна» (тест `batch_survives_reverse_order`).

use vodozemac::olm::{
    Account, DecryptionError, EncryptionError, OlmMessage, PreKeyMessage, Session, SessionConfig,
    SessionCreationError,
};

use crate::identity::{PublicIdentity, PUBLIC_IDENTITY_BYTES};
use crate::prekey::PrekeyBundle;

/// Что может пойти не так в переписке.
#[derive(Debug, thiserror::Error)]
pub enum ChatError {
    #[error("не удалось создать сессию: {0}")]
    Creation(#[from] SessionCreationError),

    #[error("не удалось зашифровать: {0}")]
    Encryption(#[from] EncryptionError),

    #[error("не удалось расшифровать: {0}")]
    Decryption(#[from] DecryptionError),

    #[error("расшифрованное не является текстом UTF-8")]
    NotText,

    #[error("внутренняя ошибка: сообщение осталось необработанным")]
    NotProcessed,

    #[error("состояние переписки не удалось сохранить: {0}")]
    Pickle(String),

    #[error(
        "СОСТОЯНИЕ ПЕРЕПИСКИ ПОВРЕЖДЕНО ИЛИ ПОДМЕНЕНО: {0}.          Продолжать эту переписку нельзя."
    )]
    Unpickle(String),
}

impl ChatError {
    /// Потеряно ли сообщение безвозвратно.
    ///
    /// Различие важно для интерфейса: «повреждено, попробуйте ещё раз» и
    /// «прочитать уже нельзя никогда» — разные сообщения пользователю, и
    /// второе нельзя показывать как первое. Молчать нельзя тем более: потеря
    /// сообщения — это то, о чём человек обязан узнать.
    pub fn is_lost_forever(&self) -> bool {
        matches!(
            self,
            Self::Decryption(
                DecryptionError::MissingMessageKey(_) | DecryptionError::TooBigMessageGap(_, _)
            )
        )
    }
}

/// Одна парная переписка.
pub struct Chat {
    session: Session,
    peer: PublicIdentity,
}

/// Сохраняет состояние аккаунта устройства.
///
/// Без этого каждый запуск приложения порождал бы **новое устройство**: у
/// аккаунта Olm свои долговременные ключи и запас одноразовых, и потеря их
/// рвёт все существующие переписки разом.
///
/// Формат — `serde_json` поверх `AccountPickle`. Собственное шифрование
/// vodozemac (`AccountPickle::encrypt`, AES-CBC с HMAC поверх base64) не
/// используется намеренно: криптостек проекта держится одного поколения, и
/// второй формат шифрования рядом с ключами — это второй набор обязанностей по
/// сопровождению. Запечатывает эти байты `crate::aead`.
///
/// Канонический вид здесь не требуется: результат не подписывается, а
/// запечатывается, и от представления это не зависит. Там, где вид обязан быть
/// однозначным, — в пакете пред-ключей и в журнале личности — `serde` не
/// применяется вовсе.
pub fn pickle_account(account: &Account) -> Result<Vec<u8>, ChatError> {
    serde_json::to_vec(&account.pickle()).map_err(|e| ChatError::Pickle(e.to_string()))
}

/// Восстанавливает аккаунт устройства из того, что вернул [`pickle_account`].
pub fn unpickle_account(bytes: &[u8]) -> Result<Account, ChatError> {
    let pickle = serde_json::from_slice(bytes).map_err(|e| ChatError::Unpickle(e.to_string()))?;
    Ok(Account::from_pickle(pickle))
}

impl Chat {
    /// Версия протокола Olm.
    ///
    /// Явно первая. Вторая в vodozemac убрана за флаг экспериментальной фичи и
    /// не стандартизована; к ней же относилось замечание февраля 2026 года о
    /// понижении версии и усечённых MAC. Пока V2 не стандартизована, брать её
    /// незачем. Решение R-009 в `docs/threat-log.md`.
    fn config() -> SessionConfig {
        SessionConfig::version_1()
    }

    /// Начинает переписку по **проверенному** пакету пред-ключей.
    ///
    /// Непроверенный сюда не передать: тип не тот. См. [`crate::prekey`].
    pub fn initiate(account: &Account, bundle: &PrekeyBundle) -> Result<Self, ChatError> {
        let session = account.create_outbound_session(
            Self::config(),
            bundle.device_curve_key(),
            bundle.one_time_key(),
        )?;
        Ok(Self {
            session,
            peer: bundle.identity().clone(),
        })
    }

    /// Принимает первое сообщение от того, чей пакет уже проверен.
    ///
    /// Пакет отправителя нужен не для украшения: `vodozemac` сверит ключ
    /// устройства из пакета с тем, что заявлен в сообщении, и откажется
    /// создавать сессию при расхождении. Так первое сообщение оказывается
    /// привязано к личности, а не просто «от кого-то».
    pub fn accept(
        account: &mut Account,
        sender: &PrekeyBundle,
        message: &PreKeyMessage,
    ) -> Result<(Self, String), ChatError> {
        let result =
            account.create_inbound_session(Self::config(), sender.device_curve_key(), message)?;
        let text = String::from_utf8(result.plaintext).map_err(|_| ChatError::NotText)?;
        Ok((
            Self {
                session: result.session,
                peer: sender.identity().clone(),
            },
            text,
        ))
    }

    /// Личность собеседника — та, чей отпечаток показывается на экране сверки.
    /// Сохраняет состояние переписки.
    ///
    /// Раскладка: `публичная личность собеседника (64) ‖ serde_json(SessionPickle)`.
    ///
    /// Собеседник хранится рядом не для удобства: в `SessionPickle` его нет, а
    /// без него `Chat` не восстановить — и, что важнее, некому было бы
    /// предъявить число сверки. Переписка без известного собеседника это
    /// переписка неизвестно с кем.
    pub fn pickle(&self) -> Result<Vec<u8>, ChatError> {
        let mut out = Vec::with_capacity(PUBLIC_IDENTITY_BYTES + 512);
        out.extend_from_slice(&self.peer.to_bytes());
        let body = serde_json::to_vec(&self.session.pickle())
            .map_err(|e| ChatError::Pickle(e.to_string()))?;
        out.extend_from_slice(&body);
        Ok(out)
    }

    /// Восстанавливает переписку из того, что вернул [`Chat::pickle`].
    ///
    /// Байты обязаны приходить из проверенного источника: успешное
    /// распечатывание AEAD говорит «это писали мы», и только это. Подменить их
    /// снаружи нельзя, а повреждение внутри границы дальше границы не идёт —
    /// отсюда отдельная ошибка вместо тихого возврата пустого состояния.
    pub fn from_pickle(bytes: &[u8]) -> Result<Self, ChatError> {
        let head = bytes
            .get(..PUBLIC_IDENTITY_BYTES)
            .ok_or_else(|| ChatError::Unpickle("запись короче публичной личности".to_string()))?;
        let tail = bytes
            .get(PUBLIC_IDENTITY_BYTES..)
            .ok_or_else(|| ChatError::Unpickle("запись без состояния храповика".to_string()))?;
        let peer =
            PublicIdentity::from_bytes(head).map_err(|e| ChatError::Unpickle(e.to_string()))?;
        let pickle =
            serde_json::from_slice(tail).map_err(|e| ChatError::Unpickle(e.to_string()))?;
        Ok(Self {
            session: Session::from_pickle(pickle),
            peer,
        })
    }

    pub fn peer(&self) -> &PublicIdentity {
        &self.peer
    }

    /// Идентификатор сессии. Совпадает у обеих сторон.
    pub fn session_id(&self) -> String {
        self.session.session_id()
    }

    pub fn encrypt(&mut self, text: &str) -> Result<OlmMessage, ChatError> {
        Ok(self.session.encrypt(text)?)
    }

    pub fn decrypt(&mut self, message: &OlmMessage) -> Result<String, ChatError> {
        let bytes = self.session.decrypt(message)?;
        String::from_utf8(bytes).map_err(|_| ChatError::NotText)
    }

    /// Расшифровывает пачку, разложив её по порядку цепочки.
    ///
    /// Результаты возвращаются **в порядке входа**: i-й результат относится к
    /// i-му сообщению, как бы оно ни переставлялось внутри. Ошибка на одном
    /// сообщении не прекращает работу — остальные будут прочитаны.
    ///
    /// Именно этот метод следует звать на всём, что пришло из сети.
    /// [`Chat::decrypt`] годится только там, где сообщение заведомо одно.
    pub fn decrypt_batch(&mut self, messages: &[OlmMessage]) -> Vec<Result<String, ChatError>> {
        let mut slots: Vec<Option<Result<String, ChatError>>> =
            messages.iter().map(|_| None).collect();

        for index in batch_order(messages) {
            let Some(message) = messages.get(index) else {
                continue;
            };
            let outcome = self.decrypt(message);
            if let Some(slot) = slots.get_mut(index) {
                *slot = Some(outcome);
            }
        }

        slots
            .into_iter()
            .map(|slot| slot.unwrap_or(Err(ChatError::NotProcessed)))
            .collect()
    }
}

/// Порядок, в котором пачку следует расшифровывать.
///
/// Правила:
///
/// 1. **Внутри одной цепочки — по возрастанию номера.** Ради этого всё и
///    затевалось: иначе первый же расшифрованный «из будущего» выбросит ключи
///    всех, кто до него, сверх сорока.
/// 2. **Цепочки — в порядке первого появления.** Их взаимный порядок из самих
///    сообщений не выводится: ключ храповика непрозрачен, а «какая цепочка
///    новее» знает только тот, кто её создал. Порядок прихода — лучшее
///    доступное приближение, и он верен всегда, кроме случая, когда сама сеть
///    переставила границу разворота переписки. Тогда часть сообщений старой
///    цепочки может не прочитаться — и об этом честно сообщается ошибкой,
///    а не проглатывается.
///
/// Сообщения установления сессии **не выделяются в особый случай**, и это
/// существенно: в Olm инициатор шлёт их до тех пор, пока не получит ответ.
/// Односторонняя пачка в двести сообщений целиком состоит из них, и если
/// раскладывать по порядку только «обычные», не поменяется ничего. Номер
/// цепочки у них лежит во вложенном сообщении и читается так же открыто.
fn batch_order(messages: &[OlmMessage]) -> Vec<usize> {
    /// Номер сообщения в цепочке и его позиция во входной пачке.
    type Member = (u64, usize);
    /// Ключ храповика и все сообщения его цепочки.
    type Chain = ([u8; 32], Vec<Member>);

    let mut order: Vec<usize> = Vec::with_capacity(messages.len());
    let mut chains: Vec<Chain> = Vec::new();

    for (position, message) in messages.iter().enumerate() {
        let inner = match message {
            OlmMessage::PreKey(prekey) => prekey.message(),
            OlmMessage::Normal(normal) => normal,
        };
        let key = inner.ratchet_key().to_bytes();
        let index = inner.chain_index();
        match chains.iter_mut().find(|(known, _)| *known == key) {
            Some((_, members)) => members.push((index, position)),
            None => chains.push((key, vec![(index, position)])),
        }
    }

    for (_, mut members) in chains {
        members.sort_by_key(|(index, _)| *index);
        order.extend(members.into_iter().map(|(_, position)| position));
    }

    order
}

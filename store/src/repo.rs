//! Что именно хранится и как оно кладётся на место.
//!
//! Общее правило на все методы: наружу отдаются готовые типы ядра, а не байты,
//! и **проверка при чтении повторяется**. Успешное распечатывание AEAD говорит
//! «эти байты писали мы» — и только это. Оно не говорит, что байты верны.
//! Поэтому журнал личности после загрузки проходит `verify()` заново, а
//! состояние переписки — свой разбор: повреждение внутри доверенной границы
//! дальше границы не идёт.

use apeiron_core::{pickle_account, unpickle_account, Chat, Identity, PublicIdentity, Sigchain};
use rusqlite::OptionalExtension;
use zeroize::Zeroizing;

use crate::record::{open_record, seal_record, Table};
use crate::{Storage, StorageError};

/// Идентификатор строки в таблицах, где строка всегда одна.
const SINGLETON: i64 = 1;

/// Контакт: собеседник и то, что мы о нём записали.
pub struct Contact {
    /// Номер строки. Нужен, чтобы привязывать к нему сессии.
    pub id: i64,
    /// Публичная личность собеседника.
    pub peer: PublicIdentity,
    /// Имя, которое дал ему владелец. Не приходит снаружи и ничего не
    /// подтверждает — см. `docs/crypto.md`, раздел 6.
    pub name: String,
}

impl Storage {
    // ── Личность ────────────────────────────────────────────────────────────

    /// Сохраняет личность. Заменяет прежнюю, если она была.
    pub fn save_identity(&self, identity: &Identity) -> Result<(), StorageError> {
        let secret = identity.export_secret();
        let sealed = seal_record(
            self.keys().identity(),
            Table::Identity,
            SINGLETON,
            secret.as_bytes(),
        )?;
        self.conn().execute(
            "INSERT INTO identity (id, sealed) VALUES (?1, ?2)
             ON CONFLICT(id) DO UPDATE SET sealed = excluded.sealed",
            rusqlite::params![SINGLETON, sealed],
        )?;
        Ok(())
    }

    /// Читает личность, если она есть.
    pub fn load_identity(&self) -> Result<Option<Identity>, StorageError> {
        let Some(sealed) = self.sealed_singleton("identity")? else {
            return Ok(None);
        };
        let plain = Zeroizing::new(open_record(
            self.keys().identity(),
            Table::Identity,
            SINGLETON,
            &sealed,
        )?);
        Ok(Some(Identity::from_secret_bytes(&plain)?))
    }

    // ── Аккаунт устройства ──────────────────────────────────────────────────

    /// Сохраняет аккаунт Olm.
    ///
    /// Без этого каждый запуск порождал бы новое устройство и рвал все
    /// переписки разом: у аккаунта свои долговременные ключи и запас
    /// одноразовых.
    pub fn save_account(
        &self,
        account: &apeiron_core::vodozemac::olm::Account,
    ) -> Result<(), StorageError> {
        self.write_account(account)
    }

    fn write_account(
        &self,
        account: &apeiron_core::vodozemac::olm::Account,
    ) -> Result<(), StorageError> {
        let plain = Zeroizing::new(pickle_account(account)?);
        let sealed = seal_record(self.keys().account(), Table::Account, SINGLETON, &plain)?;
        self.conn().execute(
            "INSERT INTO device_account (id, sealed) VALUES (?1, ?2)
             ON CONFLICT(id) DO UPDATE SET sealed = excluded.sealed",
            rusqlite::params![SINGLETON, sealed],
        )?;
        Ok(())
    }

    /// Читает аккаунт Olm, если он есть.
    pub fn load_account(
        &self,
    ) -> Result<Option<apeiron_core::vodozemac::olm::Account>, StorageError> {
        let Some(sealed) = self.sealed_singleton("device_account")? else {
            return Ok(None);
        };
        let plain = Zeroizing::new(open_record(
            self.keys().account(),
            Table::Account,
            SINGLETON,
            &sealed,
        )?);
        Ok(Some(unpickle_account(&plain)?))
    }

    // ── Журнал личности ─────────────────────────────────────────────────────

    /// Сохраняет журнал личности.
    pub fn save_sigchain(&self, chain: &Sigchain) -> Result<(), StorageError> {
        let sealed = seal_record(
            self.keys().sigchain(),
            Table::Sigchain,
            SINGLETON,
            &chain.to_bytes(),
        )?;
        self.conn().execute(
            "INSERT INTO sigchain (id, sealed) VALUES (?1, ?2)
             ON CONFLICT(id) DO UPDATE SET sealed = excluded.sealed",
            rusqlite::params![SINGLETON, sealed],
        )?;
        Ok(())
    }

    /// Читает журнал личности и **проверяет его заново**.
    ///
    /// Проверка здесь не перестраховка. Список устройств из непроверенного
    /// журнала хуже отсутствия списка: по нему решают, чья подпись считается
    /// действующей. Ядро и не даёт достать состояние иначе как через
    /// `verify()`, и хранилище это правило не обходит.
    pub fn load_sigchain(&self) -> Result<Option<Sigchain>, StorageError> {
        let Some(sealed) = self.sealed_singleton("sigchain")? else {
            return Ok(None);
        };
        let plain = open_record(self.keys().sigchain(), Table::Sigchain, SINGLETON, &sealed)?;
        let chain = Sigchain::parse(&plain)?;
        chain.verify()?;
        Ok(Some(chain))
    }

    // ── Контакты ────────────────────────────────────────────────────────────

    /// Добавляет контакт или обновляет имя существующего.
    ///
    /// Ищется он по непрозрачной метке, а не по публичному ключу: открытый
    /// список собеседников читался бы из файла базы без всякого ключа.
    pub fn save_contact(&self, peer: &PublicIdentity, name: &str) -> Result<i64, StorageError> {
        let tag = self.keys().tag().tag(&peer.to_bytes());
        let tx = self.conn().unchecked_transaction()?;

        let existing: Option<i64> = tx
            .query_row("SELECT id FROM contacts WHERE tag = ?1", [&tag[..]], |r| {
                r.get(0)
            })
            .optional()?;

        let id = match existing {
            Some(id) => id,
            None => {
                // Идентификатор строки входит в проверку подлинности записи,
                // поэтому он нужен до запечатывания. Отсюда два шага в одной
                // транзакции: сначала место, потом содержимое.
                tx.execute(
                    "INSERT INTO contacts (tag, sealed) VALUES (?1, ?2)",
                    rusqlite::params![&tag[..], Vec::<u8>::new()],
                )?;
                tx.last_insert_rowid()
            }
        };

        let sealed = seal_record(
            self.keys().contact(),
            Table::Contact,
            id,
            &encode_contact(peer, name),
        )?;
        tx.execute(
            "UPDATE contacts SET sealed = ?1 WHERE id = ?2",
            rusqlite::params![sealed, id],
        )?;
        tx.commit()?;
        Ok(id)
    }

    /// Находит контакт по публичной личности.
    pub fn find_contact(&self, peer: &PublicIdentity) -> Result<Option<Contact>, StorageError> {
        let tag = self.keys().tag().tag(&peer.to_bytes());
        let row: Option<(i64, Vec<u8>)> = self
            .conn()
            .query_row(
                "SELECT id, sealed FROM contacts WHERE tag = ?1",
                [&tag[..]],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some((id, sealed)) => Ok(Some(self.decode_contact(id, &sealed)?)),
        }
    }

    /// Все контакты.
    pub fn contacts(&self) -> Result<Vec<Contact>, StorageError> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT id, sealed FROM contacts ORDER BY id")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))?;

        let mut out = Vec::new();
        for row in rows {
            let (id, sealed) = row?;
            out.push(self.decode_contact(id, &sealed)?);
        }
        Ok(out)
    }

    fn decode_contact(&self, id: i64, sealed: &[u8]) -> Result<Contact, StorageError> {
        let plain = open_record(self.keys().contact(), Table::Contact, id, sealed)?;
        let (peer, name) = decode_contact(&plain)?;
        Ok(Contact { id, peer, name })
    }

    // ── Переписки ───────────────────────────────────────────────────────────

    /// Сохраняет состояние переписки.
    ///
    /// Контакт должен быть заведён заранее: переписка без известного
    /// собеседника — это переписка неизвестно с кем, и предъявить число сверки
    /// было бы некому.
    pub fn save_chat(&self, contact_id: i64, chat: &Chat) -> Result<(), StorageError> {
        let tx = self.conn().unchecked_transaction()?;
        self.write_chat(contact_id, chat)?;
        tx.commit()?;
        Ok(())
    }

    /// Та же запись, но без собственной транзакции — чтобы её можно было
    /// объединить с другими в одну.
    fn write_chat(&self, contact_id: i64, chat: &Chat) -> Result<(), StorageError> {
        let conn = self.conn();
        let tag = self.keys().tag().tag(chat.session_id().as_bytes());

        let existing: Option<i64> = conn
            .query_row("SELECT id FROM sessions WHERE tag = ?1", [&tag[..]], |r| {
                r.get(0)
            })
            .optional()?;

        let id = match existing {
            Some(id) => id,
            None => {
                conn.execute(
                    "INSERT INTO sessions (contact_id, tag, sealed) VALUES (?1, ?2, ?3)",
                    rusqlite::params![contact_id, &tag[..], Vec::<u8>::new()],
                )?;
                conn.last_insert_rowid()
            }
        };

        let plain = Zeroizing::new(chat.pickle()?);
        let sealed = seal_record(self.keys().session(), Table::Session, id, &plain)?;
        conn.execute(
            "UPDATE sessions SET sealed = ?1, contact_id = ?2 WHERE id = ?3",
            rusqlite::params![sealed, contact_id, id],
        )?;
        Ok(())
    }

    /// Читает переписки с указанным контактом.
    pub fn load_chats(&self, contact_id: i64) -> Result<Vec<Chat>, StorageError> {
        let conn = self.conn();
        let mut stmt =
            conn.prepare("SELECT id, sealed FROM sessions WHERE contact_id = ?1 ORDER BY id")?;
        let rows = stmt.query_map([contact_id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (id, sealed) = row?;
            let plain = Zeroizing::new(open_record(
                self.keys().session(),
                Table::Session,
                id,
                &sealed,
            )?);
            out.push(Chat::from_pickle(&plain)?);
        }
        Ok(out)
    }

    /// Сохраняет состояние переписки и аккаунта **одной транзакцией**.
    ///
    /// Раздельная запись здесь недопустима. Расшифровка сдвигает храповик и
    /// расходует одноразовый ключ аккаунта; если на диск попадёт только одна из
    /// двух половин, состояния разойдутся — и часть сообщений станет
    /// непрочитываемой навсегда. Именно ради этого в хранилище SQLite, а не
    /// набор файлов.
    pub fn save_chat_and_account(
        &self,
        contact_id: i64,
        chat: &Chat,
        account: &apeiron_core::vodozemac::olm::Account,
    ) -> Result<(), StorageError> {
        let tx = self.conn().unchecked_transaction()?;
        self.write_chat(contact_id, chat)?;
        self.write_account(account)?;
        tx.commit()?;
        Ok(())
    }

    // ── Служебное ───────────────────────────────────────────────────────────

    /// Записывает служебное значение.
    pub fn meta_set(&self, key: &str, value: &[u8]) -> Result<(), StorageError> {
        let tag = self.keys().tag().tag(key.as_bytes());
        let tx = self.conn().unchecked_transaction()?;

        let existing: Option<i64> = tx
            .query_row("SELECT id FROM meta WHERE tag = ?1", [&tag[..]], |r| {
                r.get(0)
            })
            .optional()?;

        let id = match existing {
            Some(id) => id,
            None => {
                tx.execute(
                    "INSERT INTO meta (tag, sealed) VALUES (?1, ?2)",
                    rusqlite::params![&tag[..], Vec::<u8>::new()],
                )?;
                tx.last_insert_rowid()
            }
        };

        let sealed = seal_record(self.keys().meta(), Table::Meta, id, value)?;
        tx.execute(
            "UPDATE meta SET sealed = ?1 WHERE id = ?2",
            rusqlite::params![sealed, id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Читает служебное значение.
    pub fn meta_get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        let tag = self.keys().tag().tag(key.as_bytes());
        let row: Option<(i64, Vec<u8>)> = self
            .conn()
            .query_row(
                "SELECT id, sealed FROM meta WHERE tag = ?1",
                [&tag[..]],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some((id, sealed)) => Ok(Some(open_record(
                self.keys().meta(),
                Table::Meta,
                id,
                &sealed,
            )?)),
        }
    }

    /// Строка-одиночка из таблицы, где она всегда одна.
    fn sealed_singleton(&self, table: &str) -> Result<Option<Vec<u8>>, StorageError> {
        // Имя таблицы подставляется в запрос, и это единственное место, где так
        // делается. Снаружи оно прийти не может: все вызовы — с литералами из
        // этого же файла.
        let sql = format!("SELECT sealed FROM {table} WHERE id = ?1");
        let sealed: Option<Vec<u8>> = self
            .conn()
            .query_row(&sql, [SINGLETON], |r| r.get(0))
            .optional()?;
        Ok(sealed.filter(|b| !b.is_empty()))
    }
}

/// Раскладка записи о контакте: `публичная личность (64) ‖ имя в UTF-8`.
fn encode_contact(peer: &PublicIdentity, name: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(64 + name.len());
    out.extend_from_slice(&peer.to_bytes());
    out.extend_from_slice(name.as_bytes());
    out
}

fn decode_contact(plain: &[u8]) -> Result<(PublicIdentity, String), StorageError> {
    let head = plain.get(..64).ok_or_else(|| {
        StorageError::Wrapper("запись о контакте короче публичной личности".to_string())
    })?;
    let tail = plain.get(64..).unwrap_or_default();
    let peer = PublicIdentity::from_bytes(head)?;
    Ok((peer, String::from_utf8_lossy(tail).into_owned()))
}

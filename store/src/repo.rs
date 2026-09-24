//! What exactly is stored and how it is put in its place.
//!
//! A common rule for all methods: ready-made core types are handed out, not bytes,
//! and **the check is repeated on read**. A successful AEAD open says
//! "we wrote these bytes", and only that. It does not say the bytes are correct.
//! That is why the sigchain goes through `verify()` again after loading, and the
//! conversation state through its own parsing: corruption inside the trusted boundary
//! goes no further than the boundary.

use apeiron_core::{pickle_account, unpickle_account, Chat, Identity, PublicIdentity, Sigchain};
use rusqlite::OptionalExtension;
use zeroize::Zeroizing;

use crate::record::{open_record, seal_record, Table};
use crate::{Storage, StorageError};

/// Row identifier in tables where there is always exactly one row.
const SINGLETON: i64 = 1;

/// A contact: a peer and what we have recorded about them.
pub struct Contact {
    /// Row number. Needed to bind sessions to it.
    pub id: i64,
    /// The peer's public identity.
    pub peer: PublicIdentity,
    /// The name the owner gave them. Does not come from outside and confirms
    /// nothing; see `docs/crypto.md`, section 6.
    pub name: String,
}

impl Storage {
    // ── Identity ────────────────────────────────────────────────────────────

    /// Saves the identity. Replaces the previous one, if there was one.
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

    /// Reads the identity, if there is one.
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

    // ── Device account ──────────────────────────────────────────────────────

    /// Saves the Olm account.
    ///
    /// Without this every launch would spawn a new device and break all
    /// conversations at once: the account has its own long-term keys and a supply
    /// of one-time keys.
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

    /// Reads the Olm account, if there is one.
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

    // ── Sigchain ────────────────────────────────────────────────────────────

    /// Saves the sigchain.
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

    /// Reads the sigchain and **verifies it again**.
    ///
    /// The check here is not over-caution. A device list from an unverified
    /// log is worse than no list: it decides whose signature counts as
    /// valid. The core does not allow getting the state other than through
    /// `verify()`, and the storage does not bypass this rule.
    pub fn load_sigchain(&self) -> Result<Option<Sigchain>, StorageError> {
        let Some(sealed) = self.sealed_singleton("sigchain")? else {
            return Ok(None);
        };
        let plain = open_record(self.keys().sigchain(), Table::Sigchain, SINGLETON, &sealed)?;
        let chain = Sigchain::parse(&plain)?;
        chain.verify()?;
        Ok(Some(chain))
    }

    // ── Contacts ────────────────────────────────────────────────────────────

    /// Adds a contact or updates the name of an existing one.
    ///
    /// It is looked up by an opaque tag, not by the public key: a list of peers in the
    /// clear could be read from the database file without any key.
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
                // The row identifier is part of the record's authenticity check,
                // so it is needed before sealing. Hence two steps in one
                // transaction: first the location, then the content.
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

    /// Finds a contact by public identity.
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

    /// All contacts.
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

    // ── Conversations ───────────────────────────────────────────────────────

    /// Saves the conversation state.
    ///
    /// The contact must be created beforehand: a conversation without a known
    /// peer is a conversation with who knows whom, and there would be no one to
    /// present the safety number to.
    pub fn save_chat(&self, contact_id: i64, chat: &Chat) -> Result<(), StorageError> {
        let tx = self.conn().unchecked_transaction()?;
        self.write_chat(contact_id, chat)?;
        tx.commit()?;
        Ok(())
    }

    /// The same write, but without its own transaction, so that it can be
    /// combined with others into one.
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

    /// Reads the conversations with the given contact.
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

    /// Saves the conversation state and the account **in one transaction**.
    ///
    /// Separate writes are unacceptable here. Decryption advances the ratchet and
    /// consumes a one-time key of the account; if only one of the two halves reaches
    /// the disk, the states will diverge, and some messages will become
    /// unreadable forever. This is exactly why the storage is SQLite and not
    /// a set of files.
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

    // ── Internal ────────────────────────────────────────────────────────────

    /// Writes an internal value.
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

    /// Reads an internal value.
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

    /// The single row from a table where there is always exactly one.
    fn sealed_singleton(&self, table: &str) -> Result<Option<Vec<u8>>, StorageError> {
        // The table name is substituted into the query, and this is the only place where
        // that is done. It cannot come from outside: all calls use literals from
        // this same file.
        let sql = format!("SELECT sealed FROM {table} WHERE id = ?1");
        let sealed: Option<Vec<u8>> = self
            .conn()
            .query_row(&sql, [SINGLETON], |r| r.get(0))
            .optional()?;
        Ok(sealed.filter(|b| !b.is_empty()))
    }
}

/// Layout of a contact record: `public identity (64) ‖ name in UTF-8`.
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

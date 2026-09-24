//! What exactly is stored and how it is put in its place.
//!
//! A common rule for all methods: ready-made core types are handed out, not bytes,
//! and **the check is repeated on read**. A successful AEAD open says
//! "we wrote these bytes", and only that. It does not say the bytes are correct.
//! That is why the sigchain goes through `verify()` again after loading, and the
//! conversation state through its own parsing: corruption inside the trusted boundary
//! goes no further than the boundary.

use apeiron_core::{
    pickle_account, purpose, unpickle_account, Chat, Identity, PublicIdentity, SecretKey, Sigchain,
};
use rusqlite::OptionalExtension;
use zeroize::Zeroizing;

use crate::record::{open_record, seal_record, Table};
use crate::{Storage, StorageError};

/// Row identifier in tables where there is always exactly one row.
const SINGLETON: i64 = 1;

/// Opened records with their row ids; wiped when dropped.
pub type Records = Vec<(i64, Zeroizing<Vec<u8>>)>;

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
        let tx = self.conn().unchecked_transaction()?;
        let id = self.write_contact(peer, name)?;
        tx.commit()?;
        Ok(id)
    }

    /// The same write, but without its own transaction, so that it can be combined with
    /// others into one.
    fn write_contact(&self, peer: &PublicIdentity, name: &str) -> Result<i64, StorageError> {
        let tag = self.keys().tag().tag(&peer.to_bytes());
        let tx = self.conn();

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

    // ── Conversations through the transport (schema v2) ─────────────────────
    //
    // The records here are bytes the caller encodes (the transport's pair state, a message
    // with its direction, time and status): the storage seals them in their place and knows
    // nothing of their layout, so it does not depend on the transport.

    /// Establishes a contact together with its conversation **in one transaction**: the
    /// contact, the Olm session, the transport state of the pair, the first messages, and the
    /// device account whose one-time key the introduction has just spent
    /// (`docs/transport.md` §8). Returns the contact's id and the ids of the messages.
    ///
    /// Written separately, a crash in between could leave a contact without a session, or a
    /// spent one-time key without the contact it was spent on — and a second try would then
    /// fail for good, since that key is gone.
    pub fn introduce(
        &self,
        peer: &PublicIdentity,
        name: &str,
        chat: &Chat,
        account: &apeiron_core::vodozemac::olm::Account,
        pair_state: &[u8],
        messages: &[Vec<u8>],
    ) -> Result<(i64, Vec<i64>), StorageError> {
        let tx = self.conn().unchecked_transaction()?;
        let contact = self.write_contact(peer, name)?;
        self.write_chat(contact, chat)?;
        self.write_pair_state(contact, pair_state)?;
        let mut ids = Vec::with_capacity(messages.len());
        for m in messages {
            ids.push(self.write_new_message(contact, m)?);
        }
        self.write_account(account)?;
        tx.commit()?;
        Ok((contact, ids))
    }

    /// Commits one round of a conversation **in one transaction**: the Olm session, the
    /// transport state of the pair, new messages and changed ones (`docs/transport.md` §4, §6).
    /// Returns the ids of the new messages, in order.
    ///
    /// Separately they would diverge on a crash: a session saved without the pair state would
    /// encrypt again with a chain key the peer has already seen, and the second message would
    /// be refused; a pair state saved without its messages would acknowledge what nobody can
    /// ever read.
    pub fn commit_conversation(
        &self,
        contact_id: i64,
        chat: &Chat,
        pair_state: &[u8],
        new_messages: &[Vec<u8>],
        changed_messages: &[(i64, Vec<u8>)],
    ) -> Result<Vec<i64>, StorageError> {
        let tx = self.conn().unchecked_transaction()?;
        self.write_chat(contact_id, chat)?;
        self.write_pair_state(contact_id, pair_state)?;
        let mut ids = Vec::with_capacity(new_messages.len());
        for m in new_messages {
            ids.push(self.write_new_message(contact_id, m)?);
        }
        for (id, m) in changed_messages {
            self.write_message(*id, contact_id, m)?;
        }
        tx.commit()?;
        Ok(ids)
    }

    fn write_pair_state(&self, contact_id: i64, plain: &[u8]) -> Result<(), StorageError> {
        let conn = self.conn();
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM pair_state WHERE contact_id = ?1",
                [contact_id],
                |r| r.get(0),
            )
            .optional()?;
        let id = match existing {
            Some(id) => id,
            None => {
                conn.execute(
                    "INSERT INTO pair_state (contact_id, sealed) VALUES (?1, ?2)",
                    rusqlite::params![contact_id, Vec::<u8>::new()],
                )?;
                conn.last_insert_rowid()
            }
        };
        let sealed = seal_record(self.keys().pair_state(), Table::PairState, id, plain)?;
        conn.execute(
            "UPDATE pair_state SET sealed = ?1 WHERE id = ?2",
            rusqlite::params![sealed, id],
        )?;
        Ok(())
    }

    /// The transport state of the conversation with the contact, if there is one.
    pub fn load_pair_state(
        &self,
        contact_id: i64,
    ) -> Result<Option<Zeroizing<Vec<u8>>>, StorageError> {
        let row: Option<(i64, Vec<u8>)> = self
            .conn()
            .query_row(
                "SELECT id, sealed FROM pair_state WHERE contact_id = ?1",
                [contact_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        row.map(|(id, sealed)| {
            Ok(Zeroizing::new(open_record(
                self.keys().pair_state(),
                Table::PairState,
                id,
                &sealed,
            )?))
        })
        .transpose()
    }

    fn write_new_message(&self, contact_id: i64, plain: &[u8]) -> Result<i64, StorageError> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO messages (contact_id, sealed) VALUES (?1, ?2)",
            rusqlite::params![contact_id, Vec::<u8>::new()],
        )?;
        let id = conn.last_insert_rowid();
        self.write_message(id, contact_id, plain)?;
        Ok(id)
    }

    fn write_message(&self, id: i64, contact_id: i64, plain: &[u8]) -> Result<(), StorageError> {
        let sealed = seal_record(self.keys().message(), Table::Message, id, plain)?;
        let changed = self.conn().execute(
            "UPDATE messages SET sealed = ?1 WHERE id = ?2 AND contact_id = ?3",
            rusqlite::params![sealed, id, contact_id],
        )?;
        if changed == 1 {
            Ok(())
        } else {
            Err(StorageError::NotFound("message of this contact"))
        }
    }

    /// A page of the history with the contact: at most `limit` messages older than `before`
    /// (all, if `None`), newest first. The history reaches the interface one page at a time
    /// (R-004), never whole.
    pub fn messages(
        &self,
        contact_id: i64,
        before: Option<i64>,
        limit: u32,
    ) -> Result<Records, StorageError> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, sealed FROM messages WHERE contact_id = ?1 AND id < ?2
             ORDER BY id DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(
            rusqlite::params![contact_id, before.unwrap_or(i64::MAX), limit],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)),
        )?;
        let mut out = Vec::new();
        for row in rows {
            let (id, sealed) = row?;
            let plain = open_record(self.keys().message(), Table::Message, id, &sealed)?;
            out.push((id, Zeroizing::new(plain)));
        }
        Ok(out)
    }

    /// Replaces the items the background job re-puts while the vault is locked, in one
    /// transaction. Sealed under a key derived from `background`, not from the database key:
    /// the job has no PIN (`docs/transport.md` §9).
    pub fn replace_outbox(
        &self,
        background: &SecretKey,
        items: &[Vec<u8>],
    ) -> Result<(), StorageError> {
        let key = background.derive(purpose::OUTBOX);
        let tx = self.conn().unchecked_transaction()?;
        tx.execute("DELETE FROM outbox", [])?;
        for item in items {
            tx.execute(
                "INSERT INTO outbox (sealed) VALUES (?1)",
                [Vec::<u8>::new()],
            )?;
            let id = tx.last_insert_rowid();
            let sealed = seal_record(&key, Table::Outbox, id, item)?;
            tx.execute(
                "UPDATE outbox SET sealed = ?1 WHERE id = ?2",
                rusqlite::params![sealed, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The items the background job re-puts.
    pub fn outbox(&self, background: &SecretKey) -> Result<Vec<Vec<u8>>, StorageError> {
        let key = background.derive(purpose::OUTBOX);
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT id, sealed FROM outbox ORDER BY id")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (id, sealed) = row?;
            out.push(open_record(&key, Table::Outbox, id, &sealed)?);
        }
        Ok(out)
    }

    /// Saves an invitation that waits for an answer; returns its id.
    pub fn save_invitation(&self, plain: &[u8]) -> Result<i64, StorageError> {
        let tx = self.conn().unchecked_transaction()?;
        tx.execute(
            "INSERT INTO invitations (sealed) VALUES (?1)",
            [Vec::<u8>::new()],
        )?;
        let id = tx.last_insert_rowid();
        let sealed = seal_record(self.keys().invitation(), Table::Invitation, id, plain)?;
        tx.execute(
            "UPDATE invitations SET sealed = ?1 WHERE id = ?2",
            rusqlite::params![sealed, id],
        )?;
        tx.commit()?;
        Ok(id)
    }

    /// Every invitation that waits for an answer.
    pub fn invitations(&self) -> Result<Records, StorageError> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT id, sealed FROM invitations ORDER BY id")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (id, sealed) = row?;
            let plain = open_record(self.keys().invitation(), Table::Invitation, id, &sealed)?;
            out.push((id, Zeroizing::new(plain)));
        }
        Ok(out)
    }

    /// Forgets an invitation: answered, or expired.
    pub fn delete_invitation(&self, id: i64) -> Result<(), StorageError> {
        self.conn()
            .execute("DELETE FROM invitations WHERE id = ?1", [id])?;
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
        StorageError::Wrapper("contact record shorter than a public identity".to_string())
    })?;
    let tail = plain.get(64..).unwrap_or_default();
    let peer = PublicIdentity::from_bytes(head)?;
    Ok((peer, String::from_utf8_lossy(tail).into_owned()))
}

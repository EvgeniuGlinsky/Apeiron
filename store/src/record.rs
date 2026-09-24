//! Sealing of records and binding each one to its location.
//!
//! What is encrypted is not the whole file but each record separately. The difference is
//! not convenience: a record gets a **location**, and it cannot be substituted.
//!
//! The record's public data (table name, id, schema version) goes into AEAD as
//! additional authenticated data. After that, moving a row to someone else's
//! identifier, slipping it in from another table or rolling back the schema version
//! will not work: the authenticity check will not match, and that is a rejection, not
//! different content.

use apeiron_core::{open, seal, AeadError, SecretKey};

/// Domain separator. Project convention: `apeiron/<area>/v1`.
const RECORD_DOMAIN: &[u8] = b"apeiron/storage/record/v1";

/// The table layout version.
///
/// It goes into the AEAD of every record, not only into the internal table. So rolling
/// the schema back to an old version does not pass silently: records of the new version
/// stop being readable instead of being read wrongly.
pub const SCHEMA_VERSION: u16 = 1;

/// The table a record lives in.
///
/// The numbers are fixed forever: they go into the authenticity check, and changing a
/// number makes everything written under the previous one unreadable. New ones may be
/// added; existing ones may not be changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Table {
    Meta = 1,
    Identity = 2,
    Account = 3,
    Contact = 4,
    Session = 5,
    Sigchain = 6,
}

/// Assembles the record's additional authenticated data.
fn aad(table: Table, id: i64, schema: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(RECORD_DOMAIN.len() + 1 + 8 + 2);
    out.extend_from_slice(RECORD_DOMAIN);
    out.push(table as u8);
    out.extend_from_slice(&id.to_be_bytes());
    out.extend_from_slice(&schema.to_be_bytes());
    out
}

/// Seals a record for a specific location in the database.
pub fn seal_record(
    key: &SecretKey,
    table: Table,
    id: i64,
    plaintext: &[u8],
) -> Result<Vec<u8>, AeadError> {
    seal(key, &aad(table, id, SCHEMA_VERSION), plaintext)
}

/// Opens a record, checking that it came from exactly this place.
pub fn open_record(
    key: &SecretKey,
    table: Table,
    id: i64,
    sealed: &[u8],
) -> Result<Vec<u8>, AeadError> {
    open(key, &aad(table, id, SCHEMA_VERSION), sealed)
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

    fn key() -> SecretKey {
        SecretKey::generate().expect("the OS provides randomness")
    }

    #[test]
    fn a_record_reads_back_from_its_own_place() {
        let k = key();
        let sealed = seal_record(&k, Table::Identity, 1, b"secret").unwrap();
        assert_eq!(
            open_record(&k, Table::Identity, 1, &sealed).unwrap(),
            b"secret"
        );
    }

    #[test]
    fn record_moved_to_another_id_is_rejected() {
        let k = key();
        let sealed = seal_record(&k, Table::Session, 7, b"ratchet").unwrap();
        assert!(open_record(&k, Table::Session, 8, &sealed).is_err());
    }

    #[test]
    fn record_from_another_table_is_rejected() {
        let k = key();
        let sealed = seal_record(&k, Table::Contact, 3, "someone".as_bytes()).unwrap();
        assert!(open_record(&k, Table::Session, 3, &sealed).is_err());
    }

    #[test]
    fn schema_downgrade_is_noticed() {
        let k = key();
        let sealed = seal(&k, &aad(Table::Meta, 1, SCHEMA_VERSION + 1), b"x").unwrap();
        assert!(
            open_record(&k, Table::Meta, 1, &sealed).is_err(),
            "a record of another schema version was read as our own"
        );
    }

    #[test]
    fn wrong_key_opens_nothing() {
        let sealed = seal_record(&key(), Table::Identity, 1, b"secret").unwrap();
        assert!(open_record(&key(), Table::Identity, 1, &sealed).is_err());
    }

    #[test]
    fn a_flipped_byte_is_rejected() {
        let k = key();
        let mut sealed = seal_record(&k, Table::Account, 1, b"olm").unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 0x01;
        assert!(open_record(&k, Table::Account, 1, &sealed).is_err());
    }
}

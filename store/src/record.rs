//! Sealing of records and binding each one to its location.
//!
//! What is encrypted is not the whole file but each record separately. The difference is
//! not convenience: a record gets a **location**, and it cannot be substituted.
//!
//! The record's public data (table name, id, record format) goes into AEAD as
//! additional authenticated data. After that, moving a row to someone else's
//! identifier, slipping it in from another table or reading it in another format
//! will not work: the authenticity check will not match, and that is a rejection, not
//! different content.

use apeiron_core::{open, seal, AeadError, SecretKey};

/// Domain separator. Project convention: `apeiron/<area>/v1`.
const RECORD_DOMAIN: &[u8] = b"apeiron/storage/record/v1";

/// The version of how a record is encoded. It goes into the AEAD of every record.
///
/// **It is 1, and must stay 1 for as long as records are encoded the way schema v1 wrote
/// them**: until schema v2 this number was the schema version itself, and every record on
/// every phone carries it. It changes only when the encoding of a record changes — and then
/// together with a migration that re-seals every record, since none of the old ones would
/// open any more. Test `a_record_sealed_by_schema_v1_still_opens`.
pub const RECORD_FORMAT: u16 = 1;

/// The table layout version.
///
/// Kept apart from [`RECORD_FORMAT`] since schema v2: new tables must not make every existing
/// record unreadable, which is what raising a version inside the AAD would do.
///
/// - 1: identity, account, sigchain, contacts, sessions, meta;
/// - 2: + messages, outbox, pair state, invitations (`docs/transport.md` §9).
pub const SCHEMA_VERSION: u16 = 2;

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
    // Schema v2 (`docs/transport.md` §9).
    Message = 7,
    Outbox = 8,
    // 9 is reserved for a separate inbound queue; for now the undecrypted parts are kept
    // inside the pair state, which is sealed as one record.
    PairState = 10,
    Invitation = 11,
}

/// Assembles the record's additional authenticated data.
fn aad(table: Table, id: i64, format: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(RECORD_DOMAIN.len() + 1 + 8 + 2);
    out.extend_from_slice(RECORD_DOMAIN);
    out.push(table as u8);
    out.extend_from_slice(&id.to_be_bytes());
    out.extend_from_slice(&format.to_be_bytes());
    out
}

/// Seals a record for a specific location in the database.
pub fn seal_record(
    key: &SecretKey,
    table: Table,
    id: i64,
    plaintext: &[u8],
) -> Result<Vec<u8>, AeadError> {
    seal(key, &aad(table, id, RECORD_FORMAT), plaintext)
}

/// Opens a record, checking that it came from exactly this place.
pub fn open_record(
    key: &SecretKey,
    table: Table,
    id: i64,
    sealed: &[u8],
) -> Result<Vec<u8>, AeadError> {
    open(key, &aad(table, id, RECORD_FORMAT), sealed)
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
    fn a_record_of_another_format_is_rejected() {
        let k = key();
        let sealed = seal(&k, &aad(Table::Meta, 1, RECORD_FORMAT + 1), b"x").unwrap();
        assert!(
            open_record(&k, Table::Meta, 1, &sealed).is_err(),
            "a record of another format was read as our own"
        );
    }

    /// Sealed by the code of schema v1 (commit 1b5d0fd) with the key `[7; 32]`, table Meta,
    /// id 1. Every record on the phones was sealed this way; if this stops opening, so do they.
    #[test]
    fn a_record_sealed_by_schema_v1_still_opens() {
        let k = SecretKey::from_bytes([7; 32]);
        let sealed = hex::decode(
            "0ff0ab96c862c356558311fbeb0f097e3c44bd5da504c0bf1a2c4c27bed6a927\
             47be1c550956db6ac658700f08f264a654dd54c050f5412df690b565",
        )
        .unwrap();
        assert_eq!(
            open_record(&k, Table::Meta, 1, &sealed).unwrap(),
            b"written by schema v1"
        );
    }

    /// The numbers of the tables go into every record; they are fixed forever.
    #[test]
    fn table_numbers_are_fixed() {
        let numbers = [
            (Table::Meta, 1),
            (Table::Identity, 2),
            (Table::Account, 3),
            (Table::Contact, 4),
            (Table::Session, 5),
            (Table::Sigchain, 6),
            (Table::Message, 7),
            (Table::Outbox, 8),
            (Table::PairState, 10),
            (Table::Invitation, 11),
        ];
        for (table, n) in numbers {
            assert_eq!(table as u8, n, "{table:?}");
        }
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

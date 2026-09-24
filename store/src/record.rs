//! Запечатывание записей и привязка каждой к её месту.
//!
//! Шифруется не файл целиком, а каждая запись отдельно. Разница не в удобстве:
//! у записи появляется **место**, и подменить его нельзя.
//!
//! Открытые данные записи (имя таблицы, номер, версия схемы) идут в AEAD как
//! дополнительные аутентифицируемые данные. Переложить строку на чужой
//! идентификатор, подсунуть её из другой таблицы или откатить версию схемы
//! после этого не выйдет — проверка подлинности не сойдётся, и это отказ, а не
//! другое содержимое.

use apeiron_core::{open, seal, AeadError, SecretKey};

/// Разделитель области. Соглашение проекта: `apeiron/<область>/v1`.
const RECORD_DOMAIN: &[u8] = b"apeiron/storage/record/v1";

/// Версия раскладки таблиц.
///
/// Входит в AEAD каждой записи, а не только в служебную таблицу. Поэтому откат
/// схемы на старую версию не проходит молча: записи новой версии перестают
/// читаться, вместо того чтобы читаться неправильно.
pub const SCHEMA_VERSION: u16 = 1;

/// Таблица, в которой живёт запись.
///
/// Числа зафиксированы навсегда: они входят в проверку подлинности, и смена
/// числа делает нечитаемым всё, что записано прежним. Добавлять новые можно,
/// менять существующие — нет.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Table {
    Meta = 1,
    Identity = 2,
    Account = 3,
    Contact = 4,
    Session = 5,
    Sigchain = 6,
}

/// Собирает дополнительные аутентифицируемые данные записи.
fn aad(table: Table, id: i64, schema: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(RECORD_DOMAIN.len() + 1 + 8 + 2);
    out.extend_from_slice(RECORD_DOMAIN);
    out.push(table as u8);
    out.extend_from_slice(&id.to_be_bytes());
    out.extend_from_slice(&schema.to_be_bytes());
    out
}

/// Запечатывает запись для конкретного места в базе.
pub fn seal_record(
    key: &SecretKey,
    table: Table,
    id: i64,
    plaintext: &[u8],
) -> Result<Vec<u8>, AeadError> {
    seal(key, &aad(table, id, SCHEMA_VERSION), plaintext)
}

/// Распечатывает запись, проверяя, что она пришла именно отсюда.
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
        SecretKey::generate().expect("ОС отдаёт случайность")
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
        let sealed = seal_record(&k, Table::Contact, 3, "кто-то".as_bytes()).unwrap();
        assert!(open_record(&k, Table::Session, 3, &sealed).is_err());
    }

    #[test]
    fn schema_downgrade_is_noticed() {
        let k = key();
        let sealed = seal(&k, &aad(Table::Meta, 1, SCHEMA_VERSION + 1), b"x").unwrap();
        assert!(
            open_record(&k, Table::Meta, 1, &sealed).is_err(),
            "запись другой версии схемы прочиталась как своя"
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

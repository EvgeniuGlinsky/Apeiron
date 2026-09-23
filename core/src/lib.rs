//! Ядро мессенджера.
//!
//! Здесь живёт всё, что должно быть верным: криптография, состояние протокола,
//! хранилище. FFI в этом слое нет намеренно — мост к Flutter лежит отдельным
//! тонким крейтом, чтобы ядро можно было тестировать и подвергать аудиту
//! независимо от интерфейса.
//!
//! # Правила, действующие во всём крейте
//!
//! * Открытый текст и ключевой материал не покидают Rust (решение R-004,
//!   `docs/threat-log.md`). В Dart уходит только то, что прямо сейчас на экране.
//! * Собственных криптографических примитивов здесь нет и не будет. Всё берётся
//!   готовым из проверенных реализаций (§18 исследования).
//! * Паника запрещена линтами: в криптографическом коде она превращается в отказ
//!   в обслуживании, а иногда и в утечку через сообщение об ошибке.

pub mod aead;
pub mod identity;
pub mod prekey;
pub mod random;
pub mod session;
pub mod sigchain;

pub use aead::{open, seal, AeadError, SecretKey};
pub use identity::{Identity, IdentityError, PublicIdentity};
pub use prekey::{PrekeyBundle, PrekeyError, UnverifiedPrekeyBundle};
pub use session::{Chat, ChatError};
pub use sigchain::{ChainSigner, ChainState, EntryBody, Sigchain, SigchainError};

pub use random::{random_bytes, RandomError};
/// Реэкспорт `vodozemac`.
///
/// Наш публичный API отдаёт наружу его типы (ключи, сообщения), поэтому
/// пользователь ядра обязан работать ровно с той же версией крейта. Реэкспорт
/// это гарантирует: взять другую просто неоткуда.
pub use vodozemac;

/// Версия формата, которую понимает это ядро.
pub const PROTOCOL_VERSION: u8 = 1;

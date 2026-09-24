//! Иерархия подключей.
//!
//! Из одного ключа базы выводится по ключу на каждое назначение. Метка
//! назначения обязательна и берётся из реестра `apeiron_core::purpose`, а не
//! пишется строкой по месту вызова: опечатка в строке даёт **другой ключ**, всё
//! продолжает работать, и обнаруживается это тогда, когда данные уже записаны
//! чужим ключом.
//!
//! Зачем разделять вообще: без разделения одна и та же ключевая последовательность
//! оказалась бы у разных подсистем, и ошибка в одной из них стала бы ошибкой во
//! всех.

use apeiron_core::{purpose, SecretKey};

/// Подключи по назначениям.
pub struct Keys {
    identity: SecretKey,
    account: SecretKey,
    session: SecretKey,
    sigchain: SecretKey,
    contact: SecretKey,
    meta: SecretKey,
    tag: SecretKey,
}

impl Keys {
    /// Выводит все подключи из ключа базы.
    pub fn derive(dek: &SecretKey) -> Self {
        Self {
            identity: dek.derive(purpose::IDENTITY),
            account: dek.derive(purpose::ACCOUNT),
            session: dek.derive(purpose::SESSION),
            sigchain: dek.derive(purpose::SIGCHAIN),
            contact: dek.derive(purpose::CONTACT),
            meta: dek.derive(purpose::META),
            tag: dek.derive(purpose::TAG),
        }
    }

    pub(crate) fn identity(&self) -> &SecretKey {
        &self.identity
    }

    pub(crate) fn account(&self) -> &SecretKey {
        &self.account
    }

    pub(crate) fn session(&self) -> &SecretKey {
        &self.session
    }

    pub(crate) fn sigchain(&self) -> &SecretKey {
        &self.sigchain
    }

    pub(crate) fn contact(&self) -> &SecretKey {
        &self.contact
    }

    pub(crate) fn meta(&self) -> &SecretKey {
        &self.meta
    }

    /// Ключ меток поиска.
    ///
    /// Отдельная ветвь, не связанная с расшифровкой: знание метки не приближает
    /// к содержимому. Тот же приём, что `K_addr` в исследовании (§16.2).
    pub(crate) fn tag(&self) -> &SecretKey {
        &self.tag
    }
}

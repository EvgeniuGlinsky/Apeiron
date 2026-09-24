//! The subkey hierarchy.
//!
//! One key per purpose is derived from the single database key. The purpose
//! label is mandatory and is taken from the `apeiron_core::purpose` registry rather than
//! written as a string at the call site: a typo in a string gives **a different key**,
//! everything keeps working, and this is discovered when the data has already been written
//! under the wrong key.
//!
//! Why separate at all: without separation the same key stream
//! would end up in different subsystems, and a bug in one of them would become a bug in
//! all of them.

use apeiron_core::{purpose, SecretKey};

/// Subkeys by purpose.
pub struct Keys {
    identity: SecretKey,
    account: SecretKey,
    session: SecretKey,
    sigchain: SecretKey,
    contact: SecretKey,
    meta: SecretKey,
    tag: SecretKey,
    message: SecretKey,
    pair_state: SecretKey,
    invitation: SecretKey,
}

impl Keys {
    /// Derives all subkeys from the database key.
    pub fn derive(dek: &SecretKey) -> Self {
        Self {
            identity: dek.derive(purpose::IDENTITY),
            account: dek.derive(purpose::ACCOUNT),
            session: dek.derive(purpose::SESSION),
            sigchain: dek.derive(purpose::SIGCHAIN),
            contact: dek.derive(purpose::CONTACT),
            meta: dek.derive(purpose::META),
            tag: dek.derive(purpose::TAG),
            message: dek.derive(purpose::MESSAGE),
            pair_state: dek.derive(purpose::PAIR_STATE),
            invitation: dek.derive(purpose::INVITATION),
        }
    }

    pub(crate) fn message(&self) -> &SecretKey {
        &self.message
    }

    pub(crate) fn pair_state(&self) -> &SecretKey {
        &self.pair_state
    }

    pub(crate) fn invitation(&self) -> &SecretKey {
        &self.invitation
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

    /// The lookup tag key.
    ///
    /// A separate branch, unrelated to decryption: knowing a tag brings one no closer
    /// to the content. The same technique as `K_addr` in the research (§16.2).
    pub(crate) fn tag(&self) -> &SecretKey {
        &self.tag
    }
}

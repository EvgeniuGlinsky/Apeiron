//! Pairwise conversation: the double ratchet over a verified prekey bundle.
//!
//! The ratchet is not our own: it is [`vodozemac`], the Olm implementation from
//! Matrix.org that has passed an independent audit. The project has no cryptographic
//! primitives of its own and never will (§18 of the research). Here there is only what
//! this implementation lacks for our architecture.
//!
//! # What is missing: batch receive
//!
//! Olm keeps no more than 40 skipped-message keys per receiving chain
//! (`MAX_MESSAGE_KEYS`) and rejects a gap larger than 2000
//! (`MAX_MESSAGE_GAP`). For Matrix this is enough: there the server delivers
//! messages roughly in the order they were sent.
//!
//! Not so for us. The architecture explicitly assumes delivery through a blind
//! relay with a holding window of up to a day: the device was offline, then came
//! online and picked everything up at once, in whatever order the queue gave it.
//! A hundred messages arriving back to front, with naive decryption, mean
//! sixty **lost forever**: after decrypting the hundredth, the ratchet advances
//! and throws away the keys that did not fit into forty.
//!
//! What saves us is that **the order is readable before decryption**: the header of
//! every Olm message carries the ratchet key and the chain index in the clear. So a
//! batch can be sorted and decrypted in ascending index order, and then no gaps
//! arise at all. This is what [`Chat::decrypt_batch`] does, and on two hundred
//! messages the difference is between "everything read" and "half
//! lost" (test `batch_survives_reverse_order`).

use vodozemac::olm::{
    Account, DecryptionError, EncryptionError, OlmMessage, PreKeyMessage, Session, SessionConfig,
    SessionCreationError,
};

use crate::identity::{PublicIdentity, PUBLIC_IDENTITY_BYTES};
use crate::prekey::PrekeyBundle;

/// What can go wrong in a conversation.
#[derive(Debug, thiserror::Error)]
pub enum ChatError {
    #[error("failed to create the session: {0}")]
    Creation(#[from] SessionCreationError),

    #[error("encryption failed: {0}")]
    Encryption(#[from] EncryptionError),

    #[error("decryption failed: {0}")]
    Decryption(#[from] DecryptionError),

    #[error("the decrypted data is not UTF-8 text")]
    NotText,

    #[error("internal error: the message was left unprocessed")]
    NotProcessed,

    #[error("failed to save the conversation state: {0}")]
    Pickle(String),

    #[error(
        "THE CONVERSATION STATE IS DAMAGED OR SUBSTITUTED: {0}. \
         This conversation must not be continued."
    )]
    Unpickle(String),
}

impl ChatError {
    /// Whether the message is lost irrecoverably.
    ///
    /// The distinction matters for the interface: "corrupted, try again" and
    /// "can never be read now" are different messages to the user, and the
    /// second must not be shown as the first. Staying silent is even less acceptable:
    /// losing a message is something a person must be told about.
    pub fn is_lost_forever(&self) -> bool {
        matches!(
            self,
            Self::Decryption(
                DecryptionError::MissingMessageKey(_) | DecryptionError::TooBigMessageGap(_, _)
            )
        )
    }
}

/// One pairwise conversation.
pub struct Chat {
    session: Session,
    peer: PublicIdentity,
}

/// Saves the device account state.
///
/// Without this every launch of the application would spawn **a new device**: an
/// Olm account has its own long-term keys and a supply of one-time keys, and losing
/// them breaks all existing conversations at once.
///
/// The format is `serde_json` over `AccountPickle`. vodozemac's own encryption
/// (`AccountPickle::encrypt`, AES-CBC with HMAC over base64) is deliberately not
/// used: the project's crypto stack sticks to one generation, and a second
/// encryption format next to the keys is a second set of maintenance
/// obligations. These bytes are sealed by `crate::aead`.
///
/// A canonical form is not required here: the result is not signed but
/// sealed, and that does not depend on the representation. Where the form must be
/// unambiguous (in the prekey bundle and in the sigchain) `serde` is not
/// used at all.
pub fn pickle_account(account: &Account) -> Result<Vec<u8>, ChatError> {
    serde_json::to_vec(&account.pickle()).map_err(|e| ChatError::Pickle(e.to_string()))
}

/// Restores the device account from what [`pickle_account`] returned.
pub fn unpickle_account(bytes: &[u8]) -> Result<Account, ChatError> {
    let pickle = serde_json::from_slice(bytes).map_err(|e| ChatError::Unpickle(e.to_string()))?;
    Ok(Account::from_pickle(pickle))
}

impl Chat {
    /// The Olm protocol version.
    ///
    /// Explicitly the first. The second is hidden in vodozemac behind an experimental
    /// feature flag and is not standardized; the February 2026 finding about version
    /// downgrade and truncated MACs also concerned it. Until V2 is standardized there
    /// is no reason to take it. Decision R-009 in `docs/threat-log.md`.
    fn config() -> SessionConfig {
        SessionConfig::version_1()
    }

    /// Starts a conversation from a **verified** prekey bundle.
    ///
    /// An unverified one cannot be passed here: wrong type. See [`crate::prekey`].
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

    /// Accepts the first message from someone whose bundle is already verified.
    ///
    /// The sender's bundle is not there for decoration: `vodozemac` will compare the
    /// device key from the bundle with the one claimed in the message, and will refuse
    /// to create a session if they differ. This way the first message ends up
    /// bound to an identity, not just "from someone".
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

    /// The peer's identity: the one whose fingerprint is shown on the verification screen.
    /// Saves the conversation state.
    ///
    /// Layout: `peer's public identity (64) ‖ serde_json(SessionPickle)`.
    ///
    /// The peer is stored alongside not for convenience: `SessionPickle` does not have
    /// it, and without it `Chat` cannot be restored, and, more importantly, there would
    /// be no one to present the safety number for. A conversation without a known peer
    /// is a conversation with who knows whom.
    pub fn pickle(&self) -> Result<Vec<u8>, ChatError> {
        let mut out = Vec::with_capacity(PUBLIC_IDENTITY_BYTES + 512);
        out.extend_from_slice(&self.peer.to_bytes());
        let body = serde_json::to_vec(&self.session.pickle())
            .map_err(|e| ChatError::Pickle(e.to_string()))?;
        out.extend_from_slice(&body);
        Ok(out)
    }

    /// Restores a conversation from what [`Chat::pickle`] returned.
    ///
    /// The bytes must come from a verified source: a successful AEAD
    /// open says "we wrote this", and only that. They cannot be substituted
    /// from outside, and corruption inside the boundary goes no further than the
    /// boundary; hence a separate error instead of silently returning an empty state.
    pub fn from_pickle(bytes: &[u8]) -> Result<Self, ChatError> {
        let head = bytes.get(..PUBLIC_IDENTITY_BYTES).ok_or_else(|| {
            ChatError::Unpickle("record shorter than a public identity".to_string())
        })?;
        let tail = bytes
            .get(PUBLIC_IDENTITY_BYTES..)
            .ok_or_else(|| ChatError::Unpickle("record without the ratchet state".to_string()))?;
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

    /// The session identifier. Identical on both sides.
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

    /// Decrypts a batch after sorting it in chain order.
    ///
    /// Results are returned **in input order**: the i-th result belongs to the
    /// i-th message, however it was reordered inside. An error on one
    /// message does not stop the work: the rest will be read.
    ///
    /// This is the method to call on everything that came from the network.
    /// [`Chat::decrypt`] is fit only where there is known to be a single message.
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

/// The order in which a batch should be decrypted.
///
/// Rules:
///
/// 1. **Within one chain, in ascending index order.** This is what it was all
///    for: otherwise the very first message decrypted "from the future" throws away
///    the keys of everyone before it beyond forty.
/// 2. **Chains in order of first appearance.** Their relative order cannot be
///    derived from the messages themselves: the ratchet key is opaque, and "which
///    chain is newer" is known only to whoever created it. Arrival order is the best
///    available approximation, and it is always right, except when the network itself
///    reordered the boundary where the conversation turned around. Then some messages
///    of the old chain may fail to be read, and this is honestly reported as an error,
///    not swallowed.
///
/// Session-establishment messages are **not singled out as a special case**, and this
/// matters: in Olm the initiator keeps sending them until it receives a reply.
/// A one-sided batch of two hundred messages consists of them entirely, and if
/// only the "ordinary" ones were sorted, nothing would change. Their chain
/// index lies in the nested message and is readable in the clear just the same.
fn batch_order(messages: &[OlmMessage]) -> Vec<usize> {
    /// The message's index in the chain and its position in the input batch.
    type Member = (u64, usize);
    /// The ratchet key and all messages of its chain.
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

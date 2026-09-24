//! Getting to know each other (`docs/transport.md` §8).
//!
//! An invitation is the inviter's signed prekey bundle, the secret `S` of a one-time inbox and
//! an expiry — as a QR code or as text through any channel. The one who accepts (B) puts **one
//! reply** into the inbox; the inviter (A) opens it and both have a session.
//!
//! `S` only locates the drop. The reply is encrypted to A's agreement key as well, so whoever
//! read the invitation on the way can find the reply but not open it, and does not learn who
//! answered. They *can* answer in B's name with their own keys — that is what the mandatory
//! verification is for; nothing here marks a contact as verified.

use apeiron_core::prekey::PREKEY_BUNDLE_BYTES;
use apeiron_core::vodozemac::olm::{Account, OlmMessage, PreKeyMessage};
use apeiron_core::vodozemac::Curve25519PublicKey;
use apeiron_core::{
    open, random_bytes, seal, Chat, Identity, IdentityError, PrekeyBundle, PublicIdentity,
    SecretKey,
};
use base64::Engine;
use hkdf::Hkdf;
use sha2::Sha256;
use x25519_dalek::{PublicKey as X25519Public, StaticSecret};
use zeroize::Zeroizing;

use crate::address::Slot;
use crate::envelope::ITEM_BYTES;
use crate::item::SignedItem;
use crate::pair::Pair;
use crate::TransportError;

const VERSION: u8 = 1;

/// What a pasted invitation starts with, so it is recognised among other text.
pub const TEXT_PREFIX: &str = "apeiron:";

/// `version (1) ‖ bundle (224) ‖ S (32) ‖ expiry day (4)`.
pub const INVITATION_BYTES: usize = 1 + PREKEY_BUNDLE_BYTES + 32 + 4;

/// An invitation is good for this many days.
pub const VALID_DAYS: u32 = 7;

const INBOX_ADDRESS_DOMAIN: &[u8] = b"apeiron/inbox/addr/v1";
const INBOX_KEY_DOMAIN: &[u8] = b"apeiron/inbox/v1";
const REPLY_AAD: &[u8] = b"apeiron/inbox/reply/v1";

/// `e_pub (32) ‖ nonce (24) ‖ sealed inner ‖ tag (16)` is one item long.
const REPLY_INNER_BYTES: usize = ITEM_BYTES - 32 - 24 - 16;

/// The longest Olm pre-key message the reply carries next to B's bundle.
const MAX_REPLY_OLM_BYTES: usize = REPLY_INNER_BYTES - PREKEY_BUNDLE_BYTES - 2;

/// The first text that fits into the reply, in bytes of UTF-8.
pub const MAX_FIRST_TEXT_BYTES: usize = 447;

pub struct Invitation {
    bundle: PrekeyBundle,
    secret: Zeroizing<[u8; 32]>,
    expires: u32,
}

impl std::fmt::Debug for Invitation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Invitation")
            .field("inviter", &self.bundle.identity().fingerprint())
            .field("expires", &self.expires)
            .finish_non_exhaustive()
    }
}

impl Invitation {
    /// A new invitation: a bundle with a fresh one-time key of `account`, a fresh inbox.
    pub fn create(
        me: &Identity,
        account: &mut Account,
        today: u32,
    ) -> Result<Self, TransportError> {
        Ok(Self {
            bundle: PrekeyBundle::create(me, account)
                .map_err(|e| TransportError::Invitation(e.to_string()))?,
            secret: Zeroizing::new(
                random_bytes::<32>().map_err(|e| TransportError::Invitation(e.to_string()))?,
            ),
            expires: today.saturating_add(VALID_DAYS),
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(INVITATION_BYTES);
        out.push(VERSION);
        out.extend_from_slice(&self.bundle.to_bytes());
        out.extend_from_slice(&*self.secret);
        out.extend_from_slice(&self.expires.to_be_bytes());
        out
    }

    /// The text form, for any channel: `apeiron:` and URL-safe base64.
    pub fn to_text(&self) -> String {
        format!(
            "{TEXT_PREFIX}{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(self.to_bytes())
        )
    }

    /// Reads an invitation and **verifies its bundle's signature**: an invitation that does not
    /// verify is not one.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TransportError> {
        if bytes.len() != INVITATION_BYTES {
            return Err(TransportError::Invitation("wrong length".to_string()));
        }
        let (version, rest) = bytes
            .split_first()
            .ok_or_else(|| TransportError::Invitation("empty".to_string()))?;
        if *version != VERSION {
            return Err(TransportError::Invitation(format!(
                "unknown version {version}"
            )));
        }
        let (bundle, rest) = rest.split_at(PREKEY_BUNDLE_BYTES);
        let (secret, expires) = rest.split_at(32);
        let bundle = PrekeyBundle::parse(bundle)
            .and_then(|b| b.verify())
            .map_err(|e| TransportError::Invitation(e.to_string()))?;
        Ok(Self {
            bundle,
            secret: Zeroizing::new(
                secret
                    .try_into()
                    .map_err(|_| TransportError::Invitation("secret".to_string()))?,
            ),
            expires: u32::from_be_bytes(
                expires
                    .try_into()
                    .map_err(|_| TransportError::Invitation("expiry".to_string()))?,
            ),
        })
    }

    /// Reads the text form, with whatever whitespace a messenger added around or inside it.
    pub fn from_text(text: &str) -> Result<Self, TransportError> {
        let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let body = compact
            .strip_prefix(TEXT_PREFIX)
            .ok_or_else(|| TransportError::Invitation("not an Apeiron invitation".to_string()))?;
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(body)
            .map_err(|e| TransportError::Invitation(e.to_string()))?;
        Self::from_bytes(&bytes)
    }

    pub fn inviter(&self) -> &PublicIdentity {
        self.bundle.identity()
    }

    pub fn expires(&self) -> u32 {
        self.expires
    }

    pub fn expired(&self, today: u32) -> bool {
        today > self.expires
    }

    /// The one-time key this invitation hands out. At expiry the inviter removes it from its
    /// account (`Account::remove_one_time_key`).
    pub fn one_time_key(&self) -> Curve25519PublicKey {
        self.bundle.one_time_key()
    }

    fn inbox(&self) -> Result<Slot, TransportError> {
        Slot::from_seed(&expand(&*self.secret, INBOX_ADDRESS_DOMAIN)?)
    }

    /// The address of the inbox: where the inviter asks for the reply.
    pub fn inbox_key(&self) -> Result<[u8; 32], TransportError> {
        Ok(self.inbox()?.public_key())
    }

    fn reply_key(&self, shared: &x25519_dalek::SharedSecret) -> Result<SecretKey, TransportError> {
        if !shared.was_contributory() {
            return Err(TransportError::Identity(IdentityError::NonContributory));
        }
        let mut ikm = Zeroizing::new(Vec::with_capacity(64));
        ikm.extend_from_slice(shared.as_bytes());
        ikm.extend_from_slice(&*self.secret);
        Ok(SecretKey::from_bytes(expand(&ikm, INBOX_KEY_DOMAIN)?))
    }
}

fn expand(ikm: &[u8], info: &[u8]) -> Result<[u8; 32], TransportError> {
    let mut out = [0u8; 32];
    Hkdf::<Sha256>::new(None, ikm)
        .expand(info, &mut out)
        .map_err(|_| TransportError::Derivation)?;
    Ok(out)
}

/// What the one who accepted has: a session, and a pair whose first item to put is the reply.
pub struct Accepted {
    pub chat: Chat,
    pub pair: Pair,
}

/// B accepts A's invitation and says `first_text`. The reply is put by the pair's rounds, and
/// re-put until A answers (`Event::Accepted`) — or turns out to be taken by someone else
/// (`Event::InvitationTaken`).
pub fn accept(
    me: &Identity,
    account: &mut Account,
    invitation: &Invitation,
    first_text: &str,
    today: u32,
) -> Result<Accepted, TransportError> {
    if invitation.expired(today) {
        return Err(TransportError::Invitation(
            "the invitation has expired".to_string(),
        ));
    }
    if first_text.len() > MAX_FIRST_TEXT_BYTES {
        return Err(TransportError::TooLong { parts: 2 });
    }
    // Refuses oneself and a non-contributory key before anything is made.
    me.pair_secret(invitation.inviter())?;

    let my_bundle =
        PrekeyBundle::create(me, account).map_err(|e| TransportError::Invitation(e.to_string()))?;
    let mut chat = Chat::initiate(account, &invitation.bundle)?;
    let OlmMessage::PreKey(first) = chat.encrypt(first_text)? else {
        return Err(TransportError::Internal(
            "the first message is not a pre-key message",
        ));
    };
    let olm = first.to_bytes();
    if olm.len() > MAX_REPLY_OLM_BYTES {
        return Err(TransportError::Internal(
            "the first message outgrew the reply",
        ));
    }

    let mut inner = Zeroizing::new(Vec::with_capacity(REPLY_INNER_BYTES));
    inner.extend_from_slice(&my_bundle.to_bytes());
    inner.extend_from_slice(
        &u16::try_from(olm.len())
            .map_err(|_| TransportError::Internal("reply length"))?
            .to_be_bytes(),
    );
    inner.extend_from_slice(&olm);
    inner.resize(REPLY_INNER_BYTES, 0);

    let ephemeral = StaticSecret::from(
        random_bytes::<32>().map_err(|e| TransportError::Invitation(e.to_string()))?,
    );
    let ephemeral_public = X25519Public::from(&ephemeral);
    let key =
        invitation.reply_key(&ephemeral.diffie_hellman(invitation.inviter().agreement_key()))?;

    let mut value = Vec::with_capacity(ITEM_BYTES);
    value.extend_from_slice(ephemeral_public.as_bytes());
    value.extend_from_slice(&seal(&key, REPLY_AAD, &inner)?);
    let reply = SignedItem::sign_value(&invitation.inbox()?, 1, value)?;

    let pair = Pair::new(me, invitation.inviter(), &chat.session_id())?.with_intro(reply);
    Ok(Accepted { chat, pair })
}

/// What the inviter has once the reply is opened.
pub struct Joined {
    pub chat: Chat,
    pub pair: Pair,
    /// Who answered — as they say. Not verified until the safety numbers are compared.
    pub peer: PublicIdentity,
    pub first_text: String,
}

/// A opens the reply found in the inbox of `invitation`. Everything is checked before anything
/// is made: the reply opens with A's key, B's bundle verifies, B is not A, and the Olm message
/// uses **this** invitation's one-time key — otherwise whoever holds one invitation's secret
/// could spend another invitation's key.
pub fn open_reply(
    me: &Identity,
    account: &mut Account,
    invitation: &Invitation,
    value: &[u8],
) -> Result<Joined, TransportError> {
    if value.len() != ITEM_BYTES {
        return Err(TransportError::NotOurs);
    }
    let (ephemeral, sealed) = value.split_at(32);
    let ephemeral: [u8; 32] = ephemeral.try_into().map_err(|_| TransportError::NotOurs)?;
    let key = invitation
        .reply_key(&me.diffie_hellman(&X25519Public::from(ephemeral)))
        .map_err(|_| TransportError::NotOurs)?;
    let inner = Zeroizing::new(open(&key, REPLY_AAD, sealed).map_err(|_| TransportError::NotOurs)?);

    let (bundle, rest) = inner
        .split_at_checked(PREKEY_BUNDLE_BYTES)
        .ok_or(TransportError::NotOurs)?;
    let bundle = PrekeyBundle::parse(bundle)
        .and_then(|b| b.verify())
        .map_err(|e| TransportError::Invitation(e.to_string()))?;
    let peer = bundle.identity().clone();
    me.pair_secret(&peer)?;

    let (len, rest) = rest.split_at_checked(2).ok_or(TransportError::NotOurs)?;
    let len = usize::from(u16::from_be_bytes(
        len.try_into().map_err(|_| TransportError::NotOurs)?,
    ));
    let olm = rest.get(..len).ok_or(TransportError::NotOurs)?;
    if rest.get(len..).unwrap_or_default().iter().any(|&b| b != 0) {
        return Err(TransportError::NotOurs);
    }
    let message =
        PreKeyMessage::from_bytes(olm).map_err(|e| TransportError::Invitation(e.to_string()))?;
    if message.one_time_key() != invitation.one_time_key() {
        return Err(TransportError::Invitation(
            "the reply uses another invitation's key".to_string(),
        ));
    }

    let (chat, first_text) = Chat::accept(account, &bundle, &message)?;
    let pair = Pair::new(me, &peer, &chat.session_id())?;
    Ok(Joined {
        chat,
        pair,
        peer,
        first_text,
    })
}

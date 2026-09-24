//! The sigchain (identity log): a signed, append-only chain of entries.
//!
//! This is the project's "blockchain", and deliberately the most boring one possible.
//! No consensus network, no mining, no coin: one identity keeps its own
//! log, every entry refers to the hash of the previous one and is signed.
//! Nothing more is needed for the task, and everything extra is extra code next
//! to the keys.
//!
//! # What the log answers
//!
//! The question "which devices belong to this identity right now". A new phone
//! appearing, a lost one being revoked: events the peer must see and check
//! themselves, without asking anyone's server.
//!
//! # What the log does NOT answer
//!
//! The question "whose identity is this". A chain is internally consistent for
//! anyone: anybody can start their own and sign whatever they want in it. The link
//! to a person comes **only** from fingerprint verification by voice or in person.
//!
//! # Why an entry refers to the hash of the previous one
//!
//! So that an entry cannot be **withheld**. A signature protects each entry
//! individually, but a set of signed entries can be presented incompletely:
//! for example, hiding a device revocation. The hash link makes such a selection
//! visible: the chain simply will not add up.

use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use crate::identity::{Identity, IdentityError, PublicIdentity, PUBLIC_IDENTITY_BYTES};

/// Domain separator for the entry signature.
const SIGN_DOMAIN: &[u8] = b"apeiron/sigchain/entry/v1";
/// Domain separator for the entry hash.
const HASH_DOMAIN: &[u8] = b"apeiron/sigchain/link/v1";

const KEY_BYTES: usize = 32;
const HASH_BYTES: usize = 32;
const SIGNATURE_BYTES: usize = 64;

/// Entry kind tags in serialized form.
const TAG_GENESIS: u8 = 1;
const TAG_ADD_DEVICE: u8 = 2;
const TAG_REVOKE_DEVICE: u8 = 3;

/// What can go wrong with the log.
#[derive(Debug, thiserror::Error)]
pub enum SigchainError {
    #[error("the log is empty")]
    Empty,

    #[error("entry {0}: the first entry must be the birth of the identity")]
    MissingGenesis(u64),

    #[error("entry {0}: the birth of the identity can only be the first entry")]
    RepeatedGenesis(u64),

    #[error(
        "entry {index}: number {got}, expected {expected} — the log is reordered or incomplete"
    )]
    OutOfOrder {
        index: usize,
        got: u64,
        expected: u64,
    },

    #[error(
        "entry {0}: the link to the previous entry does not match. Something was removed from \
         the log or substituted — a device revocation, for example."
    )]
    BrokenLink(u64),

    #[error("entry {0}: signed by a key that does not belong to this identity")]
    UnknownSigner(u64),

    #[error("entry {0}: signed by a revoked device")]
    RevokedSigner(u64),

    #[error("ENTRY {0}: THE SIGNATURE IS INVALID. The log must not be trusted.")]
    BadSignature(u64),

    #[error("entry {0}: the device is already in the log")]
    DuplicateDevice(u64),

    #[error("entry {0}: revokes a device that is not in the log")]
    UnknownDevice(u64),

    #[error("entry {0}: the device is already revoked")]
    AlreadyRevoked(u64),

    #[error("the log is cut off at entry {0}")]
    Truncated(usize),

    #[error("entry {0}: unknown entry kind {1}")]
    UnknownTag(usize, u8),

    #[error(transparent)]
    Identity(#[from] IdentityError),

    #[error("the key in the entry does not parse")]
    MalformedKey,
}

/// Who is entitled to sign a log entry.
///
/// Both the root identity and an already added active device may sign:
/// otherwise a second phone could be added only from the first one,
/// and after losing it, not at all.
pub trait ChainSigner {
    /// Public signing key, 32 bytes.
    fn signer_key(&self) -> [u8; KEY_BYTES];
    /// Signature, 64 bytes.
    fn sign_entry(&self, payload: &[u8]) -> [u8; SIGNATURE_BYTES];
}

impl ChainSigner for Identity {
    fn signer_key(&self) -> [u8; KEY_BYTES] {
        self.public().verifying_key().to_bytes()
    }

    fn sign_entry(&self, payload: &[u8]) -> [u8; SIGNATURE_BYTES] {
        self.sign(payload).to_bytes()
    }
}

impl ChainSigner for vodozemac::olm::Account {
    fn signer_key(&self) -> [u8; KEY_BYTES] {
        *self.ed25519_key().as_bytes()
    }

    fn sign_entry(&self, payload: &[u8]) -> [u8; SIGNATURE_BYTES] {
        self.sign(payload).to_bytes()
    }
}

/// The content of an entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryBody {
    /// Birth of the identity. Always first and the only one of its kind.
    Genesis { root: PublicIdentity },
    /// A device was added.
    AddDevice {
        ed: [u8; KEY_BYTES],
        curve: [u8; KEY_BYTES],
    },
    /// A device was revoked. Irreversible: it can come back only with a new key.
    RevokeDevice { ed: [u8; KEY_BYTES] },
}

impl EntryBody {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Genesis { root } => {
                out.push(TAG_GENESIS);
                out.extend_from_slice(&root.to_bytes());
            }
            Self::AddDevice { ed, curve } => {
                out.push(TAG_ADD_DEVICE);
                out.extend_from_slice(ed);
                out.extend_from_slice(curve);
            }
            Self::RevokeDevice { ed } => {
                out.push(TAG_REVOKE_DEVICE);
                out.extend_from_slice(ed);
            }
        }
    }
}

/// One log entry.
#[derive(Debug, Clone)]
pub struct Entry {
    seq: u64,
    prev: [u8; HASH_BYTES],
    signer: [u8; KEY_BYTES],
    body: EntryBody,
    signature: [u8; SIGNATURE_BYTES],
}

impl Entry {
    /// The bytes that get signed.
    fn signed_bytes(
        seq: u64,
        prev: &[u8; HASH_BYTES],
        signer: &[u8; KEY_BYTES],
        body: &EntryBody,
    ) -> Vec<u8> {
        let mut out = Vec::with_capacity(SIGN_DOMAIN.len() + 8 + HASH_BYTES + KEY_BYTES + 1 + 64);
        out.extend_from_slice(SIGN_DOMAIN);
        out.extend_from_slice(&seq.to_be_bytes());
        out.extend_from_slice(prev);
        out.extend_from_slice(signer);
        body.encode(&mut out);
        out
    }

    /// The full content of the entry, including the signature: what goes into the stream.
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.seq.to_be_bytes());
        out.extend_from_slice(&self.prev);
        out.extend_from_slice(&self.signer);
        self.body.encode(out);
        out.extend_from_slice(&self.signature);
    }

    /// The entry hash that the next entry will refer to.
    ///
    /// The signature goes into the hash deliberately: then substituting the signature
    /// also breaks the chain instead of remaining a local breakage of one entry.
    fn hash(&self) -> [u8; HASH_BYTES] {
        let mut encoded = Vec::new();
        self.encode(&mut encoded);

        let mut hasher = Sha256::new();
        hasher.update(HASH_DOMAIN);
        hasher.update(&encoded);
        hasher.finalize().into()
    }
}

/// The identity state restored from a verified log.
#[derive(Debug, Clone)]
pub struct ChainState {
    root: PublicIdentity,
    devices: BTreeMap<[u8; KEY_BYTES], Device>,
}

/// A device in the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub curve: [u8; KEY_BYTES],
    pub revoked: bool,
}

impl ChainState {
    /// The root identity: the one whose fingerprint is verified by voice.
    pub fn root(&self) -> &PublicIdentity {
        &self.root
    }

    /// Active devices.
    pub fn active_devices(&self) -> impl Iterator<Item = (&[u8; KEY_BYTES], &Device)> {
        self.devices.iter().filter(|(_, d)| !d.revoked)
    }

    /// Whether the device is recognized as active.
    pub fn is_active(&self, ed: &[u8; KEY_BYTES]) -> bool {
        self.devices.get(ed).is_some_and(|d| !d.revoked)
    }

    /// Whether the device is known at all, including a revoked one.
    pub fn is_known(&self, ed: &[u8; KEY_BYTES]) -> bool {
        self.devices.contains_key(ed)
    }
}

/// The sigchain (identity log).
///
/// State can be read from it **only** through [`Sigchain::verify`]; there is
/// deliberately no other way: a device list taken from an unverified
/// log is worse than no list.
#[derive(Debug, Clone)]
pub struct Sigchain {
    entries: Vec<Entry>,
}

impl Sigchain {
    /// Starts a log: the first entry is the birth of the identity, signed by itself.
    pub fn create(root: &Identity) -> Result<Self, SigchainError> {
        let body = EntryBody::Genesis {
            root: root.public(),
        };
        let mut chain = Self {
            entries: Vec::new(),
        };
        chain.push(root, body)?;
        Ok(chain)
    }

    /// Appends an entry. After the append the log must remain verifiable;
    /// otherwise the entry is not added at all.
    pub fn append(
        &mut self,
        signer: &impl ChainSigner,
        body: EntryBody,
    ) -> Result<(), SigchainError> {
        self.push(signer, body)
    }

    fn push(&mut self, signer: &impl ChainSigner, body: EntryBody) -> Result<(), SigchainError> {
        let seq = self.entries.len() as u64;
        let prev = match self.entries.last() {
            Some(last) => last.hash(),
            None => [0u8; HASH_BYTES],
        };
        let signer_key = signer.signer_key();
        let payload = Entry::signed_bytes(seq, &prev, &signer_key, &body);
        let signature = signer.sign_entry(&payload);

        self.entries.push(Entry {
            seq,
            prev,
            signer: signer_key,
            body,
            signature,
        });

        // Verify the whole thing: cheaper than repeating the rules in two places,
        // and it prevents creating a log that you yourself would not accept.
        match self.verify() {
            Ok(_) => Ok(()),
            Err(e) => {
                self.entries.pop();
                Err(e)
            }
        }
    }

    /// Verifies the whole log and restores the state.
    pub fn verify(&self) -> Result<ChainState, SigchainError> {
        let first = self.entries.first().ok_or(SigchainError::Empty)?;

        let EntryBody::Genesis { root } = &first.body else {
            return Err(SigchainError::MissingGenesis(first.seq));
        };
        let mut state = ChainState {
            root: root.clone(),
            devices: BTreeMap::new(),
        };
        let root_key = state.root.verifying_key().to_bytes();

        let mut expected_prev = [0u8; HASH_BYTES];

        for (index, entry) in self.entries.iter().enumerate() {
            let expected_seq = index as u64;
            if entry.seq != expected_seq {
                return Err(SigchainError::OutOfOrder {
                    index,
                    got: entry.seq,
                    expected: expected_seq,
                });
            }
            if entry.prev != expected_prev {
                return Err(SigchainError::BrokenLink(entry.seq));
            }

            // Who was entitled to sign at this point.
            let signer_allowed = entry.signer == root_key
                || match state.devices.get(&entry.signer) {
                    Some(device) if device.revoked => {
                        return Err(SigchainError::RevokedSigner(entry.seq))
                    }
                    Some(_) => true,
                    None => false,
                };
            if !signer_allowed {
                return Err(SigchainError::UnknownSigner(entry.seq));
            }

            verify_signature(entry)?;

            match &entry.body {
                EntryBody::Genesis { .. } => {
                    if index != 0 {
                        return Err(SigchainError::RepeatedGenesis(entry.seq));
                    }
                    if entry.signer != root_key {
                        return Err(SigchainError::UnknownSigner(entry.seq));
                    }
                }
                EntryBody::AddDevice { ed, curve } => {
                    // Re-adding is forbidden for revoked devices too: otherwise
                    // revocation would be reversible, and it must be final.
                    if state.devices.contains_key(ed) {
                        return Err(SigchainError::DuplicateDevice(entry.seq));
                    }
                    state.devices.insert(
                        *ed,
                        Device {
                            curve: *curve,
                            revoked: false,
                        },
                    );
                }
                EntryBody::RevokeDevice { ed } => match state.devices.get_mut(ed) {
                    None => return Err(SigchainError::UnknownDevice(entry.seq)),
                    Some(device) if device.revoked => {
                        return Err(SigchainError::AlreadyRevoked(entry.seq))
                    }
                    Some(device) => device.revoked = true,
                },
            }

            expected_prev = entry.hash();
        }

        Ok(state)
    }

    /// Serialization of the whole log.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for entry in &self.entries {
            entry.encode(&mut out);
        }
        out
    }

    /// Parses a log. Verification is done by [`Sigchain::verify`]; parsing confirms
    /// nothing.
    pub fn parse(bytes: &[u8]) -> Result<Self, SigchainError> {
        let mut entries = Vec::new();
        let mut at = 0usize;

        while at < bytes.len() {
            let index = entries.len();
            let seq_end = at + 8;
            let prev_end = seq_end + HASH_BYTES;
            let signer_end = prev_end + KEY_BYTES;
            let tag_end = signer_end + 1;

            let seq_bytes: [u8; 8] = bytes
                .get(at..seq_end)
                .and_then(|s| s.try_into().ok())
                .ok_or(SigchainError::Truncated(index))?;
            let prev: [u8; HASH_BYTES] = bytes
                .get(seq_end..prev_end)
                .and_then(|s| s.try_into().ok())
                .ok_or(SigchainError::Truncated(index))?;
            let signer: [u8; KEY_BYTES] = bytes
                .get(prev_end..signer_end)
                .and_then(|s| s.try_into().ok())
                .ok_or(SigchainError::Truncated(index))?;
            let tag = *bytes
                .get(signer_end..tag_end)
                .and_then(|s| s.first())
                .ok_or(SigchainError::Truncated(index))?;

            let (body, body_end) = match tag {
                TAG_GENESIS => {
                    let end = tag_end + PUBLIC_IDENTITY_BYTES;
                    let slice = bytes
                        .get(tag_end..end)
                        .ok_or(SigchainError::Truncated(index))?;
                    (
                        EntryBody::Genesis {
                            root: PublicIdentity::from_bytes(slice)?,
                        },
                        end,
                    )
                }
                TAG_ADD_DEVICE => {
                    let end = tag_end + KEY_BYTES * 2;
                    let ed: [u8; KEY_BYTES] = bytes
                        .get(tag_end..tag_end + KEY_BYTES)
                        .and_then(|s| s.try_into().ok())
                        .ok_or(SigchainError::Truncated(index))?;
                    let curve: [u8; KEY_BYTES] = bytes
                        .get(tag_end + KEY_BYTES..end)
                        .and_then(|s| s.try_into().ok())
                        .ok_or(SigchainError::Truncated(index))?;
                    (EntryBody::AddDevice { ed, curve }, end)
                }
                TAG_REVOKE_DEVICE => {
                    let end = tag_end + KEY_BYTES;
                    let ed: [u8; KEY_BYTES] = bytes
                        .get(tag_end..end)
                        .and_then(|s| s.try_into().ok())
                        .ok_or(SigchainError::Truncated(index))?;
                    (EntryBody::RevokeDevice { ed }, end)
                }
                other => return Err(SigchainError::UnknownTag(index, other)),
            };

            let signature_end = body_end + SIGNATURE_BYTES;
            let signature: [u8; SIGNATURE_BYTES] = bytes
                .get(body_end..signature_end)
                .and_then(|s| s.try_into().ok())
                .ok_or(SigchainError::Truncated(index))?;

            entries.push(Entry {
                seq: u64::from_be_bytes(seq_bytes),
                prev,
                signer,
                body,
                signature,
            });
            at = signature_end;
        }

        if entries.is_empty() {
            return Err(SigchainError::Empty);
        }
        Ok(Self { entries })
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Verification of an entry signature.
///
/// **Strict** Ed25519 verification is used. The ordinary one accepts signatures in
/// non-canonical encoding and small-order points: this barely affects strength, but
/// it means the same entry can have several differing signatures. In a log where an
/// entry is hashed together with its signature, this would turn into two different
/// "identical" chains.
fn verify_signature(entry: &Entry) -> Result<(), SigchainError> {
    let key = VerifyingKey::from_bytes(&entry.signer).map_err(|_| SigchainError::MalformedKey)?;
    let signature = Signature::from_bytes(&entry.signature);
    let payload = Entry::signed_bytes(entry.seq, &entry.prev, &entry.signer, &entry.body);

    key.verify_strict(&payload, &signature)
        .map_err(|_| SigchainError::BadSignature(entry.seq))
}

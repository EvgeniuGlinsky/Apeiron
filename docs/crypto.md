# Crypto core: what is where and why it is this way

A document about one-to-one messaging — stage 2. The archive layer, the relay and
access recovery are described in the research and the plan; here is only what
has already been written and verified.

The main rule, from which the rest follows: **we have no cryptographic
primitives of our own, and never will** (§18 of the research). We are
responsible not for the strength of the ciphers, but for the choice of
implementations, the boundaries between them, and making sure a mistake cannot
be made through inattention.

---

## 1. What it is built from

| What | With what | Where |
|---|---|---|
| Signing | Ed25519 (`ed25519-dalek` 3) | `core/src/identity.rs` |
| Key agreement | X25519 (`x25519-dalek` 3) | `core/src/identity.rs` |
| Double Ratchet | Olm (`vodozemac` 0.11) | `core/src/session.rs` |
| Key derivation | HKDF-SHA256 (`hkdf` 0.13) | `core/src/aead.rs` |
| Storage encryption | XChaCha20-Poly1305 (`chacha20poly1305` 0.11) | `core/src/aead.rs` |
| Master key storage | AES-256-GCM in AndroidKeyStore | `app/android/…/Vault.kt` |
| Local database | SQLite (`rusqlite` 0.40, bundled) | `store/` |
| Randomness | `getrandom` 0.4, directly from the OS | `core/src/random.rs` |

The versions are chosen **for a single generation**. Otherwise the binary ends
up with two independent implementations of X25519 and Ed25519 at once: twice as
much code next to the keys, twice the update obligations, and the eternal
question of which of them got the patch. `cargo deny check` catches the
divergence.

The correctness of the implementations is not taken on faith: the official
vectors of RFC 7748, RFC 5869 and RFC 8439 are run against the very crates we
use — `core/tests/rfc_vectors.rs`. The vectors are the same as in the reference
`radio-mesh-demo/s07_ratchet.py`, section A.

---

## 2. Identity and device are different things

**Identity** (`Identity`) is long-term, one per person: Ed25519 for signing plus
X25519 for key agreement. Its fingerprint is read aloud during verification.

**Device** is a specific phone with its own Olm account (`vodozemac::Account`)
and its own keys. There can be several devices; they appear and get lost.

What links them is a signature: the identity signs the device's keys. Therefore
a compromised phone is not a compromised identity — the device is revoked by an
entry in the sigchain, and the identity remains.

---

## 3. Prekey bundle

`core/src/prekey.rs`. This is what is needed to message first someone who is
not online right now.

The format is 224 bytes with a fixed layout:

```
identity (64) │ device Curve25519 key (32) │ device signing key (32)
              │ one-time key (32) │ identity signature (64)
```

What is signed is `apeiron/prekey-bundle/v1 ‖ identity ‖ three keys`. The
identity is necessarily part of the signature: otherwise a signature taken from
one bundle could be presented in a bundle with a different identity.

The encoding is our own, canonical, without `serde`. The reason is not saving
dependencies: **what is signed is a byte string**, and its form must be
unambiguous. A format that allows two representations of the same data means
two different signatures for the same meaning.

### Forgetting to verify the signature is impossible

Parsing yields an `UnverifiedPrekeyBundle`. A session can be built only from a
`PrekeyBundle`, and one can be obtained only by calling `verify()`. This is not a
convention that can be broken: code without the verification simply does not
compile.

Signature verification answers the question "do these keys belong to the
claimed identity". It **does not answer** the question "whose identity is this"
— see section 6.

---

## 4. Messaging and batch receive

`core/src/session.rs`. The ratchet is Olm, entirely someone else's. There is
one thing of ours here, but it is significant.

Olm keeps no more than **40** skipped-message keys per receiving chain
(`MAX_MESSAGE_KEYS`) and refuses gaps larger than **2000**
(`MAX_MESSAGE_GAP`). For Matrix that is enough: there the server delivers
messages roughly in sending order.

Not so for us. The architecture explicitly assumes delivery through a blind
relay with a waiting window of up to a day: a device was offline, then came
online and picked up its queue all at once, in arbitrary order. A hundred
messages in reverse order with naive decryption means sixty **permanently
lost**.

What saves us is that **the order can be read before decryption**: the ratchet
key and the chain index lie in the clear in the Olm message header.
`Chat::decrypt_batch` sorts the batch into chains, orders each by index and
decrypts in ascending order — no gaps arise at all.

A subtlety that is easy to get wrong: **session-establishment messages must not
be treated as a special case**. In Olm the initiator keeps sending them until it
receives a reply, and a one-sided batch consists entirely of them. Their chain
index lies in the nested message and is read the same way.

Verified on two hundred messages: with sorting, zero are lost; without it, more
than a hundred (`core/tests/ratchet.rs`, `batch_survives_reverse_order` and the
control `naive_reverse_order_loses_messages`).

What is actually lost is returned as an error, not swallowed.
`ChatError::is_lost_forever()` distinguishes "corrupted, try again" from "can
never be read anymore": showing the second as the first is not allowed, and
staying silent about the loss even less so.

---

## 5. Sigchain (identity log)

`core/src/sigchain.rs`. This is the project's "blockchain" — deliberately the
most boring one possible: no consensus network, no mining, no coin. One identity
keeps its own log; each entry is signed and references the hash of the previous
one.

Entries: identity birth, device addition, device revocation. Either the root
identity or an **active** device may sign — otherwise a second phone could only
be added from the first one, and after losing it, not at all.

Rules checked on every read:

* the first entry — and only the first — is identity birth;
* sequence numbers are consecutive, with no gaps;
* the reference to the previous entry's hash matches;
* the signer is the root or a non-revoked device;
* a device cannot be added twice, including **bringing it back after
  revocation**: revocation is irreversible, otherwise it means nothing;
* only a known and not yet revoked device can be revoked.

The hash reference is needed for exactly one thing: so that an entry cannot be
**removed**. A signature protects each entry individually, but a set of signed
entries can be presented incompletely — for example, hiding a device revocation.
With the chain, such a selection does not add up (`a_removed_entry_is_noticed`).

The state (the list of devices) is obtained **only** through `verify()`. There is
deliberately no other way: a list from an unverified log is worse than no list.

Signature verification is **strict** (`verify_strict`). The ordinary one permits
a non-canonical signature encoding, that is, several different valid signatures
for one message. Where the signature is part of the hash — and here it is — this
would turn into two different "identical" chains.

---

## 6. What cryptography does not do

Verification. At all.

The Double Ratchet provides security, the bundle signature ties keys to an
identity, the sigchain provides device history. None of this answers the
question of whom the identity belongs to. A man-in-the-middle who has
substituted the bundles of both sides gets a **fully working** conversation with
each of them: the signatures are valid, the encryption works, neither side
notices anything.

This is not an argument but an executable test
(`mitm_succeeds_without_fingerprint_check_and_fails_with_it`): the
man-in-the-middle reads the plaintext, and the cryptography is not broken
anywhere.

The only thing that gives him away is **a mismatch in the safety number**, read
aloud or compared in person. Hence the product requirement: a physical channel
(a QR code at a meeting, reading the digits out by voice) must be the only way to
add a contact, not a setting that can be skipped.

---

## 7. Storage

`core/src/aead.rs` holds the primitives, `store/` everything else. Details are in
`docs/storage.md`; here is only what matters when reading about the
cryptography.

`SecretKey` wipes itself on drop, subkeys are derived per purpose via HKDF with
a mandatory label, `seal`/`open` are XChaCha20-Poly1305.

XChaCha rather than ChaCha because of the nonce length: 192 bits versus 96. With
96 bits random nonces are dangerous and are customarily treated as a counter,
and a counter requires reliably persisting state between runs — on a phone that
gets switched off at an arbitrary moment. 192 bits allow the nonce to be random
with no state at all.

**Purpose labels are now a registry, not strings at the call site**
(`aead::purpose`). A typo in a string produced a different key, everything kept
working, and this would have been discovered only once data had already been
written under the wrong key.

The master key has a place to live. The device's hardware key (KEK; StrongBox,
or TEE when that is unavailable) wraps the database key (DEK), subkeys are
derived from the database key, and each record is encrypted separately and bound
to its location: the table name, the row number and the schema version are part
of authentication.

Two corrections to how this was written earlier — **R-010** in
`docs/threat-log.md`:

1. **The hardware attempt counter counts the system PIN, not ours.** So only
   half of R-002 is done: a copied data directory is useless, but a phone seized
   unlocked is not. The second half will be closed by a PIN mixed into the
   derivation of the database key, and until then this is exactly how it must
   be stated.
2. **Keystore is called from Kotlin, not from Rust.** The prohibition in the
   stage brief was about Dart, where wiping memory is impossible in principle;
   Kotlin is not Dart, and the key still never reaches Dart. In exchange, all
   the fiddling with method descriptors and parsing Java exceptions is gone, and
   Gradle checks the resulting code on every build.

A separate note on wording: for symmetric keys, attestation **does not exist** —
they have no certificate chain, and `KeyInfo` is a self-report by the framework
in our own process. That is why the screen says "The system reports:
StrongBox", not "key in StrongBox, verified".

## 8. What remains of stage 2

* ~~master key in the hardware store and a local DB under AEAD (R-002)~~ —
  half done: the hardware is there, the PIN is not, see R-010;
* PIN and a shuffled PIN pad (R-001, R-007) — which is also the second half of
  R-002;
* the chat screen and the mandatory verification screen;
* moving the core behind the FFI boundary so that plaintext does not reach Dart
  (R-004) — for now only the identity is exposed in the bridge.

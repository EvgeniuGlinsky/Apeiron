# Storage: what is where, what is protected and what leaks

A document about how state survives a restart and what the PIN protects —
decision **R-002** with the corrections from **R-010**, and **R-011**, the PIN
(`docs/threat-log.md`).

Before storage existed, every launch created a new identity and a new device:
the fingerprint read aloud during verification was different after a restart,
and all conversations broke. That made the chat screen, the PIN and verification
itself pointless — all of them are built on top of storage.

---

## 1. Keys

```
HMAC key — HMAC-SHA256 in AndroidKeyStore, alias apeiron.kek.v2, non-exportable
           setIsStrongBoxBacked(true) → fallback to TEE
           setUnlockedDeviceRequired(true)
  PIN ──Argon2id──▶ x0 ──k × HMAC in the hardware──▶ xk ──HKDF──▶ W
  W seals → DEK, the database key: 32 random bytes, file vault.bin
       └ derive(purpose) → subkeys
            apeiron/storage/identity/v1    identity secret
            apeiron/storage/account/v1     this device's Olm account
            apeiron/storage/session/v1     ratchet state
            apeiron/storage/sigchain/v1    sigchain (identity log)
            apeiron/storage/contact/v1     contacts
            apeiron/storage/meta/v1        service data
            apeiron/storage/tag/v1         lookup tags
```

Why the PIN goes **into** the hardware on every attempt, and not next to it,
is R-011: a secret the hardware unwraps once gives someone with root all PINs
offline; a chain of hardware operations per guess does not.

Still two levels: the PIN seals the database key, and the subkeys come from the
database key. Changing the PIN or the hardware key later (for example
`setUserAuthenticationParameters` at stage 6) means re-sealing thirty-two bytes,
not re-encrypting every conversation — that is, in practice, "it will never be
done".

The same technique gives **cryptographic erasure** (R-005): destroy the alias and
`vault.bin`, and the database turns into noise. With the limit R-005 now names:
an image of the keystore's own files taken earlier still holds the key blob,
because Android does not let an app ask for rollback-resistant keys.

Purpose labels are kept as a registry in `core/src/aead.rs` (`purpose`), not as
strings at the call site. A typo in a string gives **a different key**,
everything keeps working, and this is discovered only once data has already been
written under the wrong key. `apeiron/storage/pin-wrap/v1` joined it with the PIN.

### No migration from builds before the PIN

Builds before the PIN wrapped the database key with an AES key
(`apeiron.kek.v1`, `kdf_id = 1`). Such data existed only on test phones, and it is
**not** carried over. The migration would have been the riskiest code in the
vault: unwrap with the old key, create the new one, re-seal, and delete the old
key only once the new wrapper is durably on disk — where "durably" on f2fs
depends on checkpoints, and deleting the key too early loses everything. All of
that for one run on test data.

Instead the app recognises `kdf_id = 1` and says so, and offers to start over.
It never starts over on its own; `destroy()` removes the legacy alias together
with the new one. The migration order written here before is kept for the day a
real change of the hardware key comes — then it is written together with its
test, as this document demanded.

---

## 2. Wrapper format (`vault.bin`)

```
"APVLT1" (6) ‖ kdf_id = 2 (1) ‖ level at creation (1) ‖ rounds k (4, BE)
  ‖ Argon2id KiB (4, BE) ‖ Argon2id passes (4, BE) ‖ Argon2id lanes (1)
  ‖ salt (16) ‖ key check value (16)
  ‖ XChaCha20-Poly1305(W, aad = the whole header above, DEK)
```

The whole header is the authenticated data of the sealed key: a tweaked level
byte, a smaller `k` or a different salt all make opening fail. This became
possible with the PIN: the AEAD is our own now, and the question of `updateAAD`
in a particular StrongBox is gone.

Bounds are checked **before** anything is computed or counted: `0 < k ≤ 20 000`,
Argon2id memory up to 256 MiB, up to 10 passes, one lane. A planted header must
not be able to hang the app or make it allocate gigabytes.

The key check value is `HMAC_hw("apeiron/key-check/v1")[..16]`. It does not
depend on the PIN and leaks nothing about it. It exists so that a key that is
present but not the one — reissued by firmware, restored from somewhere — is
reported as such instead of as a wrong PIN.

`k` is calibrated when the PIN is set: sixteen operations are discarded (cold
keystore, JIT), three batches of 64 are timed, the **fastest** is taken — a slow
moment would give a short chain, the direction that must not happen — and `k`
is chosen for about 0.7 s of chain, but never below 128 on a TEE (4 on
StrongBox, which is tens of times slower).

Before the new wrapper is used, it is read back from disk and opened again with
the same PIN. A chain that is not reproducible would otherwise lock the owner
out of everything written from then on.

The write is atomic: temporary file (`vault.bin.tmp`) → flush to disk → rename →
flush the directory.

### The attempt counter (`pin.state`)

```
"APPIN1" (6) ‖ in a row (4) ‖ total (4) ‖ has anchor (1) ‖ BOOT_COUNT (8)
  ‖ elapsedRealtime (8) ‖ writer mark (8)
```

Not in the database — the database cannot be opened without the PIN. Written
**before** the chain is asked, so that cutting power at the verdict does not
leave the attempt uncounted; put back only if the hardware failed before
producing a result. A missing or unreadable counter while a vault exists means
the first delay step, not zero. Against root the file is worthless and
`docs/threat-log.md` (R-011) says so.

---

## 3. Database

SQLite via `rusqlite` with the `bundled` feature. The reason is not convenient
queries: the ratchet state and the message record must reach the disk **in a
single transaction**. A divergence between them means messages that can never
be read, because the ratchet has moved forward and there is nothing left to read
the skipped ones with.

**We do not use SQLCipher.** We encrypt ourselves, per record, with our own
`aead::seal` — the same XChaCha20-Poly1305 that passed the official RFC 8439
vectors. This way the crypto stack stays within one generation (`cargo deny`
watches over that), and a record gains a **location**.

### Binding a record to its location

The additional authenticated data of each record:

```
b"apeiron/storage/record/v1" ‖ table tag (1) ‖ row number (8, BE) ‖ schema version (2, BE)
```

After this, moving a row to someone else's identifier, slipping it in from
another table or rolling back the schema will not work: authentication will not
check out, and the result is a refusal, not different content. Encrypting the
whole file does not give this.

The table tag numbers are fixed forever. New ones can be added, existing ones
cannot be changed: changing a number makes everything written under the old one
unreadable.

### Lookup without disclosure

A contact is **not** looked up by its public key in the clear. Otherwise the list
of correspondents could be read straight from the database file, without any key
— which is exactly what the database was supposed to close off.

Instead, the lookup tag `HKDF(tag key, "apeiron/lookup/v1" ‖ value)[0..16]` is
used: deterministic, suitable for a unique index, and meaningless without the
database key. The tag key is derived under a separate purpose and is unrelated
to the decryption keys — the same technique as the separate `K_addr` branch in
the research (§16.2): knowing the address brings you no closer to the content.

### Schema v1

`meta`, `identity`, `device_account`, `sigchain`, `contacts`, `sessions`,
`schema_version`. In every table only service numbers and lookup tags are stored
in the clear; everything else is sealed bytes.

There is no message table here yet: it will come with the chat screen. The
migration mechanism, however, has been built right away, and the rule for it is:
**every migration brings its own test proving that the data survived the
transition.** Retroactively there is nothing to base such a test on.

Rolling the app back onto a database written by a newer schema is forbidden: the
old code would read the new records wrongly, and silently. On top of that, the
schema version is part of the authentication of every record, so "wrongly" here
means "not at all".

`PRAGMA synchronous = FULL`, `journal_mode = WAL`: phones get switched off at an
arbitrary moment.

---

## 4. What leaks from here

Honestly and completely:

* **the number of records** — how many contacts, sessions and service rows are in
  the database;
* **record sizes** — the ciphertext length reveals the plaintext length up to the
  tag and the nonce;
* **file modification times** — when the app was used;
* **the very fact** that the app is installed, and the package name.

What does not leak: the content of conversations, the identity secret, the
correspondents' public keys, their names, the ratchet state, the sigchain.

Padding sizes to a uniform length is not done here. On the delivery layer it is
mandatory (stage 3, fixed-size envelopes), but in the local database an
adversary who has the file has already bypassed everything else — there is no
point paying for this with a constant cost in space.

---

## 5. Where things live in the code

| What | Where | `unsafe` |
|---|---|---|
| Primitives, AEAD, key derivation | `core/` | `forbid` |
| Hierarchy, schema, record sealing | `store/` | `forbid` |
| One JNI call | `platform/` | `deny`, exemption on one module |
| Keystore work | `app/android/.../Vault.kt` | — |

`apeiron-store` **is tested in full on the development machine** against a test
vault key source. The fake lives behind the `testing` feature, which is not
enabled anywhere in the app build, and its getting into an Android build is a
compile error, not an APK with a software key inside.

---

## 6. When the key disappears

The state "the wrapper is on disk, but the key is not in the secure module" is
not an edge case but an ordinary one:

| Event | Alias | `vault.bin` |
|---|---|---|
| uninstalling the app, "clear data" | disappears | disappears |
| an update signed with the same key | survives | survives |
| removing the screen lock | **may disappear** | survives |
| a backup, transfer to a new phone | **not transferred** | transferred |
| a firmware update on some devices | **sometimes disappears** | survives |

That is why creating a key is allowed **only** when there is no wrapper yet. If
the wrapper exists but the key does not, that is "key gone", and the app says so
directly instead of creating a new key and destroying the conversations.

With the PIN there is a fourth way to the same outcome, and it is deterministic:
the key is present but its check value does not match the header — a key that is
not the one. It gets its own state, "the key is not the one", so that it never
reads as a wrong PIN and never costs an attempt.

Backup is disabled in two ways at once — `allowBackup="false"` (works on API
28–30) and `dataExtractionRules` excluding both cloud backup and device-to-device
transfer (read on 31+). Some firmware ignores this anyway, so the "key gone"
screen is mandatory regardless.

This also has a consequence for the plan: the entry "key loss = loss of the
entire history — the most frequent real threat to an ordinary user" is
confirmed, and the recovery scheme (stage 6, Shamir 3 of 5) stops being an
embellishment at the end.

### About Direct Boot mode — for stage 3

It is safe now: the files live in storage encrypted with the user's credential
key and there are no components with `directBootAware`. The release manifest
requests the network since the DHT measurement (R-012), but nothing runs before
the first unlock. The app simply does not work until the phone has been unlocked
at least once after boot.

Stage 3 will bring receiving push notifications, and with it a window in which
code runs before the first unlock. There `setUnlockedDeviceRequired` will fail
without exception — and that will be correct behavior, not a malfunction. This
is written down in advance so that it will not have to be discovered on the
device then.

---

## 7. Memory wiping: what can honestly be said

With the PIN the database key no longer passes through Kotlin at all: Kotlin
returns the end of the HMAC chain, and the key is derived and opened in Rust. What
does pass through Kotlin is `x0` (the Argon2id output) and the chain's values.
They are wiped as the loop goes and, after copying, on the Rust side — but
**this narrows the window rather than closing it**: the ART garbage collector
moves objects, the returned array stays on the heap until collected, and `Mac`
has its own buffers that cannot be reached. Someone who reads `x0` from the
Java heap can test PINs against the next hardware output, so the window matters.

The PIN itself is never a string anywhere: Rust assembles it from tapped
positions of a layout it drew, keeps the digits in a wiped buffer, and compares
the two entries of a new PIN itself.

Writing "wiped" would be untrue. The same argument is the basis of R-004 — why
secrets are not handed to Dart — and holding Java to a weaker standard would be
inconsistent. The difference between Java and Dart lies elsewhere: in Dart
wiping is impossible **in principle**, while here it is possible and done, just
without a guarantee.

---

## 8. On-device self-check

`store/src/selfcheck.rs`. It exists because there is only one on-device check,
on a live phone, and it must answer every question at once.

The main line of the report is a real pair of Olm accounts: a message encrypted
during the previous launch is decrypted during this one. What is checked is the
ratchet itself, not "the file is in place".

On the first launch such lines are marked **not passed**: on the first launch it
is impossible to prove that state survived a restart. A green check mark where
nothing was checked is exactly the false confidence that all this was set up to
fight.

The self-check does nothing destructive. The tampering checks work on in-memory
copies, and erasure is never called: it requires a separate deliberate action.

Survival of state can be credited only if the marker was written by **a
different process**. Each process has its own random token, and it is written
next to the marker: opening the check screen a second time in the same session is
not a restart, and it earns no green check mark. A random number rather than a
process ID: the system reuses those, and a collision would give a false answer in
exactly the direction that must not happen.

The PIN adds lines that only the phone can answer: the parameters of the vault,
the time of this unlock, one hardware operation (the fastest of three batches),
whether the hardware serves several operations at once, the resulting estimate
of guessing with root, a random wrong PIN rejected (born inside, not counted — not
an oracle), whether the attempt counter survived a restart, and whether the boot
clock is readable.

### What was verified on a live device and what was not

Before the PIN, passed on a phone with **TEE**: the StrongBox attempt with fallback, honest
naming of the level, survival of the identity, the device account and the
ratchet state across a real process restart, locking and unlocking, binding a
record to its location with production keys.

Not verified on live hardware, and this is stated honestly:

* **the StrongBox path** — no device with it was available. The fallback code has
  been exercised, the code of the successful StrongBox branch has not;
* `setUnlockedDeviceRequired` on a locked phone: this case cannot be triggered
  from inside the app;
* the "key gone" branch: it can only be induced by destroying the owner's data.

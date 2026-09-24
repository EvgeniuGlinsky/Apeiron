# Storage: what is where, what is protected and what leaks

A document about how state survives a restart — decision **R-002** with the
corrections from **R-010** (`docs/threat-log.md`).

Before storage existed, every launch created a new identity and a new device:
the fingerprint read aloud during verification was different after a restart,
and all conversations broke. That made the chat screen, the PIN and verification
itself pointless — all of them are built on top of storage.

---

## 1. Two levels of keys

```
KEK  — AES-256-GCM in AndroidKeyStore, alias apeiron.kek.v1, non-exportable
       setIsStrongBoxBacked(true) → fallback to TEE
       setUnlockedDeviceRequired(true)
       setRandomizedEncryptionRequired(true)
  └ wraps → DEK, the database key: 32 random bytes, file vault.bin
       └ derive(purpose) → subkeys
            apeiron/storage/identity/v1    identity secret
            apeiron/storage/account/v1     this device's Olm account
            apeiron/storage/session/v1     ratchet state
            apeiron/storage/sigchain/v1    sigchain (identity log)
            apeiron/storage/contact/v1     contacts
            apeiron/storage/meta/v1        service data
            apeiron/storage/tag/v1         lookup tags
```

Two levels rather than one, for a single reason: **Keystore key parameters
cannot be changed after creation.** When the PIN arrives (R-001),
`apeiron.kek.v1` will have to be discarded and a `v2` created. With two levels
this means re-wrapping thirty-two bytes; with one, re-encrypting all
conversations — that is, in practice, "it will never be done".

The same technique gives **cryptographic erasure** (R-005) for free: destroy the
alias and `vault.bin`, and the database turns into noise, even if a copy has
already been taken.

Purpose labels are kept as a registry in `core/src/aead.rs` (`purpose`), not as
strings at the call site. A typo in a string gives **a different key**,
everything keeps working, and this is discovered only once data has already been
written under the wrong key.

### Alias migration v1 → v2

The order is non-negotiable, and it is written down here in advance because it
will have to be executed on live data:

1. unwrap the database key via `v1`;
2. create `v2` with the new access conditions;
3. wrap the same database key under `v2`;
4. **atomically** write the new wrapper and flush it to disk;
5. **and only then** `deleteEntry("apeiron.kek.v1")`.

A crash in the middle with any other order means losing everything.

**So far this order is only written down, not programmed.** The second alias
will not exist until there is a PIN, and writing the migration "just in case"
would mean writing untestable code next to a key. What has been done now is the
place for it: the `kdf_id` field in the wrapper format and a versioned alias
name. What must be done together with the PIN is the migration itself **and a
test that pins down the order of the steps**, because it will have to be checked
on the owner's live data.

---

## 2. Wrapper format (`vault.bin`)

```
"APVLT1" (6) ‖ kdf_id (1) ‖ level at creation (1) ‖ sealed
sealed = AES-GCM(KEK, kdf_id ‖ level ‖ database key)
```

The header is repeated **inside** the sealed text. After opening, the recovered
values are compared with those in the file; a mismatch means refusal. Otherwise
a tweaked level byte would change the label on screen in a reassuring
direction, without touching anything else.

Doing the same through additional authenticated data on the Keystore side would
be more natural, but the behavior of `updateAAD` in a specific StrongBox
implementation cannot be checked on the development machine, and there is no
point spending the single round with the phone on it. AES-256-GCM is guaranteed
to be available in StrongBox; there is no such certainty about AAD.

`kdf_id` is a slot for the PIN: `1` means "the database key is the KEK output as
is", `2` will appear together with mixing in the PIN.

The nonce is issued by Keystore: the key has
`setRandomizedEncryptionRequired` enabled, and supplying our own IV on
encryption is forbidden. This is a decision, not a default — it directly closes
off nonce reuse, which is what CVE-2021-25444 in Samsung's keymaster is built
on.

The write is atomic: temporary file → flush to disk → rename → flush the
directory.

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
key, there are no components with `directBootAware`, and the release manifest
does not even have the internet permission. The app simply does not work until
the phone has been unlocked at least once after boot.

Stage 3 will bring receiving push notifications, and with it a window in which
code runs before the first unlock. There `setUnlockedDeviceRequired` will fail
without exception — and that will be correct behavior, not a malfunction. This
is written down in advance so that it will not have to be discovered on the
device then.

---

## 7. Memory wiping: what can honestly be said

The database key arrives from Kotlin as a byte array. The array is wiped both on
the Kotlin side and, after copying, on the Rust side — but **this narrows the
window rather than closing it**: the ART garbage collector moves objects, and
`Cipher` has its own intermediate buffers that cannot be reached.

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

### What was verified on a live device and what was not

Passed on a phone with **TEE**: the StrongBox attempt with fallback, honest
naming of the level, survival of the identity, the device account and the
ratchet state across a real process restart, locking and unlocking, binding a
record to its location with production keys.

Not verified on live hardware, and this is stated honestly:

* **the StrongBox path** — no device with it was available. The fallback code has
  been exercised, the code of the successful StrongBox branch has not;
* `setUnlockedDeviceRequired` on a locked phone: this case cannot be triggered
  from inside the app;
* the "key gone" branch: it can only be induced by destroying the owner's data.

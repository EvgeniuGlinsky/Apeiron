# Threat and decision log

A living document. Every protection idea is recorded here and tagged by adversary.

**Rule:** an idea without a named adversary is not accepted. Twenty features of which three work
are worse than three that work — the other seventeen are extra code where bugs live, and false
confidence for the user who relies on them.

---

## Adversary table

P1–P7 are taken from §6 of the research, where they appear as П1–П7 (the research is in Russian).
P8 was added by us — it is not in the research, because that was about storage, but for the stated
goal ("protection against state-level arbitrariness") it is decisive.

| # | Adversary | Capabilities |
|---|---|---|
| **P1** | Service operator | Sees metadata, can delete data, can be coerced |
| **P2** | Thief / device seizure | Physical access to the phone, unlocked or not |
| **P3** | Curious network participant | An ordinary client; stores other people's fragments, observes its own connections |
| **P4** | Sybil adversary | Spins up thousands of nodes for little money |
| **P5** | Telecom operator / ISP | Sees one participant's traffic |
| **P6** | Global passive observer | Sees all network traffic without interfering |
| **P7** | State coercion | Legally compels handing over data or building in access |
| **P8** | **Compromised endpoint** | Pegasus class: reads the screen before encryption and after decryption |

**What the architecture gives by default** (§6 of the research): a win against P1 and P7 — this is
the project's only genuine motive. Against P2–P6 — a loss relative to a centralized E2EE
messenger, and against P2 a catastrophic one. Against P8 nothing that lives inside the app helps.

---

## Entry format

```
### R-NNN. Title
- **Works against:**
- **Does NOT work against (important to spell out):**
- **Verdict:** accepted / accepted with changes / deferred / rejected
- **Rationale:**
- **Cost:**
```

---

## Entries

### R-001. PIN on every return to the app
- **Works against:** P2 — a phone seized **unlocked**. This is the most common real seizure
  scenario.
- **Does NOT work against:** P8 (while the app is open, the data is decrypted), P1, P5–P7.
- **Verdict:** **accepted.**
- **Rationale:** cheap to implement, closes a widespread and underestimated threat.
- **Cost:** UX friction. Requires R-007 as a companion. On the desktop it is not read
  literally — see R-008.

### R-002. Encrypting the local database with a key derived from the PIN
- **Works against:** P2 — an extracted storage image.
- **Does NOT work against:** P8. And **in its original form it does not work against P2 either**.
- **Verdict:** **accepted with changes.**
- **Rationale:** six digits are 10⁶ options, and an adversary with a storage image brute-forces
  **offline**, without our delays and counters. Even with Argon2id at 512 MB (~1 s per attempt on a
  phone), a GPU rig goes through the whole range in minutes to hours. A PIN by itself cannot be a
  key.
  **Change:** the master key lives in the hardware store (Android StrongBox / TEE) with
  `setUserAuthenticationRequired`; the attempt counter is built into the hardware and cannot be
  bypassed by a dump. The PIN unlocks the key inside the hardware module rather than generating it.
- **Cost:** binding to the device. Transfer to a new phone only via the recovery scheme
  (stage 6 of the plan). On devices without StrongBox — degradation to TEE, and where even that is
  missing — an honest warning to the user, not a silent fallback to a weak scheme.

### R-003. Different PINs → different accounts (plausible deniability)
- **Works against:** P2 in the everyday sense — a curious relative, a thief, a superficial
  inspection.
- **Does NOT work against:** **P7, and this is the main point.** Also P1, P5, P6, P8.
- **Verdict:** **deferred; not to be implemented as protection against P7 under any wording.**
- **Rationale:** plausible deniability breaks down not because the hidden account is found, but
  because its very possibility becomes known. Specifically:
  1. An analyst decompiles the APK and sees multi-profile code — now it is known that a second PIN
     may exist;
  2. the data file weighs 2 GB, while the account shown accounts for 50 MB;
  3. the flash controller decides for itself where to write (wear leveling) — two images taken some
     time apart will show changes in blocks that, according to the owner, are empty;
  4. traffic at the relay is larger than the visible account explains.

  The situation "I cannot decrypt this" turns into "he is hiding something and refuses to
  cooperate". In Britain this is an explicit offense: RIPA Part III, refusal to disclose a key under
  a notice — up to 2 years, up to 5 in national security cases. In the US, compelled decryption is
  not settled and is decided differently from case to case. Jurisdiction determines everything; a
  lawyer is needed, not a developer's opinion.
  Twenty years of literature on TrueCrypt/VeraCrypt hidden volumes are about exactly this.
- **Cost:** if it is done — only with an honest label in the interface saying that this protects
  against everyday curiosity, not against an investigation.

### R-004. Decrypted text lives only in Rust
- **Works against:** P2 — a RAM dump from a locked device.
- **Does NOT work against:** P8.
- **Verdict:** **accepted. An architectural decision; it changes the core/UI boundary.**
- **Rationale:** the claim "if they dig into memory, without the PIN they get nothing" is true
  only if keys and plaintext in RAM are actually wiped on lock. This does not happen by itself.
  Rust has `zeroize`; **in Dart this is impossible in principle** — the garbage collector copies
  objects when compacting the heap and gives no guarantee that the previous copy of a string has
  been wiped.
  Consequence: only what is on screen right now goes to Dart, and it is wiped on lock. Storage,
  keys, history, ratchet state — do not leave Rust.
- **Cost:** more code at the FFI boundary, finer call granularity. Settling this in week 15 would
  have meant a rewrite, so the decision was made now.

### R-005. Cryptographic erasure instead of hiding
- **Works against:** P1, P2, P3, P7.
- **Does NOT work against:** P8.
- **Verdict:** **accepted** (already built in, §12.2 and §16.2 of the research).
- **Rationale:** a separate key per block; once `K_blk` is destroyed, the block is unrecoverable,
  even if all 20 fragments are intact in the adversary's hands. Stronger than a hidden account,
  because there is nothing to demand: the data does not exist, rather than being hidden. This is
  also the path to GDPR compliance with undeletable distributed storage.
- **Cost:** zero; the mechanism is needed anyway for fragment lifetimes.
- **A limit on the phone, said plainly:** Android does not let an app request
  rollback-resistant keys. Deleting a keystore alias makes the key unusable from now on, but an
  image of the keystore's own files taken **earlier** still holds the key blob. On the phone,
  "the key is destroyed" means "from now on", not "retroactively".

### R-006. Short retention / auto-delete per chat
- **Works against:** P2, P3, P7.
- **Does NOT work against:** P5, P6, P8.
- **Verdict:** **accepted.**
- **Rationale:** what does not exist cannot be extracted. The cheapest protection on the list.
- **Cost:** product-level — users do not like losing history. A per-chat setting, not a global
  one.

### R-007. Shuffled digit layout on the PIN pad
- **Works against:** P2 — shoulder surfing, surveillance cameras.
- **Does NOT work against:** everything else.
- **Verdict:** **accepted.** A necessary companion to R-001: frequent PIN entry in public creates,
  by itself, a threat that did not exist before.
- **Cost:** minor inconvenience, slower entry.

### R-008. On the desktop, R-001 triggers on inactivity, not on focus
- **Works against:** P2 — a computer left with the app open.
- **Does NOT work against:** P8, P1, P5–P7 — same as R-001.
- **Verdict:** **accepted.** On the desktop, R-001 read literally means "loss of focus", and that
  is a porting mistake: there, windows are switched dozens of times an hour.
- **Rationale:** a rule that gets in the way of work gets disabled — and then there is no
  protection at all. The event R-001 catches on the phone is "the device has left the owner's
  hands"; on the phone it coincides with the app going to the background (and with the screen
  turning off, which sends the app to the background), but on the desktop it does not. The
  equivalent there is "stepped away from the computer", and it is measured by inactivity: 3 minutes
  without input, plus immediately when the window is minimized or on exit.
  The logic lives in `app/lib/lock_policy.dart`, separate from the widget, and is covered by tests:
  an identity left unlocked looks exactly the same as one locked on time, and a mistake here cannot
  be seen by eye.
- **Cost:** on the desktop there is a window of up to 3 minutes between the person leaving and the
  lock — against P2 entering the room right away this does not work. The timeout is stated in the
  interface directly, not left unsaid.

### R-009. We take the Double Ratchet ready-made: vodozemac
- **Works against:** P1, P5, P6 — the content of conversations is closed to everyone except the
  other party, with forward secrecy.
- **Does NOT work against:** P2 (while the app is open), P8, and — more importantly —
  **it does not work against P1/P5/P6 without key verification**, see below.
- **Verdict:** **accepted**, version pinned exactly (`=0.11.0`).
- **Rationale:** we do not write our own primitives (§18). Of the ready-made Olm implementations,
  vodozemac is the only one in Rust that has passed an independent audit (Least Authority,
  March 2022, 10 findings, 8 fixed during the audit). We do not take MLS at the start:
  `openmls` requires an agreed order of commits, and in P2P that complexity is shifted onto the
  clients.

  **Known findings and what about them.** In February 2026 an analysis with seven points was
  published; the maintainers responded publicly. The essentials:

  | Finding | Status |
  |---|---|
  | Non-contributory Diffie-Hellman: zero public keys were accepted, yielding a predictable all-zero shared secret | **fixed in 0.10.0**: `diffie_hellman()` returns an `Option`, and a `NonContributoryKey` error was added. Checked by our test `zero_public_key_is_rejected`, not taken on faith |
  | Version downgrade and MACs truncated to 64 bits in V2 | does not concern us: V2 is not standardized and in 0.10.0 was moved behind an experimental feature flag. We are explicitly on V1 |
  | Non-strict Ed25519 signature verification | **fixed in 0.10.0**: strict verification became the default behavior. We have also switched to `verify_strict` ourselves |
  | No more than 40 skipped-message keys per chain | not fixed and will not be. Significant for our architecture; worked around by sorting the batch into order before decryption — `docs/crypto.md`, section 4 |
  | Deterministic IV in the pickle format | not used: we serialize state with our own AEAD |
  | CheckCode entropy in ECIES | not used |

- **Cost:** dependence on someone else's pace of fixes. Hence the exact version pin (an update
  is a deliberate action that involves reading the changelog), `cargo deny check` in CI, and our
  own tests for the properties we rely on. Written down separately: **cryptography does not replace
  key verification**. A man-in-the-middle who has substituted the bundles of both sides gets a fully
  working conversation with each — this is checked by a test, not stated as a caveat in the
  documentation.


### R-010. R-002 as written is unachievable, and Keystore work is done from Kotlin
- **Works against:** P2 — an extracted storage image, a copied data directory, a cloud
  backup, transfer to another phone.
- **Does NOT work against:** **P2 in the "phone seized unlocked" scenario** — that is, against
  the most common seizure case. Also P1, P5–P8.
- **Verdict:** **accepted**, both corrections.

#### Correction one: the hardware counter does not count our PIN

R-002 is written like this: "the master key lives in the hardware store with
`setUserAuthenticationRequired`… **The PIN unlocks the key inside the hardware
module**". That will not work, and this is not an implementation detail.
`setUserAuthenticationRequired` binds the key to the *device credential* — the
system screen lock. The hardware counter counts attempts at entering the
**system** PIN, not ours. Android does not allow making the hardware count
attempts at our six-digit PIN: there is no such API.

The correct construction splits in two, and both halves do what R-002 was
written for:

1. **The hardware** holds a non-exportable key. It cannot be extracted, so
   offline brute force is impossible.
2. **Our PIN** (R-001, the next task) is mixed into the derivation of the
   database key. Brute-forcing the PIN will require the hardware key, that is,
   **the presence of this very phone for every attempt**.

The first half is done now. Until the second one, this is exactly how it must be
stated: a copied data directory is useless, but a phone seized unlocked is not.

**Superseded by R-011.** Point 2, read literally, does not deliver its own promise:
mixing the PIN in *after* the hardware has unwrapped a secret gives that secret to
anyone with root in one call. The PIN has to go into a hardware operation on every
attempt; R-011 does that, and the AES key of this decision is gone.

`setUserAuthenticationRequired` **is not enabled**, and not because it is hard.
It adds one more — guaranteed — item to the list of ways to lose everything:
changing or removing the screen lock, after which the key is irreversibly
invalidated. Keystore keys go missing in the field as it is — according to years
of developer complaints, after firmware updates on some devices. Until the
recovery scheme exists (stage 6), adding one more such way is not acceptable.

An intermediate option remains on the table:
`setUserAuthenticationParameters(timeout > 0, AUTH_DEVICE_CREDENTIAL)` gives a
hardware attempt counter without a single line of UI. That is why the alias
migration `v1 → v2` was designed right away: Keystore key parameters do not
change after creation, and the transition will have to be made on live data.

#### Correction two: Keystore is called from Kotlin, not from Rust

The stage brief prescribed going to Keystore "via JNI directly from Rust, not
through a Dart plugin — otherwise the key material will pass through Dart and
violate R-004". **The stated reason is correct; the conclusion drawn from it is
not.** The reason for the prohibition is Dart, where wiping memory is impossible
in principle. Kotlin is not Dart: the key travels Keystore → Kotlin → JNI → Rust
and never reaches Dart.

The cost of the difference is large. On the Rust side, working with Keystore
comes with hand-assembled method descriptors; building a `String[]` via
`NewObjectArray`; the local reference table, where an attached native thread is
guaranteed sixteen slots while the key-creation sequence needs about fifty;
checking for an exception after every call, because an unchecked exception kills
the process when the thread detaches; and telling Keystore failure types apart
by comparing strings with class names. All of this is code under
`cfg(target_os = "android")`, which does not compile on the development machine
at all.

In Kotlin the same thing is a `try/catch` with real types, and Gradle checks the
file on every build: a wrong method name becomes a compile error, not a crash on
the phone.

- **Cost:** the rule "no Java classes of our own" is revoked. The class
  `io.apeiron.apeiron.Vault` is called from Rust by name, so it is protected from
  renaming by a rule in `proguard-rules.pro`, and the presence of its symbol in
  the finished library is checked by a build guard.

#### What this implies for the interface

A hardware failure is a **state**, not an error. "Key gone" leads to a screen
where the person makes a decision, not to a red line with a Java class name. And
that screen can be reached **only** from three explicit conditions:
`containsAlias` returned false, `getKey` returned null,
`KeyPermanentlyInvalidatedException`. Everything else is "retry".

The reason is not theoretical: `setUnlockedDeviceRequired` fails on an
**unlocked** device if it was unlocked with weak biometrics — a confirmed
firmware defect. Interpreting that as "key lost" and reissuing the key would
mean destroying the owner's conversations over a transient failure. This is
closed off by the test `transient_failure_never_maps_to_gone`, not by
carefulness.

And one more thing about wording: for symmetric keys attestation does not exist;
`KeyInfo` is a self-report by the framework in our own process. That is why the
interface says "The system reports: StrongBox", not "key in StrongBox,
verified".

### R-011. The PIN goes into the hardware on every attempt
- **Works against:** P2 — a phone seized **unlocked**, including by someone with root on it
  (forensic tooling, that is, P7 in practice): the vault does not open without the PIN, and every
  guess costs `k` sequential operations in the secure hardware of this very phone. Also everything
  R-010 already covered: a copied data directory, a backup, transfer to another phone.
- **Does NOT work against:** P8 (while the app is open, the data is decrypted). Against extraction
  of the key from the secure hardware it holds only as far as Argon2id and the length of the PIN
  do. P1, P5–P7 otherwise.
- **Verdict:** **accepted**, and implemented. It replaces the second half of R-010 as it was
  written there.
- **Rationale.** R-010 planned to "mix the PIN into the derivation of the database key". Read
  literally — unwrap with the hardware key, then combine with the PIN in software — that does not
  deliver what it promises: someone with root makes **one** call to the keystore as the app, gets
  the unwrapped secret, and tries all 10⁶ six-digit PINs offline in milliseconds, or in minutes
  with Argon2id. "The phone is present for every attempt" holds only if the PIN goes **into** a
  hardware operation on every attempt. Hence:

  ```
  x0 = Argon2id(pin, salt; 64 MiB, 2 passes)       on the CPU
  xk = HMAC_hw(… HMAC_hw(x0) …), k rounds           non-exportable key, one operation per round
  W  = HKDF(xk, "apeiron/storage/pin-wrap/v1")
  vault.bin = header ‖ XChaCha20-Poly1305(W, aad = header, DEK)
  ```

  The chain is sequential: one guess cannot be parallelised and cannot be moved to other hardware.
  `k` is calibrated on the phone so that the chain costs about 0.7 s. The same idea as a passcode
  entangled with a device UID key, minus the hardware-enforced delays that Android does not offer
  to apps (see R-010: the hardware counter counts only the system PIN).

  **What the numbers are, honestly.** The self-check measures one operation and the parallelism of
  the hardware on the phone itself and reports the estimate. With the chain at 0.7 s and a safety
  factor of ten (root can talk to the secure hardware directly and skip keystore2, the database
  and the HAL), someone with root on this phone needs on average: 6 digits — hours; 8 digits —
  weeks; 10 digits — years. The length of the PIN is the lever, and the setup screen says so.

  **Argon2id — kept, and for a reason other than the obvious one.** Against the person holding the
  phone it adds nothing: they can compute it elsewhere, for all PINs, in advance, and the chain is
  the bottleneck. It matters in one case — the key extracted from the secure hardware, when the
  chain becomes free (key-blob extraction bugs in Samsung's keymaster TA, CVE-2021-25444 and
  CVE-2021-25490, and boot-ROM attacks on some chip families are real). Then 6–8 digits fall
  anyway; with 10 digits and more Argon2id is the difference between hours and years on a GPU.
  An earlier draft of this decision rejected Argon2id with a wrong estimate; the review caught it.

  **Order of an attempt:** header and bounds (a planted `k = 2³²` must not hang the app) → key
  present → key check value → delay gate → **count the attempt and flush to disk** → derive → open.
  Counting before the verdict is what stops the power-cut trick that once bypassed the iOS passcode
  counter. A hardware failure before the chain produced its result is not a verdict and puts the
  counter back. Tests: `store/tests/pin.rs`, including one where the test key records what the
  counter file held at the moment the hardware was asked.

  **Key check value.** The header carries `HMAC_hw(label)[..16]`. A key that is present but not the
  one the vault was made with — reissued by firmware, restored from somewhere — is reported as its
  own state, not as a wrong PIN: otherwise the owner would collect delays for a PIN that was right
  and never learn why. This is a fourth condition for "the data cannot be opened", next to the
  three of R-010, and it is deterministic.

  **Delays, not auto-wipe.** Five attempts are free; then 30 s, 1 min, 5 min, 15 min, then an hour
  each. They run on the boot clock (`elapsedRealtime` plus `BOOT_COUNT`): changing the time does not
  shorten them, and a reboot restarts the current step in full — never shorter, never longer.
  Against root the counter is worthless (the file can be put back), and this is said: what holds
  there is the cost of an attempt, not the file. There is no wipe after N failures: against root it
  does not trigger, against a thief the delays suffice, and a child playing with the phone is a
  real way to lose everything. It stays an open question, together with the duress PIN.

  **The PIN does not pass through Dart as a string.** Rust draws the layout (R-007) for every
  attempt, Dart sends the position tapped, Rust maps it to a digit and keeps the digits in a wiped
  buffer; the two entries of a new PIN are compared in Rust. What Dart does see, said plainly: the
  layout and the pointer events. What Kotlin sees: `x0`, which is enough to test PINs offline
  against the next hardware output — it lives on the Java heap until collected. The window is
  narrowed, not closed, as in `docs/storage.md` §7. `FLAG_SECURE` keeps the pad out of
  screenshots, screen recordings and the recent-apps thumbnail. An accessibility service reads the
  keys — that is P8, a compromised device, and nothing inside the app changes that.

  **No migration of pre-PIN data.** Data written before the PIN existed only on test phones. Carrying
  it over would have been the riskiest code in the vault — unwrap with the old key, create the new
  one, re-seal, and delete the old key only after the new wrapper is durably on disk, where f2fs
  checkpoints make "durably" subtle — for one run on test data. The app recognises such data and
  offers to start over; it never does so on its own.

  **Forgotten PIN = everything lost** until the recovery scheme (stage 6). The setup screen says it.
- **Cost:** about a second on every return to the app (R-001 locks on every exit); a forgotten PIN
  cannot be recovered; Keystore blobs cannot be revoked (see R-005); the honest estimate for six
  digits against a forensic lab is hours.

### R-012. Delivery without servers: a dead drop in Mainline DHT
- **Works against:** P1 and P7 in the sense that matters to the project: there is no operator,
  no relay of ours and no cloud to be compelled or switched off. Envelopes go into Mainline DHT —
  millions of machines running BitTorrent clients, owned by nobody, which twenty years of attempts
  have not shut down.
- **Does NOT work against:** P5 and P6 for metadata: the DHT nodes near an envelope's address see
  the IP of whoever puts it and of whoever fetches it, and a P4 adversary can run such nodes
  (around 300 000 Sybil nodes were measured in Mainline, R16 of the research). A provider in a
  censoring country can throttle DHT traffic. Content stays end-to-end encrypted regardless.
- **Verdict:** **under measurement.** Nothing is built on it until `tools/dht-probe` and the
  self-check measurement on a phone show that envelopes survive long enough.
- **Rationale.** No always-on intermediary at all is impossible for a logical reason, not a
  technical one: if the sender's phone is asleep when the receiver's wakes up, the message has to
  be somewhere in between. The research concludes that this role cannot be removed (§2.1, §10.3)
  and assigns it to blind relays. The DHT plays the same role without anyone owning it.

  **Two modes.** While the app is open it asks the DHT every few seconds. In the background it does
  **not** listen: staying reachable through carrier NAT means keep-alives every few tens of seconds,
  ~150 mW, about a quarter of the battery a day (§10.4 of the research) — exactly what Briar paid.
  Instead a periodic job (every 15 minutes at best, rarer in Doze) re-puts unacknowledged envelopes
  — the signed packet is repeated, no keys are needed — and, as a separate later step, fetches new
  ones undecrypted. Checking is cheap; listening is expensive.

  **What it costs:** an envelope is at most 1000 bytes (about 700 characters of text; longer
  messages go in parts; media do not go this way); background delivery takes minutes to hours;
  entry into the network goes through well-known bootstrap addresses — losing them does not stop a
  phone that already knows nodes, and those are cached.
- **Cost:** the release build requests the network permission from now on.

---

## Open questions

- **Biometrics instead of a PIN for the frequent case.** More convenient, but in a number of
  jurisdictions compelling someone to apply a finger is legally easier than compelling them to
  hand over a password. The case law is contradictory and changing. To be decided after consulting
  a lawyer, not before.
- **Wiping after N wrong PINs.** Deliberately not done (R-011): useless against root, dangerous
  in a child's hands. Belongs with the duress PIN below and is decided together with it.
- **A duress PIN that wipes data, instead of showing a decoy account.** It does not create the
  "prove there is no second PIN" trap, but destroying data is itself prosecuted in many
  jurisdictions. Both options are bad in different ways.
- **Size of the anonymity set.** A messenger used by 200 activists is more dangerous for them than
  Signal with a hundred million users: the very fact of using a rare tool is a signal for P5 and
  P6. This is an argument against niche positioning, and it is not solved by code.
- **What to do about P8.** Inside the app — nothing. An honest answer to the user, a separate
  section of the documentation, and no promises that P8 refutes.

---

## Wordings that must not be used in the interface and materials

A direct consequence of §16.7 of the research and of its verification in `docs/research-verification.md`.

| Not allowed | Allowed |
|---|---|
| "Protects you from the state" | "There is no operator the state can compel" |
| "Complete anonymity" | "The content is protected; who talks to whom is visible" |
| "No one will know you use it" | (nothing; it is not true) |
| "Military-grade encryption" | The name and version of the protocol |
| "A hidden account will protect you during a search" | "A hidden account protects against everyday curiosity" |

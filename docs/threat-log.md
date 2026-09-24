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

---

## Open questions

- **Biometrics instead of a PIN for the frequent case.** More convenient, but in a number of
  jurisdictions compelling someone to apply a finger is legally easier than compelling them to
  hand over a password. The case law is contradictory and changing. To be decided after consulting
  a lawyer, not before.
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

A direct consequence of §16.7 of the research and of the verification above.

| Not allowed | Allowed |
|---|---|
| "Protects you from the state" | "There is no operator the state can compel" |
| "Complete anonymity" | "The content is protected; who talks to whom is visible" |
| "No one will know you use it" | (nothing; it is not true) |
| "Military-grade encryption" | The name and version of the protocol |
| "A hidden account will protect you during a search" | "A hidden account protects against everyday curiosity" |

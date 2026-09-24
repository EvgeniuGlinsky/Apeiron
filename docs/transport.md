# Transport: messages through Mainline DHT (stage 3)

**Status: being built (24.09.2026) — steps 1–4 of §10 done, the screens of step 5 outlined, read
receipts in the state (§3), step 7 redesigned as a foreground service (R-014).**
Decision record: R-012 in `docs/threat-log.md`. §12 lists what the reviews found and what they
changed; it is kept so the same holes are not dug again.

The shape of the problem: two phones, no server of ours, and neither is reachable while it is
not in the foreground. A message has to wait somewhere between the moment the sender's phone
puts it and the moment the receiver's phone asks. That somewhere is Mainline DHT: millions of
BitTorrent nodes, owned by nobody.

## 1. What the DHT gives, and what it does not

BEP 44 mutable items:

- the key of an item is an Ed25519 public key plus an optional salt; the item lives on the
  ~8–20 nodes closest to `SHA1(key ‖ salt)`;
- the value is at most 1000 bytes **bencoded**, so a byte string of at most 996 bytes;
- anyone can read an item; only the holder of the signing key can make a new one. The signature
  covers `seq ‖ value` only, so **anyone who has seen a signed item can put it again**, forever;
- a node MUST refuse a put whose `seq` is not higher than the stored one, unless the value is the
  same. `mainline`'s own server does not enforce the equal-`seq` half; nothing below relies on it;
- items MAY expire after 2 hours and SHOULD be re-put hourly; `mainline` nodes keep at most 1000
  items in an LRU, so survival depends on load;
- nobody pushes anything to anyone. The receiver asks.

Measured on 24.09.2026 (desktop; one phone, Samsung A24, on Wi-Fi):

| What | Result |
|---|---|
| put | every batch 24/24 (desktop), 12/12 (phone); p50 3.3–4.5 s |
| first response | p50 0.17–0.25 s one at a time; 0.5–0.9 s with 24–48 lookups at once |
| desktop items read by the desktop | 24/24 at 1 min … 2.75 h in both runs, **24/24 at 4 h**; 8/12 h — *to be filled in* |
| desktop items read by the phone | **24/24 at 3 h 49 min** |
| phone items read by the desktop | 12/12 at 3 min; 2/4/8 h — *to be filled in* |
| phone items read by the phone | 36/36 at 65–67 min |
| wrong values under our keys | 0 |

## 2. Keys and addresses

Every message part goes to its own address, used once. An address is an Ed25519 key pair derived
from a secret only the two people share, so that to anyone else the addresses of one conversation
are unrelated random keys spread over the whole key space, and nobody else can make an item at
such an address.

**The pair secret comes from the two identities, not from the invitation:**

    K_pair = HKDF-SHA256(ikm  = X25519(my agreement secret, peer's agreement public),
                         info = "apeiron/pair/v1" ‖ lower identity ‖ higher identity)

"Lower/higher" orders the two 64-byte public identities bytewise, so both sides get the same key.
The agreement is refused when it is **not contributory** (`SharedSecret::was_contributory`: a
low-order public key would make `K_pair` computable from the public identities alone) and when the
peer is oneself (both directions would share addresses).

Why not a random secret carried in the invitation: an invitation often travels through someone
else's messenger, and whoever reads it there would know the address schedule — and, since the
signing keys follow from it, could replace every waiting envelope. With static Diffie–Hellman the
invitation carries only public keys and the inbox secret of §8, which locates one drop and nothing
else.

Per direction, bound to the one Olm session of the pair:

    K_dir(A→B)   = HKDF(K_pair, "apeiron/addr/v1" ‖ A ‖ B ‖ session_id)
    seed_n       = HKDF(K_dir,  "apeiron/addr/seed/v1" ‖ n as 8 bytes big-endian)
    address_n    = Ed25519 key from HKDF(seed_n, "apeiron/addr/key/v1"), no salt, seq = 1
    K_env_n      = HKDF(seed_n, "apeiron/addr/envelope/v1")

    state_d      = HKDF(K_dir,  "apeiron/state/v1" ‖ d as 4 bytes big-endian)   (d = UTC day)
    state address and K_env: as above, from state_d

`session_id` is in `K_dir` because the same two identities can be introduced again (a deleted
contact, a session rebuilt after damage): a new schedule must not start at an index whose old,
signed item anyone may still re-put. **One Olm session per contact**; a re-introduction abandons
the old addresses.

**Before the first put to a fresh address, the sender asks for it.** If something is already
there, the schedule is reused or known to someone else: nothing is put and the person is warned.

**What this costs.** The address schedule has no forward secrecy. Whoever later obtains either
identity secret (a seized phone plus its PIN) can compute every past and future address of all of
that person's pairs, open the outer layer of every envelope they recorded (lengths, kinds, the
device keys in pre-key messages), and occupy future addresses ahead of time to silence the pair —
the check above detects that, it does not prevent it. The fix is address epochs rotated in-band;
it is required **before any public release**, not after it.

**One device per identity.** Addresses and the `seq` of state items belong to the identity, not the
device. A second device of the same identity (the sigchain allows `AddDevice`) would sign state
items with the same `seq` and different values, and the nodes would keep whichever came first.
Until devices get schedules of their own, an identity lives on one phone.

## 3. Items

The value of every item is exactly `E = 900` bytes, the measured size. 996 is allowed by BEP 44
and gives 10 % more text; it is measured before it is used.

    value = nonce (24) ‖ XChaCha20-Poly1305(K_env, aad = "apeiron/envelope/v1", inner) ‖ tag (16)
    inner = E − 40 = 860 bytes:  version (1) ‖ kind (1) ‖ body_len (2) ‖ body ‖ zeros

**The outer layer is required.** An Olm pre-key message carries the sender's device Curve25519 key
in the clear; without the outer layer the nodes that store the first envelope of each conversation
could link all of that person's conversations. Under it every value is random-looking and of one
length. The nonce is random: an item is created once and committed before its first put (§4), and a
random nonce keeps even a bug there from exposing two plaintexts under one key.

**Message part** (`kind = 1`): `part (1) ‖ parts (1) ‖ olm_type (1) ‖ olm bytes`, so at most 853
bytes of Olm message. A message is a run of consecutive indices, so it needs no identifier of its
own: it is named by the index of its first part. vodozemac V1 adds 48 bytes to a normal message and
154 to a pre-key message, plus PKCS#7 padding: **799 bytes of UTF-8 per part in a normal message,
687 in a pre-key message** — about 399 or 343 Cyrillic characters. The budgets are the largest that
fit, far into a chain too (test `budgets_fit_the_envelope`), and an Olm message that outgrew its
budget is an error, never cut. At most **32 parts** a message (≈ 25 KB of Latin text, ≈ 12 KB of
Cyrillic).

**State** (`kind = 2`): each side keeps one live item per contact and day, at the state address of
its own outgoing direction (`state_d` of `K_dir(me→peer)`). It says what this side has received
from the peer and what it has sent to the peer:

    next_recv (8)  how many of the peer's indices I have, without a gap
    recv_bits (8)  which of the next 64 I also have
    next_send (8)  how many indices I have used towards the peer
    send_floor (8) the lowest index I still re-put; below it, I have given up
    [next_read (8)] the peer's messages whose first index is below this were shown to me

State items are never acknowledged. Their `seq` grows within the day.

**The read mark** (24.09.2026) is the read receipt: the first index of the newest message of the
peer's that the open chat has shown, plus one. The peer's messages arrive in order, so everything
before it was shown too; the sender's message turns "read" when the mark passes its first index.
The body length tells the two forms apart — 32 bytes without the mark, 40 with it — so a state of
the first builds still reads, and a side that does not send receipts makes its state the old way:
nothing in it says the setting exists. The reverse does not hold: a build before this one refuses
a 40-byte state, and a phone that cannot read its peer's state sees neither acknowledgements nor
new messages — both phones take the update. The setting is mutual, as in Signal: off, no mark is sent
and none is shown. What it tells: the peer learns when a message was shown. The network learns
little new — an open chat already shows in how often it asks (§7) — except that the state item is
re-put when the chat is opened.

**Olm version.** Sessions are V1: 8-byte MACs (`encrypt_truncated_mac`). A third party cannot even
submit a forgery — the outer Poly1305 and the address signature stop it first — and the peer holds
the keys anyway, so the truncation costs nothing here. (R-009 said the opposite about which version
truncates; corrected there.)

## 4. Sending

One owner per contact — a task holding that contact's `Chat` and pair state — does all sending and
receiving for it, one thing at a time. Two load-modify-save cycles on one `Chat` would reuse an Olm
message key or lose a ratchet step. **Checked, not assumed** (`apeiron-messenger`): every commit
first compares the stored conversation record with the one this owner last loaded or wrote, and a
stale owner is refused and must load again, instead of writing an old session over a new one.

1. Split the text; for each part Olm-encrypt, wrap into an envelope for index `next_send + i`,
   sign.
2. **One SQLite transaction** for the whole message: the Olm session after encryption, every part
   into the outbox, `next_send` advanced, the new state item signed and into the outbox. A crash
   before the commit leaves nothing; after it, everything. Parts 1..k of n on the wire without the
   rest cannot happen.
3. Put the parts (asking first, §2), then the state item. An item is stored exactly as signed, so
   every later re-put repeats the same bytes and needs no keys — this is what lets the background
   job work while the vault is locked.

## 5. Re-putting, acknowledgement, giving up

- **Nothing that is not re-put occupies an index.** Every index holds a message part.
- A part is re-put every `R = 60 min` (the BEP 44 recommendation; measured survival is ≥ 4 h, so
  this has margin) until the peer's state says it has it (`next_recv > n`, or its bit is set).
- The state item is re-put every `R` and re-signed when it changes. Signing needs the pair secret,
  so it happens only while the vault is open; **when the vault locks**, the state items for today
  and the next 6 days are signed with the values of that moment and left to the background job.
  The `seq` numbers they take must be committed **in the same transaction as the outbox**
  (step 7): otherwise the pair, restored on day d+2, signs the `seq` the background job already
  put with another value, and the nodes refuse it.
- After `T = 7 days` a part stops being re-put: `send_floor` rises above it, the receiver learns
  from the state that it is gone and marks it lost, and the sender shows the message as **not
  delivered**. Nothing is dropped silently, on either side.

**The background job is not the app's to schedule — unless it is a foreground service.**
WorkManager runs every 15 minutes at best; under Doze and the rare/restricted App Standby buckets,
hours to a day. So the background job is a **foreground service** with its notification (R-014,
decided 24.09.2026): the process is not killed and the background limits do not apply. It still
does not listen. It wakes on a timer, bootstraps from the routing table saved by the previous run
(`Dht::to_bootstrap`, not the four well-known nodes), re-puts the outbox, asks, and sleeps. The
interval starts at 15 minutes and is a constant for stage 4 to decide against the battery budget
(≤ 5 % a day); background delivery is promised only if items survive longer than that interval
(§1 — they do: 24/24 at 8 h).

## 6. Receiving

- One lookup per contact per round: the peer's state item for today (and yesterday's, in the first
  hour after UTC midnight). The open chat every 5 s, the list of all contacts every 15 s (60 s in
  the first outline — nearly the whole of the wait, since a message is found in 1–4 s), only while
  the app is in front and the vault is open, and the list not while a chat is open over it.
- The state says exactly which indices exist (`next_send`) and which are gone (`send_floor`); the
  missing ones in between are fetched at once (`probe::get_many` already does this). If no state is
  found — the peer has been away for days — `next_recv` itself is still asked for.
- Fetched envelopes are stripped of the outer layer and kept, sealed, in an inbound queue keyed by
  index. **Duplicates are dropped by index before Olm ever sees them.**
- **Decryption goes strictly in index order**, the contiguous run from `next_recv`. A gap is waited
  for — its sender re-puts it — until the peer's `send_floor` passes it; then it is marked lost and
  skipped. Olm therefore sees messages in the order they were encrypted and needs its store of
  skipped keys (40 per chain, 5 chains) only across a gap declared lost. A message whose chain Olm
  no longer keeps is classified as lost, not retried.
- One transaction per received batch: the Olm session, the messages, `next_recv`/`recv_bits`, the
  inbound rows removed, the new state item.
- A round is **receive, then send**: what arrived is acknowledged in the same round, not the next
  one. **But nothing that acknowledges is put before it is stored**: receive → choose what to put
  (`Pair::due`, which makes the new state item) → commit → put (`engine::put_due`) → commit the
  times of the puts. `engine::round` puts before the caller can commit; a crash in between leaves
  the peer believing messages arrived that were never kept — it stops re-putting them — and the
  state item's `seq` taken by a value the restored pair does not know. `apeiron-messenger` runs
  the order above; its test fails a commit mid-round and checks that nothing was acknowledged.
- **In the background the phone does not listen and does not fetch** (R-012). The service of R-014
  only asks whether anything is at the address of each contact's `next_recv` — address public keys
  for a few indices ahead, prepared when the vault locks, without `K_env` — and shows "a new
  message" with neither name nor text. Everything else waits for the PIN.

## 7. What is seen

This goes into the interface wording as it stands here.

- **The provider and anyone on the path see everything but the content.** The DHT protocol is
  plain UDP: every lookup and every put, its target, the 900-byte value and the time. Two
  providers in one jurisdiction can join targets into a graph of who talks to whom **by IP**.
- Every node asked along a lookup — dozens per get — sees the target and the asker's IP; the nodes
  near the target also see who put it.
- **Polling links the two sides.** The receiver asks for an address before or after the sender puts
  it; whoever sees both learns *these two IP addresses exchanged a message at this time*. A Sybil
  adversary with many nodes (about 300 000 were measured, R16 of the research) sees a share of all
  targets and builds the same graph.
- **The pattern is readable.** One target asked every 5 s is an open chat; a set of targets every
  60 s is the number of contacts. State items differ from message parts by their `seq`.
- **Using the app is visible.** `mainline` sends its version (`RS`, 0, 5) and `ro=1` in every
  message; together with 900-byte salt-less items this marks the subscriber as an Apeiron user to
  their provider. (This is also why "No one will know you use it" is a forbidden wording.)
- On an open network the node switches itself into server mode after 15 minutes
  (`mainline`'s adaptive mode cannot be turned off by configuration) and starts storing other
  people's items. Behind carrier NAT it stays a client.
- The content stays protected; who talks to whom does not. Volunteer relays or Tor as the way into
  the DHT are the mitigation, outside this stage.

## 8. Getting to know each other

**Invitation:** format version ‖ prekey bundle (224, signed by the identity, carries it) ‖ inbox
secret `S` (32) ‖ expiry (day). As a QR code, or as text through any channel.

**B accepts.** B verifies the bundle, refuses an expired invitation, oneself and a non-contributory
key, and puts **one reply** to the inbox address `Ed25519(HKDF(S, "apeiron/inbox/addr/v1"))`:

    value = e_pub (32) ‖ nonce (24) ‖ XChaCha20-Poly1305(K_inbox, body) ‖ tag (16), padded to E
    K_inbox = HKDF(X25519(e, A's agreement key) ‖ S, "apeiron/inbox/v1")
    body    = B's prekey bundle (224) ‖ Olm pre-key message

The Olm message is encrypted to A's bundle and may carry a short first text (about 440 bytes).
`S` only locates the drop: whoever read the invitation can find the reply, but not open it, so they
do not learn B's identity. They **can** answer in B's name with their own keys, or replace B's reply
— that is what verification (below) is for. `e_pub` makes the reply distinguishable from random for
the nodes that store it: one item per introduction; Elligator 2 would hide it and is noted, not done.

**A checks, before anything is created:** the reply opens; the bundle verifies; the identity is not
A's and its key is contributory; the Olm message uses **this** invitation's one-time key
(`PreKeyMessage::one_time_key`) — otherwise whoever holds one invitation's `S` could spend another
invitation's key; vodozemac itself checks that the Olm identity key is the bundle's device key. If B
is already a contact, the new session replaces the old one and the old addresses are abandoned (§2).

**Lifecycle.** A asks each open inbox every 60 s while the app is open and the vault unlocked. B
re-puts its reply every `R` and shows the contact as **waiting** until A's first state item for B
appears, which is the acknowledgement. At expiry A stops asking and removes the one-time key
(`Account::remove_one_time_key`). A forwarded invitation serves the first valid reply; the others
never get A's state and stay "waiting", then "not accepted".

**Done (24.09.2026)** in `transport/src/invite.rs`: the invitation is 261 bytes —
`version ‖ bundle (224) ‖ S ‖ expiry day` — and as text `apeiron:` plus URL-safe base64 (whitespace
a messenger adds is ignored); the first text fits in 447 bytes of UTF-8; the reply rides in the
pair (`Pair::with_intro`), is re-put until the inviter's first state arrives (`Event::Accepted`),
and a taken inbox is reported (`Event::InvitationTaken`). Tests in `transport/tests/invite.rs`,
one for every check above.

**With the store (`apeiron-messenger`, 24.09.2026):**

- An invitation is saved **with the account** that published its one-time key, in one
  transaction: saved without it, the reply could never be opened. Opening a reply writes the
  contact, the session, the pair, the first text and the account, and forgets the invitation, in
  one transaction (`Storage::introduce`); the contact's other sessions are removed.
- **Expiry.** B accepts through day `expires`; A still opens a reply on day `expires + 1` and
  retires the invitation after that, removing its one-time key (vodozemac `low-level-api`, for
  this one function). B stops re-putting the reply after the same day and shows "not accepted" —
  but keeps listening for another `T`: if A took the reply at the last moment, A's state still
  makes the contact accepted. B removes the one-time key of its own bundle at once: it travels
  only for its signature.
- **Who is already a contact.** B refuses an invitation from someone it has a working (live or
  waiting) conversation with: pasting the same invitation twice would otherwise replace a live
  conversation by one whose reply is squatted by its own first reply. A closed conversation, or
  one that does not load, is replaced — that is the way back. To introduce again after deleting a
  contact, the other side deletes it too.
- **Crossed invitations** (A accepted B's, B accepted A's): each side keeps the session of the
  invitation made by the **lower** identity (bytewise, as for `K_pair`). Decided by how the session
  was made, never by its state: a waiting session can become live before the second reply is
  opened, and a rule by state leaves the two sides on different sessions for good. The losing
  side's queued messages are shown as not delivered.

**Verified is always the person's own act, on each side.** Either both compare the safety number
(`PublicIdentity::safety_number`) and confirm it, or one scans the other's verification code with
the camera. Scanning an *invitation* never marks anything as verified: the app cannot tell a screen
across the table from a photo of it or a forwarded picture. Until then the contact is marked **not
verified** everywhere it is shown.

**How it is done today (24.09.2026):** in steps — call by voice, not through the channel the
invitation went through; both open the screen; each reads one row; "match" or "do not match".
"Do not match" takes the mark back and shows the two fingerprints the number is made of, so the two
people can tell a replaced key from a fault of the app. The owner's own fingerprint is drawn plain
and says it is not what is compared; copper belongs to the safety number alone — the first build
drew both alike and said the fingerprint was read aloud, and on two phones people compared one with
the other. **The 30 digits are a collision target** for an intermediary who knows both identities
in advance (≈ 2⁵⁰, R-013); short codes with a commitment (SAS) replace them next.

**SAS with a commitment — the design, after an adversarial review (24.09.2026), not built yet.**
Both people have the chat open, on a voice call not carried by the messenger the invitation went
through. The run's state is sealed in the conversation record (a format 3 of it) and every change of
it commits **in the same transaction** as the service message it sends. **One live run per contact.**

1. I → R `Start{v = 1, sas_id (16), created_at, commit}`, `commit = SHA-256("apeiron/sas/commit/v1"
   ‖ sas_id ‖ e_I_pub)`, `e_I` a fresh X25519 pair.
2. R, **only after its person taps "compare"**: the Start is younger than the expiry, its `sas_id`
   is unknown, no run is live — or, while R's own Start is unanswered, the lower `sas_id` wins and
   a tie cancels. A fresh `e_R`; `Key{sas_id, e_R_pub}`.
3. I: its live run, the first Key, `e_R ≠ e_I`, contributory. Derives, **deletes `e_I`'s secret**,
   shows the emoji, sends `Key{sas_id, e_I_pub}`.
4. R: the commitment holds, `e_I ≠ e_R`, contributory. Derives, deletes `e_R`'s secret, shows.

        T         = SHA-256("apeiron/sas/transcript/v1" ‖ v ‖ sas_id ‖ lower identity ‖ higher
                            identity ‖ Olm session id ‖ e_I_pub ‖ e_R_pub)   — identities as held here
        PRK       = HKDF-Extract(no salt, X25519(e_mine, e_theirs))
        sas_bytes = HKDF-Expand(PRK, "apeiron/sas/v1/show" ‖ T, 6)  → 42 bits → 7 emoji of 64
        K_mac     = HKDF-Expand(PRK, "apeiron/sas/v1/mac" ‖ T, 32)

5. The person decides. **Match:** verified on this phone only, then `Done{sas_id, HMAC-SHA256(K_mac,
   "match" ‖ T)}`. **Do not match:** the mark is taken back, `Cancel{sas_id, mismatch}`.
6. A Done is checked: a bad MAC says "the phones disagree — not a match" and takes the mark back; a
   good one is shown **only after the local decision** ("their phone agrees") and marks nothing.
7. `Cancel{sas_id, reason}` at any time; a later Start cancels the live run and counts as a run;
   at most **3 runs per contact an hour**; 10 minutes to expire, checked at unlock too.

What the review changed, and why each rule is there:

- *Critical:* an ephemeral key serving two runs voids the commitment — the relay knows it before
  choosing its own and grinds 2⁴² toward a target, GPU-hours. → A fresh key per `sas_id`, made in
  the transaction that sends it; a run is never resumed; a used `sas_id` is remembered.
- "A failed attempt shows as a mismatch" was false: the initiator learns the code as soon as the
  responder's key arrives, and a relay can always take that role and cancel instead. → R answers
  only on its person's tap; every start, cancel and expiry is a system row in the history; the rate
  limit; after two runs cut short the screen says that this is itself a warning sign.
- Identities are sorted in `T` (as for `K_pair`), not placed by role: an honest run must either
  match or end with a Cancel and a reason — **never show a mismatch**, or people learn that a
  mismatch is a glitch. For the same reason the mismatch screen has no "try again".
- Done carries a MAC and never counts as the decision: unauthenticated, it lets the relay show
  "Bob confirmed" before A compared. Checked, it catches a wrong `contact.peer` or a bug at 256 bits
  when the person only skimmed the emoji.
- Service messages ride in the Olm conversation as `"\0" ‖ "apeiron-sas/1:" ‖ base64url(body)` plus
  a readable tail for older builds. Sending a text with a NUL is refused; a NUL-prefixed payload
  that does not parse is dropped, never shown. They are acknowledged like texts (they hold
  indices) but stay out of the history, read marks, unread counts and ticks; stale ones — re-put
  for up to 7 days — are ignored by `created_at` and by the remembered `sas_id`s. The background
  "new message" would still fire for one.
- The emoji: numbered 1–7, the name in both languages (an en screen and an ru screen are compared
  by gist otherwise), both buttons of equal weight, the peer's state hidden until the local
  decision. The system emoji font differs by vendor; bundled images are the better choice.

## 9. Storage (schema v2)

**Records are not re-sealed.** Today `SCHEMA_VERSION` is part of every record's AAD, so raising it
would make every existing record fail to open. It is split: `RECORD_FORMAT = 1` goes into the AAD
and changes only when the encoding of a record changes; `SCHEMA_VERSION` describes the tables and
goes to 2. The v1 → v2 migration only creates tables, in one transaction. Its test builds a v1
database with every kind of record, opens it with the v2 code and reads every table; and again with
a crash injected in the middle, after which the database is still a readable v1.

New tables, with their tags fixed now: `messages` (7), `outbox` (8), `pair_state` (10),
`invitations` (11). Tag 9 is reserved for a separate inbound queue: for now the parts that arrived
and are not yet decrypted live inside the pair state, which is sealed as one record and committed
with the session. The storage keeps these records as opaque sealed bytes and does not depend on
the transport. A round of a conversation — the Olm session, the pair state, new and changed
messages — is one call, one transaction (`Storage::commit_conversation`). History lives in Rust
and reaches Dart one visible page at a time (R-004).

Done (24.09.2026): `RECORD_FORMAT` split, v1 → v2 in one transaction, a record sealed by the v1
code kept as a known answer (`a_record_sealed_by_schema_v1_still_opens`), every kind of v1 record
read back after the migration, a step cut short leaves v1 as it was (`store/tests/migration.rs`,
`store/src/lib.rs`).

**Records of the messenger** (`messenger/src/record.rs`), sealed by the store in their place:

- a message: `format ‖ kind (mine / theirs / lost) ‖ status ‖ first index? ‖ time ‖ [to, for a
  loss] ‖ text`. The status of mine only moves forward: queued < sent < delivered < read; not
  delivered and "address taken" are final, and what was delivered can only become read. The first
  text of an introduction has no index — it travels in the reply, not in the schedule — and so is
  never marked read;
- a conversation, in `pair_state`: `format ‖ origin (I invited / I accepted) ‖ the first text's
  row? ‖ read mark (the last row shown; from format 2) ‖ the pending messages (first index → row)
  ‖ the pair`. The pair holds its session id and refuses to be restored for another session; from
  its format 2 it also holds both read marks, the peer's and mine. A record of format 1 reads with
  the mark at 0, so what arrived before the update counts as unread until the chat is shown once.
  Each record kind has its own format number; each old one has a test on bytes laid out by hand.

**Order of the history** is the order in which entries appeared on this phone; the time of a
received message is when it was decrypted. An envelope carries no time of its sender, and sorting
by the sender's clock is not worth trusting it.

**The background key.** The background re-put needs the outbox without the PIN, so the signed items
are sealed under a separate Keystore key without user authentication. Such a key also works on a
phone that is locked but has been unlocked once since boot — the usual state in which phones are
seized — not only on one taken unlocked. What it opens:

- the address public keys of every waiting envelope — and by watching who fetches those targets (as
  a DHT node, or as the provider) one learns **the IP address of each contact** the owner is waiting
  on;
- when those items were made and how many parts they have: message lengths to about 780 bytes. Only
  hour-granular times are stored;
- with the service of R-014, the address public keys of the next few **incoming** parts of every
  contact: whoever holds the locked phone can watch who puts to them — the contacts' IP addresses
  again, now also of those who write to the owner, not only of those the owner waits on.

Not the content, not the names, not the pair secrets: those stay under the PIN. The background path
never migrates and refuses a `SCHEMA_VERSION` other than its own.

## 10. Order of work

1. `transport` crate: §2–§6 as pure logic behind a DHT trait, with a fake DHT in tests — order,
   loss, duplicates, gaps, giving up, re-put, state, parts, a re-introduction, a squatted address.
   `Identity` gets a contributory, not-self agreement. All on the desktop.
2. The real DHT behind the same trait, and two instances talking through the real DHT from the
   desktop.
3. Schema v2 (`RECORD_FORMAT` split, new tables) and its migration tests.
4. The bridge: history and sessions in Rust (R-004). **Done:** `apeiron-messenger`, the owner of a
   conversation, and a thin chat API in the bridge.
5. Screens: contacts, invitation (QR and text), verification, chat. **Outlined** with the text
   invitation; QR waits for the scanner below.
6. Active polling while the app is in front. For now a timer in Dart (the open chat 5 s, the list
   15 s) calls a round; a thread of its own, and a send that never waits for a round in flight,
   come here.
7. Background re-put by a **foreground service** (R-014; revised 24.09.2026 from WorkManager):
   Kotlin service → JNI → Rust, the background key in Keystore, the outbox and the `seq` numbers
   signed ahead in one transaction, the incoming addresses prepared at lock, the saved routing
   table, "a new message" without content.
8. Two phones. Stage 4 then measures the battery and the real background interval.

Open before step 5: a QR scanner that needs neither Google Play services nor the network.

## 11. Constants

| Name | Value | Where from |
|---|---|---|
| `E` | 900 bytes | measured; 996 allowed, not yet measured |
| parts per message | ≤ 32 | Olm's skipped-key store, background time limits |
| `R` | 60 min | BEP 44; survival ≥ 4 h measured |
| `T` | 7 days | then "not delivered" |
| state items pre-signed at lock | 7 days | covers a week without opening the app |
| open chat / the list | 5 s / 15 s | while the app is in front |
| service wake-up | 15 min | a start; stage 4 decides against ≤ 5 % battery a day |

## 12. What the review changed

An adversarial review of the first draft (24.09.2026) found two critical and ten major problems.
All are fixed above:

1. *Critical.* A lost index silently stalled a direction for good (acks put once, a receiver that
   only looked at `next_recv`, a 7-day give-up nobody learned about). → State items with
   `next_send` and `send_floor`; nothing un-re-put occupies an index (§3, §5, §6).
2. *Critical.* "Scanned in person — verified" was false for the inviter, who only ever received B's
   keys through the inbox, authenticated by a secret printed in the QR. → Verified only by each
   side's own act (§8).
3. Acks: a lost one was never repeated, none were sent when the vault locked right after reading,
   and ack-only envelopes could ping-pong. → State items, pre-signed at lock, never acknowledged.
4. Olm's limits (5 chains, 40 skipped keys) lost late or reordered parts. → Decrypt strictly in
   index order, dedupe before Olm, ≤ 32 parts (§6).
5. A re-introduction restarted the schedule at index 0, where old signed items can be re-put by
   anyone. → `session_id` in `K_dir`, one session per contact, `seq = 1`, ask before the first put.
6. The inbox reply showed B's identity to anyone who had read the invitation. → Encrypted to A's
   agreement key as well (§8).
7. What A must check in the reply, and how an inbox lives and ends, were not said. → §8.
8. Sending and receiving on one `Chat` were not serialized. → One owner per contact; one
   transaction per message and per received batch (§4, §6).
9. §7 named only the nodes near an address. → The provider, the lookup path, readable patterns,
   the client fingerprint, adaptive server mode.
10. The re-put interval is not the app's to choose in the background. → Measured on the phone, and
    that comparison decides the promise (§5).
11. The list of what the background key opens was incomplete. → §9.
12. Schema v2 would have had to re-seal every record. → `RECORD_FORMAT` split (§9).

Minor: contributory and not-self agreement (§2); sizes recomputed from vodozemac's encoding (§3);
what an identity compromise gives (§2); `mainline` specifics (§1, §7); and R-009 had the truncated
MAC on the wrong Olm version (§3).

**The second review (24.09.2026), of the plan for step 4**, found one critical and three major
problems in it, all fixed before the code: crossed invitations were settled by the state of the
session, which changes before the second reply is opened (→ by origin, §8); B closed the contact at
expiry while A might have opened the reply on its last day (→ B keeps listening, §8); the `seq`
numbers signed at lock were not saved (→ with the outbox, §5); two owners of one conversation could
write an old session over a new one (→ the stored record is compared before every commit, §4).

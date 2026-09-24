# Apeiron — design system

## Name

**Apeiron**, ἄπειρον — "the boundless" in Anaximander. The first principle that is itself none of
the things in the world, yet encompasses everything and governs everything. A third entity: in the
world and outside it.

For the product this is literal: a network that exists as long as its participants carry it, and is
located nowhere in particular. "We are here and nowhere."

---

## The rule from which everything else follows

Scandinavian restraint provides **air and discipline**. Runic geometry provides **the stroke**.
Runes were carved in wood and stone, so they have no curves — only straight lines and angles.

> **The only hard rule of the graphics: 0°, 45°, 90°. No rounding. Stroke ends cut flat.**

A hard rule is what turns a set of elements into a system. Prestige here comes from discipline,
not from gilding.

---

## About runes: what we do not do and why

Scandinavian symbolism has been partly appropriated by far-right movements. The double ᛋ (sowilo),
ᛟ (othala) in a certain form, the sun wheel and a number of other signs read unambiguously,
regardless of the author's intentions.

For a product addressed to journalists and human rights defenders, showing up with such a sign is
the end of its reputation at launch, and no explanations work afterwards.

**Decision:** we take the geometry of runes, but **use no historical rune at all** — not in the
mark, the typeface, the icons or the styling. The aesthetic is kept in full, the risk disappears
entirely.

---

## Mark

Apeiron is that which **has no boundary**. So the mark must be **open**.

- A vertical stem, with branches coming off it at 45°.
- Open at the top and bottom: the lines run beyond the mark's field rather than ending within it.
- **No rings, circles or closed contours** — they directly contradict the name.
- Built on a 24×24 grid, stroke of 2 units, optical correction on the diagonals only.

The mark must not read as a specific rune. Test: if a native speaker sees a letter in it, we redo
it.

---

## Palette

Cold, muted. Two accents, no more.

### Dark theme — primary

| Role | Token | Value |
|---|---|---|
| Background | `basalt-950` | `#0B0E11` |
| Surface | `basalt-900` | `#12161A` |
| Raised surface | `basalt-800` | `#1A2026` |
| Borders | `stone-700` | `#2A333B` |
| Dividers | `stone-600` | `#3A444D` |
| Secondary text | `fog-400` | `#8A97A3` |
| Primary text | `bone-100` | `#E8E6E1` |
| **Accent** | `glacier-400` | `#8FB3C9` |
| **Verification states** | `ember-400` | `#C77B52` |
| Alert | `rust-500` | `#B4543A` |

### Light theme — a mirror

| Role | Token | Value |
|---|---|---|
| Background | `bone-50` | `#F4F2ED` |
| Surface | `bone-100` | `#E8E6E1` |
| Primary text | `basalt-900` | `#12161A` |
| Secondary text | `stone-600` | `#3A444D` |
| Accent | `glacier-600` | `#4A7290` |

### Rationale

- The primary text is **not pure white** but bone `#E8E6E1`. Pure white on dark hurts the eyes and
  looks cheap; warm bone is a Scandinavian technique, reading as birch and bone.
- The accent is a **glacier blue**, desaturated. Saturated accents cheapen.
- **Copper** `ember-400` — not gold. Bronze is a genuine material of this culture, and it reads as
  more expensive than gold precisely because it is quieter. Reserved for key verification states:
  verification must look different from everything else in the app.
- The red `rust-500` is muted down to rust — an alert should not scream, it should be noticeable.

---

## Typography

| Role | Typeface | Why |
|---|---|---|
| Interface, text | **Inter** | Neutral, impeccable legibility, full Cyrillic |
| Headings, large type | **Syne** | Geometric and angular, gives the right character without caricature |
| Fingerprints, keys, codes | **JetBrains Mono** | Distinguishes `0`/`O` and `1`/`l`/`I` — here this is a security matter |

Rules:

- Scale: 12 / 14 / 16 / 20 / 28 / 40. No intermediate sizes.
- Line height: 1.5 for text, 1.2 for headings.
- Letter spacing `+0.02em` in headings, `+0.08em` in small uppercase labels.
- Uppercase only in labels of up to three words.

---

## Grid and air

- Base unit **4**, layout step **8**.
- Screen margins: 20 on a phone, 32 or more on desktop.
- Between semantic blocks — no less than 28.
- Text line length — no more than 68 characters.

Air here is not decoration: a sparse layout is the only thing that makes a dense technical
interface look expensive.

---

## Icons

- 24×24 grid, stroke **2**, flat ends.
- **Only 0°, 45°, 90°.** Not a single rounding, not a single arc.
- An outline icon is preferable to a filled one.

---

## Motion

- Durations: 120 ms (response), 220 ms (transition), 400 ms (screen appearance).
- Curve `cubic-bezier(0.2, 0, 0, 1)`.
- **No springs and no bounces.** Calm reads as confidence, and a jumpy interface reads as cheap.

---

## Fingerprint verification screen: where design does security work

This is the only screen where aesthetics bear directly on protection, and so it is designed
separately from everything else.

Thirty digits in six groups of five — what people read aloud to the other person. Without this
verification, a strong cipher is completely defeated by an active man-in-the-middle (demonstration:
`s07_ratchet.py`, section F).

Requirements:

- The digits are set in **JetBrains Mono, 28**, letter spacing `+0.12em`, with large gaps between
  the groups. The setting must read as a **carved inscription**, not as a technical string.
- Group background — `basalt-800`, a thin `stone-700` border, corners **not rounded**.
- The screen's accent is **copper** `ember-400`, not glacier. Verification looks like nothing else
  in the app, and that is deliberate.
- The "not verified" state is always shown, explicitly. Silence here is unacceptable.
- A mismatch — `rust-500`, full-screen, with plain words: someone is between you.

The point of the technique: people look closely at what is beautiful. A close look at this screen
is exactly what catches the man-in-the-middle. Here the design does not decorate the function, it
performs it.

---

## Interface honesty

A direct consequence of §16.7 of the research. The design must not create confidence that the
system does not have.

- The archive assembly state is shown as it is: "received 5 of 8". Hiding the delay behind an
  endless spinner is forbidden — it creates a sense of breakage and hides the truth about the
  system.
- The "metadata not hidden" indicator is always present, not buried in third-level settings.
- Forbidden wordings — in `threat-log.md`, the section at the end.

An expensive look and honesty do not contradict each other here: trust is exactly what people pay
for.

---

## Wordmark and icon

### The APEIRON wordmark

Latin letters drawn by the rules of runic carving: **not a single curve, uniform thickness, flat
ends, horizontals reduced to a minimum**. The word stays instantly readable but takes on the look
of an inscription in stone.

The last rule is not a stylistic one. Runes were carved across the wood grain: a horizontal cut ran
along the grain, split the blank and was barely visible. A horizontal is kept only in `E`, which
without it stops reading as a letter.

The strict "only 0°, 45°, 90°" is deliberately relaxed here: under it `O` would have to be as wide
as its own height, and `E` could not be drawn at all. The carving constraint is more precise and
gentler.

The stroke thickness is **1/9 of the letter height**. That is the limit: at 1/6 the counters of
`P`, `R` and `O` fill in.

Implementation — `app/lib/wordmark.dart`, contact sheet — `flutter test test/wordmark_sheet.dart`.

### Icon: the raven

Huginn and Muninn are Odin's ravens, who fly over the world and return to tell what they saw.
Messengers in the literal sense. Their names translate as **"thought"** and **"memory"**, which
matches the layering of the system: delivery is responsible for "now", the archive for what is
kept.

**Curves are allowed here.** This is the only exception to the system's rule: the wordmark and the
interface icons stay on straight lines with flat ends, but the bird is alive. A feather is never
straight, and straight-line versions read as a comb.

The silhouette was taken ready-made, not drawn: six iterations of hand-picking Bézier control
points produced a bird, but not a realistic one. Source and rationale —
`app/assets/brand/PROVENANCE.md`.

Parameters:
* **rotation of −45°** — chosen on a contact sheet from the series 0 / −20 / −32 / −45. A raised
  beak reads as gaining altitude, a lowered one as descending;
* **horizontal flip** — the source flies to the left, and with left-to-right writing, sending reads
  as movement to the right;
* **bird's share of the icon 0.70** — below 0.55 the icon looks empty, above 0.75 the silhouette
  runs into the edges.

The backplate is **basalt**. A dark icon among colorful ones on the home screen stands out.
Rejected: copper (lowest contrast), glacier (too similar to Telegram), bone (a white icon on some
launchers reads as "failed to load").

Contact sheet — `flutter test test/raven_sheet.dart`.

### Android launcher icon

The icon is **not rasterized**. Android understands the same `pathData` syntax as SVG, so the
resources get the same coordinates, recomputed once: flipping the potrace output, mirroring, the
−45° rotation, fitting. Two earlier approaches — a generator on top of `flutter_test` and a
headless browser — broke precisely on rasterization and produced the wrong scale.

This is computed by `app/tool/gen_android_icon.dart` (a plain `dart run`, no Flutter), the geometry
by `app/lib/path_data.dart`, the mark's numbers by `app/lib/brand/mark_geometry.dart`. The same
`path_data.dart` underlies `svg_path.dart`, which the app draws with: the parsing and the
transformations are shared by everyone, so there is nothing for the icon and the screen to diverge
on.

What goes in:

| Layer | File | For whom |
|---|---|---|
| adaptive icon | `mipmap-anydpi-v26/ic_launcher.xml` | Android 8 and newer |
| foreground layer | `drawable/ic_launcher_foreground.xml` | same |
| monochrome layer | `drawable/ic_launcher_monochrome.xml` | themed icons, Android 13+ |
| backplate | `values/ic_launcher_background.xml` — basalt | same |
| the whole icon | `mipmap-anydpi/ic_launcher.xml` | Android 7, which has no masks yet — kept as a fallback, although since `minSdk = 28` (Android 9) no supported device reads it |

There are no density PNGs at all: the `anydpi` qualifier is older than the project's lower bound
(Android 9). Flutter's stock PNGs have been removed — a test makes sure they do not come back.

Sizes. The adaptive icon canvas is 108 dp, of which the system shows the central 72 — that is the
backplate, whose role in the app is played by the circle. The bird takes the same 0.70 of it.
Android promises to show a 66 dp circle under any mask; the test measures **along the outline
itself, not the contour's bounding box** (the bounding box is computed from control points and is
always wider than the paint) and requires the paint not to extend beyond 33 dp from the center.

Check — `flutter test test/android_icon_test.dart`: the resources in the repository are compared
with the generator's output, the colors with the tokens, and the contour from `pathData` with the
very `Path` that `ApeironRaven` draws, by subpath lengths and by points along the outline. There is
no rasterization in the test: `toImage()` inside `testWidgets` keeps the process from exiting, and
point comparison is stricter than pixel comparison — it does not depend on anti-aliasing.

Contact sheet — `flutter test test/android_icon_sheet.dart`: the same icon under circle,
squircle, rounded-square and square masks, at working sizes down to 28 px, and the themed icon in
both system themes.

Along the way, the launch splash screen was switched from white (Flutter's default) to basalt: a
white flash before a dark app is visible, and that is a defect.

### Rejected directions

The history is in git; here is the outcome, so as not to go around a second time:

* **an abstract mark of a stem and branches** — with strict 45° on a single stem, any two branches
  give either a Latin "K", a crossed-out stem, or ᛉ (algiz, an appropriated rune). There is no
  fourth option within these constraints;
* **fire, a signal beacon** — the meaning matched exactly (a chain of vitar passes news along
  without a center), but the viewer does not know this and simply reads "fire": a fitness app or a
  discount looks the same;
* **fire crossed with a replica** — it worked, but remained an explanation rather than an image.

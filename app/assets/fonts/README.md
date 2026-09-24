# Fonts

Typeface files go here. All three are under the SIL Open Font License: bundling them into the app
is allowed, and so are modifying and redistributing them.

## Why not `google_fonts`

The `google_fonts` package by default **downloads fonts from fonts.gstatic.com on first launch**.
For a messenger that promises no outbound requests, this tells Google that the app has been
installed — from the user's IP address, before they have done anything. Unacceptable. Fonts come
only from here, from assets.

## What is here

| File | Typeface | Weight | Where it is used |
|---|---|---|---|
| `Inter-Regular.otf` | Inter | 400 | body text |
| `Inter-Medium.otf` | Inter | 500 | labels |
| `Inter-SemiBold.otf` | Inter | 600 | interface headings |
| `SyneSemiBold.ttf` | Syne | 600 | large headings |
| `SyneBold.ttf` | Syne | 700 | logo, titles |
| `JetBrainsMonoNL-Regular.ttf` | JetBrains Mono NL | 400 | keys, codes |
| `JetBrainsMonoNL-SemiBold.ttf` | JetBrains Mono NL | 600 | **fingerprint on the verification screen** |

They are declared in `app/pubspec.yaml` and referenced from `lib/theme/tokens.dart`. The NL
("no ligatures") cut of JetBrains Mono is deliberate: a ligature could merge two characters of a
key into one glyph.

## Where to get them

- Inter — https://rsms.me/inter/ or https://fonts.google.com/specimen/Inter
- Syne — https://fonts.google.com/specimen/Syne
- JetBrains Mono — https://www.jetbrains.com/lp/mono/

Download them as an archive from the website, not via the package.

## Why JetBrains Mono specifically for the fingerprint

It unambiguously distinguishes `0` from `O`, and `1` from `l` and `I`. On the verification screen
this is not a matter of taste: a misread digit means the user accepted someone else's key as the
other person's key — that is, let through the man-in-the-middle that all this cryptography is
built against.

## Replacing a file

Keep the family names in `app/pubspec.yaml` and `lib/theme/tokens.dart` in step, run
`flutter pub get` and rebuild.

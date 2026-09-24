# Fonts

Typeface files go here. All three are under the SIL Open Font License: bundling them into the app
is allowed, and so are modifying and redistributing them.

## Why not `google_fonts`

The `google_fonts` package by default **downloads fonts from fonts.gstatic.com on first launch**.
For a messenger that promises no outbound requests, this tells Google that the app has been
installed — from the user's IP address, before they have done anything. Unacceptable. Fonts come
only from here, from assets.

## What needs to be added

| File | Typeface | Weight | Where it is used |
|---|---|---|---|
| `Inter-Regular.ttf` | Inter | 400 | body text |
| `Inter-Medium.ttf` | Inter | 500 | labels |
| `Inter-SemiBold.ttf` | Inter | 600 | interface headings |
| `Syne-SemiBold.ttf` | Syne | 600 | large headings |
| `Syne-Bold.ttf` | Syne | 700 | logo, titles |
| `JetBrainsMono-Regular.ttf` | JetBrains Mono | 400 | keys, codes |
| `JetBrainsMono-SemiBold.ttf` | JetBrains Mono | 600 | **fingerprint on the verification screen** |

Variable versions (`Inter-VariableFont.ttf` and the like) also work — then there will be fewer
files, but the declaration in `pubspec.yaml` will need adjusting.

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

## After adding the files

1. Declare them in `app/pubspec.yaml` under `flutter: fonts:`.
2. In `lib/theme/tokens.dart`, replace `uiFont = null` with `'Inter'` and `displayFont = null`
   with `'Syne'`.
3. Run `flutter pub get` and rebuild.

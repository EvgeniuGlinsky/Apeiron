# Apeiron

A decentralized messenger. ἄπειρον — "the boundless" in Anaximander: the first principle that is
itself none of the things in the world, yet encompasses everything. A network that exists as long
as its participants carry it, and is located nowhere in particular.

## What it promises and what it does not

**Promises:**

- The content of conversations is fully protected.
- There is no operator that can be coerced, shut down or compelled to hand over data. This is the
  project's only genuine motive, and it is real.

**Does not promise:**

- Metadata privacy. Who talks to whom and when is visible. For a real-time messenger without
  cover traffic, the lower bound on the observer's advantage is δ ≥ 0.999999. This is a theorem
  (Das et al., IEEE S&P 2018), not an oversight.
- Protection against a compromised device. Against Pegasus-class attacks, nothing that lives
  inside the app helps: the adversary reads the screen before encryption.
- Better survival of data after device loss than centralized solutions offer — not until stage 6
  is done.

Forbidden wordings for the interface and materials are in [`docs/threat-log.md`](docs/threat-log.md).

## Structure

```
core/     core: cryptography and protocol. No FFI, no unsafe, tested separately
store/    local storage under AEAD: key hierarchy, schema, record sealing
platform/ access to the device's hardware key store. One JNI call, Android only
relay/    blind relay (stage 3, a stub for now)
app/      Flutter client
  rust/   thin bridge to the core via flutter_rust_bridge
docs/     decisions and specifications
```

## Documents

| File | About |
|---|---|
| [`docs/threat-log.md`](docs/threat-log.md) | Adversaries P1–P8 and the log of protection decisions |
| [`docs/storage.md`](docs/storage.md) | Storage and the PIN: key hierarchy, formats, what is protected and what leaks |
| [`docs/crypto.md`](docs/crypto.md) | Crypto core: what it is built from, what is checked, what cryptography does not do |
| [`docs/build-guards.md`](docs/build-guards.md) | Build guards: why an unusable APK was once produced and what now prevents a repeat |
| [`docs/design.md`](docs/design.md) | Design system |
| [`docs/research-verification.md`](docs/research-verification.md) | Verification of the research the plan rests on |

The original research (58 pages, 34 sources, reproducible calculations) is not part of the
repository — the author keeps it. Verification of its load-bearing claims is in
[`docs/research-verification.md`](docs/research-verification.md): it states exactly what was
checked and how, so that the conclusions can be challenged without having the document itself.

## Building

Requires Rust stable, Flutter, Android SDK with NDK, JDK 21.

```bash
# core and relay
cargo test --workspace

# client
cd app
flutter test
flutter build apk --debug --target-platform android-arm64
flutter build windows --debug
```

After changing the public API in `app/rust/src/api/`, the bindings must be regenerated:

```bash
cd app && flutter_rust_bridge_codegen generate
```

Variants of the mark are laid out on contact sheets for comparison (the process does not
exit — wait for the PNG to appear):

```bash
cd app && flutter test test/wordmark_sheet.dart      # → build/mark/wordmark-sheet.png
cd app && flutter test test/raven_sheet.dart         # → build/mark/raven-sheet.png
cd app && flutter test test/android_icon_sheet.dart  # → build/mark/android-icon-sheet.png
```

## Pitfalls

**`flutter build windows` fails with `MSB8066 ... code -1`.** MSBuild hides the real error.
The cause is usually that cargokit's precompiled tool was left broken — for example, the `dart`
process was killed during the build. The fix is to delete its cache:

```bash
rm app/build/windows/x64/plugins/rust_lib_apeiron/cargokit_build/tool/bin/build_tool_runner.dill
rm app/build/windows/x64/plugins/rust_lib_apeiron/cargokit_build/tool/.dart_tool/package_info.prev
```

To see the real cause rather than `code -1`: `flutter build windows --debug -v`.

**Python fails with `UnicodeEncodeError` on scripts containing Cyrillic.** The Windows console is
in cp1252. Run with `PYTHONIOENCODING=utf-8` — this is a defect of the environment, not of the
scripts.

## Rules that must not be broken

Derived from §18 of the research; details and rationale are in the documents above.

- Do not write our own cryptographic primitives or encryption modes.
- Do not use encryption without authentication.
- Do not apply erasure coding before encryption.
- Do not use Shamir's secret sharing for bulk data — only for keys.
- Do not rely on guaranteed background execution on a mobile platform.
- Do not promise metadata privacy without paying for it in latency or traffic.
- Plaintext and keys do not leave Rust: wiping memory in Dart is impossible.
- Fonts are bundled in assets. `google_fonts` downloads them from Google's servers on first
  launch — for this app that leaks the fact of installation.

## License

AGPL-3.0-or-later.

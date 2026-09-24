# Build guards

A document about why the build once produced an unusable APK and reported
success, and what now prevents that from happening again.

## What happened

On September 23, 2026 `flutter build apk --release` completed successfully and
produced an APK **without `librust_lib_apeiron.so`**. The app would have crashed
on launch: without this library there are neither keys nor identity — the core
lives entirely in Rust.

The output contained exactly one line indicating trouble:

```
SEVERE: rustup not found in PATH.
```

followed by `√ Built build\app\outputs\flutter-apk\app-release.apk (44.4MB)`.

It was caught by chance, during a manual review of the APK's contents before
handing it out.

## Why it happened

There turned out to be two independent causes.

### 1. The environment did not reach the build

`CARGO_HOME`, `RUSTUP_HOME` and the path to `cargo/bin` are set correctly at
user level. But the terminal the build ran from had been started **before**
these variables appeared, and inherited an environment without them. The Gradle
daemon, started from that terminal, received the same empty environment and
keeps it until restarted: setting the variables in the session after the daemon
has started is not enough.

Cargokit looks for `rustup` only in `CARGO_HOME/bin`, `%USERPROFILE%\.cargo\bin`
and on `PATH`. The path to it cannot be set via a Gradle property or through
`cargokit.yaml` — cargokit has no such setting; this was checked against its
source.

### 2. The batch file swallowed the exit code

This is the main cause, and far nastier than the first.

Cargokit, not finding rustup, honestly prints `SEVERE` and exits with code 1.
Gradle calls it via `execOperations.exec {}` without `ignoreExitValue`, which
means it **must** fail. It did not.

The culprit is `cargokit/run_build_tool.cmd`. It ended like this:

```bat
"%DART%" "%PRECOMPILED%" %*

REM 253 means invalid snapshot version.
If %ERRORLEVEL% equ 253 (
    ...
)
```

The last executed statement is an `If` with a false condition. By itself it
succeeds, and its success becomes the exit code of the whole file. The `dart`
error is lost without a trace.

Verified by a separate experiment on three variants of the batch file:

| Batch file | Exit code |
|---|---|
| `cmd /c exit 1` and nothing else | 1 — correct |
| the same plus `If %ERRORLEVEL% equ 253 (...)` at the end | **0 — error lost** |
| the same plus `exit /b %ERRORLEVEL%` | 1 — correct |

This is a cargokit bug on Windows, not a quirk of the machine. On macOS and
Linux it does not exist: `run_build_tool.sh` starts with `set -e`.

On Windows desktop there is no hole either, but for a different reason:
`cargokit.cmake` declares the `.dll` as the `OUTPUT` of a custom command, and
without the file the target is simply not considered built.

## What is in place now

### Patch in cargokit

`exit /b %ERRORLEVEL%` with the comment `APEIRON PATCH` has been added to the end
of `app/rust_builder/cargokit/run_build_tool.cmd`.

**This is a vendored file.** Updating `flutter_rust_bridge` or cargokit will
overwrite it, and the patch will disappear silently. That is why it is not the
only measure.

### Guard in our Gradle

`app/android/app/build.gradle.kts`, two tasks:

* **`checkRustToolchain`** — before compilation starts, checks that `rustup` is
  visible at all, using the same search rules as cargokit. Fails within seconds
  with a recipe, instead of two minutes of compilation and the message "Maybe
  you need to install Rust?", which is misleading: Rust is installed on the
  machine, the build just cannot see it. Attached to `preBuild`.
* **`verifyRustLib<Variant>`** — after packaging, opens the **finished APK** and
  requires `librust_lib_apeiron.so` to be present for every packaged
  architecture. The list of architectures is not fixed in advance: it depends on
  how the build was invoked (`--target-platform`, `--split-per-abi`), so what is
  checked is what actually ended up inside. Attached to `assemble<Variant>` —
  that is, to what `flutter build apk` invokes.

Both live in our file and will survive a cargokit update.

### Verifier for the finished file

`app/tool/verify_apk.dart` is a separate program that does not care who built
the APK or how:

```bash
cd app && dart run tool/verify_apk.dart build/app/outputs/flutter-apk/app-release.apk
```

It checks: the Rust library for each architecture, the Flutter engine, presence
of the signing block, the manifest, the bytecode, the resource table, the
Flutter asset manifest, and **the three bundled typefaces by name** (Inter, Syne,
JetBrains Mono NL) — because `google_fonts` must not be used, and a font missing
from the APK means the text falls back to the system font.

The zip is read by our own code (`app/tool/apk_report.dart`): only the entry
names are needed, no decompression at all. Pulling in a dependency for this, in
a project that promises verifiability, is a bad trade.

**Run it before every APK that is handed out.** Exit code: 0 — valid,
1 — invalid, 2 — nothing to check.

### The rest

* `rust-toolchain.toml` — pins the Android targets and the components
  (`rustfmt`, `clippy`). Without the targets cargokit builds nothing.
* Strict lints are extended to the bridge `app/rust`: `unwrap`, `expect`, `panic`
  are forbidden — this is code on the FFI boundary, where a panic brings down the
  whole process. `unsafe_code = "deny"` (not `forbid`, as in the core) with a
  single exemption on the `mod frb_generated` declaration: FFI without `unsafe`
  does not exist, and `forbid` cannot be lifted even deliberately.
* `.github/workflows/checks.yml` — formatting, lints with `-D warnings`, Rust
  tests, `cargo deny check`, Flutter analysis and tests. Third-party actions are
  pinned **by SHA, not by tag**: a tag in someone else's repository can be moved,
  a SHA cannot.
* `deny.toml` — a license allowlist and a ban on yanked versions.

## How it was verified that the guards work

A guard that has never fired is not a guard. Each one was tested by deliberate
breakage:

| What we break | Expected | What happened |
|---|---|---|
| remove `rustup` from the environment | failure before compilation | `checkRustToolchain` failed the build in 2 s with a recipe |
| change the name of the library being looked for | failure on the finished APK | `verifyRustLibDebug` failed the build, naming all three architectures |
| call cargokit without the environment | Gradle sees a non-zero code | `finished with non-zero exit value 3`, the build failed |
| an APK without the Rust library | the verifier rejects it | tests in `test/apk_report_test.dart`, 9 checks |

## What these guards do not catch

Stated honestly, so there is no false sense of security:

* **A stale library.** If cargokit did not run, but a `.so` from a previous
  successful build was left in intermediate directories, the APK will be built
  with it, and the presence check will pass. That is exactly how the debug APK
  survived that day. What closes this is not the presence check but the
  batch-file patch: now a cargokit failure fails the build, and packaging is
  never reached.
* **Substitution of the library's contents.** What is checked is the file's
  presence, not its provenance. Reproducible builds are what works against this,
  and we do not have them yet.
* **AAB.** The APK is what is checked. When it comes to publishing in a store,
  the check will need to be extended to `SingleArtifact.BUNDLE`.

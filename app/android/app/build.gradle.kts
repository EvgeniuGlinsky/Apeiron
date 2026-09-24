import com.android.build.api.artifact.SingleArtifact
import java.io.File
import java.util.zip.ZipFile

plugins {
    id("com.android.application")
    id("kotlin-android")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

android {
    namespace = "io.apeiron.apeiron"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = JavaVersion.VERSION_17.toString()
    }

    defaultConfig {
        // TODO: Specify your own unique Application ID (https://developer.android.com/studio/build/application-id.html).
        applicationId = "io.apeiron.apeiron"
        // You can update the following values to match your application needs.
        // For more information, see: https://flutter.dev/to/review-gradle-config.
        // minSdk is set as a number, not flutter.minSdkVersion, deliberately.
        // Both setIsStrongBoxBacked and setUnlockedDeviceRequired appear in
        // API 28, and master-key storage rests on them (R-002). Below 28 they
        // are unavailable, and the scheme would degrade into a software key —
        // that is, into a second code branch nobody here would ever execute.
        // Cost: API 24 covers 96.6 % of devices, API 28 — 93.5 %
        // (Statcounter, April 2026). Android 7 and 8 are lost.
        minSdk = 28
        targetSdk = flutter.targetSdkVersion
        versionCode = flutter.versionCode
        versionName = flutter.versionName
    }

    buildTypes {
        release {
            // TODO: Add your own signing config for the release build.
            // Signing with the debug keys for now, so `flutter run --release` works.
            signingConfig = signingConfigs.getByName("debug")
        }
    }
}

flutter {
    source = "../.."
}

// ─────────────────────────────────────────────────────────────────────────────
// Build guards
//
// On 23.09.2026 the release build produced an APK **without**
// `librust_lib_apeiron.so` and reported success. The app would have crashed
// on launch: without this library there are no keys, no identity — nothing.
//
// There were two causes, and both are closed.
//  1. Cargokit did not find rustup (environment variables did not reach the
//     Gradle daemon) and exited with an error — but `run_build_tool.cmd`
//     returned 0. Fixed in the batch file itself: see the APEIRON PATCH edit
//     in `app/rust_builder/cargokit/run_build_tool.cmd`.
//  2. Nothing checked the finished artifact. `verifyRustLib` below closes it.
//
// The first fix lives in vendored code and will be lost on a cargokit
// update — the second is ours and will survive. Hence two, not one.
// Details: `docs/build-guards.md`.
// ─────────────────────────────────────────────────────────────────────────────

val rustLibraryName = "librust_lib_apeiron.so"

// The symbol through which Kotlin gives Rust access to the hardware key.
// The name is set explicitly both in Rust (export_name) and here: if they
// diverge, the build stops here, not the app on the phone.
val rustJniSymbol = "Java_io_apeiron_apeiron_Vault_nativeRegister"

/**
 * Requires that the built APK contains the native Rust library for **every**
 * packaged architecture.
 *
 * The list of architectures is deliberately not fixed in advance: it depends
 * on how the build is invoked (`--target-platform`, `--split-per-abi`). What
 * is checked is what actually got into the APK: if it has `lib/arm64-v8a/`,
 * our library must be in it too.
 */
abstract class VerifyRustLib : DefaultTask() {
    @get:InputFiles
    abstract val apkDirectory: DirectoryProperty

    @get:Input
    abstract val libraryName: Property<String>

    /**
     * Name of the JNI symbol that must be exported from the library.
     *
     * Through it Kotlin gives Rust a reference to Vault and the data
     * directory path. The symbol is declared in a dependency crate, not in the
     * cdylib itself, and it is lost silently: `System.loadLibrary` does not
     * complain about a missing symbol, the app simply ends up without access
     * to the hardware key — and this is discovered only on the phone. It has
     * been checked that it is in place now; the rule is needed so that it
     * stays that way.
     */
    @get:Input
    abstract val requiredSymbol: Property<String>

    @TaskAction
    fun verify() {
        val lib = libraryName.get()
        val symbol = requiredSymbol.get()
        val dir = apkDirectory.get().asFile
        val apks = dir.listFiles { f: File -> f.name.endsWith(".apk") }
            ?.sortedBy { it.name }
            .orEmpty()

        if (apks.isEmpty()) {
            throw GradleException("Build guard: there is no APK in $dir — nothing to check.")
        }

        for (apk in apks) {
            ZipFile(apk).use { zip ->
                val nativeLibs = zip.entries().asSequence()
                    .map { it.name }
                    .filter { it.startsWith("lib/") && it.endsWith(".so") }
                    .toList()

                val abis = nativeLibs.mapNotNull { it.split('/').getOrNull(1) }
                    .distinct()
                    .sorted()

                if (abis.isEmpty()) {
                    throw GradleException(report(apk.name, "the APK has no native libraries at all"))
                }

                val missing = abis.filterNot { abi -> nativeLibs.contains("lib/$abi/$lib") }
                if (missing.isNotEmpty()) {
                    throw GradleException(
                        report(apk.name, "no $lib for: ${missing.joinToString(", ")}")
                    )
                }

                // The symbol is searched for directly in the bytes: it sits
                // in the dynamic symbol table, which `strip = "symbols"`
                // does not touch. So neither nm nor readelf from the NDK
                // is needed.
                val needle = symbol.toByteArray(Charsets.US_ASCII)
                val without = abis.filterNot { abi ->
                    zip.getInputStream(zip.getEntry("lib/$abi/$lib")).use { input ->
                        containsBytes(input.readBytes(), needle)
                    }
                }
                if (without.isNotEmpty()) {
                    throw GradleException(
                        report(apk.name, "$lib has no symbol $symbol for: ${without.joinToString(", ")}")
                    )
                }

                logger.lifecycle(
                    "Build guard: ${apk.name} — $lib and symbol $symbol in place for ${abis.joinToString(", ")}"
                )
            }
        }
    }

    private fun containsBytes(haystack: ByteArray, needle: ByteArray): Boolean {
        if (needle.isEmpty() || haystack.size < needle.size) return false
        outer@ for (i in 0..haystack.size - needle.size) {
            for (j in needle.indices) {
                if (haystack[i + j] != needle[j]) continue@outer
            }
            return true
        }
        return false
    }

    private fun report(apk: String, what: String): String = """
        |
        |Build guard: the APK is unfit and will not be shipped.
        |
        |  APK:      $apk
        |  Problem:  $what
        |
        |This almost always means the Rust library did not build. Look above
        |in the output for a SEVERE line from cargokit. The most common cause on this
        |machine is that rustup is not visible to the Gradle daemon:
        |
        |  1) set the variables in the session the build runs from:
        |     CARGO_HOME, RUSTUP_HOME and PATH with the cargo/bin directory;
        |  2) stop the daemon that holds the old environment:
        |     app/android/gradlew.bat --stop
        |  3) build again.
        |
        |The daemon inherits the environment of the process that started it and keeps it
        |until restarted — setting the variables without --stop is not enough.
        |
    """.trimMargin()
}

/**
 * Early check: whether rustup is visible at all.
 *
 * Without it you learn about the trouble after a minute or two of
 * compilation, and the cargokit message ("Maybe you need to install Rust?")
 * is misleading: Rust is installed on this machine, it is just not visible to
 * the build.
 *
 * We search where cargokit itself searches (`build_tool/lib/src/rustup.dart`):
 * in `CARGO_HOME/bin`, in `%USERPROFILE%\.cargo\bin` and across all of `PATH`.
 * The path cannot be set via a Gradle property or `cargokit.yaml` — cargokit
 * has no such setting, verified against its sources.
 */
abstract class CheckRustToolchain : DefaultTask() {
    @get:Input
    @get:Optional
    abstract val cargoHome: Property<String>

    @get:Input
    @get:Optional
    abstract val userProfile: Property<String>

    @get:Input
    @get:Optional
    abstract val searchPath: Property<String>

    @TaskAction
    fun check() {
        val names = listOf("rustup.exe", "rustup")
        val roots = mutableListOf<File>()

        val home = cargoHome.orNull
        if (home != null && home.isNotBlank()) roots.add(File(home, "bin"))

        val profile = userProfile.orNull
        if (profile != null && profile.isNotBlank()) {
            roots.add(File(File(profile, ".cargo"), "bin"))
        }

        val path = searchPath.orNull
        if (path != null) {
            for (part in path.split(File.pathSeparatorChar)) {
                if (part.isNotBlank()) roots.add(File(part))
            }
        }

        var found: File? = null
        for (dir in roots) {
            for (name in names) {
                val candidate = File(dir, name)
                if (candidate.isFile) {
                    found = candidate
                    break
                }
            }
            if (found != null) break
        }

        if (found == null) {
            throw GradleException(
                """
                |
                |Build guard: rustup is not visible — the Rust library will not build.
                |
                |Rust is installed on the machine, but the build environment does not see it.
                |This is not "install Rust", whatever cargokit writes further on.
                |
                |  1) CARGO_HOME, RUSTUP_HOME and PATH with the cargo/bin directory —
                |     in the session the build runs from;
                |  2) app/android/gradlew.bat --stop — the daemon holds the old environment;
                |  3) build again.
                |
                """.trimMargin()
            )
        }
        logger.lifecycle("Build guard: rustup found — $found")
    }
}

val checkRustToolchain = tasks.register<CheckRustToolchain>("checkRustToolchain") {
    group = "verification"
    description = "Checks that rustup is visible to the build before compilation starts."
    cargoHome.set(providers.environmentVariable("CARGO_HOME"))
    userProfile.set(providers.environmentVariable("USERPROFILE"))
    searchPath.set(providers.environmentVariable("PATH"))
}

tasks.named("preBuild") {
    dependsOn(checkRustToolchain)
}

androidComponents {
    onVariants { variant ->
        val suffix = variant.name.replaceFirstChar { it.uppercase() }

        val verify = tasks.register<VerifyRustLib>("verifyRustLib$suffix") {
            group = "verification"
            description = "Requires $rustLibraryName to be present in the APK of variant ${variant.name}."
            apkDirectory.set(variant.artifacts.get(SingleArtifact.APK))
            libraryName.set(rustLibraryName)
            requiredSymbol.set(rustJniSymbol)
        }

        // `assemble` is what `flutter build apk` invokes. The check becomes
        // part of the build, not a separate ritual someone will forget to do.
        //
        // The binding is lazy (`matching`/`configureEach`, not `named`): when
        // variants are traversed, the task `assembleDebug` does not exist
        // yet, and looking it up by name fails with UnknownTaskException.
        tasks.matching { it.name == "assemble$suffix" }.configureEach {
            dependsOn(verify)
        }
    }
}

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
        // minSdk задан числом, а не flutter.minSdkVersion, намеренно.
        // И setIsStrongBoxBacked, и setUnlockedDeviceRequired появляются в API 28,
        // на которых держится хранение мастер-ключа (R-002). Ниже 28 они
        // недоступны, и схема выродилась бы в программный ключ — то есть во
        // вторую ветку кода, которую здесь никто никогда не выполнит.
        // Цена: API 24 покрывает 96,6 % устройств, API 28 — 93,5 %
        // (Statcounter, апрель 2026). Теряются Android 7 и 8.
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
// Предохранители сборки
//
// 23.09.2026 сборка release выдала APK **без** `librust_lib_apeiron.so` и
// отрапортовала успехом. Приложение упало бы на запуске: без этой библиотеки
// нет ни ключей, ни личности — вообще ничего.
//
// Причин было две, и закрыты обе.
//  1. Cargokit не нашёл rustup (переменные окружения не дошли до демона Gradle)
//     и завершился с ошибкой — но `run_build_tool.cmd` возвращал 0. Исправлено
//     в самом батнике: см. правку APEIRON PATCH в
//     `app/rust_builder/cargokit/run_build_tool.cmd`.
//  2. Ничто не проверяло готовый артефакт. Это закрывает `verifyRustLib` ниже.
//
// Первая правка живёт в вендорном коде и уедет при обновлении cargokit —
// вторая наша и переживёт. Поэтому их две, а не одна.
// Подробности: `docs/build-guards.md`.
// ─────────────────────────────────────────────────────────────────────────────

val rustLibraryName = "librust_lib_apeiron.so"

// Символ, через который Kotlin отдаёт Rust доступ к аппаратному ключу.
// Имя задано явно и в Rust (export_name), и здесь: если они разойдутся,
// сборка встанет тут, а не приложение на телефоне.
val rustJniSymbol = "Java_io_apeiron_apeiron_Vault_nativeRegister"

/**
 * Требует, чтобы в собранном APK для **каждой** упакованной архитектуры лежала
 * нативная библиотека Rust.
 *
 * Список архитектур не задан заранее намеренно: он зависит от того, как вызвана
 * сборка (`--target-platform`, `--split-per-abi`). Проверяется то, что реально
 * попало в APK: если там есть `lib/arm64-v8a/`, то в нём обязана быть и наша
 * библиотека.
 */
abstract class VerifyRustLib : DefaultTask() {
    @get:InputFiles
    abstract val apkDirectory: DirectoryProperty

    @get:Input
    abstract val libraryName: Property<String>

    /**
     * Имя символа JNI, который обязан быть экспортирован из библиотеки.
     *
     * Через него Kotlin отдаёт Rust ссылку на Vault и путь к каталогу данных.
     * Символ объявлен в крейте-зависимости, а не в самом cdylib, и теряется он
     * молча: `System.loadLibrary` на отсутствующий символ не жалуется, а
     * приложение просто остаётся без доступа к аппаратному ключу — и выясняется
     * это уже на телефоне. Проверено, что сейчас он на месте; правило нужно,
     * чтобы так и осталось.
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
            throw GradleException("Предохранитель: в $dir нет ни одного APK — проверять нечего.")
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
                    throw GradleException(report(apk.name, "в APK нет нативных библиотек вовсе"))
                }

                val missing = abis.filterNot { abi -> nativeLibs.contains("lib/$abi/$lib") }
                if (missing.isNotEmpty()) {
                    throw GradleException(
                        report(apk.name, "нет $lib для: ${missing.joinToString(", ")}")
                    )
                }

                // Символ ищется прямо в байтах: он лежит в таблице
                // динамических символов, которую `strip = "symbols"` не трогает.
                // Так не нужны ни nm, ni readelf из NDK.
                val needle = symbol.toByteArray(Charsets.US_ASCII)
                val without = abis.filterNot { abi ->
                    zip.getInputStream(zip.getEntry("lib/$abi/$lib")).use { input ->
                        containsBytes(input.readBytes(), needle)
                    }
                }
                if (without.isNotEmpty()) {
                    throw GradleException(
                        report(apk.name, "в $lib нет символа $symbol для: ${without.joinToString(", ")}")
                    )
                }

                logger.lifecycle(
                    "Предохранитель: ${apk.name} — $lib и символ $symbol на месте для ${abis.joinToString(", ")}"
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
        |Предохранитель сборки: APK негоден и наружу не пойдёт.
        |
        |  APK:      $apk
        |  Проблема: $what
        |
        |Почти всегда это значит, что не собралась библиотека Rust. Ищите выше
        |в выводе строку SEVERE от cargokit. Самая частая причина на этой
        |машине — rustup не виден демону Gradle:
        |
        |  1) задать переменные в той сессии, из которой идёт сборка:
        |     CARGO_HOME, RUSTUP_HOME и PATH с каталогом cargo/bin;
        |  2) остановить демон, который держит старое окружение:
        |     app/android/gradlew.bat --stop
        |  3) собрать заново.
        |
        |Демон наследует окружение процесса, который его поднял, и держит его
        |до перезапуска — задать переменные без --stop недостаточно.
        |
    """.trimMargin()
}

/**
 * Ранняя проверка: виден ли rustup вообще.
 *
 * Без неё о беде узнаёшь через минуту-две компиляции, и сообщение cargokit
 * («Maybe you need to install Rust?») уводит в сторону: Rust на этой машине
 * установлен, просто не виден сборке.
 *
 * Ищем там же, где ищет сам cargokit (`build_tool/lib/src/rustup.dart`):
 * в `CARGO_HOME/bin`, в `%USERPROFILE%\.cargo\bin` и по всему `PATH`.
 * Задать путь через свойство Gradle или `cargokit.yaml` нельзя — такой
 * настройки в cargokit нет, проверено по его исходникам.
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
                |Предохранитель сборки: rustup не виден — библиотека Rust не соберётся.
                |
                |Rust на машине установлен, но окружение сборки его не видит. Это
                |не «поставьте Rust», что бы ни писал cargokit дальше.
                |
                |  1) CARGO_HOME, RUSTUP_HOME и PATH с каталогом cargo/bin —
                |     в той сессии, откуда идёт сборка;
                |  2) app/android/gradlew.bat --stop — демон держит старое окружение;
                |  3) собрать заново.
                |
                """.trimMargin()
            )
        }
        logger.lifecycle("Предохранитель: rustup найден — $found")
    }
}

val checkRustToolchain = tasks.register<CheckRustToolchain>("checkRustToolchain") {
    group = "verification"
    description = "Проверяет, что rustup виден сборке, до того как начнётся компиляция."
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
            description = "Требует наличия $rustLibraryName в APK варианта ${variant.name}."
            apkDirectory.set(variant.artifacts.get(SingleArtifact.APK))
            libraryName.set(rustLibraryName)
            requiredSymbol.set(rustJniSymbol)
        }

        // `assemble` — то, что вызывает `flutter build apk`. Проверка становится
        // частью сборки, а не отдельным ритуалом, который забудут выполнить.
        //
        // Привязка ленивая (`matching`/`configureEach`, а не `named`): на момент
        // обхода вариантов задач `assembleDebug` ещё не существует, и обращение
        // по имени падает с UnknownTaskException.
        tasks.matching { it.name == "assemble$suffix" }.configureEach {
            dependsOn(verify)
        }
    }
}

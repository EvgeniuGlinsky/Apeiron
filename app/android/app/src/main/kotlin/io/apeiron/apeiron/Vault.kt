package io.apeiron.apeiron

import android.app.KeyguardManager
import android.content.Context
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyInfo
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.security.keystore.StrongBoxUnavailableException
import android.security.keystore.UserNotAuthenticatedException
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.SecretKeyFactory
import javax.crypto.spec.GCMParameterSpec

/**
 * Hardware key store: everything that requires Android Keystore.
 *
 * ## Why this is in Kotlin, not Rust
 *
 * The stage note prescribed going to Keystore "over JNI directly from Rust,
 * not through a Dart plugin — otherwise key material will pass through Dart
 * and violate R-004". The reason is right, the conclusion is not: Kotlin is
 * not Dart. The key goes Keystore → Kotlin → JNI → Rust and never reaches
 * Dart, R-004 is intact.
 *
 * But the difference in cost is large. If the caller is Rust, working with
 * Keystore comes with hand-built method descriptors; building `String[]`
 * via `NewObjectArray`; a local reference table where an attached native
 * thread is guaranteed sixteen slots while this sequence needs about fifty;
 * an exception check after every call, because an unchecked exception kills
 * the process when the thread detaches; and telling Keystore failure types
 * apart by comparing strings with class names. All of this is code under
 * `cfg(target_os = "android")`, which does not compile on the development
 * machine at all, and every typo in it costs a "build APK → install → get
 * an answer" cycle.
 *
 * Here instead there is `try/catch` with real types, and Gradle checks the
 * file on every build: a wrong method name becomes a compile error, not a
 * crash on the phone. Decision R-010 in `docs/threat-log.md`.
 *
 * ## What is here and what is not
 *
 * Here: creating a non-exportable hardware key (KEK), sealing and opening
 * short data with it, determining the hardware level, erasure.
 *
 * Not here: file handling, the wrapper format, mutexes, or any state at all
 * apart from a context reference. All of that belongs to Rust. This file is
 * an adapter to Keystore, and nothing more.
 */
object Vault {
    /** All is well. */
    private const val STATUS_OK = 0

    /** Retry later; the data is intact. Everything not GONE lands here. */
    private const val STATUS_TRANSIENT = 1

    /**
     * Key gone. Nothing can decrypt the vault; the only way out is to
     * start over.
     *
     * Reachable from exactly three conditions, and this is the most important
     * place in the file: `containsAlias` returned false, `getKey` returned
     * null, `KeyPermanentlyInvalidatedException`. Everything else is TRANSIENT.
     *
     * The reason is not theoretical. `setUnlockedDeviceRequired` throws
     * `UserNotAuthenticatedException` on an **unlocked** device if it was
     * unlocked with weak biometrics — a confirmed firmware defect. If that is
     * interpreted as "key lost" and the KEK is reissued, the app will destroy
     * the owner's conversations because of a transient failure. This must be
     * structurally impossible, not "we wrote it carefully".
     */
    private const val STATUS_GONE = 2

    /** Our own error. Like TRANSIENT, it does not touch the data. */
    private const val STATUS_INTERNAL = 3

    private const val ALIAS = "apeiron.kek.v1"
    private const val PROVIDER = "AndroidKeyStore"
    private const val TRANSFORMATION = "AES/GCM/NoPadding"
    private const val GCM_TAG_BITS = 128
    private const val GCM_IV_BYTES = 12

    /**
     * Limit on what may be passed through the KEK at all.
     *
     * StrongBox is tens of times slower than TEE: a megabyte takes on the
     * order of fifteen seconds to encrypt through it. Thirty-four bytes pass
     * through the KEK — the wrapper of the database key — and the limit is
     * there so that one day nobody passes the database itself through it,
     * after which the app would freeze before the owner's eyes.
     */
    private const val MAX_WRAP_BYTES = 64

    private lateinit var appContext: Context

    /** What happened on the last attempt to create the key — for the report. */
    private var lastKeyNote: String = "ключ ещё не запрашивался"

    /** How the attempt to connect to the core ended. The only way to learn
     * of a failure: before the Flutter engine exists, there is nowhere to
     * show it. */
    private var registrationNote: String = "связь с ядром ещё не устанавливалась"

    /**
     * Called from [MainActivity] before Dart starts.
     *
     * The library is loaded here too: `System.loadLibrary` and `dlopen` from
     * Dart land in the same linker namespace and find the same object by
     * soname, so there remains a single copy of the code.
     *
     * **It throws nothing outward, and this is not overcaution.** The call
     * comes from `onCreate` before the Flutter engine is up: an exception from
     * here is a black screen and not a single line on it, because there is
     * nothing to show it with. `UnsatisfiedLinkError` is quite reachable here
     * (no `.so` for this architecture, the native method symbol not found),
     * and the difference between "the app does not start" and "the app
     * started and says what exactly failed" is exactly the difference that
     * diagnostics are written for.
     *
     * On failure Rust is simply left without the references and responds with
     * a clear error, and the cause goes into the report.
     */
    @JvmStatic
    fun register(context: Context) {
        appContext = context.applicationContext
        try {
            System.loadLibrary("rust_lib_apeiron")
            nativeRegister(this, appContext.filesDir.absolutePath)
            registrationNote = "связь с ядром установлена"
        } catch (e: Throwable) {
            // Throwable, not Exception: UnsatisfiedLinkError is an Error.
            registrationNote = "СВЯЗЬ С ЯДРОМ НЕ УСТАНОВЛЕНА: ${describe(e)}"
        }
    }

    /**
     * Gives Rust a reference to itself and the data directory path.
     *
     * The instance is passed as an explicit parameter, not taken from the
     * implicit JNI arguments: this way it does not matter whether Kotlin
     * compiles this method as static or as an instance method. The class is
     * not passed — the method is looked up by the object's own class.
     * `FindClass` is not called anywhere on the Rust side: the system class
     * loader would not see the app's class from a worker thread anyway.
     */
    private external fun nativeRegister(self: Any, filesDir: String)

    // ─────────────────────────────────────────────────────────────────────────
    // What Rust calls. One response format for all: the first byte is the
    // status, then either the payload or the message text in UTF-8.
    // ─────────────────────────────────────────────────────────────────────────

    /**
     * Makes sure the key is in place and reports the hardware level.
     *
     * Response: `[STATUS_OK, level] + note in UTF-8`, where level is the raw
     * number from `KeyInfo.getSecurityLevel()`, or `[status] + message`.
     *
     * The note says **how the key came to be**: whether StrongBox was tried
     * and how the attempt ended. It is handed out rather than kept here,
     * because here it lives only until the process ends — and disappears
     * exactly by the time someone wants to read it. Rust will store it,
     * together with the wrapper.
     *
     * @param allowCreate create the key if it is missing. Rust passes `false`
     *   when the wrapper file already exists: in that situation a missing key
     *   means not "first run" but "key gone", and reissuing it would destroy
     *   the data irreversibly.
     */
    fun ensureKey(allowCreate: Boolean): ByteArray {
        return try {
            val store = openStore()
            val existing = loadKey(store)
            if (existing != null) {
                return ok(securityLevelOf(existing), "")
            }
            if (!allowCreate) {
                lastKeyNote = "алиаса нет, а файл обёртки есть"
                return fail(
                    STATUS_GONE,
                    "ключ хранилища отсутствует в защищённом модуле устройства",
                )
            }
            val created = createKey(store)
            ok(securityLevelOf(created), lastKeyNote)
        } catch (e: KeyPermanentlyInvalidatedException) {
            lastKeyNote = describe(e)
            fail(STATUS_GONE, "ключ хранилища необратимо обесценен системой")
        } catch (e: Exception) {
            lastKeyNote = describe(e)
            fail(STATUS_TRANSIENT, describe(e))
        }
    }

    /**
     * Seals short data with the KEK.
     *
     * Response: `[STATUS_OK] + nonce (12) + ciphertext`, or
     * `[status] + message`.
     *
     * The nonce is issued by Keystore, not by us: the key has
     * `setRandomizedEncryptionRequired` enabled, and a custom IV on encryption
     * is forbidden.
     */
    fun wrap(plain: ByteArray): ByteArray {
        if (plain.isEmpty() || plain.size > MAX_WRAP_BYTES) {
            return fail(
                STATUS_INTERNAL,
                "через KEK пропускают не больше $MAX_WRAP_BYTES байт, а дали ${plain.size}",
            )
        }
        return try {
            val key = loadKey(openStore())
                ?: return fail(
                    STATUS_GONE,
                    "ключ хранилища отсутствует в защищённом модуле устройства",
                )
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.ENCRYPT_MODE, key)
            val iv = cipher.iv
            if (iv.size != GCM_IV_BYTES) {
                return fail(STATUS_INTERNAL, "неожиданная длина одноразового числа: ${iv.size}")
            }
            byteArrayOf(STATUS_OK.toByte()) + iv + cipher.doFinal(plain)
        } catch (e: KeyPermanentlyInvalidatedException) {
            fail(STATUS_GONE, "ключ хранилища необратимо обесценен системой")
        } catch (e: Exception) {
            fail(STATUS_TRANSIENT, describe(e))
        }
    }

    /**
     * Opens what [wrap] sealed.
     *
     * Input is `nonce (12) + ciphertext`, output is
     * `[STATUS_OK] + plaintext` or `[status] + message`.
     */
    fun unwrap(ivAndCt: ByteArray): ByteArray {
        if (ivAndCt.size <= GCM_IV_BYTES) {
            return fail(STATUS_INTERNAL, "обёртка короче служебных полей")
        }
        var plain: ByteArray? = null
        return try {
            val key = loadKey(openStore())
                ?: return fail(
                    STATUS_GONE,
                    "ключ хранилища отсутствует в защищённом модуле устройства",
                )
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(
                Cipher.DECRYPT_MODE,
                key,
                GCMParameterSpec(GCM_TAG_BITS, ivAndCt, 0, GCM_IV_BYTES),
            )
            plain = cipher.doFinal(ivAndCt, GCM_IV_BYTES, ivAndCt.size - GCM_IV_BYTES)
            byteArrayOf(STATUS_OK.toByte()) + plain
        } catch (e: KeyPermanentlyInvalidatedException) {
            fail(STATUS_GONE, "ключ хранилища необратимо обесценен системой")
        } catch (e: UserNotAuthenticatedException) {
            // Arrives both when the device is locked and when it was unlocked
            // by a method the hardware finds insufficient. The type cannot
            // tell them apart, so this is NOT "key gone" but "retry".
            fail(
                STATUS_TRANSIENT,
                "устройство заперто или разблокировано способом, которого не хватает",
            )
        } catch (e: Exception) {
            fail(STATUS_TRANSIENT, describe(e))
        } finally {
            // Narrows the window but does not close it: the ART garbage
            // collector moves objects, and Cipher has its own intermediate
            // buffers out of reach from here. Writing "wiped" would be untrue.
            plain?.fill(0)
        }
    }

    /**
     * Erases the key. After this the vault cannot be recovered — that is the
     * point (R-005, cryptographic erasure: the key is destroyed, not the data).
     */
    fun destroy(): ByteArray {
        return try {
            val store = openStore()
            if (store.containsAlias(ALIAS)) {
                store.deleteEntry(ALIAS)
            }
            lastKeyNote = "ключ стёрт"
            byteArrayOf(STATUS_OK.toByte())
        } catch (e: Exception) {
            fail(STATUS_TRANSIENT, describe(e))
        }
    }

    /**
     * Full platform report. Contains no secrets.
     *
     * There is only one on-device check, and it must answer all questions at
     * once, not just the one someone thought to ask.
     */
    fun diagnostics(): String {
        val out = StringBuilder()
        out.append("Android SDK_INT: ").append(Build.VERSION.SDK_INT).append('\n')
        out.append("устройство: ").append(Build.MANUFACTURER)
            .append(' ').append(Build.MODEL).append('\n')
        out.append("алиас: ").append(ALIAS).append('\n')
        try {
            val store = openStore()
            val present = store.containsAlias(ALIAS)
            out.append("алиас на месте: ").append(if (present) "да" else "нет").append('\n')
            if (present) {
                val key = loadKey(store)
                if (key == null) {
                    out.append("ключ читается: НЕТ, getKey вернул null\n")
                } else {
                    val level = securityLevelOf(key)
                    out.append("уровень, сырое число: ").append(level).append('\n')
                    out.append("уровень: ").append(levelName(level)).append('\n')
                }
            }
        } catch (e: Exception) {
            out.append("чтение хранилища: ").append(describe(e)).append('\n')
        }
        out.append("о ключе в этом запуске: ").append(lastKeyNote).append('\n')
        out.append("StrongBox заявлен системой: ")
            .append(if (hasStrongBoxFeature()) "да" else "нет").append('\n')
        out.append("устройство заперто сейчас: ").append(deviceLockedNote()).append('\n')
        out.append("блокировка экрана задана: ").append(secureLockNote()).append('\n')
        return out.toString()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Internal
    // ─────────────────────────────────────────────────────────────────────────

    /**
     * `load(null)` is mandatory: without it the very first `containsAlias`
     * gives `KeyStoreException: Uninitialized keystore`.
     */
    private fun openStore(): KeyStore {
        val store = KeyStore.getInstance(PROVIDER)
        store.load(null)
        return store
    }

    private fun loadKey(store: KeyStore): SecretKey? {
        if (!store.containsAlias(ALIAS)) {
            return null
        }
        return store.getKey(ALIAS, null) as? SecretKey
    }

    /**
     * Creates the key: tries StrongBox first, and on any failure falls back
     * to TEE.
     *
     * The fallback catches `Exception` as a whole, not just
     * `StrongBoxUnavailableException`: AOSP maps only a single failure code to
     * this type, the others arrive as `ProviderException` or
     * `KeyStoreException`, and on some firmware generation fails
     * non-reproducibly. The alias is deleted before the retry — a failed
     * generation sometimes leaves a half-written entry, and the next
     * `containsAlias` would lie.
     *
     * This is reached only when there is no wrapper file yet, so nothing can
     * be lost.
     */
    private fun createKey(store: KeyStore): SecretKey {
        try {
            val key = generate(strongBox = true)
            probe(key)
            lastKeyNote = "создан со StrongBox"
            return key
        } catch (e: StrongBoxUnavailableException) {
            lastKeyNote = "StrongBox недоступен (${describe(e)})"
        } catch (e: Exception) {
            lastKeyNote = "StrongBox не дался (${describe(e)})"
        }
        runCatching { if (store.containsAlias(ALIAS)) store.deleteEntry(ALIAS) }
        val key = generate(strongBox = false)
        probe(key)
        lastKeyNote = "$lastKeyNote; создан без StrongBox"
        return key
    }

    private fun generate(strongBox: Boolean): SecretKey {
        val builder = KeyGenParameterSpec.Builder(
            ALIAS,
            KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
        )
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            // A decision, not a default: banning a custom IV rules out the
            // nonce reuse on which CVE-2021-25444 in Samsung's keymaster
            // is built.
            .setRandomizedEncryptionRequired(true)
            // The hardware refuses to work while the phone is locked. This is
            // what file-system encryption does not give: on a locked but booted
            // phone the storage is already decrypted.
            .setUnlockedDeviceRequired(true)
        if (strongBox) {
            builder.setIsStrongBoxBacked(true)
        }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, PROVIDER)
        generator.init(builder.build())
        return generator.generateKey()
    }

    /**
     * Full round trip: encrypt and decrypt, comparing the result.
     *
     * The alias is considered accepted only after it. StrongBox can refuse not
     * at `generateKey` but later — at `Cipher.init` or `doFinal`, and then a
     * "successfully created" key would turn out broken in the owner's hands.
     */
    private fun probe(key: SecretKey) {
        val sample = ByteArray(32) { it.toByte() }
        val enc = Cipher.getInstance(TRANSFORMATION)
        enc.init(Cipher.ENCRYPT_MODE, key)
        val iv = enc.iv
        val ct = enc.doFinal(sample)
        val dec = Cipher.getInstance(TRANSFORMATION)
        dec.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(GCM_TAG_BITS, iv))
        val back = dec.doFinal(ct)
        val same = back.contentEquals(sample)
        back.fill(0)
        if (!same) {
            throw IllegalStateException("ключ не прошёл проверочный круг: расшифровано не то")
        }
    }

    /**
     * What the system **reports** about the key's protection level.
     *
     * Precisely "reports". There is no attestation for symmetric keys: they
     * have no certificate chain, and `KeyInfo` is a self-report by the
     * framework, executed in our own process. A compromised device will return
     * anything. The number is fit for an honest label in the UI and unfit as
     * proof.
     *
     * The numbers are the same as in `KeyProperties`: -2 unknown, -1 hardware
     * without specifics, 0 software, 1 TEE, 2 StrongBox. On API 28-30
     * `getSecurityLevel()` does not exist yet, which leaves
     * `isInsideSecureHardware()`, which distinguishes only "hardware or not
     * hardware" — hence -1.
     */
    private fun securityLevelOf(key: SecretKey): Int {
        return try {
            val factory = SecretKeyFactory.getInstance(key.algorithm, PROVIDER)
            val info = factory.getKeySpec(key, KeyInfo::class.java) as KeyInfo
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                info.securityLevel
            } else {
                @Suppress("DEPRECATION")
                if (info.isInsideSecureHardware) {
                    KeyProperties.SECURITY_LEVEL_UNKNOWN_SECURE
                } else {
                    KeyProperties.SECURITY_LEVEL_SOFTWARE
                }
            }
        } catch (e: Exception) {
            KeyProperties.SECURITY_LEVEL_UNKNOWN
        }
    }

    private fun levelName(level: Int): String = when (level) {
        KeyProperties.SECURITY_LEVEL_STRONGBOX -> "StrongBox"
        KeyProperties.SECURITY_LEVEL_TRUSTED_ENVIRONMENT -> "TEE"
        KeyProperties.SECURITY_LEVEL_SOFTWARE -> "программный"
        KeyProperties.SECURITY_LEVEL_UNKNOWN_SECURE -> "железо без уточнения"
        else -> "неизвестно"
    }

    private fun hasStrongBoxFeature(): Boolean = runCatching {
        appContext.packageManager.hasSystemFeature("android.hardware.strongbox_keystore")
    }.getOrDefault(false)

    private fun keyguard(): KeyguardManager =
        appContext.getSystemService(Context.KEYGUARD_SERVICE) as KeyguardManager

    private fun deviceLockedNote(): String = runCatching {
        if (keyguard().isDeviceLocked) "да" else "нет"
    }.getOrElse { "выяснить не удалось" }

    private fun secureLockNote(): String = runCatching {
        if (keyguard().isDeviceSecure) "да" else "НЕТ — хранилище защищено слабее"
    }.getOrElse { "выяснить не удалось" }

    /**
     * The exception class together with its message.
     *
     * Failures are told apart by type, not by this string — it is needed by
     * the report, and only by it. It does not go into the owner's red banner:
     * a Java class name is useless to them and exposes internals.
     */
    private fun describe(e: Throwable): String {
        val name = e.javaClass.name
        val msg = e.message
        val self = if (msg.isNullOrBlank()) name else "$name: $msg"
        val cause = e.cause
        return if (cause != null && cause !== e) "$self <- ${cause.javaClass.name}" else self
    }

    private fun ok(level: Int, note: String): ByteArray =
        byteArrayOf(STATUS_OK.toByte(), level.toByte()) + note.toByteArray(Charsets.UTF_8)

    private fun fail(status: Int, message: String): ByteArray =
        byteArrayOf(status.toByte()) + message.toByteArray(Charsets.UTF_8)
}

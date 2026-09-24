package io.apeiron.apeiron

import android.app.KeyguardManager
import android.content.Context
import android.os.Build
import android.os.SystemClock
import android.provider.Settings
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyInfo
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.security.keystore.StrongBoxUnavailableException
import java.nio.ByteBuffer
import java.security.KeyStore
import javax.crypto.KeyGenerator
import javax.crypto.Mac
import javax.crypto.SecretKey
import javax.crypto.SecretKeyFactory

/**
 * Hardware key store: everything that requires Android Keystore.
 *
 * ## Why this is in Kotlin, not Rust
 *
 * The stage note prescribed going to Keystore "over JNI directly from Rust,
 * not through a Dart plugin — otherwise key material will pass through Dart
 * and violate R-004". The reason is right, the conclusion is not: Kotlin is
 * not Dart. Everything goes Keystore → Kotlin → JNI → Rust and never reaches
 * Dart, R-004 is intact.
 *
 * But the difference in cost is large. If the caller is Rust, working with
 * Keystore comes with hand-built method descriptors, a local reference table
 * where an attached native thread is guaranteed sixteen slots, an exception
 * check after every call, and telling Keystore failure types apart by
 * comparing class names. All of it under `cfg(target_os = "android")`, which
 * does not compile on the development machine at all. Here there is
 * `try/catch` with real types, and Gradle checks the file on every build.
 * Decision R-010 in `docs/threat-log.md`.
 *
 * ## What the key is for (R-011)
 *
 * A non-exportable HMAC-SHA256 key. The PIN goes into it on every attempt:
 * Rust derives a value from the PIN and asks for `k` sequential HMACs of it
 * ([hmacChain]); the wrap key of the database key is derived from the result.
 * Each guess therefore costs `k` operations inside the secure hardware of
 * this phone. The earlier design — an AES key that unwraps the database key,
 * with the PIN mixed in afterwards — gave the unwrapped secret away in a
 * single call to anyone with root, after which all PINs are tried offline.
 *
 * ## What is here and what is not
 *
 * Here: creating the key, the HMAC chain, the boot clock, the hardware level,
 * erasure. Not here: files, formats, the attempt counter, mutexes, or any
 * state apart from a context reference. All of that belongs to Rust.
 */
object Vault {
    /** All is well. */
    private const val STATUS_OK = 0

    /** Retry later; the data is intact. Everything not GONE lands here. */
    private const val STATUS_TRANSIENT = 1

    /**
     * Key gone. Nothing can open the vault; the only way out is to start over.
     *
     * Reachable from exactly three conditions, and this is the most important
     * place in the file: `containsAlias` returned false, `getKey` returned
     * null, `KeyPermanentlyInvalidatedException`. Everything else is TRANSIENT.
     *
     * The reason is not theoretical. `setUnlockedDeviceRequired` makes the
     * keystore refuse on an **unlocked** device if it was unlocked with weak
     * biometrics — a confirmed firmware defect. If that is interpreted as "key
     * lost" and the key is reissued, the app destroys the owner's
     * conversations because of a transient failure.
     */
    private const val STATUS_GONE = 2

    /** Our own error. Like TRANSIENT, it does not touch the data. */
    private const val STATUS_INTERNAL = 3

    /** The HMAC key behind the PIN. */
    private const val ALIAS = "apeiron.kek.v2"

    /**
     * The AES key of builds before the PIN. Never used any more, only deleted:
     * data sealed with it is not carried over (see `docs/storage.md`).
     */
    private const val LEGACY_ALIAS = "apeiron.kek.v1"

    private const val PROVIDER = "AndroidKeyStore"
    private const val MAC_ALGORITHM = "HmacSHA256"
    private const val CHAIN_BYTES = 32

    /** Same bound as on the Rust side; a larger number is our own error. */
    private const val MAX_ROUNDS = 20_000

    /**
     * How many times one step of the chain may fail before the whole chain is
     * reported as transient. A single operation can fail for reasons that say
     * nothing about the key (a busy backend, an operation pruned by
     * keystore2); HMAC is deterministic, so repeating the step is safe.
     */
    private const val STEP_RETRIES = 3

    private lateinit var appContext: Context

    /** What happened on the last attempt to create the key — for the report. */
    private var lastKeyNote: String = "key not requested yet"

    /** How the attempt to connect to the core ended. The only way to learn
     * of a failure: before the Flutter engine exists, there is nowhere to
     * show it. */
    private var registrationNote: String = "not connected to the core yet"

    /**
     * Called from [MainActivity] before Dart starts.
     *
     * The library is loaded here too: `System.loadLibrary` and `dlopen` from
     * Dart land in the same linker namespace and find the same object by
     * soname, so there remains a single copy of the code.
     *
     * **It throws nothing outward.** The call comes from `onCreate` before the
     * Flutter engine is up: an exception from here is a black screen without a
     * single line on it. On failure Rust is left without the references and
     * responds with a clear error, and the cause goes into the report.
     */
    @JvmStatic
    fun register(context: Context) {
        appContext = context.applicationContext
        try {
            System.loadLibrary("rust_lib_apeiron")
            nativeRegister(this, appContext.filesDir.absolutePath)
            registrationNote = "connected to the core"
        } catch (e: Throwable) {
            // Throwable, not Exception: UnsatisfiedLinkError is an Error.
            registrationNote = "NOT CONNECTED TO THE CORE: ${describe(e)}"
        }
    }

    /**
     * Gives Rust a reference to itself and the data directory path.
     *
     * The instance is passed as an explicit parameter, not taken from the
     * implicit JNI arguments: this way it does not matter whether Kotlin
     * compiles this method as static or as an instance method. `FindClass` is
     * not called anywhere on the Rust side: the system class loader would not
     * see the app's class from a worker thread anyway.
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
     * number from `KeyInfo.getSecurityLevel()`, or `[status] + message`. The
     * note says how the key came to be (whether StrongBox was tried and how it
     * ended); Rust stores it, because here it would die with the process.
     *
     * @param allowCreate create the key if it is missing. Rust passes `false`
     *   when the wrapper file already exists: then a missing key means "key
     *   gone", and reissuing it would destroy the data irreversibly.
     */
    fun ensureKey(allowCreate: Boolean): ByteArray {
        return try {
            val store = openStore()
            val existing = loadKey(store)
            if (existing != null) {
                return ok(securityLevelOf(existing), "")
            }
            if (!allowCreate) {
                lastKeyNote = "no alias, but the wrapper file exists"
                return fail(STATUS_GONE, "the vault key is missing from the secure hardware")
            }
            val created = createKey(store)
            ok(securityLevelOf(created), lastKeyNote)
        } catch (e: KeyPermanentlyInvalidatedException) {
            lastKeyNote = describe(e)
            fail(STATUS_GONE, "the vault key was permanently invalidated by the system")
        } catch (e: Exception) {
            lastKeyNote = describe(e)
            fail(STATUS_TRANSIENT, describe(e))
        }
    }

    /**
     * `x(i+1) = HMAC(x(i))`, `rounds` times, starting from `input`.
     *
     * Response: `[STATUS_OK] + 32 bytes`, or `[status] + message`.
     *
     * The loop is here and not in Rust so that the chain costs one JNI call,
     * not `rounds` of them. Every step is still a separate operation inside the
     * secure hardware — that is the whole point.
     *
     * Intermediate values are wiped as the loop goes. This narrows the window
     * and does not close it: the ART collector moves objects, and the returned
     * array stays on the Java heap until collected.
     */
    fun hmacChain(input: ByteArray, rounds: Int): ByteArray {
        if (input.size != CHAIN_BYTES || rounds < 1 || rounds > MAX_ROUNDS) {
            return fail(STATUS_INTERNAL, "bad chain arguments: ${input.size} bytes, $rounds rounds")
        }
        var x = input.copyOf()
        return try {
            val key = loadKey(openStore())
                ?: return fail(STATUS_GONE, "the vault key is missing from the secure hardware")
            var mac = newMac(key)
            var done = 0
            var failures = 0
            while (done < rounds) {
                val next = try {
                    mac.doFinal(x)
                } catch (e: KeyPermanentlyInvalidatedException) {
                    throw e
                } catch (e: Exception) {
                    failures++
                    if (failures > STEP_RETRIES) throw e
                    mac = newMac(key)
                    continue
                }
                x.fill(0)
                x = next
                done++
            }
            byteArrayOf(STATUS_OK.toByte()) + x
        } catch (e: KeyPermanentlyInvalidatedException) {
            fail(STATUS_GONE, "the vault key was permanently invalidated by the system")
        } catch (e: Exception) {
            fail(STATUS_TRANSIENT, describe(e))
        } finally {
            x.fill(0)
        }
    }

    /**
     * The boot clock: `[STATUS_OK] + BOOT_COUNT (8, BE) + elapsedRealtime (8, BE)`.
     *
     * Delays between PIN attempts run on this clock and not on wall time, which
     * the holder of the phone can change in the settings. `BOOT_COUNT` is
     * missing on some firmware; then it is -1, and Rust recognises a reboot by
     * the elapsed time going backwards.
     */
    fun bootClock(): ByteArray {
        val boots = try {
            Settings.Global.getInt(appContext.contentResolver, Settings.Global.BOOT_COUNT).toLong()
        } catch (e: Exception) {
            -1L
        }
        val elapsed = SystemClock.elapsedRealtime()
        val buf = ByteBuffer.allocate(16).putLong(boots).putLong(elapsed).array()
        return byteArrayOf(STATUS_OK.toByte()) + buf
    }

    /**
     * Erases the key, and the legacy key of builds before the PIN if it is
     * still there. After this the vault cannot be recovered — that is the
     * point (R-005, cryptographic erasure).
     */
    fun destroy(): ByteArray {
        return try {
            val store = openStore()
            for (alias in listOf(ALIAS, LEGACY_ALIAS)) {
                if (store.containsAlias(alias)) {
                    store.deleteEntry(alias)
                }
            }
            lastKeyNote = "key erased"
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
        out.append("device: ").append(Build.MANUFACTURER)
            .append(' ').append(Build.MODEL).append('\n')
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            // Which secure hardware we sit on matters for R-011: key extraction
            // from the TEE is a known class of attacks on some chip families.
            out.append("SoC: ").append(Build.SOC_MANUFACTURER)
                .append(' ').append(Build.SOC_MODEL).append('\n')
        }
        out.append("core link: ").append(registrationNote).append('\n')
        out.append("alias: ").append(ALIAS).append('\n')
        try {
            val store = openStore()
            val present = store.containsAlias(ALIAS)
            out.append("alias present: ").append(if (present) "yes" else "no").append('\n')
            if (present) {
                val key = loadKey(store)
                if (key == null) {
                    out.append("key readable: NO, getKey returned null\n")
                } else {
                    val level = securityLevelOf(key)
                    out.append("level, raw: ").append(level).append('\n')
                    out.append("level: ").append(levelName(level)).append('\n')
                }
            }
            out.append("legacy alias ").append(LEGACY_ALIAS).append(" present: ")
                .append(if (store.containsAlias(LEGACY_ALIAS)) "yes" else "no").append('\n')
        } catch (e: Exception) {
            out.append("reading the keystore: ").append(describe(e)).append('\n')
        }
        out.append("about the key in this run: ").append(lastKeyNote).append('\n')
        out.append("StrongBox declared by the system: ")
            .append(if (hasStrongBoxFeature()) "yes" else "no").append('\n')
        out.append("device locked now: ").append(deviceLockedNote()).append('\n')
        out.append("screen lock set: ").append(secureLockNote()).append('\n')
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

    private fun newMac(key: SecretKey): Mac {
        val mac = Mac.getInstance(MAC_ALGORITHM)
        mac.init(key)
        return mac
    }

    /**
     * Creates the key: tries StrongBox first, and on any failure falls back
     * to TEE.
     *
     * The fallback catches `Exception` as a whole, not just
     * `StrongBoxUnavailableException`: AOSP maps only one failure code to
     * this type, the others arrive as `ProviderException` or
     * `KeyStoreException`, and on some firmware generation fails
     * non-reproducibly. The alias is deleted before the retry — a failed
     * generation sometimes leaves a half-written entry.
     *
     * This is reached only when there is no wrapper file yet, so nothing can
     * be lost.
     */
    private fun createKey(store: KeyStore): SecretKey {
        try {
            val key = generate(strongBox = true)
            probe(key)
            lastKeyNote = "created with StrongBox"
            return key
        } catch (e: StrongBoxUnavailableException) {
            lastKeyNote = "StrongBox unavailable (${describe(e)})"
        } catch (e: Exception) {
            lastKeyNote = "StrongBox failed (${describe(e)})"
        }
        runCatching { if (store.containsAlias(ALIAS)) store.deleteEntry(ALIAS) }
        val key = generate(strongBox = false)
        probe(key)
        lastKeyNote = "$lastKeyNote; created without StrongBox"
        return key
    }

    private fun generate(strongBox: Boolean): SecretKey {
        val builder = KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_SIGN)
            .setKeySize(256)
            // The hardware refuses to work while the phone is locked. This is
            // what file-system encryption does not give: on a locked but booted
            // phone the storage is already decrypted.
            .setUnlockedDeviceRequired(true)
        if (strongBox) {
            builder.setIsStrongBoxBacked(true)
        }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_HMAC_SHA256, PROVIDER)
        generator.init(builder.build())
        return generator.generateKey()
    }

    /**
     * The key is accepted only after two HMACs of the same input agree.
     *
     * StrongBox can refuse not at `generateKey` but later, at `Mac.init` or
     * `doFinal`, and then a "successfully created" key would turn out broken
     * in the owner's hands. And a chain that is not deterministic would seal
     * the database key under a value that can never be reproduced.
     */
    private fun probe(key: SecretKey) {
        val sample = ByteArray(CHAIN_BYTES) { it.toByte() }
        val a = newMac(key).doFinal(sample)
        val b = newMac(key).doFinal(sample)
        val same = a.size == CHAIN_BYTES && a.contentEquals(b)
        a.fill(0)
        b.fill(0)
        if (!same) {
            throw IllegalStateException("the key failed the probe: two HMACs of one input differ")
        }
    }

    /**
     * What the system **reports** about the key's protection level.
     *
     * Precisely "reports". There is no attestation for symmetric keys, and
     * `KeyInfo` is a self-report by the framework, executed in our own
     * process. The number is fit for an honest label in the UI and unfit as
     * proof. The numbers are those of `KeyProperties`: -2 unknown, -1
     * hardware without specifics, 0 software, 1 TEE, 2 StrongBox. On API
     * 28-30 only `isInsideSecureHardware()` exists — hence -1.
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
        KeyProperties.SECURITY_LEVEL_SOFTWARE -> "software"
        KeyProperties.SECURITY_LEVEL_UNKNOWN_SECURE -> "hardware, unspecified"
        else -> "unknown"
    }

    private fun hasStrongBoxFeature(): Boolean = runCatching {
        appContext.packageManager.hasSystemFeature("android.hardware.strongbox_keystore")
    }.getOrDefault(false)

    private fun keyguard(): KeyguardManager =
        appContext.getSystemService(Context.KEYGUARD_SERVICE) as KeyguardManager

    private fun deviceLockedNote(): String = runCatching {
        if (keyguard().isDeviceLocked) "yes" else "no"
    }.getOrElse { "could not tell" }

    private fun secureLockNote(): String = runCatching {
        if (keyguard().isDeviceSecure) "yes" else "NO — the vault is protected more weakly"
    }.getOrElse { "could not tell" }

    /**
     * The exception class together with its message.
     *
     * Failures are told apart by type, not by this string — it is for the
     * report only. It does not go into the owner's red banner: a Java class
     * name is useless to them and exposes internals.
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

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
 * Аппаратное хранилище ключа: всё, что требует Android Keystore.
 *
 * ## Почему это на Kotlin, а не на Rust
 *
 * Записка этапа предписывала ходить в Keystore «через JNI прямо из Rust, а не
 * через Dart-плагин, — иначе ключевой материал пройдёт через Dart и нарушит
 * R-004». Причина верна, вывод из неё — нет: Kotlin это не Dart. Ключ идёт
 * Keystore → Kotlin → JNI → Rust и в Dart не попадает никогда, R-004 цел.
 *
 * А разница в цене большая. Если вызывающая сторона — Rust, к работе с Keystore
 * прилагаются дескрипторы методов, собранные вручную; построение `String[]`
 * через `NewObjectArray`; таблица локальных ссылок, где на присоединённом
 * нативном потоке гарантировано шестнадцать слотов, а этой последовательности
 * нужно полсотни; проверка исключения после каждого вызова, потому что
 * непроверенное исключение убивает процесс при отсоединении потока; и
 * различение типов отказа Keystore сравнением строк с именами классов. Всё это
 * — код под `cfg(target_os = "android")`, который на рабочей машине не
 * компилируется вообще, и каждая опечатка в нём стоит цикла «собрать APK →
 * поставить → получить ответ».
 *
 * Здесь же `try/catch` с настоящими типами, а Gradle проверяет файл при каждой
 * сборке: неверное имя метода становится ошибкой компиляции, а не крахом на
 * телефоне. Решение R-010 в `docs/threat-log.md`.
 *
 * ## Что здесь есть и чего нет
 *
 * Есть: создание невыгружаемого ключа KEK, запечатывание и распечатывание им
 * коротких данных, определение уровня железа, стирание.
 *
 * Нет: работы с файлами, формата обёртки, мьютексов и вообще состояния, кроме
 * ссылки на контекст. Всё это принадлежит Rust. Этот файл — переходник к
 * Keystore, и только.
 */
object Vault {
    /** Всё в порядке. */
    private const val STATUS_OK = 0

    /** Повторить позже; данные целы. Сюда попадает всё, что не GONE. */
    private const val STATUS_TRANSIENT = 1

    /**
     * Ключ исчез. Расшифровать хранилище нельзя ничем, единственный выход —
     * начать заново.
     *
     * Достижим ровно из трёх условий, и это самое важное место файла:
     * `containsAlias` вернул false, `getKey` вернул null,
     * `KeyPermanentlyInvalidatedException`. Всё прочее — TRANSIENT.
     *
     * Причина не теоретическая. `setUnlockedDeviceRequired` бросает
     * `UserNotAuthenticatedException` на **разблокированном** устройстве, если
     * его разблокировали слабой биометрией — это подтверждённый дефект прошивок.
     * Если истолковать такое как «ключ потерян» и перевыпустить KEK, приложение
     * уничтожит переписку владельца из-за преходящего сбоя. Это должно быть
     * структурно невозможно, а не «мы аккуратно написали».
     */
    private const val STATUS_GONE = 2

    /** Наша собственная ошибка. Как и TRANSIENT, данных не трогает. */
    private const val STATUS_INTERNAL = 3

    private const val ALIAS = "apeiron.kek.v1"
    private const val PROVIDER = "AndroidKeyStore"
    private const val TRANSFORMATION = "AES/GCM/NoPadding"
    private const val GCM_TAG_BITS = 128
    private const val GCM_IV_BYTES = 12

    /**
     * Предел на то, что вообще можно пропускать через KEK.
     *
     * StrongBox медленнее TEE в десятки раз: мегабайт через него шифруется
     * порядка пятнадцати секунд. Через KEK проходят тридцать четыре байта —
     * обёртка ключа базы, — и предел стоит затем, чтобы однажды через него не
     * пустили саму базу, после чего приложение замрёт на глазах у владельца.
     */
    private const val MAX_WRAP_BYTES = 64

    private lateinit var appContext: Context

    /** Что произошло при последней попытке создать ключ — для отчёта. */
    private var lastKeyNote: String = "ключ ещё не запрашивался"

    /** Чем кончилась попытка связаться с ядром. Единственный способ узнать о
     * неудаче: до появления движка Flutter показать её негде. */
    private var registrationNote: String = "связь с ядром ещё не устанавливалась"

    /**
     * Вызывается из [MainActivity] до старта Dart.
     *
     * Здесь же грузится библиотека: `System.loadLibrary` и `dlopen` из Dart
     * попадают в одно пространство имён компоновщика и находят один и тот же
     * объект по soname, поэтому копия кода остаётся одна.
     *
     * **Наружу не бросает ничего, и это не перестраховка.** Вызов приходит из
     * `onCreate` до того, как поднят движок Flutter: исключение отсюда — это
     * чёрный экран и ни одной строчки на экране, потому что показывать её
     * нечем. `UnsatisfiedLinkError` тут вполне достижим (нет `.so` под эту
     * архитектуру, не нашёлся символ нативного метода), и разница между
     * «приложение не запускается» и «приложение запустилось и говорит, что
     * именно не вышло» — это ровно та разница, ради которой пишется
     * диагностика.
     *
     * При неудаче Rust просто остаётся без ссылок и отвечает внятной ошибкой,
     * а причина ложится в отчёт.
     */
    @JvmStatic
    fun register(context: Context) {
        appContext = context.applicationContext
        try {
            System.loadLibrary("rust_lib_apeiron")
            nativeRegister(this, appContext.filesDir.absolutePath)
            registrationNote = "связь с ядром установлена"
        } catch (e: Throwable) {
            // Throwable, а не Exception: UnsatisfiedLinkError — это Error.
            registrationNote = "СВЯЗЬ С ЯДРОМ НЕ УСТАНОВЛЕНА: ${describe(e)}"
        }
    }

    /**
     * Отдаёт Rust ссылку на себя и путь к каталогу данных.
     *
     * Экземпляр передаётся явным параметром, а не берётся из служебных
     * аргументов JNI: так не имеет значения, скомпилирует Kotlin этот метод
     * статическим или методом экземпляра. Класс не передаётся — метод ищется
     * по классу самого объекта. `FindClass` на стороне Rust не вызывается
     * нигде: системный загрузчик классов всё равно не увидел бы класс
     * приложения с рабочего потока.
     */
    private external fun nativeRegister(self: Any, filesDir: String)

    // ─────────────────────────────────────────────────────────────────────────
    // То, что зовёт Rust. Формат ответа один на все: первый байт — состояние,
    // дальше либо полезные данные, либо текст сообщения в UTF-8.
    // ─────────────────────────────────────────────────────────────────────────

    /**
     * Убеждается, что ключ на месте, и сообщает уровень железа.
     *
     * Ответ: `[STATUS_OK, уровень]`, где уровень — сырое число из
     * `KeyInfo.getSecurityLevel()`, либо `[состояние] + сообщение`.
     *
     * @param allowCreate создавать ключ, если его нет. Rust передаёт `false`,
     *   когда файл обёртки уже существует: в этой ситуации отсутствие ключа
     *   означает не «первый запуск», а «ключ исчез», и перевыпуск уничтожил бы
     *   данные безвозвратно.
     */
    fun ensureKey(allowCreate: Boolean): ByteArray {
        return try {
            val store = openStore()
            val existing = loadKey(store)
            if (existing != null) {
                return byteArrayOf(STATUS_OK.toByte(), securityLevelOf(existing).toByte())
            }
            if (!allowCreate) {
                lastKeyNote = "алиаса нет, а файл обёртки есть"
                return fail(
                    STATUS_GONE,
                    "ключ хранилища отсутствует в защищённом модуле устройства",
                )
            }
            val created = createKey(store)
            byteArrayOf(STATUS_OK.toByte(), securityLevelOf(created).toByte())
        } catch (e: KeyPermanentlyInvalidatedException) {
            lastKeyNote = describe(e)
            fail(STATUS_GONE, "ключ хранилища необратимо обесценен системой")
        } catch (e: Exception) {
            lastKeyNote = describe(e)
            fail(STATUS_TRANSIENT, describe(e))
        }
    }

    /**
     * Запечатывает короткие данные ключом KEK.
     *
     * Ответ: `[STATUS_OK] + одноразовое число (12) + шифртекст`, либо
     * `[состояние] + сообщение`.
     *
     * Одноразовое число выдаёт Keystore, а не мы: у ключа включено
     * `setRandomizedEncryptionRequired`, и свой IV на шифровании запрещён.
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
     * Распечатывает то, что запечатал [wrap].
     *
     * На входе `одноразовое число (12) + шифртекст`, на выходе
     * `[STATUS_OK] + открытый текст` либо `[состояние] + сообщение`.
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
            // Приходит и когда устройство заперто, и когда его разблокировали
            // способом, которого железу не хватает. Различить по типу нельзя,
            // поэтому это НЕ «ключ исчез», а «повторите».
            fail(
                STATUS_TRANSIENT,
                "устройство заперто или разблокировано способом, которого не хватает",
            )
        } catch (e: Exception) {
            fail(STATUS_TRANSIENT, describe(e))
        } finally {
            // Сужает окно, но не закрывает его: сборщик мусора ART перемещает
            // объекты, а у Cipher свои промежуточные буферы, до которых отсюда
            // не дотянуться. Писать «затёрто» было бы неправдой.
            plain?.fill(0)
        }
    }

    /**
     * Стирает ключ. После этого хранилище не восстановить — в этом и смысл
     * (R-005, криптографическое стирание: уничтожается ключ, а не данные).
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
     * Полный отчёт о платформе. Секретов не содержит.
     *
     * Проверка на устройстве одна, и она обязана ответить на все вопросы сразу,
     * а не на тот, который догадались задать.
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
        out.append("последнее о ключе: ").append(lastKeyNote).append('\n')
        out.append("StrongBox заявлен системой: ")
            .append(if (hasStrongBoxFeature()) "да" else "нет").append('\n')
        out.append("устройство заперто сейчас: ").append(deviceLockedNote()).append('\n')
        out.append("блокировка экрана задана: ").append(secureLockNote()).append('\n')
        return out.toString()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Внутреннее
    // ─────────────────────────────────────────────────────────────────────────

    /**
     * `load(null)` обязателен: без него первый же `containsAlias` даёт
     * `KeyStoreException: Uninitialized keystore`.
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
     * Создаёт ключ: сначала пробует StrongBox, при любой неудаче откатывается
     * на TEE.
     *
     * Откат ловит `Exception` целиком, а не только
     * `StrongBoxUnavailableException`: AOSP отображает в этот тип единственный
     * код отказа, остальные приезжают как `ProviderException` или
     * `KeyStoreException`, а на части прошивок генерация падает
     * невоспроизводимо. Перед повторной попыткой алиас удаляется — неудачная
     * генерация местами оставляет полузапись, и следующий `containsAlias`
     * соврал бы.
     *
     * Сюда попадают только когда файла обёртки ещё нет, то есть терять нечего.
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
            // Решение, а не умолчание: запрет на свой IV закрывает повторное
            // использование одноразового числа, на котором построен
            // CVE-2021-25444 в keymaster Samsung.
            .setRandomizedEncryptionRequired(true)
            // Железо отказывается работать, пока телефон заперт. Это то, чего не
            // даёт шифрование файловой системы: на запертом, но загруженном
            // телефоне хранилище уже расшифровано.
            .setUnlockedDeviceRequired(true)
        if (strongBox) {
            builder.setIsStrongBoxBacked(true)
        }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, PROVIDER)
        generator.init(builder.build())
        return generator.generateKey()
    }

    /**
     * Полный круг: зашифровать и расшифровать, сверив результат.
     *
     * Алиас считается принятым только после него. StrongBox умеет отказывать не
     * на `generateKey`, а позже — на `Cipher.init` или `doFinal`, и тогда
     * «успешно созданный» ключ оказался бы нерабочим уже у владельца.
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
     * Что система **сообщает** об уровне защиты ключа.
     *
     * Именно «сообщает». Для симметричных ключей аттестации не существует:
     * цепочки сертификатов у них нет, и `KeyInfo` — это самоотчёт фреймворка,
     * исполняемый в нашем же процессе. Скомпрометированное устройство вернёт
     * что угодно. Число годится для честной надписи в интерфейсе и не годится
     * как доказательство.
     *
     * Числа те же, что у `KeyProperties`: -2 неизвестно, -1 железо без
     * уточнения, 0 программный, 1 TEE, 2 StrongBox. На API 28-30
     * `getSecurityLevel()` ещё нет, и остаётся `isInsideSecureHardware()`,
     * который различает только «железо или не железо» — отсюда -1.
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
     * Класс исключения вместе с сообщением.
     *
     * Отказы различаются по типу, а не по этой строке — она нужна отчёту, и
     * только ему. В красный баннер владельцу она не идёт: имя класса Java ему
     * бесполезно и выносит наружу внутренности.
     */
    private fun describe(e: Throwable): String {
        val name = e.javaClass.name
        val msg = e.message
        val self = if (msg.isNullOrBlank()) name else "$name: $msg"
        val cause = e.cause
        return if (cause != null && cause !== e) "$self <- ${cause.javaClass.name}" else self
    }

    private fun fail(status: Int, message: String): ByteArray =
        byteArrayOf(status.toByte()) + message.toByteArray(Charsets.UTF_8)
}

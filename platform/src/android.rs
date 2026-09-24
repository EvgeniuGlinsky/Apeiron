//! Обращение к `io.apeiron.apeiron.Vault` — единственное место с JNI.
//!
//! # Правила, которые здесь соблюдаются буквально
//!
//! * хранится только [`JavaVM`] и глобальная ссылка на объект. Никогда `Env` и
//!   никогда локальная ссылка: `Env` привязан к потоку, а вызовы приходят из
//!   пула потоков flutter_rust_bridge;
//! * присоединение потока — с областью видимости, крейт `jni` отсоединяет сам.
//!   Не «навсегда»: пул заменяет поток, в котором случилась паника, а поток,
//!   завершившийся присоединённым, роняет виртуальную машину;
//! * `FindClass` не вызывается нигде. Системный загрузчик классов не видит
//!   классов приложения, и с рабочего потока поиск вернул бы null. Ссылка на
//!   объект приходит из Kotlin при регистрации, а метод разыскивается по
//!   классу самого объекта;
//! * паники нет ни одной. В релизе `panic = "abort"`, и паника здесь — это
//!   мгновенная смерть процесса без единой строчки в журнале.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use jni::objects::{JByteArray, JObject, JString, JValue};
use jni::refs::Global;
use jni::{jni_sig, jni_str, Env, JavaVM};
use zeroize::{Zeroize, Zeroizing};

use crate::{KeyStatus, KeyWrapper, PlatformError, SecurityLevel};

/// Состояния, которыми отвечает Kotlin. Первый байт каждого ответа.
const STATUS_OK: u8 = 0;
const STATUS_TRANSIENT: u8 = 1;
const STATUS_GONE: u8 = 2;
const STATUS_INTERNAL: u8 = 3;

static VM: OnceLock<JavaVM> = OnceLock::new();
static VAULT: OnceLock<Global<JObject<'static>>> = OnceLock::new();
static FILES_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Единственное место во всём проекте, где снят запрет на `unsafe`.
///
/// Снят он не ради тела функции — там его нет, — а ради `#[export_name]`,
/// который порождает макрос `native_method!`: линт `unsafe_code` считает
/// экспорт символа небезопасной конструкцией наравне с блоком `unsafe`, и это
/// справедливо: имя символа связывает нас с виртуальной машиной Java по
/// соглашению, которое компилятор проверить не может.
#[allow(unsafe_code)]
mod jni_entry {
    use super::{register_impl, Env, Global, JObject, JString, JavaVM, FILES_DIR, VAULT, VM};

    /// Экспортируемый символ для `Vault.nativeRegister`.
    ///
    /// Имя задано явно, а не оставлено на автоматическое искажение: под этим же
    /// именем его ищет предохранитель, проверяющий готовый APK. Перегрузок у
    /// метода нет, поэтому короткой формы достаточно.
    ///
    /// Константа никем не используется — она нужна ради побочного действия
    /// макроса, который и создаёт экспорт.
    #[allow(dead_code)]
    pub(super) const NATIVE_REGISTER: jni::NativeMethod = jni::native_method! {
        java_type = "io.apeiron.apeiron.Vault",
        export = "Java_io_apeiron_apeiron_Vault_nativeRegister",
        fn native_register(self_obj: JObject, files_dir: JString) -> void,
    };

    /// Принимает от Kotlin ссылку на объект `Vault` и путь к каталогу данных.
    ///
    /// Ошибку наружу не отдаёт: бросить исключение здесь значило бы уронить
    /// приложение в конструкторе Activity, до появления Flutter-движка, то есть
    /// без единой строчки диагностики на экране. При неудаче глобальные ссылки
    /// просто остаются незаполненными, и первое же обращение к хранилищу
    /// скажет об этом внятно.
    fn native_register<'local>(
        env: &mut Env<'local>,
        _this: JObject<'local>,
        self_obj: JObject<'local>,
        files_dir: JString<'local>,
    ) -> Result<(), jni::errors::Error> {
        register_impl(env, self_obj, files_dir);
        Ok(())
    }

    /// Заполняет глобальные ссылки. Вынесено из `native_register`, чтобы тело
    /// экспортируемой функции оставалось в три строки.
    pub(super) fn fill(vm: JavaVM, vault: Global<JObject<'static>>, dir: std::path::PathBuf) {
        let _ = VM.set(vm);
        let _ = VAULT.set(vault);
        let _ = FILES_DIR.set(dir);
    }
}

fn register_impl<'local>(
    env: &mut Env<'local>,
    self_obj: JObject<'local>,
    files_dir: JString<'local>,
) {
    let Ok(vm) = env.get_java_vm() else {
        return;
    };
    let Ok(global) = env.new_global_ref(&self_obj) else {
        return;
    };
    let Ok(text) = files_dir.try_to_string(env) else {
        return;
    };
    let dir = PathBuf::from(text);
    jni_entry::fill(vm, global, dir);
}

/// Каталог, в котором приложению разрешено хранить свои файлы.
///
/// Путь приходит из Kotlin (`context.filesDir`), а не добывается из Rust через
/// `ActivityThread.currentApplication()`: с Android 11 доступ к скрытым API
/// ограничен, в том числе из JNI. Путь — не секрет, R-004 про ключи и открытый
/// текст.
pub fn storage_dir() -> Result<&'static Path, PlatformError> {
    FILES_DIR
        .get()
        .map(PathBuf::as_path)
        .ok_or_else(not_registered)
}

fn not_registered() -> PlatformError {
    PlatformError::Internal(
        "Vault не зарегистрирован: приложение запущено в обход MainActivity".to_string(),
    )
}

impl From<jni::errors::Error> for PlatformError {
    /// Исключение Java сюда попасть не должно: Kotlin ловит всё сам и отвечает
    /// состоянием. Если оно всё-таки здесь — значит разошлись имя или подпись
    /// метода, и это наша ошибка, а не отказ железа.
    fn from(e: jni::errors::Error) -> Self {
        PlatformError::Internal(e.to_string())
    }
}

/// Аппаратный ключ Android.
///
/// Состояния не имеет: всё, что нужно, лежит в глобальных ссылках, заполненных
/// при регистрации.
#[derive(Debug, Clone, Copy, Default)]
pub struct AndroidVault;

/// Разбирает ответ Kotlin: первый байт — состояние, дальше данные или текст.
fn decode(reply: &[u8]) -> Result<&[u8], PlatformError> {
    let (status, rest) = reply
        .split_first()
        .ok_or_else(|| PlatformError::Internal("пустой ответ от Vault".to_string()))?;
    let text = || String::from_utf8_lossy(rest).into_owned();
    match *status {
        STATUS_OK => Ok(rest),
        STATUS_TRANSIENT => Err(PlatformError::Transient(text())),
        STATUS_GONE => Err(PlatformError::Gone),
        STATUS_INTERNAL => Err(PlatformError::Internal(text())),
        other => Err(PlatformError::Internal(format!(
            "Vault вернул неизвестное состояние {other}"
        ))),
    }
}

/// Довод единственного вызываемого метода.
enum Arg<'a> {
    None,
    Bool(bool),
    Bytes(&'a [u8]),
}

impl KeyWrapper for AndroidVault {
    fn ensure_key(&self, allow_create: bool) -> Result<KeyStatus, PlatformError> {
        let reply = call(
            jni_str!("ensureKey"),
            &jni_sig!((allow: boolean) -> jbyte[]),
            Arg::Bool(allow_create),
        )?;
        let body = decode(&reply)?;
        let raw = body
            .first()
            .ok_or_else(|| PlatformError::Internal("ответ ensureKey без уровня".to_string()))?;
        // Уровень приходит знаковым байтом: значений у getSecurityLevel() пять,
        // и два из них отрицательные.
        let level = SecurityLevel::from_raw(i32::from(*raw as i8));
        let note = String::from_utf8_lossy(body.get(1..).unwrap_or_default()).into_owned();
        Ok(KeyStatus { level, note })
    }

    fn wrap(&self, plain: &[u8]) -> Result<Vec<u8>, PlatformError> {
        let reply = call(
            jni_str!("wrap"),
            &jni_sig!((data: jbyte[]) -> jbyte[]),
            Arg::Bytes(plain),
        )?;
        Ok(decode(&reply)?.to_vec())
    }

    fn unwrap(&self, iv_and_ct: &[u8]) -> Result<Zeroizing<Vec<u8>>, PlatformError> {
        let mut reply = call(
            jni_str!("unwrap"),
            &jni_sig!((data: jbyte[]) -> jbyte[]),
            Arg::Bytes(iv_and_ct),
        )?;
        let out = decode(&reply).map(|body| Zeroizing::new(body.to_vec()));
        // Затирает копию, сделанную на стороне Rust. Копию на куче Java
        // затирает Kotlin — и там, и там это сужает окно, а не закрывает его.
        reply.zeroize();
        out
    }

    fn destroy(&self) -> Result<(), PlatformError> {
        let reply = call(jni_str!("destroy"), &jni_sig!(() -> jbyte[]), Arg::None)?;
        decode(&reply).map(|_| ())
    }

    fn diagnostics(&self) -> Result<String, PlatformError> {
        let vm = VM.get().ok_or_else(not_registered)?;
        let vault = VAULT.get().ok_or_else(not_registered)?;
        vm.attach_current_thread(|env| {
            env.with_local_frame(16, |env| {
                let value = env.call_method(
                    vault,
                    jni_str!("diagnostics"),
                    &jni_sig!(() -> java.lang.String),
                    &[],
                )?;
                let obj = value.l()?;
                let text = env.cast_local::<JString>(obj)?;
                Ok(text.try_to_string(env)?)
            })
        })
    }
}

/// Общий вызов метода `Vault`, возвращающего `byte[]`.
///
/// Довод строится внутри кадра локальных ссылок: массив, созданный снаружи,
/// пережил бы кадр и утёк. Кадр открывается явно, хотя присоединение потока
/// даёт свой: так число ссылок задано здесь и видно рядом с вызовом.
fn call(
    name: &jni::strings::JNIStr,
    sig: &jni::signature::MethodSignature,
    arg: Arg<'_>,
) -> Result<Vec<u8>, PlatformError> {
    let vm = VM.get().ok_or_else(not_registered)?;
    let vault = VAULT.get().ok_or_else(not_registered)?;
    vm.attach_current_thread(|env| {
        env.with_local_frame(24, |env| {
            let array = match arg {
                Arg::Bytes(bytes) => Some(env.byte_array_from_slice(bytes)?),
                _ => None,
            };
            let args: Vec<JValue> = match (&array, &arg) {
                (Some(a), _) => vec![JValue::Object(a.as_ref())],
                (None, Arg::Bool(v)) => vec![JValue::Bool(*v)],
                _ => Vec::new(),
            };
            let value = env.call_method(vault, name, sig, &args)?;
            let obj = value.l()?;
            let bytes = env.cast_local::<JByteArray>(obj)?;
            Ok(env.convert_byte_array(&bytes)?)
        })
    })
}

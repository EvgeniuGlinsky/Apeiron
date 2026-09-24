//! Calls into `io.apeiron.apeiron.Vault`: the only place with JNI.
//!
//! # Rules that are followed literally here
//!
//! * only the [`JavaVM`] and a global reference to the object are stored. Never an `Env` and
//!   never a local reference: an `Env` is bound to a thread, and the calls come from the
//!   flutter_rust_bridge thread pool;
//! * thread attachment is scoped; the `jni` crate detaches by itself.
//!   Not "permanently": the pool replaces a thread in which a panic happened, and a thread
//!   that exits while attached brings down the virtual machine;
//! * `FindClass` is not called anywhere. The system class loader does not see the
//!   application's classes, and from a worker thread the lookup would return null. The
//!   reference to the object comes from Kotlin at registration, and the method is looked up
//!   via the class of the object itself;
//! * there is not a single panic. In release `panic = "abort"`, and a panic here is
//!   instant death of the process without a single line in the log.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use jni::objects::{JByteArray, JObject, JString, JValue};
use jni::refs::Global;
use jni::{jni_sig, jni_str, Env, JavaVM};
use zeroize::{Zeroize, Zeroizing};

use crate::{KeyStatus, KeyWrapper, PlatformError, SecurityLevel};

/// The statuses Kotlin replies with. The first byte of every reply.
const STATUS_OK: u8 = 0;
const STATUS_TRANSIENT: u8 = 1;
const STATUS_GONE: u8 = 2;
const STATUS_INTERNAL: u8 = 3;

static VM: OnceLock<JavaVM> = OnceLock::new();
static VAULT: OnceLock<Global<JObject<'static>>> = OnceLock::new();
static FILES_DIR: OnceLock<PathBuf> = OnceLock::new();

/// The only place in the whole project where the ban on `unsafe` is lifted.
///
/// It is lifted not for the function body (there is none there) but for the
/// `#[export_name]` that the `native_method!` macro generates: the `unsafe_code` lint treats
/// exporting a symbol as an unsafe construct on a par with an `unsafe` block, and this is
/// fair: the symbol name binds us to the Java virtual machine by a
/// convention the compiler cannot check.
#[allow(unsafe_code)]
mod jni_entry {
    use super::{register_impl, Env, Global, JObject, JString, JavaVM, FILES_DIR, VAULT, VM};

    /// The exported symbol for `Vault.nativeRegister`.
    ///
    /// The name is given explicitly rather than left to automatic mangling: under this same
    /// name the build guard that checks the finished APK looks for it. The method has no
    /// overloads, so the short form is enough.
    ///
    /// The constant is not used by anyone: it is needed for the side effect of the
    /// macro, which is what creates the export.
    #[allow(dead_code)]
    pub(super) const NATIVE_REGISTER: jni::NativeMethod = jni::native_method! {
        java_type = "io.apeiron.apeiron.Vault",
        export = "Java_io_apeiron_apeiron_Vault_nativeRegister",
        fn native_register(self_obj: JObject, files_dir: JString) -> void,
    };

    /// Receives from Kotlin a reference to the `Vault` object and the data directory path.
    ///
    /// It does not return an error: throwing an exception here would mean crashing the app
    /// in the Activity constructor, before the Flutter engine appears, i.e. without
    /// a single line of diagnostics on the screen. On failure the global references
    /// simply stay unfilled, and the very first access to the storage
    /// will say so clearly.
    fn native_register<'local>(
        env: &mut Env<'local>,
        _this: JObject<'local>,
        self_obj: JObject<'local>,
        files_dir: JString<'local>,
    ) -> Result<(), jni::errors::Error> {
        register_impl(env, self_obj, files_dir);
        Ok(())
    }

    /// Fills the global references. Moved out of `native_register` so that the body
    /// of the exported function stays at three lines.
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

/// The directory where the application is allowed to keep its files.
///
/// The path comes from Kotlin (`context.filesDir`) rather than being obtained from Rust via
/// `ActivityThread.currentApplication()`: since Android 11 access to hidden APIs is
/// restricted, including from JNI. The path is not a secret; R-004 is about keys and
/// plaintext.
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
    /// A Java exception must not get here: Kotlin catches everything itself and replies
    /// with a status. If one is here after all, the method name or signature have
    /// diverged, and that is our bug, not a hardware failure.
    fn from(e: jni::errors::Error) -> Self {
        PlatformError::Internal(e.to_string())
    }
}

/// The Android hardware key.
///
/// Has no state: everything needed lives in the global references filled
/// at registration.
#[derive(Debug, Clone, Copy, Default)]
pub struct AndroidVault;

/// Parses a Kotlin reply: the first byte is the status, then data or text.
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

/// The argument of the only method called.
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
        // The level comes as a signed byte: getSecurityLevel() has five values,
        // and two of them are negative.
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
        // Wipes the copy made on the Rust side. The copy on the Java heap is
        // wiped by Kotlin; in both places this narrows the window rather than closing it.
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

/// A common call of a `Vault` method that returns `byte[]`.
///
/// The argument is built inside a local reference frame: an array created outside
/// would outlive the frame and leak. The frame is opened explicitly, although attaching the
/// thread gives one of its own: so the reference count is set here, visible next to the call.
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

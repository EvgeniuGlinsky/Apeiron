//! Access to the platform's hardware key store.
//!
//! The crate does exactly one thing: it lets the storage run an HMAC chain on a key that
//! lives in the device's secure module and is never exported — the hardware half of the
//! PIN (R-011, [`HardwareKey`]). There is no wrapper format, no files, no attempt counter,
//! no mutexes and no state of any kind here: all of that belongs to `apeiron-store`,
//! which is tested in full on the development machine.
//!
//! # Why this is a separate crate
//!
//! `apeiron-core` declares `unsafe_code = "forbid"`, and that cannot be lifted even
//! deliberately. There is no JNI without `unsafe`: exporting a symbol for the Java
//! virtual machine is `#[export_name]`, and that falls under the same lint. That is why
//! it is `deny` here, not `forbid`, and there is exactly one exemption: on the `jni_entry`
//! module in `android.rs`.
//!
//! # Where the Keystore logic is
//!
//! In Kotlin, in `app/android/app/src/main/kotlin/io/apeiron/apeiron/Vault.kt`.
//! The rationale for the reversal is there too, in the file header, and in decision R-010
//! (`docs/threat-log.md`). In short: the ban in the stage memo was on **Dart**,
//! because from there the key can no longer be wiped; Kotlin is not Dart, the key goes
//! Keystore → Kotlin → JNI → Rust and never reaches Dart. In exchange, all the fuss with
//! method descriptors, the local reference table and parsing Java exceptions
//! goes away, and Gradle checks the resulting code on every build, instead of
//! the failure being discovered on the phone.

mod hardware;

#[cfg(target_os = "android")]
mod android;

pub use hardware::{BootClock, HardwareKey};

#[cfg(target_os = "android")]
pub use android::{storage_dir, AndroidVault};

/// What went wrong when accessing the hardware store.
///
/// Three variants are enough, and they are split not by the cause of the failure but by
/// **what the application should do next**. That is the only distinction that has
/// consequences.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    /// Retry later. The data is intact.
    ///
    /// Everything ends up here except the three explicit conditions of [`PlatformError::Gone`],
    /// and this is done on purpose. `setUnlockedDeviceRequired` fails on an
    /// **unlocked** device if it was unlocked with weak
    /// biometrics: a confirmed firmware defect. Interpreting a transient failure
    /// as "key lost" would mean destroying the owner's conversations.
    #[error("the device's secure module is unavailable right now: {0}")]
    Transient(String),

    /// The key is gone. Nothing can decrypt the storage.
    ///
    /// Comes from exactly three conditions checked on the Kotlin side:
    /// `containsAlias` returned false, `getKey` returned null,
    /// `KeyPermanentlyInvalidatedException`. It happens not only on
    /// reinstall: Keystore keys disappear when the screen lock is removed and,
    /// according to years of developer complaints, after firmware updates on some
    /// devices.
    #[error(
        "THE STORAGE KEY IS GONE FROM THIS PHONE'S SECURE MODULE. \
         Nothing can decrypt the conversations. The only way out is to start over."
    )]
    Gone,

    /// Our own error. Like [`PlatformError::Transient`], it does not touch the
    /// data.
    #[error("internal error accessing the key store: {0}")]
    Internal(String),
}

impl PlatformError {
    /// Whether the attempt can be retried without losing data.
    pub fn is_retryable(&self) -> bool {
        !matches!(self, Self::Gone)
    }
}

/// What the system **reports** about the key's protection level.
///
/// Precisely "reports", and the word was chosen with care. For symmetric keys there is no
/// attestation: they have no certificate chain, and `KeyInfo` is the framework's
/// self-report, executed in our own process. A compromised device
/// will return anything at all. The number is fit for an honest label on the screen and
/// unfit as proof, and that is how it must be shown.
///
/// The raw number is kept as is. `KeyInfo.getSecurityLevel()` has five values,
/// not three: besides "software", "TEE" and "StrongBox" there are "unknown"
/// and "hardware, unspecified", and the latter really does come from devices with an old
/// keymaster. Collapsing them into three branches would drop such devices into
/// "software key" and scare the owner for nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecurityLevel {
    raw: i32,
}

impl SecurityLevel {
    /// Unknown (`KeyProperties.SECURITY_LEVEL_UNKNOWN`).
    pub const UNKNOWN: i32 = -2;
    /// Hardware, but which exactly cannot be said (`SECURITY_LEVEL_UNKNOWN_SECURE`).
    pub const UNKNOWN_SECURE: i32 = -1;
    /// Software implementation (`SECURITY_LEVEL_SOFTWARE`).
    pub const SOFTWARE: i32 = 0;
    /// Trusted execution environment (`SECURITY_LEVEL_TRUSTED_ENVIRONMENT`).
    pub const TRUSTED_ENVIRONMENT: i32 = 1;
    /// A separate secure element (`SECURITY_LEVEL_STRONGBOX`).
    pub const STRONGBOX: i32 = 2;

    /// From the raw number, as the system returned it.
    pub fn from_raw(raw: i32) -> Self {
        Self { raw }
    }

    /// The raw number. Shown next to the name, so that an unfamiliar value
    /// is visible rather than replaced by the nearest familiar one.
    pub fn raw(&self) -> i32 {
        self.raw
    }

    /// The name, in English, for diagnostics. The app shows its own localized name, chosen
    /// by the raw number.
    pub fn name(&self) -> &'static str {
        match self.raw {
            Self::STRONGBOX => "StrongBox",
            Self::TRUSTED_ENVIRONMENT => "TEE",
            Self::SOFTWARE => "software",
            Self::UNKNOWN_SECURE => "secure hardware, unspecified",
            _ => "unknown",
        }
    }

    /// Whether the key lives in hardware.
    ///
    /// "Unknown" is counted as a negative answer on purpose: doubt must be
    /// resolved in favor of a warning, not in favor of reassurance.
    pub fn is_hardware(&self) -> bool {
        matches!(
            self.raw,
            Self::STRONGBOX | Self::TRUSTED_ENVIRONMENT | Self::UNKNOWN_SECURE
        )
    }
}

/// The state of the hardware key: where it lives and how it came about.
#[derive(Debug, Clone)]
pub struct KeyStatus {
    /// What the system reports about the protection level.
    pub level: SecurityLevel,
    /// How the key came about: whether StrongBox was tried and how the attempt ended.
    ///
    /// Empty if the key was not created during this launch. The note lives on the platform
    /// side only until the end of the process, and that is the whole reason to hand it over:
    /// otherwise it disappears exactly by the time someone wants to read it,
    /// and it is the most interesting line of the report on a device where StrongBox exists
    /// but misbehaves.
    pub note: String,
}

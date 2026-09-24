//! The hardware key behind the PIN (R-011).
//!
//! # Why an HMAC chain and not a wrapped key
//!
//! The first version wrapped the database key with an AES key in the keystore and would
//! have mixed the PIN in *after* unwrapping. That does not stop someone with root on the
//! phone: one call to the keystore gives them the unwrapped secret, and all 10⁶ six-digit
//! PINs are then tried offline in milliseconds.
//!
//! So the PIN goes **into** the hardware on every attempt. The keystore holds a
//! non-exportable HMAC key, and the wrap key is derived from `k` sequential HMACs of a
//! value derived from the PIN. Each guess costs `k` operations inside the secure hardware
//! of this particular phone; they cannot be moved elsewhere and one guess cannot be
//! parallelised. The same idea as a passcode entangled with the device UID key, minus the
//! hardware-enforced delays that Android does not offer to apps.
//!
//! What this does not survive: extraction of the key from the secure hardware. Then the
//! chain is free, and only the Argon2id step in front of it and the length of the PIN
//! remain (see `docs/threat-log.md`, R-011).

use zeroize::Zeroizing;

use crate::{KeyStatus, PlatformError};

/// A reading of the boot clock: `Settings.Global.BOOT_COUNT` and `elapsedRealtime`.
///
/// Delays between PIN attempts are measured on this clock and not on wall time, which the
/// holder of the phone can change in the settings. `boot_count` is `-1` where the firmware
/// does not report it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootClock {
    pub boot_count: i64,
    pub elapsed_ms: i64,
}

/// The non-exportable key in the secure hardware.
pub trait HardwareKey {
    /// Makes sure the key exists and reports its level.
    ///
    /// `allow_create` may be `true` **only** while no wrapper file exists. Otherwise a
    /// missing key means "key gone", not "first launch", and creating a new one would
    /// destroy the data for good.
    fn ensure_key(&self, allow_create: bool) -> Result<KeyStatus, PlatformError>;

    /// `x(i+1) = HMAC-SHA256_K(x(i))`, `rounds` times, starting from `input`.
    ///
    /// Every round is a separate operation inside the secure hardware. [`PlatformError::Gone`]
    /// comes only from the same three explicit conditions as everywhere (no alias, `getKey`
    /// returned null, the key permanently invalidated); any other failure in the middle of
    /// the chain, including one that looks like a missing key, is transient.
    fn hmac_chain(
        &self,
        input: &[u8; 32],
        rounds: u32,
    ) -> Result<Zeroizing<[u8; 32]>, PlatformError>;

    /// The boot clock right now.
    fn boot_clock(&self) -> Result<BootClock, PlatformError>;

    /// Destroys the key, and the legacy key of builds before the PIN if it is still there.
    /// The vault cannot be recovered afterwards — that is the point (R-005).
    fn destroy(&self) -> Result<(), PlatformError>;

    /// Report about the platform. Contains no secrets.
    fn diagnostics(&self) -> Result<String, PlatformError>;
}

//! The scrambled PIN pad (R-007), as seen from Dart.
//!
//! Dart gets the layout to draw and the number of digits to show as dots, and sends back
//! only the position that was tapped. The digits themselves are assembled in Rust
//! (`crate::pin_entry`) and never cross this boundary.

use apeiron_store::{PinPrefs, PIN_LENGTHS};

use crate::paths::storage_dir;
use crate::pin_entry;

/// How the pad looks: the PIN's length, if known, and whether the keys are scrambled.
pub struct PinPadPrefs {
    /// 0 when unknown (a PIN set by an earlier build): the pad waits for "done".
    pub digits: u8,
    pub scrambled: bool,
}

/// The pad's settings. Readable while the vault is locked: they hold no secret.
#[flutter_rust_bridge::frb]
pub fn pin_pad_prefs() -> PinPadPrefs {
    let prefs = storage_dir()
        .map(|dir| PinPrefs::load(&dir))
        .unwrap_or_default();
    PinPadPrefs {
        digits: prefs.digits.unwrap_or(0),
        scrambled: prefs.scrambled,
    }
}

/// Sets the pad's settings: a length of 4, 6 or 8 digits (0 keeps it unknown) and the layout.
#[flutter_rust_bridge::frb]
pub fn set_pin_pad_prefs(digits: u8, scrambled: bool) -> Result<(), String> {
    if digits != 0 && !PIN_LENGTHS.contains(&usize::from(digits)) {
        return Err(apeiron_store::StorageError::BadPin.to_string());
    }
    let dir = storage_dir()?;
    PinPrefs {
        digits: (digits != 0).then_some(digits),
        scrambled,
    }
    .save(&dir)
    .map_err(|e| e.to_string())
}

/// Starts an attempt: a fresh layout — scrambled unless the owner chose the usual one — and
/// an empty pad.
///
/// Returns the digit shown at each of the ten positions, in reading order: three rows of
/// three, then the middle of the bottom row.
#[flutter_rust_bridge::frb]
pub fn pin_pad_begin() -> Result<Vec<u8>, String> {
    pin_entry::begin(pin_pad_prefs().scrambled)
}

/// The key at `position` was tapped. Returns how many digits are entered.
#[flutter_rust_bridge::frb]
pub fn pin_pad_press(position: u8) -> Result<u32, String> {
    pin_entry::press(position)
}

/// Removes the last digit. Returns how many digits are entered.
#[flutter_rust_bridge::frb]
pub fn pin_pad_erase() -> Result<u32, String> {
    pin_entry::erase()
}

/// Forgets everything typed, including a pending first entry of a new PIN.
#[flutter_rust_bridge::frb]
pub fn pin_pad_clear() -> Result<(), String> {
    pin_entry::clear()
}

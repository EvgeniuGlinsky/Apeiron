//! The scrambled PIN pad (R-007), as seen from Dart.
//!
//! Dart gets the layout to draw and the number of digits to show as dots, and sends back
//! only the position that was tapped. The digits themselves are assembled in Rust
//! (`crate::pin_entry`) and never cross this boundary.

use crate::pin_entry;

/// Starts an attempt: a fresh random layout and an empty pad.
///
/// Returns the digit shown at each of the ten positions, in reading order: three rows of
/// three, then the middle of the bottom row.
#[flutter_rust_bridge::frb]
pub fn pin_pad_begin() -> Result<Vec<u8>, String> {
    pin_entry::begin()
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

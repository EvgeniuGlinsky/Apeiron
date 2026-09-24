//! How the PIN pad looks: the length of the PIN and whether the keys are scrambled.
//!
//! A separate, open file next to the vault: the pad needs it before anything is unlocked. It
//! holds no secret — the length shows as dots on the screen anyway — and whoever can write
//! into the app's data directory can delete the vault outright, so sealing it would protect
//! nothing. A missing or unreadable file reads as the default: scrambled keys, length unknown
//! (the pad then waits for the "done" key, as for a PIN set by an earlier build).

use std::path::Path;

use crate::fsutil::write_atomically;
use crate::StorageError;

pub const PIN_PREFS_FILE: &str = "pin.prefs";

const MAGIC: &[u8; 6] = b"APPRF1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinPrefs {
    /// Digits of the PIN, if known. The pad submits by itself at the last one.
    pub digits: Option<u8>,
    /// Scrambled keys (R-007), or the usual 1-2-3 layout.
    pub scrambled: bool,
}

impl Default for PinPrefs {
    fn default() -> Self {
        Self {
            digits: None,
            scrambled: true,
        }
    }
}

impl PinPrefs {
    pub fn load(dir: &Path) -> Self {
        let Ok(raw) = std::fs::read(dir.join(PIN_PREFS_FILE)) else {
            return Self::default();
        };
        match raw.split_at_checked(MAGIC.len()) {
            Some((magic, [digits, scrambled])) if magic == MAGIC && *scrambled <= 1 => Self {
                digits: (*digits != 0).then_some(*digits),
                scrambled: *scrambled == 1,
            },
            _ => Self::default(),
        }
    }

    pub fn save(&self, dir: &Path) -> Result<(), StorageError> {
        std::fs::create_dir_all(dir)?;
        let mut out = MAGIC.to_vec();
        out.push(self.digits.unwrap_or(0));
        out.push(u8::from(self.scrambled));
        write_atomically(dir, &dir.join(PIN_PREFS_FILE), &out)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn prefs_round_trip_and_default_when_unreadable() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(PinPrefs::load(dir.path()), PinPrefs::default());
        let prefs = PinPrefs {
            digits: Some(8),
            scrambled: false,
        };
        prefs.save(dir.path()).unwrap();
        assert_eq!(PinPrefs::load(dir.path()), prefs);
        std::fs::write(dir.path().join(PIN_PREFS_FILE), b"APPRF1\x08\x07").unwrap();
        assert_eq!(PinPrefs::load(dir.path()), PinPrefs::default());
    }
}

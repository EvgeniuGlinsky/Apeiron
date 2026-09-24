//! The PIN as it is being typed: the scrambled layout and the digits.
//!
//! Lives outside `api/` on purpose: flutter_rust_bridge parses everything there, and
//! nothing here may cross the boundary. Dart gets the layout to draw and the number of
//! digits to show as dots; it sends back only the position that was tapped. The digits
//! themselves exist only here, in a buffer that is wiped when dropped (R-004, R-007).

use std::sync::{Mutex, OnceLock};

use apeiron_core::random_bytes;
use zeroize::Zeroizing;

/// Longest accepted PIN. Beyond this the pad stops accepting digits.
pub(crate) const MAX_DIGITS: usize = 16;

struct Entry {
    layout: [u8; 10],
    digits: Zeroizing<Vec<u8>>,
    /// First entry while a new PIN is being set; compared with the confirmation here,
    /// not in Dart.
    first: Option<Zeroizing<Vec<u8>>>,
}

static ENTRY: OnceLock<Mutex<Entry>> = OnceLock::new();

fn entry() -> &'static Mutex<Entry> {
    ENTRY.get_or_init(|| {
        Mutex::new(Entry {
            layout: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
            digits: Zeroizing::new(Vec::with_capacity(MAX_DIGITS)),
            first: None,
        })
    })
}

const POISONED: &str = "internal lock poisoned: restart the app";

/// A uniformly random permutation of the ten digits (Fisher–Yates with rejection
/// sampling, so that no position is more likely than another).
fn shuffled() -> Result<[u8; 10], String> {
    let mut layout = [0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9];
    for i in (1..layout.len()).rev() {
        let bound = (i + 1) as u8;
        // Largest multiple of `bound` that fits in a byte; values above it are
        // rejected, otherwise the low digits would come up slightly more often.
        let limit = u8::MAX - (u8::MAX % bound);
        let j = loop {
            let [b] = random_bytes::<1>().map_err(|e| e.to_string())?;
            if b < limit {
                break usize::from(b % bound);
            }
        };
        layout.swap(i, j);
    }
    Ok(layout)
}

/// Starts a new attempt: a fresh layout and no digits. Returns the layout.
pub(crate) fn begin() -> Result<Vec<u8>, String> {
    let layout = shuffled()?;
    let mut e = entry().lock().map_err(|_| POISONED.to_string())?;
    e.layout = layout;
    e.digits.clear();
    Ok(layout.to_vec())
}

/// Adds the digit at `position`. Returns how many digits are entered.
pub(crate) fn press(position: u8) -> Result<u32, String> {
    let mut e = entry().lock().map_err(|_| POISONED.to_string())?;
    let digit = *e
        .layout
        .get(usize::from(position))
        .ok_or_else(|| format!("no key at position {position}"))?;
    if e.digits.len() < MAX_DIGITS {
        e.digits.push(b'0' + digit);
    }
    Ok(e.digits.len() as u32)
}

/// Removes the last digit. Returns how many digits are entered.
pub(crate) fn erase() -> Result<u32, String> {
    let mut e = entry().lock().map_err(|_| POISONED.to_string())?;
    e.digits.pop();
    Ok(e.digits.len() as u32)
}

/// Forgets everything typed, including a pending first entry.
pub(crate) fn clear() -> Result<(), String> {
    let mut e = entry().lock().map_err(|_| POISONED.to_string())?;
    e.digits.clear();
    e.first = None;
    Ok(())
}

/// Takes the typed digits out, leaving the pad empty.
pub(crate) fn take() -> Result<Zeroizing<Vec<u8>>, String> {
    let mut e = entry().lock().map_err(|_| POISONED.to_string())?;
    let taken = Zeroizing::new(e.digits.to_vec());
    e.digits.clear();
    Ok(taken)
}

/// Stores the typed digits as the first entry of a new PIN.
pub(crate) fn keep_as_first() -> Result<u32, String> {
    let digits = take()?;
    let len = digits.len() as u32;
    let mut e = entry().lock().map_err(|_| POISONED.to_string())?;
    e.first = Some(digits);
    Ok(len)
}

/// The first entry of a new PIN (if any) and its confirmation.
pub(crate) type FirstAndConfirmation = (Option<Zeroizing<Vec<u8>>>, Zeroizing<Vec<u8>>);

/// Takes the first entry (if any) together with the typed confirmation.
pub(crate) fn take_first_and_confirmation() -> Result<FirstAndConfirmation, String> {
    let confirmation = take()?;
    let mut e = entry().lock().map_err(|_| POISONED.to_string())?;
    Ok((e.first.take(), confirmation))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_a_permutation_and_changes() {
        let mut seen_different = false;
        let first = shuffled().expect("randomness");
        for _ in 0..20 {
            let l = shuffled().expect("randomness");
            let mut sorted = l;
            sorted.sort_unstable();
            assert_eq!(sorted, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
            seen_different |= l != first;
        }
        assert!(seen_different, "twenty layouts in a row were identical");
    }

    #[test]
    fn every_digit_lands_everywhere() {
        // A crude uniformity check: over many layouts each digit visits each position.
        let mut hits = [[0u32; 10]; 10];
        for _ in 0..2_000 {
            for (pos, d) in shuffled().expect("randomness").iter().enumerate() {
                hits[usize::from(*d)][pos] += 1;
            }
        }
        for (d, row) in hits.iter().enumerate() {
            for (pos, n) in row.iter().enumerate() {
                assert!(
                    *n > 100,
                    "digit {d} at position {pos} only {n} times of 2000"
                );
            }
        }
    }
}

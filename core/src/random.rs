//! The source of randomness.
//!
//! One for the whole core and as short as possible: "give me N bytes from the
//! operating system". There are deliberately no intermediate generators: every layer
//! between us and the OS kernel is one more place where randomness may turn out not
//! to be random, and everything else rests on it.
//!
//! **Failure is not suppressed.** `getrandom` may fail to deliver bytes: in a sandboxed
//! environment, when file descriptors are exhausted, very early in system startup.
//! The surrounding libraries usually panic at this point (`x25519-dalek` does
//! `.expect("getrandom failure")`). We must not panic (in the core this is explicitly
//! forbidden by a lint), and there is no need to: the failure is raised upward as an
//! error, and the caller decides. Silently returning predictable bytes would be the
//! worst possible outcome, so there is no such path here.

use thiserror::Error;
use zeroize::Zeroize;

/// The operating system did not provide randomness.
#[derive(Debug, Error)]
#[error("операционная система не выдала случайные байты: {0}")]
pub struct RandomError(getrandom::Error);

/// Random bytes from the OS kernel.
pub fn random_bytes<const N: usize>() -> Result<[u8; N], RandomError> {
    let mut bytes = [0u8; N];
    if let Err(e) = getrandom::fill(&mut bytes) {
        // Wipe whatever managed to get written: a partially filled buffer is
        // predictable exactly to the extent it is not filled.
        bytes.zeroize();
        return Err(RandomError(e));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn returns_requested_length() {
        let a = random_bytes::<32>().expect("the OS must provide randomness");
        assert_eq!(a.len(), 32);
    }

    #[test]
    fn two_calls_differ() {
        // This is not a test of randomness quality (a test cannot show that),
        // but of gross breakage such as "forgot to fill the buffer".
        let a = random_bytes::<32>().expect("the OS must provide randomness");
        let b = random_bytes::<32>().expect("the OS must provide randomness");
        assert_ne!(a, b);
        assert_ne!(a, [0u8; 32]);
    }
}

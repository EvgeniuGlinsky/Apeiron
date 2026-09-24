//! Official test vectors of the standards.
//!
//! The readiness criterion of stage 2 requires running against our implementation the same
//! vectors that the reference `radio-mesh-demo/s07_ratchet.py` passes (section A).
//! The vectors here are exactly the same: checked against that file, and it, in turn,
//! against the RFC texts.
//!
//! # Why this, if the primitives are someone else's
//!
//! Precisely because they are someone else's. We do not write cryptography ourselves, but we
//! are responsible for choosing implementations, and "the crate is popular" is not a check.
//! This is the check: the standard says that for this input the output must be this, and it is.
//!
//! The test catches not a bug in the crates themselves (it would have been found before us),
//! but our own: the wrong argument order, salt and label mixed up,
//! a version swap when updating a dependency. That is exactly the class of bugs
//! that is invisible to the eye and does not show itself until the break-in.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;

fn unhex(s: &str) -> Vec<u8> {
    hex::decode(s).expect("the vector is written in hex")
}

fn array32(s: &str) -> [u8; 32] {
    unhex(s).try_into().expect("32 bytes")
}

// ─── RFC 7748. X25519 ────────────────────────────────────────────────────────

/// Scalar multiplication, RFC 7748 section 5.2.
#[test]
fn rfc7748_scalar_multiplication() {
    let k = array32("a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4");
    let u = array32("e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c");
    let expected = array32("c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552");

    assert_eq!(x25519_dalek::x25519(k, u), expected);
}

/// Key exchange, RFC 7748 section 6.1: public keys and the shared secret.
#[test]
fn rfc7748_key_exchange() {
    let alice_secret = array32("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a");
    let alice_public = array32("8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a");
    let bob_secret = array32("5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb");
    let bob_public = array32("de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f");
    let shared = array32("4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742");

    let base = x25519_dalek::X25519_BASEPOINT_BYTES;
    assert_eq!(x25519_dalek::x25519(alice_secret, base), alice_public);
    assert_eq!(x25519_dalek::x25519(bob_secret, base), bob_public);

    // Both sides arrive at one secret: the very one in the standard.
    assert_eq!(x25519_dalek::x25519(alice_secret, bob_public), shared);
    assert_eq!(x25519_dalek::x25519(bob_secret, alice_public), shared);
}

// ─── RFC 5869. HKDF-SHA256 ───────────────────────────────────────────────────

/// Test Case 1 from Appendix A: both the intermediate PRK and the final material.
#[test]
fn rfc5869_test_case_1() {
    let ikm = vec![0x0b; 22];
    let salt = unhex("000102030405060708090a0b0c");
    let info = unhex("f0f1f2f3f4f5f6f7f8f9");
    let expected_prk = unhex("077709362c2e32df0ddc3f0dc47bba6390b6c73bb50f9c3122ec844ad7c2b3e5");
    let expected_okm = unhex(
        "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865",
    );

    let (prk, hk) = Hkdf::<Sha256>::extract(Some(&salt), &ikm);
    assert_eq!(prk.as_slice(), expected_prk.as_slice(), "PRK");

    let mut okm = vec![0u8; expected_okm.len()];
    hk.expand(&info, &mut okm).expect("42 bytes are derived");
    assert_eq!(okm, expected_okm, "OKM");
}

// ─── RFC 8439. ChaCha20-Poly1305 ─────────────────────────────────────────────

/// AEAD, RFC 8439 section 2.8.2: ciphertext, tag, and rejection of a forgery.
#[test]
fn rfc8439_aead() {
    let key: Vec<u8> = (0x80u8..0xA0).collect();
    let nonce_bytes = unhex("070000004041424344454647");
    let aad = unhex("50515253c0c1c2c3c4c5c6c7");
    let plaintext = "Ladies and Gentlemen of the class of '99: If I could offer you \
         only one tip for the future, sunscreen would be it."
        .as_bytes();

    let expected_ciphertext = unhex(
        "d31a8d34648e60db7b86afbc53ef7ec2a4aded51296e08fea9e2b5a736ee62d6\
         3dbea45e8ca9671282fafb69da92728b1a71de0a9e060b2905d6a5b67ecd3b36\
         92ddbd7f2d778b8c9803aee328091b58fab324e4fad675945585808b4831d7bc\
         3ff4def08e4b7a9de576d26586cec64b6116",
    );
    let expected_tag = unhex("1ae10b594f09e26a7e902ecbd0600691");

    let cipher = ChaCha20Poly1305::new_from_slice(&key).expect("the key is 32 bytes");
    let nonce = Nonce::try_from(nonce_bytes.as_slice()).expect("12 bytes");

    let sealed = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .expect("encrypted");

    // The crate returns the ciphertext and tag as one piece; the standard gives them
    // separately.
    let (ciphertext, tag) = sealed.split_at(sealed.len() - 16);
    assert_eq!(ciphertext, expected_ciphertext.as_slice(), "ciphertext");
    assert_eq!(tag, expected_tag.as_slice(), "authentication tag");

    let opened = cipher
        .decrypt(
            &nonce,
            Payload {
                msg: &sealed,
                aad: &aad,
            },
        )
        .expect("decrypts back");
    assert_eq!(opened, plaintext);

    // A forgery must be rejected: that is the whole point of authenticated
    // encryption. Encryption without authenticity checking is explicitly forbidden (§18).
    let mut forged = sealed.clone();
    forged[5] ^= 1;
    assert!(
        cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: &forged,
                    aad: &aad
                }
            )
            .is_err(),
        "a ciphertext forgery must be rejected"
    );

    // Substitution of the unencrypted but protected part, too.
    let mut other_aad = aad.clone();
    other_aad[0] ^= 1;
    assert!(
        cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: &sealed,
                    aad: &other_aad
                }
            )
            .is_err(),
        "substitution of associated data must be rejected"
    );
}

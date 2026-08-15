//! Integration tests for PDF security handlers (ISO 32000-2 clause 7.6).
//!
//! These previously asserted that `SecurityHandler::new_v5` returned `Ok` for any
//! password and any file id. It did, and that was the defect: the handler invented its
//! own salts from `/ID` instead of reading `/U` and `/UE`, so it produced a key that
//! could not decrypt anything and never said so. The tests passed throughout.
//!
//! What follows asserts the opposite property — that a handler is only built when the
//! password actually authenticates against the document's own strings.

use fepdf_syntax::security::{AesV5Spec, Cipher, SecurityHandler, StandardSpec};

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

#[test]
fn aes256_refuses_a_password_that_does_not_authenticate() {
    // Well-formed lengths, but the hashes are zeros, so neither Algorithm 2.A path
    // matches. Building a handler here is what produced silent garbage before.
    let spec = AesV5Spec {
        u: &[0u8; 48],
        ue: &[0u8; 32],
        o: &[0u8; 48],
        oe: &[0u8; 32],
        revision: 6,
        encrypt_metadata: true,
    };
    assert!(SecurityHandler::new_aes256("", &spec).is_none());
    assert!(SecurityHandler::new_aes256("anything", &spec).is_none());
}

#[test]
fn aes256_refuses_strings_of_the_wrong_length() {
    // `/U` is 48 bytes: hash, validation salt, key salt. Anything shorter cannot be
    // split, and guessing at the split is how a reader ends up with a plausible key.
    let spec = AesV5Spec {
        u: &[0u8; 32],
        ue: &[0u8; 32],
        o: &[0u8; 48],
        oe: &[0u8; 32],
        revision: 6,
        encrypt_metadata: true,
    };
    assert!(SecurityHandler::new_aes256("", &spec).is_none());
}

#[test]
fn the_standard_handler_validates_against_the_documents_own_u() {
    // From `samples/unicode_16.pdf`: V4/R4, AESV2, empty user password.
    let o = hex("fd2c3d3ce19144d01850580c7870bd45fba3474163aac53f0647ad421d4d7030");
    let u = hex("18ff103cead285b9cf3b3b9694b40cc328bf4e5e4e758a4164004e56fffa0108");
    let id = hex("3232363431643233306330623665393339323565656363616430313364346533");
    let spec = StandardSpec {
        owner: &o,
        permissions: -1036,
        file_id: &id,
        encrypt_metadata: true,
        revision: 4,
        key_len: 16,
        cipher: Cipher::Aes,
    };

    let right = SecurityHandler::new_standard("", &spec).expect("builds");
    assert!(right.user_password_matches(&u, &id));
    assert!(right.should_decrypt_metadata());

    let wrong = SecurityHandler::new_standard("not the password", &spec).expect("builds");
    assert!(!wrong.user_password_matches(&u, &id), "a wrong password must not authenticate");
}

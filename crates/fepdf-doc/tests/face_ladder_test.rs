//! Which face this engine reaches for, and what it does when it cannot have one.
//!
//! The ladder has two rungs and ends in a refusal
//! ([ADR-0090](../../../docs/adr/0090-the-face-a-document-embeds-is-not-a-licence-to-set-new-text.md)):
//! a face installed here whose own terms permit an embedding that may be edited, then
//! nothing. 9.9.1 is why there is no third rung, and why silence from a program that
//! states no `fsType` is a refusal rather than a permission.

use fepdf_doc::apply::font::{NoFace, face_for};

/// **What is chosen is chosen for reasons the face stated**, not for being first.
#[test]
fn the_face_chosen_permits_being_embedded_for_editing() {
    match face_for("Hello") {
        Ok((name, program)) => {
            let permission = fepdf_font::embedding::embedding_permission(&program);
            assert!(
                permission.is_some_and(|p| p.allows_embedding_for_editing()),
                "{name} was chosen and its terms do not permit this: {permission:?}"
            );
            assert!(
                fepdf_font::subset::glyphs_for(&program, "Hello").is_ok(),
                "{name} was chosen and does not draw the text"
            );
        }
        // A machine whose faces all refuse is a machine where refusing is right, and the
        // refusal still has to say which faces refused and on what terms.
        Err(NoFace::Forbidden { refused }) => {
            assert!(!refused.is_empty(), "a refusal on terms names no face and no terms");
        }
        Err(other) => panic!("no face draws Latin text here: {other}"),
    }
}

/// A character nothing installed draws is named back, rather than substituted.
#[test]
fn a_character_no_installed_face_draws_is_named() {
    // U+E000 is the first private use codepoint, which no shipped face assigns.
    match face_for("\u{E000}") {
        Err(NoFace::NoGlyph { character }) => assert_eq!(character, '\u{E000}'),
        Err(other) => panic!("the wrong refusal: {other}"),
        Ok((name, _)) => panic!("{name} claims to draw a private use codepoint"),
    }
}

/// The refusal a reader sees says what to do about it.
#[test]
fn a_refusal_says_what_stopped_it() {
    let missing = NoFace::NoGlyph { character: '図' };
    assert!(missing.to_string().contains('図'));

    let forbidden =
        NoFace::Forbidden { refused: vec![("Serif".to_string(), "PreviewAndPrint".to_string())] };
    let said = forbidden.to_string();
    assert!(said.contains("Serif"), "the face that refused is not named: {said}");
    assert!(said.contains("PreviewAndPrint"), "the terms are not given: {said}");
}

/// What this machine answers, for the record.
///
/// `cargo test -p fepdf --test face_ladder_test -- --nocapture`
#[test]
fn what_this_machine_offers_is_printable() {
    for text in ["Hello", "図面", "\u{E000}"] {
        match face_for(text) {
            Ok((name, program)) => println!("{text:?}: {name}, {} bytes", program.len()),
            Err(why) => println!("{text:?}: refused — {why}"),
        }
    }
}

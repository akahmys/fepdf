//! What "this page has no text" means, and the two things it used to mean at once.
//!
//! An empty extraction is a legitimate answer for a blank page and for one drawn entirely
//! in vector paths — measured, 24 of `fugaku.pdf`'s 25 pages are the second. It is a
//! defect when glyphs were drawn and none of them could be named in Unicode: measured, 192
//! of `bokutokitan.pdf`'s 195 pages and 64,556 glyphs, returned as the same empty string a
//! blank page returns and with nothing recorded beside it.

use fepdf_content::{RenderBackend, TextGlyph, TextState};
use fepdf_doc::remediation::TextExtractionBackend;
use fepdf_model::interpretation::Severity;
use kurbo::Affine;

fn glyph(unicode: &str) -> TextGlyph {
    TextGlyph {
        gid: 1,
        name: None,
        char_code: 1,
        unicode: unicode.to_string(),
        width: 1.0,
        vx: 0.0,
        vy: 0.0,
        is_fallback: false,
    }
}

fn extract(glyphs: &[TextGlyph]) -> (String, Vec<fepdf_model::interpretation::Decision>) {
    let mut backend = TextExtractionBackend::new();
    if !glyphs.is_empty() {
        backend.show_text(
            glyphs,
            10.0,
            Affine::IDENTITY,
            TextState { tc: 0.0, tw: 0.0, th: 1.0, is_vertical: false },
            0,
        );
    }
    let decisions = backend.take_decisions();
    (backend.finish(), decisions)
}

#[test]
fn a_page_with_no_glyphs_at_all_records_nothing() {
    // Blank, or drawn in vector paths. Empty is the right answer and there is nothing to
    // say about it — a decision here would fire on every page of every graphics-only file.
    let (text, decisions) = extract(&[]);
    assert!(text.is_empty());
    assert!(decisions.is_empty(), "{decisions:?}");
}

#[test]
fn glyphs_that_all_carry_text_record_nothing() {
    let (text, decisions) = extract(&[glyph("a"), glyph("b")]);
    assert_eq!(text, "ab");
    assert!(decisions.is_empty(), "{decisions:?}");
}

#[test]
fn glyphs_with_no_unicode_are_counted_and_recorded() {
    // The defect. Without this the caller receives "" and cannot tell it from a blank page.
    let (text, decisions) = extract(&[glyph(""), glyph(""), glyph("")]);
    assert!(text.is_empty(), "nothing came out: {text:?}");
    assert_eq!(decisions.len(), 1, "{decisions:?}");
    assert_eq!(decisions[0].severity, Severity::Violation);
    assert_eq!(decisions[0].clause, "9.10.2");
    assert!(decisions[0].found.contains("3 of 3"), "it says how many: {:?}", decisions[0]);
}

#[test]
fn a_page_that_loses_only_some_of_its_glyphs_still_says_so() {
    // The partial case is the one an eye would miss: text comes out, so the page looks
    // extracted, and a third of it is gone.
    let (text, decisions) = extract(&[glyph("a"), glyph(""), glyph("c")]);
    assert_eq!(text, "ac", "what survived");
    assert_eq!(decisions.len(), 1, "{decisions:?}");
    assert!(decisions[0].found.contains("1 of 3"), "{:?}", decisions[0]);
}

#[test]
fn the_action_names_the_missing_collection_when_that_is_the_cause() {
    // Two causes, one symptom. A machine without the CMap collections cannot map an
    // Adobe-Japan1 CID whatever the file says, and that is the one a reader can act on —
    // so when it applies, the decision says where the data was looked for.
    let (_, decisions) = extract(&[glyph("")]);
    let action = &decisions[0].action;
    let carried = fepdf_model::resources::locate(fepdf_model::resources::Resource::Cmaps).is_some();
    if carried {
        assert!(action.contains("/ToUnicode"), "{action}");
    } else {
        assert!(action.contains("looked in"), "{action}");
    }
}

//! What "this page has no text" means, and the two things it used to mean at once.
//!
//! An empty extraction is a legitimate answer for a blank page and for one drawn entirely
//! in vector paths. It is a defect when glyphs were drawn and none of them could be named
//! in Unicode: measured, 192 of `bokutokitan.pdf`'s 195 pages and 64,556 glyphs, returned
//! as the same empty string a blank page returns and with nothing recorded beside it.
//!
//! **`fugaku.pdf` used to be this file's example of the legitimate case**, on the measured
//! ground that 24 of its 25 pages draw no glyphs at all. The glyph count was right and the
//! conclusion was wrong: those pages carry 2,622 `/ActualText` spans (14.9.4), so the
//! document does say what it shows — badly, one character per span and mostly punctuation,
//! but it says it. Reading a page's text is not the same question as counting its glyphs,
//! and this file could not tell the two apart until something went looking.

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
        source: if unicode.is_empty() {
            fepdf_model::font::UnicodeSource::Unmapped
        } else {
            fepdf_model::font::UnicodeSource::ToUnicode
        },
    }
}

/// Runs, each at a given device x and y, with a size. What comes out is the text.
fn extract_runs(runs: &[(f64, f64, f64, &str)]) -> String {
    let mut backend = TextExtractionBackend::new();
    for (x, y, size, text) in runs {
        // Widths are thousandths of an em (9.2.4), so a glyph one em wide is 1000.
        let glyphs: Vec<TextGlyph> =
            text.chars().map(|c| TextGlyph { width: 1000.0, ..glyph(&c.to_string()) }).collect();
        let at = Affine::new([1.0, 0.0, 0.0, 1.0, *x, *y]);
        backend.show_text(
            &glyphs,
            *size,
            at,
            TextState { tc: 0.0, tw: 0.0, th: 1.0, is_vertical: false },
            0,
        );
    }
    backend.finish()
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

/// Each glyph is one unit wide at size 1, so a run of n characters advances n.
///
/// A `TJ` array delivers one `show_text` per element, so most gaps between calls are
/// kerning rather than word breaks. Measured across a table page, two prose pages and a
/// page of vertical Japanese: kerning reaches 0.055 em at the 99th percentile, every real
/// separation is at least 0.15 em, and **nothing falls between 0.15 and 0.25**. The
/// threshold sits in that empty band.
#[test]
fn runs_separated_along_a_line_are_words_and_get_a_space() {
    // "Regions" ends at x = 7.0; "Labels" starts a full em later.
    let text = extract_runs(&[(0.0, 100.0, 1.0, "Regions"), (8.0, 100.0, 1.0, "Labels")]);
    assert_eq!(text, "Regions Labels");
}

#[test]
fn a_kerning_sized_gap_is_not_a_word_break() {
    // 0.05 em, which is where kerning lives. Splitting here puts spaces inside words.
    let text = extract_runs(&[(0.0, 100.0, 1.0, "Va"), (2.05, 100.0, 1.0, "lue")]);
    // "Va" is two ems wide, so it ends at 2.0 and the next run begins 0.05 em later.
    assert_eq!(text, "Value");
}

#[test]
fn a_run_on_a_new_line_is_not_also_spaced() {
    // The newline already separates them; a space as well would be two separators.
    let text = extract_runs(&[(0.0, 100.0, 1.0, "one"), (500.0, 20.0, 1.0, "two")]);
    assert_eq!(text, "one\ntwo");
}

#[test]
fn nothing_is_inserted_before_the_first_run_or_after_whitespace() {
    assert_eq!(extract_runs(&[(500.0, 100.0, 1.0, "first")]), "first");
    let text = extract_runs(&[(0.0, 100.0, 1.0, "a "), (9.0, 100.0, 1.0, "b")]);
    assert_eq!(text, "a b", "one space, not two");
}

/// What a marked-content section says it shows, when that differs from what it draws.
///
/// 14.9.4: `/ActualText` replaces the content of its section for extraction. The glyphs
/// are still what appears on the page, so rendering is unaffected — only the reader's
/// copy of the text changes, which is the only place the difference exists.
mod actual_text {
    use super::{TextExtractionBackend, glyph};
    use fepdf_content::{RenderBackend, TextGlyph, TextState};
    use kurbo::Affine;

    /// Draws `drawn` inside a section declaring `says`, and returns the extracted text.
    fn replaced(before: &str, says: Option<&str>, drawn: &str, after: &str) -> String {
        let mut backend = TextExtractionBackend::new();
        let show = |backend: &mut TextExtractionBackend, text: &str, x: f64| {
            let glyphs: Vec<TextGlyph> = text
                .chars()
                .map(|c| TextGlyph { width: 1000.0, ..glyph(&c.to_string()) })
                .collect();
            backend.show_text(
                &glyphs,
                10.0,
                Affine::new([1.0, 0.0, 0.0, 1.0, x, 0.0]),
                TextState { tc: 0.0, tw: 0.0, th: 1.0, is_vertical: false },
                0,
            );
        };
        show(&mut backend, before, 0.0);
        if let Some(says) = says {
            backend.begin_actual_text(says);
        }
        show(&mut backend, drawn, 1.0);
        if says.is_some() {
            backend.end_actual_text();
        }
        show(&mut backend, after, 2.0);
        backend.finish()
    }

    #[test]
    fn the_section_speaks_and_the_glyphs_do_not() {
        // `volvo_xc90.pdf` draws its Chinese notices as `.notdef` and puts the characters
        // in the span. Taking the glyphs as well would print both.
        assert_eq!(replaced("Taiwan", Some("警語"), "\u{0}\u{0}", ""), "Taiwan警語");
    }

    #[test]
    fn a_glyph_that_is_replaced_is_not_a_glyph_that_was_lost() {
        let mut backend = TextExtractionBackend::new();
        backend.begin_actual_text("-");
        // The real case: a code whose `/ToUnicode` says `U+0000`, which is not an empty
        // string and so was never counted, and not a character anyone can read either.
        backend.show_text(
            &[TextGlyph { width: 1000.0, ..glyph("\u{0}") }],
            10.0,
            Affine::IDENTITY,
            TextState { tc: 0.0, tw: 0.0, th: 1.0, is_vertical: false },
            0,
        );
        backend.end_actual_text();
        let (seen, unmapped, replaced) = backend.tally();
        assert_eq!((seen, unmapped, replaced), (1, 0, 1), "counted as replaced, not lost");
        assert_eq!(backend.finish(), "-");
    }

    #[test]
    fn an_empty_section_stands_for_no_text_and_that_is_an_answer() {
        // A decorative glyph, or the second half of a hyphenated word: the document is
        // saying "this shows nothing", which is different from saying nothing.
        assert_eq!(replaced("ab", Some(""), "XY", "cd"), "abcd");
    }

    #[test]
    fn nesting_takes_the_outer_section_because_it_already_covers_the_inner() {
        let mut backend = TextExtractionBackend::new();
        backend.begin_actual_text("outer");
        backend.begin_actual_text("inner");
        backend.end_actual_text();
        backend.end_actual_text();
        assert_eq!(backend.finish(), "outer", "the inner text describes part of the outer");
    }

    #[test]
    fn an_unbalanced_end_leaves_the_rest_of_the_page_readable() {
        // A content stream with more `EMC`s than sections is wrong, and refusing it would
        // take the rest of the page's text with it — the failure ADR-0018 was written
        // about, reached here through a different door.
        let mut backend = TextExtractionBackend::new();
        backend.end_actual_text();
        assert_eq!(replaced("ab", None, "cd", ""), "abcd");
        assert_eq!(backend.finish(), "");
    }
}

mod reading_order {
    use super::{TextExtractionBackend, glyph};
    use fepdf_content::{RenderBackend, TextGlyph, TextState};
    use kurbo::Affine;

    #[test]
    fn footer_drawn_first_is_reordered_to_bottom() {
        let mut backend = TextExtractionBackend::new();
        let show = |backend: &mut TextExtractionBackend, text: &str, x: f64, y: f64| {
            let glyphs: Vec<TextGlyph> = text
                .chars()
                .map(|c| TextGlyph { width: 1000.0, ..glyph(&c.to_string()) })
                .collect();
            backend.show_text(
                &glyphs,
                10.0,
                Affine::new([1.0, 0.0, 0.0, 1.0, x, y]),
                TextState { tc: 0.0, tw: 0.0, th: 1.0, is_vertical: false },
                0,
            );
        };
        // Emit footer page number at bottom of page (y = 50) first
        show(&mut backend, "3", 300.0, 50.0);
        // Emit title at top of page (y = 750) second
        show(&mut backend, "Title", 50.0, 750.0);
        // Emit body in middle of page (y = 600) third
        show(&mut backend, "Body text", 50.0, 600.0);

        assert_eq!(backend.finish(), "Title\nBody text\n3");
    }

    #[test]
    fn same_line_fragments_drawn_out_of_order_are_sorted_left_to_right() {
        let mut backend = TextExtractionBackend::new();
        let show = |backend: &mut TextExtractionBackend, text: &str, x: f64, y: f64| {
            let glyphs: Vec<TextGlyph> = text
                .chars()
                .map(|c| TextGlyph { width: 1000.0, ..glyph(&c.to_string()) })
                .collect();
            backend.show_text(
                &glyphs,
                10.0,
                Affine::new([1.0, 0.0, 0.0, 1.0, x, y]),
                TextState { tc: 0.0, tw: 0.0, th: 1.0, is_vertical: false },
                0,
            );
        };
        // Emit right fragment first
        show(&mut backend, "World", 100.0, 500.0);
        // Emit left fragment second
        show(&mut backend, "Hello", 20.0, 500.0);

        assert_eq!(backend.finish(), "Hello World");
    }

    #[test]
    fn vertical_text_columns_flow_right_to_left() {
        let mut backend = TextExtractionBackend::new();
        let show = |backend: &mut TextExtractionBackend, text: &str, x: f64, y: f64| {
            let glyphs: Vec<TextGlyph> = text
                .chars()
                .map(|c| TextGlyph { width: 1000.0, ..glyph(&c.to_string()) })
                .collect();
            backend.show_text(
                &glyphs,
                10.0,
                Affine::new([1.0, 0.0, 0.0, 1.0, x, y]),
                TextState { tc: 0.0, tw: 0.0, th: 1.0, is_vertical: true },
                0,
            );
        };
        // Emit left column (x = 100) first
        show(&mut backend, "第二列", 100.0, 700.0);
        // Emit right column (x = 200) second
        show(&mut backend, "第一列", 200.0, 700.0);

        assert_eq!(backend.finish(), "第一列\n第二列");
    }
}

/// **A `cm` between two runs moves the page, and the sort has to know.**
///
/// `show_text` is handed the text transform, not the whole CTM, so a run's `y` used to be
/// its offset from whatever `cm` was last in force. That was harmless while extraction
/// emitted runs in arrival order; ADR-0047 ordered them by `y` and gave it meaning, and
/// `volvo_xc90.pdf` — which draws its note boxes inside `q … cm … Q` — went from 61 pages
/// agreeing with PDFKit to **0**, each box sorting against its own origin. Composing the
/// tracked CTM took it to 182, past where it was before the sort existed.
///
/// **The two transforms are made to disagree on purpose.** A first version of this test
/// gave both runs the same text `y` and differed only in the `cm`; dropping the
/// composition left them tied, arrival order settled it, and the test passed against the
/// defect it exists for. Here the text transforms say BOTTOM first and only the CTM says
/// TOP first, so there is no reading of the numbers that is right by accident.
#[test]
fn a_run_is_placed_by_the_ctm_in_force_and_not_by_its_text_transform_alone() {
    let mut backend = TextExtractionBackend::new();
    let state = TextState { tc: 0.0, tw: 0.0, th: 1.0, is_vertical: false };
    let run = |text: &str| -> Vec<TextGlyph> {
        text.chars().map(|c| TextGlyph { width: 1000.0, ..glyph(&c.to_string()) }).collect()
    };

    // TOP:    cm 700 + text  10 = 710 on the page
    // BOTTOM: cm 100 + text 300 = 400 on the page
    // By text transform alone BOTTOM is the higher of the two, and wrong.
    backend.push_state();
    backend.transform(Affine::translate((0.0, 700.0)));
    backend.show_text(&run("TOP"), 10.0, Affine::translate((0.0, 10.0)), state, 0);
    backend.pop_state();

    backend.push_state();
    backend.transform(Affine::translate((0.0, 100.0)));
    backend.show_text(&run("BOTTOM"), 10.0, Affine::translate((0.0, 300.0)), state, 1);
    backend.pop_state();

    let out = backend.finish();
    assert!(out.contains("TOP") && out.contains("BOTTOM"), "both runs reach the output: {out:?}");
    assert!(
        out.find("TOP") < out.find("BOTTOM"),
        "the run 310 units higher on the page came out second, so the sort is reading the \
         text transform rather than the CTM in force: {out:?}"
    );
}

/// The other half: `pop_state` has to restore what `push_state` saved.
///
/// Also built so that leaving the block's translation in force gives the *wrong* answer
/// rather than the same one — `AFTER` is at 300 in text space, which composes to 800 and
/// sorts above `INSIDE` when the pop does nothing.
#[test]
fn popping_the_graphics_state_restores_the_transform_that_was_pushed() {
    let mut backend = TextExtractionBackend::new();
    let state = TextState { tc: 0.0, tw: 0.0, th: 1.0, is_vertical: false };
    let run = |text: &str| -> Vec<TextGlyph> {
        text.chars().map(|c| TextGlyph { width: 1000.0, ..glyph(&c.to_string()) }).collect()
    };

    // INSIDE: cm 500 + text 10 = 510.  AFTER: no cm, text 300 = 300.
    backend.push_state();
    backend.transform(Affine::translate((0.0, 500.0)));
    backend.show_text(&run("INSIDE"), 10.0, Affine::translate((0.0, 10.0)), state, 0);
    backend.pop_state();
    backend.show_text(&run("AFTER"), 10.0, Affine::translate((0.0, 300.0)), state, 1);

    let out = backend.finish();
    assert!(
        out.find("INSIDE") < out.find("AFTER"),
        "the run inside the block sorted below the one after it, so `pop_state` left the \
         block's translation in force and put AFTER at 800: {out:?}"
    );
}

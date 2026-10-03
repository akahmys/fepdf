//! What a page marked with `/OC` actually draws (8.11).
//!
//! Thirteen constructions, each a page that paints a square in the **top-left** quarter
//! under some optional-content condition and a square in the **bottom-right** with none —
//! so every case says both "the right thing was hidden" and "the rest of the page
//! survived". The same thirteen were put to PDFKit as rendered pages
//! (`scripts/test/crosscheck_image.sh`, and see
//! `docs/adr/0021-optional-content-hides-only-what-the-document-unambiguously-turns-off.md`):
//! it agrees on two of them and paints the other eleven, so these assertions are held to
//! the clause rather than to a second implementation.
//!
//! The backend records rather than rasterises, which is why this runs without a GPU: the
//! question is whether the interpreter *called* the backend, and a pixel is a slower way
//! of asking.

use fepdf::{IngestionOptions, PdfDocument};
use kurbo::Affine;

use fepdf_fixtures::recorder::Recorder;

use fepdf_fixtures::assemble;

/// The square under test, in the top-left quarter of a 200×200 page.
const TOP_LEFT: &str = "0 0 0 rg 0 100 100 100 re f\n";
/// The square that is never conditional, in the bottom-right.
const BOTTOM_RIGHT: &str = "0 0 0 rg 100 0 100 100 re f\n";

/// A stream object with `extra` merged into its dictionary.
fn stream(extra: &str, data: &str) -> String {
    format!("<< {extra} /Length {} >>\nstream\n{data}endstream", data.len())
}

/// One 200×200 page: `oc` goes in the catalogue, `resources` in the page, `content` is
/// the stream, and `extra` are objects 5 onward.
fn page(oc: &str, resources: &str, content: &str, extra: &[String]) -> Vec<u8> {
    let mut bodies = vec![
        format!("<< /Type /Catalog /Pages 2 0 R {oc} >>"),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] \
             /Resources << {resources} >> /Contents 4 0 R >>"
        ),
        stream("", content),
    ];
    bodies.extend_from_slice(extra);
    assemble(&bodies)
}

/// The group every fixture below refers to as object 5.
fn group(extra: &str) -> String {
    format!("<< /Type /OCG /Name (Layer) {extra} >>")
}

/// Interprets page 1 and reports what the backend was asked to draw.
/// Whether a fill landed in the top-left or bottom-right quadrant of the 200x200 page.
fn painted_top_left(drawn: &Recorder) -> bool {
    drawn.fills().iter().any(|b| b.x0 < 50.0 && b.y0 > 50.0)
}

fn painted_bottom_right(drawn: &Recorder) -> bool {
    drawn.fills().iter().any(|b| b.x0 > 50.0 && b.y0 < 50.0)
}

/// The graphics-state operators that reached the backend: `q`, `Q`, `cm` and a fill colour.
fn state_calls(drawn: &Recorder) -> usize {
    drawn.count("push_state")
        + drawn.count("pop_state")
        + drawn.count("transform")
        + drawn.count("fill_color")
}

fn draw(file: Vec<u8>) -> Recorder {
    let document = PdfDocument::open_with_options(file.into(), &IngestionOptions::default())
        .expect("the fixture opens");
    let mut recorder = Recorder::new();
    document.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page interprets");
    recorder
}

/// Asserts the conditional square was withheld and the unconditional one was not.
fn assert_hidden(what: &str, recorder: &Recorder) {
    assert!(!painted_top_left(recorder), "{what}: the hidden square was painted");
    assert!(painted_bottom_right(recorder), "{what}: the rest of the page went with it");
}

/// Asserts both squares reached the page.
fn assert_drawn(what: &str, recorder: &Recorder) {
    assert!(painted_top_left(recorder), "{what}: the visible square was withheld");
    assert!(painted_bottom_right(recorder), "{what}: the rest of the page went with it");
}

/// A `/OC` section wrapping the content, with `/MC0` naming object 5.
fn marked(content: &str) -> String {
    format!("/OC /MC0 BDC\n{content}EMC\n{BOTTOM_RIGHT}")
}

// --- the default configuration's own entries (8.11.4.4, Table 100) -------------------

#[test]
fn a_group_in_the_off_array_is_not_drawn() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /BaseState /ON /OFF [5 0 R] >> >>",
        "/Properties << /MC0 5 0 R >>",
        &marked(TOP_LEFT),
        &[group("")],
    );
    assert_hidden("a group in /OFF", &draw(file));
}

#[test]
fn a_group_in_the_on_array_is_drawn() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /BaseState /ON /ON [5 0 R] >> >>",
        "/Properties << /MC0 5 0 R >>",
        &marked(TOP_LEFT),
        &[group("")],
    );
    assert_drawn("a group in /ON", &draw(file));
}

#[test]
fn base_state_off_turns_off_every_declared_group() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /BaseState /OFF >> >>",
        "/Properties << /MC0 5 0 R >>",
        &marked(TOP_LEFT),
        &[group("")],
    );
    assert_hidden("/BaseState /OFF", &draw(file));
}

#[test]
fn the_on_array_rescues_a_group_from_base_state_off() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /BaseState /OFF /ON [5 0 R] >> >>",
        "/Properties << /MC0 5 0 R >>",
        &marked(TOP_LEFT),
        &[group("")],
    );
    assert_drawn("/BaseState /OFF with /ON", &draw(file));
}

/// 8.11.2.2: a group whose `/Intent` does not meet the configuration's is "not
/// considered", so its state never applies and its content is drawn — even from `/OFF`.
#[test]
fn a_group_whose_intent_the_configuration_does_not_share_is_not_considered() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /Intent /View /OFF [5 0 R] >> >>",
        "/Properties << /MC0 5 0 R >>",
        &marked(TOP_LEFT),
        &[group("/Intent /Design")],
    );
    assert_drawn("a /Design group under a /View configuration", &draw(file));
}

/// And the same group *is* considered where the configuration asks for its intent.
#[test]
fn a_configuration_that_shares_the_intent_does_consider_it() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /Intent [/View /Design] /OFF [5 0 R] >> >>",
        "/Properties << /MC0 5 0 R >>",
        &marked(TOP_LEFT),
        &[group("/Intent /Design")],
    );
    assert_hidden("a /Design group under a /Design configuration", &draw(file));
}

// --- usage, applied through /AS (8.11.4.5) -------------------------------------------

#[test]
fn a_view_usage_named_by_as_turns_a_group_that_is_on_off() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /ON [5 0 R] \
         /AS [<< /Event /View /Category [/View] /OCGs [5 0 R] >>] >> >>",
        "/Properties << /MC0 5 0 R >>",
        &marked(TOP_LEFT),
        &[group("/Usage << /View << /ViewState /OFF >> >>")],
    );
    assert_hidden("a /View usage applied by /AS", &draw(file));
}

/// The same usage without an `/AS` entry does nothing: 8.11.4.5 makes the application the
/// thing that acts, and a `/Usage` alone is a description.
#[test]
fn a_view_usage_no_as_entry_names_changes_nothing() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /ON [5 0 R] >> >>",
        "/Properties << /MC0 5 0 R >>",
        &marked(TOP_LEFT),
        &[group("/Usage << /View << /ViewState /OFF >> >>")],
    );
    assert_drawn("a /View usage with no /AS", &draw(file));
}

/// `/Print` is read and not applied — nothing here prints, and hiding a layer on a screen
/// because a printer would omit it is a different document from the one that was opened.
#[test]
fn a_print_usage_does_not_hide_anything_on_screen() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /ON [5 0 R] \
         /AS [<< /Event /Print /Category [/Print] /OCGs [5 0 R] >>] >> >>",
        "/Properties << /MC0 5 0 R >>",
        &marked(TOP_LEFT),
        &[group("/Usage << /Print << /PrintState /OFF >> >>")],
    );
    assert_drawn("a /Print usage", &draw(file));
}

// --- membership dictionaries (8.11.2.3, Table 97) ------------------------------------

#[test]
fn an_ocmd_defaults_to_any_on() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R 7 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC0 6 0 R >>",
        &marked(TOP_LEFT),
        &[group(""), "<< /Type /OCMD /OCGs [5 0 R 7 0 R] >>".to_string(), group("")],
    );
    assert_drawn("an OCMD with one group on", &draw(file));
}

#[test]
fn an_ocmd_with_all_on_needs_every_group() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R 7 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC0 6 0 R >>",
        &marked(TOP_LEFT),
        &[group(""), "<< /Type /OCMD /OCGs [5 0 R 7 0 R] /P /AllOn >>".to_string(), group("")],
    );
    assert_hidden("an OCMD with /AllOn", &draw(file));
}

#[test]
fn an_ocmd_with_any_off_is_visible_when_one_is_off() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R 7 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC0 6 0 R >>",
        &marked(TOP_LEFT),
        &[group(""), "<< /Type /OCMD /OCGs [5 0 R 7 0 R] /P /AnyOff >>".to_string(), group("")],
    );
    assert_drawn("an OCMD with /AnyOff", &draw(file));
}

#[test]
fn a_visibility_expression_negates() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /ON [5 0 R] >> >>",
        "/Properties << /MC0 6 0 R >>",
        &marked(TOP_LEFT),
        &[group(""), "<< /Type /OCMD /VE [/Not 5 0 R] >>".to_string()],
    );
    assert_hidden("/VE [/Not <on>]", &draw(file));
}

#[test]
fn a_visibility_expression_nests() {
    // `/And` of "5 is on" and "not (7 is on)". 5 is on and 7 is off, so both hold.
    let file = page(
        "/OCProperties << /OCGs [5 0 R 7 0 R] /D << /ON [5 0 R] /OFF [7 0 R] >> >>",
        "/Properties << /MC0 6 0 R >>",
        &marked(TOP_LEFT),
        &[group(""), "<< /Type /OCMD /VE [/And 5 0 R [/Not 7 0 R]] >>".to_string(), group("")],
    );
    assert_drawn("/VE [/And <on> [/Not <off>]]", &draw(file));
}

/// `/VE` wins over `/P` and `/OCGs`, which Table 97 says shall be ignored beside it.
#[test]
fn a_visibility_expression_overrides_the_policy() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /ON [5 0 R] >> >>",
        "/Properties << /MC0 6 0 R >>",
        &marked(TOP_LEFT),
        &[group(""), "<< /Type /OCMD /OCGs [5 0 R] /P /AnyOn /VE [/Not 5 0 R] >>".to_string()],
    );
    assert_hidden("/VE beside /P", &draw(file));
}

/// An expression that refers to itself terminates, and the content is drawn rather than
/// hidden on an answer the evaluator never reached.
#[test]
fn a_visibility_expression_that_loops_does_not_hang_or_hide() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC0 6 0 R >>",
        &marked(TOP_LEFT),
        &[group(""), "<< /Type /OCMD /VE 7 0 R >>".to_string(), "[/Not 7 0 R]".to_string()],
    );
    assert_drawn("a /VE that contains itself", &draw(file));
}

// --- 8.11.3.2: the /OC entry on an XObject -------------------------------------------

#[test]
fn a_form_xobject_with_a_hidden_oc_is_not_executed() {
    let form = stream("/Type /XObject /Subtype /Form /BBox [0 0 200 200] /OC 5 0 R", TOP_LEFT);
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/XObject << /Fm0 6 0 R >>",
        &format!("/Fm0 Do\n{BOTTOM_RIGHT}"),
        &[group(""), form],
    );
    assert_hidden("a form XObject with /OC", &draw(file));
}

#[test]
fn an_image_xobject_with_a_hidden_oc_is_not_drawn() {
    let image = stream(
        "/Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceGray \
         /BitsPerComponent 8 /OC 5 0 R",
        "\u{0}\u{0}\u{0}\u{0}",
    );
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/XObject << /Im0 6 0 R >>",
        &format!("q 100 0 0 100 0 100 cm /Im0 Do Q\n{BOTTOM_RIGHT}"),
        &[group(""), image],
    );
    let recorder = draw(file);
    assert_eq!(recorder.count("image"), 0, "the hidden image was drawn");
    assert!(painted_bottom_right(&recorder), "the rest of the page went with it");
}

// --- the two forms a real file used, and this engine did not read -------------------

/// **A membership dictionary may be written in place.** 8.11.2 requires the *group* to be
/// an indirect object, because only a reference gives it the identity `/OFF` names; an
/// OCMD needs none, since it reaches its groups through `/OCGs`. Conflating the two made
/// every inline OCMD unreadable, and unreadable means drawn.
///
/// Found on `pdf20examples/pdf20-utf8-test.pdf`, whose form XObjects carry
/// `/OC << /Type /OCMD /OCGs 3 0 R >>` with both layers off. PDFKit hides them; this
/// engine drew them until the corpus that contains the file was fetched.
#[test]
fn an_ocmd_written_in_place_is_read_rather_than_refused() {
    let form = stream(
        "/Type /XObject /Subtype /Form /BBox [0 0 200 200] \
         /OC << /Type /OCMD /OCGs [5 0 R] >>",
        TOP_LEFT,
    );
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/XObject << /Fm0 6 0 R >>",
        &format!("/Fm0 Do\n{BOTTOM_RIGHT}"),
        &[group(""), form],
    );
    assert_hidden("an inline OCMD", &draw(file));
}

/// Table 97 lets `/OCGs` be one group instead of an array, and the same file writes it
/// that way. Reading only the array form made an OCMD with one group look like an OCMD
/// with none — which is visible, so the layer was drawn for the second reason as well.
#[test]
fn an_ocmd_naming_a_single_group_rather_than_an_array_is_read() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC0 6 0 R >>",
        &marked(TOP_LEFT),
        &[group(""), "<< /Type /OCMD /OCGs 5 0 R >>".to_string()],
    );
    assert_hidden("an OCMD whose /OCGs is one reference", &draw(file));
}

/// Both at once, which is the shape the file actually writes.
#[test]
fn an_inline_ocmd_naming_a_single_group_is_read() {
    let form = stream(
        "/Type /XObject /Subtype /Form /BBox [0 0 200 200] \
         /OC << /Type /OCMD /OCGs 5 0 R >>",
        TOP_LEFT,
    );
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/XObject << /Fm0 6 0 R >>",
        &format!("/Fm0 Do\n{BOTTOM_RIGHT}"),
        &[group(""), form],
    );
    assert_hidden("pdf20-utf8-test's shape", &draw(file));
}

/// The half of the old rule that was right, and stays: a *group* written in place names
/// nothing `/OCProperties` could have turned off, so it is drawn and recorded.
#[test]
fn a_group_written_in_place_still_names_nothing() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC0 << /Type /OCG /Name (Layer) >> >>",
        &marked(TOP_LEFT),
        &[group("")],
    );
    assert_drawn("an OCG written in place", &draw(file));
}

// --- nesting, and what may not follow a section out ----------------------------------

/// The `EMC` of a section opened *inside* a hidden one must not bring the page back.
/// PDFKit paints this fixture, which is how the case was found.
#[test]
fn a_section_nested_inside_a_hidden_one_stays_hidden() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC0 5 0 R >>",
        &format!(
            "/OC /MC0 BDC\n/Span << /ActualText (x) >> BDC\nEMC\n{TOP_LEFT}EMC\n{BOTTOM_RIGHT}"
        ),
        &[group("")],
    );
    assert_hidden("a /Span inside a hidden /OC", &draw(file));
}

/// An `EMC` with nothing open is ignored rather than revealing what follows.
#[test]
fn an_unbalanced_emc_does_not_reveal_a_hidden_section() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC0 5 0 R >>",
        &format!("EMC\n/OC /MC0 BDC\n{TOP_LEFT}EMC\n{BOTTOM_RIGHT}"),
        &[group("")],
    );
    assert_hidden("a stray EMC before the section", &draw(file));
}

/// A form XObject that closes a section it never opened must not reveal the section the
/// `Do` was inside.
#[test]
fn a_form_cannot_close_a_section_it_did_not_open() {
    let form = stream("/Type /XObject /Subtype /Form /BBox [0 0 200 200]", "EMC\n");
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC0 5 0 R >> /XObject << /Fm0 6 0 R >>",
        &format!("/OC /MC0 BDC\n/Fm0 Do\n{TOP_LEFT}EMC\n{BOTTOM_RIGHT}"),
        &[group(""), form],
    );
    assert_hidden("a form with a stray EMC", &draw(file));
}

// --- nothing is hidden on a doubt ----------------------------------------------------

#[test]
fn an_oc_naming_a_property_the_page_does_not_carry_is_drawn() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC1 5 0 R >>",
        &marked(TOP_LEFT),
        &[group("")],
    );
    assert_drawn("/OC naming an absent property", &draw(file));
}

#[test]
fn content_marked_oc_in_a_document_with_no_ocproperties_is_drawn() {
    let file = page("", "/Properties << /MC0 5 0 R >>", &marked(TOP_LEFT), &[group("")]);
    assert_drawn("/OC with no /OCProperties", &draw(file));
}

#[test]
fn a_policy_table_97_does_not_define_draws_rather_than_guesses() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC0 6 0 R >>",
        &marked(TOP_LEFT),
        &[group(""), "<< /Type /OCMD /OCGs [5 0 R] /P /Sometimes >>".to_string()],
    );
    assert_drawn("an OCMD with an undefined /P", &draw(file));
}

// --- what a hidden section still does ------------------------------------------------

/// Marks are withheld; the graphics state is not. A viewer that skipped `q`, `Q` and `cm`
/// inside a hidden section would leave it with the wrong transformation, so the wrapper
/// forwards everything that is not a mark.
#[test]
fn a_hidden_section_still_changes_the_graphics_state() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC0 5 0 R >>",
        &format!("/OC /MC0 BDC\nq 2 0 0 2 0 0 cm 1 0 0 rg Q\n{TOP_LEFT}EMC\n{BOTTOM_RIGHT}"),
        &[group("")],
    );
    let recorder = draw(file);
    assert!(!painted_top_left(&recorder), "the hidden square was painted");
    assert!(
        state_calls(&recorder) >= 3,
        "state operators inside the hidden section did not reach the backend: {} calls",
        state_calls(&recorder)
    );
}

/// Text inside a hidden section is not shown, and therefore is not extracted either —
/// what is drawn and what comes out of `inspect text` are the same content stream, and a
/// layer that is off is not on the page in either sense.
#[test]
fn text_inside_a_hidden_section_is_not_shown() {
    let content =
        format!("/OC /MC0 BDC\nBT /F1 24 Tf 1 0 0 1 20 60 Tm (HIDDEN) Tj ET\nEMC\n{BOTTOM_RIGHT}");
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC0 5 0 R >> /Font << /F1 6 0 R >>",
        &content,
        &[group(""), "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string()],
    );
    let recorder = draw(file);
    assert_eq!(recorder.count("text"), 0, "text in a hidden layer reached the backend");
    assert!(painted_bottom_right(&recorder), "the rest of the page went with it");
}

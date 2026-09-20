//! A layer this engine writes, read back by the reader that honours layers.
//!
//! `UpdateLayers` wrote the optional content groups and a default configuration, and
//! **nothing anywhere was ever marked `/OC`** — so every group the engine created was
//! empty whatever its state, and `LayerGroup::printable` reached no `/Usage`. The engine
//! could describe a "draft" underlay and could not make one.
//!
//! These tests are a round trip rather than a claim about the writer: the file is written
//! out, opened again, and *drawn*. What decides is whether the decoration reached the
//! backend, which is the same question `crates/fepdf/tests/optional_content_test.rs` asks
//! about files this project did not write.

use fepdf::{IngestionOptions, PdfDocument};
use fepdf_doc::operation::{DecorationPosition, Operation, PageSelection};
use fepdf_model::document::extensions::{LayerGroup, OptionalContentProperties, VisibilityState};
use kurbo::Affine;

use fepdf_fixtures::recorder::Recorder;

/// Writes a one-page document carrying one layer in `state`, with a decoration in it,
/// and hands back the bytes as they were written to disk.
fn document_with_a_decorated_layer(state: VisibilityState) -> Vec<u8> {
    let mut doc = PdfDocument::create_empty().expect("an empty document");
    doc.apply(Operation::UpdateLayers(OptionalContentProperties {
        layers: vec![LayerGroup {
            name: "Draft".to_string(),
            default_state: state,
            printable: false,
        }],
    }))
    .expect("the layer is written");
    doc.apply(Operation::AddPageDecoration {
        pages: PageSelection::All,
        text: "DRAFT".to_string(),
        position: DecorationPosition::BottomCenter,
        layer: Some("Draft".to_string()),
    })
    .expect("the decoration goes in the layer");

    // **Two tests ask for the same state**, and cargo runs them on parallel threads. With
    // the state alone in the name they wrote and read one path at once, and whichever
    // read first got a file the other was still writing: "the file has no document
    // catalogue (ISO 7.7.2); it may be truncated before the trailer", on a document this
    // engine had just written correctly.
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let nth = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out = std::env::temp_dir()
        .join(format!("fepdf-layer-{state:?}-{}-{nth}.pdf", std::process::id()));
    let _ = doc.save_as_version(&out, "2.0").expect("the document is written");
    let bytes = std::fs::read(&out).expect("the file is on disk");
    let _ = std::fs::remove_file(&out);
    bytes
}

/// How many glyph runs page 1 of `bytes` puts on the page.
fn runs_drawn(bytes: Vec<u8>) -> usize {
    let doc = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the written document opens");
    let mut runs = Recorder::new();
    doc.render_page(0, &mut runs, Affine::IDENTITY).expect("the page interprets");
    runs.count("text")
}

#[test]
fn a_decoration_written_into_a_layer_that_is_off_is_not_drawn() {
    assert_eq!(
        runs_drawn(document_with_a_decorated_layer(VisibilityState::Off)),
        0,
        "the decoration was drawn although its layer is off — either nothing was marked \
         /OC, or the group is not the one the configuration turns off"
    );
}

/// The control. Without it, "the layer works" and "the decoration was never written" look
/// identical from the outside.
#[test]
fn the_same_decoration_in_a_layer_that_is_on_is_drawn() {
    assert_eq!(
        runs_drawn(document_with_a_decorated_layer(VisibilityState::On)),
        1,
        "the decoration did not reach the page at all"
    );
}

/// The `/OC` has to be resolvable, not merely present: the marked-content section names
/// the group through the page's `/Properties`, and the reader draws anything it cannot
/// resolve. So a file that named a group nothing declares would pass the control above
/// and fail the first test for the wrong reason — this checks the document really carries
/// the layer under the name it was asked for.
#[test]
fn the_group_the_decoration_names_is_the_one_the_document_declares() {
    let bytes = document_with_a_decorated_layer(VisibilityState::Off);
    let doc = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the written document opens");
    let found = fepdf_model::optional_content::group_named(doc.inner(), "Draft")
        .expect("the catalogue reads");
    assert!(found.is_some(), "the document declares no group named Draft");
}

/// `printable` used to be a field the writer dropped. It reaches the file as a `/Usage`
/// `/Print` state, with the `/AS` entry that makes it act (8.11.4.5) — a usage dictionary
/// with no application beside it is a description nothing consults.
#[test]
fn printable_reaches_the_file_as_a_usage_that_something_applies() {
    let bytes = document_with_a_decorated_layer(VisibilityState::On);
    let doc = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the written document opens");
    let catalog = doc.inner().catalog().expect("the catalogue reads");
    let properties = catalog.oc_properties.expect("/OCProperties is written");
    let configuration = properties.default_configuration.expect("/D is written");
    let applications = configuration.usage_applications.expect("/AS is written");
    assert!(
        applications
            .0
            .iter()
            .any(|a| a.event.as_ref().map(fepdf_model::PdfName::as_str) == Some("Print")),
        "nothing applies the /Print usage, so `printable` still reaches no reader"
    );

    let handle = fepdf_model::optional_content::group_named(doc.inner(), "Draft")
        .expect("the catalogue reads")
        .expect("the group is declared");
    let group = fepdf_model::optional_content::group_at(doc.inner().arena(), handle)
        .expect("the group reads");
    let state = group.usage.and_then(|u| u.print).and_then(|p| p.state);
    assert_eq!(
        state,
        Some(fepdf_model::optional_content::OnOff::Off),
        "the layer was written with printable = false and the file does not say so"
    );
}

/// Naming a layer the document does not have is refused. Drawing it unconditionally
/// instead would put a "draft" mark on every page of a document whose author asked for
/// one they could switch off, and say nothing.
#[test]
fn a_decoration_naming_a_layer_that_does_not_exist_is_refused() {
    let mut doc = PdfDocument::create_empty().expect("an empty document");
    let outcome = doc.apply(Operation::AddPageDecoration {
        pages: PageSelection::All,
        text: "DRAFT".to_string(),
        position: DecorationPosition::BottomCenter,
        layer: Some("Nonexistent".to_string()),
    });
    let error = outcome.expect_err("a layer that does not exist should not be invented");
    assert!(
        format!("{error}").contains("Nonexistent"),
        "the refusal should name the layer it could not find: {error}"
    );
}

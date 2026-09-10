//! Integration tests for the public `fepdf` facade surface.
//!
//! Placed under `tests/` per RR-15 Rule 14 (Test Code Separation).

#![allow(clippy::float_cmp)]
use bytes::Bytes;
use fepdf::{Operation, PageSelection, PdfDocument, PdfStandard, Quarter, RotateMode, SaveOptions};
use std::fmt::Write as _;

/// Assembles indirect objects into a file, cross-reference and trailer included.
///
/// Written out by hand rather than by a library, so the tests exercise the reader on
/// bytes a producer would actually emit, offsets and all. Objects are numbered from 1
/// in the order given, and `root` names which of them is the catalogue.
/// A PDF 1.7 file with an `/ID`, which is what these tests are about: `version_is_read`
/// asserts the header and the encryption tests need the trailer entry.
fn assemble(bodies: &[String], root: usize) -> Bytes {
    const ID: &str = "<0123456789abcdef0123456789abcdef>";
    Bytes::from(
        fepdf_fixtures::Pdf::new()
            .version("1.7")
            .root(root)
            .trailer_entries(&format!("/ID [{ID} {ID}]"))
            .assemble(bodies),
    )
}

/// The smallest conforming document: a catalogue and an empty page tree.
fn get_minimal_pdf() -> Bytes {
    assemble(
        &[
            "<< /Type /Pages /Kids [] /Count 0 >>".to_string(),
            "<< /Type /Catalog /Pages 1 0 R >>".to_string(),
        ],
        2,
    )
}

/// **A save setting has to reach the file, and this asserted that a field held a bool.**
///
/// `test_document_save_settings_sync` set `vacuum`, `strip` and `password` through three
/// facade setters, read them back through three getters, then saved and asserted `is_ok`.
/// Its comment said "Verify SaveOptions serialization syncing" and nothing verified it.
/// The three setters wrote to fields on `PdfDocument` that duplicated three fields of
/// `SaveOptions`, and `save_with_options` — the way every frontend saves — never read
/// them: `set_strip(true)` followed by a save with default options stripped nothing, in
/// silence. They are gone, and this asks the question their name claimed.
#[test]
fn a_save_option_reaches_the_file_it_writes() {
    let doc = PdfDocument::open(get_minimal_pdf()).expect("the fixture opens");
    let path = std::env::temp_dir().join("fepdf_save_option_reaches.pdf");

    let options = SaveOptions { password: Some("secret".to_string()), ..SaveOptions::default() };
    doc.save_with_options(&path, "2.0", &options).expect("the document is written");

    let written = std::fs::read(&path).expect("the output is there");
    assert!(
        written.windows(8).any(|w| w == b"/Encrypt"),
        "a password in the options has to reach the trailer, or the file is not encrypted"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_upgrade_to_standard() {
    let data = get_minimal_pdf();
    let mut doc = PdfDocument::open(data).unwrap();

    // Verify base version
    assert_eq!(doc.inner().arena().version(), 1.7);

    // Upgrade to modern PDF 2.0
    doc.apply(Operation::Upgrade { standard: PdfStandard::ISO32000_2 }).unwrap();
    assert_eq!(doc.inner().arena().version(), 2.0);

    // Upgrade to PDF/A-4 standard and check GTS tag
    doc.apply(Operation::Upgrade { standard: PdfStandard::A4 }).unwrap();
    assert_eq!(doc.inner().arena().version(), 2.0);

    let cah = doc.inner().catalog_handle().unwrap();
    let cadh = doc.inner().resolve_to_dict(cah).unwrap();
    let catalog = doc.inner().arena().get_dict(cadh).unwrap();
    let gts_key = doc.inner().arena().name("GTS_PDFA14");
    assert!(catalog.contains_key(&gts_key));
}

#[test]
fn test_heuristic_retag_execution() {
    // A page with a large line above a small one, which is the shape the heuristic reads
    // as a heading over body text.
    let data = assemble(
        &[
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
              /Resources << /Font << /F1 5 0 R >> >> >>"
                .to_string(),
            {
                // One large line over several small ones. The heuristic takes the *most
                // common* size as the body, so a page of two lines would make the larger
                // one the body on a tie — this is the shape a real page has.
                let mut body = String::from("BT /F1 24 Tf 72 700 Td (Chapter One) Tj ET\n");
                for line in 0..6 {
                    let y = 660 - line * 20;
                    let _ = writeln!(
                        body,
                        "BT /F1 10 Tf 72 {y} Td (Body line {line} of the chapter.) Tj ET"
                    );
                }
                format!("<< /Length {} >>\nstream\n{body}endstream", body.len())
            },
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ],
        1,
    );

    let mut doc = PdfDocument::open(data).expect("the fixture opens");
    assert!(
        doc.extract_struct_tree().is_none(),
        "the fixture starts untagged, or this proves nothing"
    );

    doc.apply(Operation::Retag).expect("retagging runs");

    let tree = doc.extract_struct_tree().expect("Retag left the document untagged");
    assert!(
        !tree.children.is_empty(),
        "Retag left the document with no structure tree at all: {tree:?}"
    );

    // The tags it chose, flattened, so this says what was inferred and not merely that
    // something was.
    let mut tags = Vec::new();
    let mut stack = vec![&tree];
    while let Some(node) = stack.pop() {
        tags.push(node.tag.clone());
        stack.extend(node.children.iter());
    }
    assert!(
        tags.iter().any(|t| t.starts_with('H')),
        "the larger line should have become a heading: {tags:?}"
    );
    assert!(tags.iter().any(|t| t == "P"), "and the smaller one a paragraph: {tags:?}");
}

#[test]
fn test_cielab_to_srgb_conversion() {
    use fepdf_model::graphics::Color;
    // Test pure black: L=0, a=0, b=0 -> Rgb(0, 0, 0)
    let lab_black = Color::Lab(0.0, 0.0, 0.0);
    assert_eq!(lab_black.to_rgb(), Color::Rgb(0.0, 0.0, 0.0));

    // Test white point D65 reference: L=100, a=0, b=0 -> Rgb(1, 1, 1)
    let lab_white = Color::Lab(100.0, 0.0, 0.0);
    let rgb_white = lab_white.to_rgb();
    match rgb_white {
        Color::Rgb(r, g, b) => {
            assert!((r - 1.0).abs() < 1e-4);
            assert!((g - 1.0).abs() < 1e-4);
            assert!((b - 1.0).abs() < 1e-4);
        }
        Color::Gray(_) | Color::Cmyk(..) | Color::Lab(..) => panic!("Expected Rgb"),
    }
}

/// Revision 5 and 6 derive their key from `/U`, `/UE`, `/O` and `/OE`, and from no
/// part of `/ID`.
///
/// This asserted the reverse until it was measured: `new_v5` took a file id, invented
/// salts from it, and returned `Ok` for every password — so the test passed while the
/// handler produced a key that could not decrypt anything. Passing on a wrong answer is
/// worse than failing, and a test written from the code rather than the clause will do
/// that indefinitely.
#[test]
fn test_aes256_requires_the_documents_own_strings() {
    use fepdf::{AesV5Spec, SecurityHandler};
    let spec = AesV5Spec {
        u: &[0u8; 48],
        ue: &[0u8; 32],
        o: &[0u8; 48],
        oe: &[0u8; 32],
        revision: 6,
        encrypt_metadata: true,
    };
    assert!(
        SecurityHandler::new_aes256("password", &spec).is_none(),
        "a password that authenticates against neither /U nor /O must not build a handler"
    );
}

#[test]
fn test_open_invalid_pdf_bytes() {
    let invalid_bytes = Bytes::from_static(b"NOT A VALID PDF HEADER");
    let doc = PdfDocument::open(invalid_bytes);
    assert!(doc.is_err());
}

#[test]
fn test_document_page_count() {
    let data = get_minimal_pdf();
    let doc = PdfDocument::open(data).unwrap();
    assert_eq!(doc.page_count().unwrap(), 0);
}

/// A document with `count` pages, all children of one page tree node.
fn get_multipage_pdf(count: usize) -> Bytes {
    let pages_number = count + 1;
    let mut bodies: Vec<String> = (0..count)
        .map(|_| format!("<< /Type /Page /Parent {pages_number} 0 R /MediaBox [0 0 612 792] >>"))
        .collect();

    let kids: Vec<String> = (1..=count).map(|n| format!("{n} 0 R")).collect();
    bodies.push(format!("<< /Type /Pages /Kids [{}] /Count {count} >>", kids.join(" ")));
    bodies.push(format!("<< /Type /Catalog /Pages {pages_number} 0 R >>"));
    assemble(&bodies, count + 2)
}

/// Like `get_multipage_pdf`, but every page is a different width, so a test can say
/// *which* page it is looking at. These pages carry no content stream, so text cannot.
fn get_distinguishable_pdf(count: usize) -> Bytes {
    let pages_number = count + 1;
    let mut bodies: Vec<String> = (0..count)
        .map(|i| {
            let width = 100 + i * 10;
            format!("<< /Type /Page /Parent {pages_number} 0 R /MediaBox [0 0 {width} 792] >>")
        })
        .collect();
    let kids: Vec<String> = (1..=count).map(|n| format!("{n} 0 R")).collect();
    bodies.push(format!("<< /Type /Pages /Kids [{}] /Count {count} >>", kids.join(" ")));
    bodies.push(format!("<< /Type /Catalog /Pages {pages_number} 0 R >>"));
    assemble(&bodies, count + 2)
}

/// Pages that carry a content stream, a resource dictionary and a font, so that a clone
/// can be asked whether it kept what a page is made of.
///
/// `get_distinguishable_pdf` deliberately has no `/Contents`, which is why it could not see
/// the defect below: `/MediaBox` is a direct value and survives a clone that drops every
/// reference the page holds.
fn get_pdf_with_contents(count: usize) -> Bytes {
    let mut bodies: Vec<String> = Vec::new();
    // 1..=count: pages. Then the stream for each, then resources, font, pages, catalogue.
    let first_stream = count + 1;
    let resources = 2 * count + 1;
    let font = resources + 1;
    let pages = font + 1;
    let catalog = pages + 1;

    for i in 0..count {
        let stream = first_stream + i;
        bodies.push(format!(
            "<< /Type /Page /Parent {pages} 0 R /MediaBox [0 0 200 200] \
              /Resources {resources} 0 R /Contents {stream} 0 R >>"
        ));
    }
    for i in 0..count {
        let text = format!("BT /F1 12 Tf 10 100 Td (page {i}) Tj ET");
        bodies.push(format!("<< /Length {} >>\nstream\n{text}\nendstream", text.len()));
    }
    bodies.push(format!("<< /Font << /F1 {font} 0 R >> >>"));
    bodies.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string());
    let kids: Vec<String> = (1..=count).map(|n| format!("{n} 0 R")).collect();
    bodies.push(format!("<< /Type /Pages /Kids [{}] /Count {count} >>", kids.join(" ")));
    bodies.push(format!("<< /Type /Catalog /Pages {pages} 0 R >>"));
    assemble(&bodies, catalog)
}

/// The widths of every page, in order.
fn widths(doc: &PdfDocument) -> Vec<u32> {
    #![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    (0..doc.page_count().unwrap()).map(|i| doc.get_page_size(i).unwrap().0 as u32).collect()
}

#[test]
fn test_reorder_and_remove_pages() {
    let data = get_multipage_pdf(3);
    let mut doc = PdfDocument::open(data).unwrap();
    assert_eq!(doc.page_count().unwrap(), 3);

    // Test reorder page
    assert!(doc.apply(Operation::Reorder { from: 0, to: 2 }).is_ok());
    assert_eq!(doc.page_count().unwrap(), 3);

    // Test remove page
    assert!(doc.apply(Operation::RemovePages(PageSelection::Single(1))).is_ok());
    assert_eq!(doc.page_count().unwrap(), 2);
}

#[test]
fn test_insert_pages_from() {
    let data1 = get_multipage_pdf(2);
    let data2 = get_multipage_pdf(3);
    let mut doc1 = PdfDocument::open(data1).unwrap();

    assert_eq!(doc1.page_count().unwrap(), 2);

    doc1.apply(Operation::InsertFrom { source: data2.to_vec(), at: 1 }).unwrap();
    assert_eq!(doc1.page_count().unwrap(), 5);
}

/// `Rotate` with `Absolute`, which is what `set_page_rotation` did before Rule D removed
/// it from the facade. `Quarter` is why the operation cannot express the 45° the old
/// signature accepted (ARCHITECTURE §4.1).
fn rotate_to(doc: &mut PdfDocument, page: usize, quarter: Quarter) -> fepdf::PdfResult<()> {
    doc.apply(Operation::Rotate {
        pages: PageSelection::Single(page),
        mode: RotateMode::Absolute(quarter),
    })
}

/// Duplicating several pages at once must place each clone after *its own* original.
///
/// The failure this guards is an ascending loop: insert after page 0, and page 1's clone
/// lands after what is now page 2. `apply_duplicate_pages` walks the selection descending
/// so that each insertion leaves the indices still to be handled where they were.
///
/// Verified by putting the loop back in ascending order, and the measured failure is
/// worse than the one predicted when this was written: not a mis-ordering but
/// `100 100 100 100 110 120` — **page 0 cloned three times**, because after the first
/// insertion indices 1 and 2 name clones rather than the originals they were chosen from.
#[test]
fn duplicating_several_pages_keeps_each_clone_beside_its_original() {
    let mut doc = PdfDocument::open(get_distinguishable_pdf(3)).unwrap();
    assert_eq!(widths(&doc), vec![100, 110, 120]);

    doc.apply(Operation::DuplicatePages(PageSelection::Indices(vec![0, 1, 2]))).unwrap();
    assert_eq!(
        widths(&doc),
        vec![100, 100, 110, 110, 120, 120],
        "clones are not beside their originals"
    );
}

/// **A duplicated page keeps what it is made of, not just its size.**
///
/// `ObjectCloner::clone_object` queues each reference and leaves an `Object::Null`
/// placeholder in its place until the queue is drained, and `apply_duplicate_pages` never
/// drained it. Every clone therefore came out with `/Contents`, `/Resources` and `/Annots`
/// all `Null`, and the document answered `expected a stream, found null` the moment the new
/// page was rendered. It was reported from the viewer, on a page duplicated by hand.
///
/// The test that was here checked page widths, and `/MediaBox` is a direct value: the one
/// part of a page dictionary that survives a clone which drops every reference.
#[test]
fn a_duplicated_page_keeps_its_contents() {
    let mut doc = PdfDocument::open(get_pdf_with_contents(2)).unwrap();
    let original = doc.extract_text(1).unwrap();
    assert!(original.contains("page 1"), "the fixture itself must have text: {original:?}");

    doc.apply(Operation::DuplicatePages(PageSelection::Single(1))).unwrap();
    assert_eq!(doc.page_count().unwrap(), 3);

    let clone = doc.extract_text(2).unwrap();
    assert_eq!(clone, original, "the clone reads differently from the page it came from");
}

/// The same for a page brought in from another document, which clones the same way.
#[test]
fn a_page_inserted_from_another_document_keeps_its_contents() {
    let mut doc = PdfDocument::open(get_pdf_with_contents(1)).unwrap();
    let source = get_pdf_with_contents(2);

    doc.apply(Operation::InsertFrom { source: source.to_vec(), at: 1 }).unwrap();
    assert_eq!(doc.page_count().unwrap(), 3);
    assert!(doc.extract_text(1).unwrap().contains("page 0"), "the inserted page is empty");
}

/// A selection naming a page the document does not have is refused, not silently dropped.
#[test]
fn duplicating_a_page_that_is_not_there_is_an_error() {
    let mut doc = PdfDocument::open(get_multipage_pdf(2)).unwrap();
    assert!(doc.apply(Operation::DuplicatePages(PageSelection::Single(5))).is_err());
    assert_eq!(doc.page_count().unwrap(), 2);
}

#[test]
fn test_page_rotation() {
    let data = get_multipage_pdf(2);
    let mut doc = PdfDocument::open(data).unwrap();

    assert_eq!(doc.get_page_rotation(0).unwrap(), 0);
    let (w0, h0) = doc.get_page_size(0).unwrap();

    assert!(rotate_to(&mut doc, 0, Quarter::Q90).is_ok());
    assert_eq!(doc.get_page_rotation(0).unwrap(), 90);
    let (w90, h90) = doc.get_page_size(0).unwrap();
    assert_eq!((w90, h90), (h0, w0));

    assert!(rotate_to(&mut doc, 0, Quarter::Q180).is_ok());
    assert_eq!(doc.get_page_rotation(0).unwrap(), 180);
    let (w180, h180) = doc.get_page_size(0).unwrap();
    assert_eq!((w180, h180), (w0, h0));

    assert!(rotate_to(&mut doc, 0, Quarter::Q270).is_ok());
    assert_eq!(doc.get_page_rotation(0).unwrap(), 270);
    let (w270, h270) = doc.get_page_size(0).unwrap();
    assert_eq!((w270, h270), (h0, w0));
}

#[test]
fn rotate_absolute_sets_the_angle_regardless_of_current_rotation() {
    let mut doc = PdfDocument::open(get_multipage_pdf(2)).unwrap();
    rotate_to(&mut doc, 0, Quarter::Q90).unwrap();

    doc.apply(fepdf::Operation::Rotate {
        pages: fepdf::PageSelection::Single(0),
        mode: fepdf::RotateMode::Absolute(fepdf::Quarter::Q90),
    })
    .unwrap();

    // Absolute means "set to", so a page already at 90 stays at 90.
    assert_eq!(doc.get_page_rotation(0).unwrap(), 90);
}

#[test]
fn rotate_relative_accumulates_and_wraps() {
    let mut doc = PdfDocument::open(get_multipage_pdf(2)).unwrap();
    rotate_to(&mut doc, 0, Quarter::Q90).unwrap();

    doc.apply(fepdf::Operation::Rotate {
        pages: fepdf::PageSelection::Single(0),
        mode: fepdf::RotateMode::Relative(fepdf::Quarter::Q90),
    })
    .unwrap();
    assert_eq!(doc.get_page_rotation(0).unwrap(), 180);

    // 180 + 270 wraps rather than reaching 450.
    doc.apply(fepdf::Operation::Rotate {
        pages: fepdf::PageSelection::Single(0),
        mode: fepdf::RotateMode::Relative(fepdf::Quarter::Q270),
    })
    .unwrap();
    assert_eq!(doc.get_page_rotation(0).unwrap(), 90);
}

#[test]
fn rotate_applies_to_every_selected_page() {
    let mut doc = PdfDocument::open(get_multipage_pdf(3)).unwrap();

    doc.apply(fepdf::Operation::Rotate {
        pages: fepdf::PageSelection::Indices(vec![0, 2]),
        mode: fepdf::RotateMode::Relative(fepdf::Quarter::Q90),
    })
    .unwrap();

    assert_eq!(doc.get_page_rotation(0).unwrap(), 90);
    assert_eq!(doc.get_page_rotation(1).unwrap(), 0);
    assert_eq!(doc.get_page_rotation(2).unwrap(), 90);
}

#[test]
fn quarter_rejects_angles_that_are_not_multiples_of_90() {
    // The type is what stops `--angle 45` reaching /Rotate, where ISO 32000-2
    // requires a multiple of 90.
    assert!(fepdf::Quarter::from_degrees(45).is_none());
    assert!(fepdf::Quarter::from_degrees(1).is_none());
    assert_eq!(fepdf::Quarter::from_degrees(450), Some(fepdf::Quarter::Q90));
    assert_eq!(fepdf::Quarter::from_degrees(-90), Some(fepdf::Quarter::Q270));
}

#[test]
fn implemented_operations_still_succeed() {
    let mut doc = PdfDocument::open(get_multipage_pdf(2)).unwrap();
    assert!(
        doc.apply(fepdf::Operation::Rotate {
            pages: fepdf::PageSelection::All,
            mode: fepdf::RotateMode::Relative(fepdf::Quarter::Q90),
        })
        .is_ok()
    );
    assert_eq!(doc.get_page_rotation(0).unwrap(), 90);

    assert!(
        doc.apply(fepdf::Operation::SetPageLabels(vec![fepdf::PageLabelSpec {
            start_page: 0,
            style: fepdf::PageLabelStyle::Decimal,
            prefix: None,
            start_number: 1,
        }]))
        .is_ok()
    );

    assert!(
        doc.apply(fepdf::Operation::AddAnnotation(fepdf::AnnotationSpec {
            page: 0,
            rect: [50.0, 50.0, 150.0, 150.0],
            kind: fepdf::AnnotationKind::TextComment { contents: "Test Comment".to_string() },
        }))
        .is_ok()
    );

    assert!(
        doc.apply(fepdf::Operation::ApplyBatesNumbering {
            pages: fepdf::PageSelection::All,
            prefix: "TEST-".to_string(),
            start_number: 100,
            digits: 6,
            position: fepdf::DecorationPosition::BottomRight,
        })
        .is_ok()
    );

    assert!(
        doc.apply(fepdf::Operation::SetPronunciationLexicon {
            lexicon_xml_bytes: b"<lexicon/>".to_vec(),
        })
        .is_ok()
    );

    assert!(
        doc.apply(fepdf::Operation::ExecuteAction(
            fepdf::PdfAction::Named("NextPage".to_string(),)
        ))
        .is_ok()
    );
}

/// A page that declares `/UserUnit` (14.11.2, Table 31) is that many seventy-seconds of an
/// inch per unit, so a 400-unit page at `/UserUnit 10` is 55 inches rather than 5.5.
///
/// **The engine had no notion of it at all.** It is how a drawing exceeds the 14,400-unit
/// limit a box can express, and a renderer that ignores it produces an image a tenth of the
/// size the document asks for. One file in 524 across both corpora declares one, and it is
/// in `pdf-differences` — a corpus built to expose readers disagreeing.
#[test]
fn a_user_unit_is_read_and_defaults_to_one() {
    let doc = PdfDocument::open(get_pdf_with_contents(1)).unwrap();
    assert!(
        (doc.get_page_user_unit(0).unwrap() - 1.0).abs() < f64::EPSILON,
        "a page that declares nothing is 1.0, which Table 31 makes the default"
    );
}

/// A value that is absent, unreadable or not positive falls back to Table 31's default
/// rather than scaling a page to nothing or to infinity.
#[test]
fn an_impossible_user_unit_is_refused() {
    for declared in ["0", "-3", "/Name", "(text)"] {
        let bodies = vec![
            format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /UserUnit {declared} >>"),
            "<< /Type /Pages /Kids [1 0 R] /Count 1 >>".to_string(),
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        ];
        let doc = PdfDocument::open(assemble(&bodies, 3)).unwrap();
        let unit = doc.get_page_user_unit(0).unwrap();
        assert!((unit - 1.0).abs() < f64::EPSILON, "/UserUnit {declared} gave {unit}");
    }
}

/// And a real one is read, whether written as an integer or a real.
#[test]
fn a_declared_user_unit_is_taken_as_written() {
    for (declared, expected) in [("10", 10.0), ("2.5", 2.5), ("1", 1.0)] {
        let bodies = vec![
            format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /UserUnit {declared} >>"),
            "<< /Type /Pages /Kids [1 0 R] /Count 1 >>".to_string(),
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        ];
        let doc = PdfDocument::open(assemble(&bodies, 3)).unwrap();
        let unit = doc.get_page_user_unit(0).unwrap();
        assert!((unit - expected).abs() < f64::EPSILON, "/UserUnit {declared} gave {unit}");
    }
}

/// **Every caller gets the same fallback faces.** There were three assemblies of them and
/// they did not agree: the model's had the platform's own fonts under it, the renderer's
/// had nothing under it and returned an empty map wherever a resource directory was absent
/// — which is every released archive — and the CLI's was a hand-written list of macOS and
/// Debian paths with no Windows in it at all.
///
/// The test is that the map is not empty and that the two public entry points answer alike.
/// It cannot say *which* faces a machine has, since that is the machine's business; it can
/// say that asking through a different crate does not change the answer.
#[test]
fn the_fallback_faces_do_not_depend_on_which_crate_asked() {
    let direct = fepdf::fallback_fonts();
    assert!(!direct.is_empty(), "no fallback face was found by any route");

    // The one face nothing used to put in the map: `Default` is what a font resource gets
    // when nothing about it suggests a face, and it was in no list of missing types.
    assert!(
        direct.contains_key(&fepdf::FallbackFontType::Default),
        "the default face is seeded, which is what the CLI used to patch by hand"
    );

    #[cfg(feature = "render")]
    {
        let through_renderer = fepdf::VelloBackend::load_system_fonts();
        let mine: Vec<_> = direct.keys().copied().collect();
        let theirs: Vec<_> = through_renderer.keys().copied().collect();
        assert_eq!(mine, theirs, "the renderer and the free function disagree");
    }
}

/// Applies `history` to a fresh open of `data` and reports the pages that come out.
///
/// This is what `fepdf-gui`'s worker does to take an operation back: `ARCHITECTURE.md`
/// §4.1 has undo *recorded and replayed* rather than inverted, because nothing in the
/// engine inverts an operation and two of them — `Retag`, `ApplyBatesNumbering` — have no
/// inverse to write.
fn replay(data: &Bytes, history: &[Operation]) -> Vec<u32> {
    let mut doc = PdfDocument::open(data.clone()).expect("the fixture opens");
    for operation in history {
        doc.apply(operation.clone()).expect("every operation in a history applied once");
    }
    widths(&doc)
}

/// **The property undo rests on.** The same operations over the same bytes reach the same
/// document, so replaying a shortened history puts the reader back where they were rather
/// than somewhere near it. Nothing checked this while the GUI depended on it.
#[test]
fn replaying_a_history_reaches_the_same_document() {
    let data = get_distinguishable_pdf(6);
    let history = vec![
        Operation::RemovePages(PageSelection::Single(1)),
        Operation::DuplicatePages(PageSelection::Single(0)),
        Operation::ReorderBatch { sources: vec![0, 1], target: 3 },
        Operation::Rotate {
            pages: PageSelection::Indices(vec![0, 2]),
            mode: RotateMode::Relative(Quarter::Q90),
        },
    ];

    let once = replay(&data, &history);
    let twice = replay(&data, &history);
    assert_eq!(once, twice, "a replay is not a second answer");
    assert_ne!(once, widths(&PdfDocument::open(data).unwrap()), "the history did something");
}

/// **Replaying all but the last operation is the state before that operation**, which is
/// what undo means. Taken one at a time, back to the file itself.
#[test]
fn replaying_all_but_the_last_is_the_state_before_it() {
    let data = get_distinguishable_pdf(5);
    let history = [
        Operation::RemovePages(PageSelection::Single(3)),
        Operation::DuplicatePages(PageSelection::Single(1)),
        Operation::RemovePages(PageSelection::Single(0)),
    ];

    // Each prefix is what the document looked like after that many operations.
    let after: Vec<Vec<u32>> = (0..=history.len()).map(|n| replay(&data, &history[..n])).collect();

    for n in (1..=history.len()).rev() {
        let undone = replay(&data, &history[..n - 1]);
        assert_eq!(undone, after[n - 1], "undoing operation {n} lands on the state before it");
        assert_ne!(undone, after[n], "and that state is not the one it came from");
    }

    assert_eq!(after[0], widths(&PdfDocument::open(data).unwrap()), "undone to the file itself");
}

//! How many fonts a document has, which is one number and not two.
//!
//! `inspect info` reported **24 fonts for `samples/constitution.pdf`, 72 for `fugaku.pdf`
//! and 14 for `print_sample.pdf`**, against 12, 36 and 7 with `--no-refinement`: exactly
//! twice, on every sample, from a tool whose business is telling people what is in their
//! documents.
//!
//! **Refinement commits every dictionary to a new handle**, so the one it replaced stays
//! in the arena unreferenced, and `list_fonts` walked every handle there rather than what
//! the document reaches. A count through objects instead would drop the orphan — and
//! would also drop a font written *directly* into a resource dictionary, which is legal
//! (7.3.10) and is the shape this engine's own decorations used until 2026-09-19. What
//! settles it is reaching the fonts the way a reader does.

use fepdf::{IngestionOptions, PdfDocument};

/// The nine samples, by name.
fn samples() -> Vec<std::path::PathBuf> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .expect("samples/ is in the tree")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("pdf"))
        .collect();
    files.sort();
    files
}

fn fonts_in(path: &std::path::Path, refine: bool) -> Option<usize> {
    let bytes = std::fs::read(path).ok()?;
    let options = IngestionOptions { active_refinement: refine, ..IngestionOptions::default() };
    Some(PdfDocument::open_with_options(bytes.into(), &options).ok()?.fonts().len())
}

/// **A document has the fonts it has, however it was read.**
///
/// Refinement changes what the engine understands of a file and not what the file holds,
/// so a count that moves when it is switched on is counting the engine rather than the
/// document.
#[test]
fn the_font_count_does_not_depend_on_how_the_file_was_read() {
    let mut disagreed = Vec::new();
    for path in samples() {
        let (refined, plain) = (fonts_in(&path, true), fonts_in(&path, false));
        if refined != plain {
            disagreed.push(format!("{:?}: {refined:?} refined, {plain:?} plain", path.file_name()));
        }
    }
    assert!(disagreed.is_empty(), "the same files count differently: {disagreed:#?}");
}

/// **A font written directly into a resource dictionary is still a font.**
///
/// 7.3.10 lets any object be direct, and a font dictionary in a `/Font` resource needs no
/// object of its own. Counting through objects would have fixed the doubling and lost
/// these — including the ones this engine itself wrote until 2026-09-19 — so this is the
/// half of the fix that is not about the number going down.
#[test]
fn a_font_written_directly_into_the_resources_is_counted() {
    let content = "BT /F1 24 Tf 1 0 0 1 40 700 Tm (A) Tj ET";
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
          /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> \
          >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
    ];
    let doc = PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&bodies).into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens");

    let fonts = doc.fonts();
    assert_eq!(fonts.len(), 1, "a direct font dictionary was not counted: {fonts:?}");
    assert_eq!(fonts[0].name, "Helvetica");
}

/// A font nothing reaches is not a font this document has.
#[test]
fn a_font_no_page_reaches_is_not_counted() {
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
        // An orphan, of exactly the shape refinement leaves behind.
        "<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>",
    ];
    let doc = PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&bodies).into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens");
    assert!(doc.fonts().is_empty(), "an unreachable font was counted: {:?}", doc.fonts());
}

/// A one-page document whose `/Font` resource holds `entry`.
fn page_with_font(entry: &str, extra: &[&str]) -> Vec<u8> {
    let mut bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] \
               /Resources << /Font << /F1 {entry} >> >> >>"
        ),
    ];
    bodies.extend(extra.iter().map(|b| (*b).to_string()));
    fepdf_fixtures::assemble(&bodies).into_iter().collect()
}

fn opened(bytes: Vec<u8>) -> PdfDocument {
    PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the fixture opens")
}

/// **A font dictionary written direct has no object number, and says so.**
///
/// 7.3.10 lets any object be direct, and this engine's own decorations wrote fonts that
/// way until 2026-09-19. `FontSummary::object_id` was a `u32` filled by scanning every
/// object in the arena for one holding this dictionary and, when that came back empty,
/// by **fabricating a handle out of the dictionary's own index** — a number from the
/// `dicts` pool reported as one from the `objects` pool. `inspect debug` hands it to
/// `get_font`, so the wrong number was not only printed.
#[test]
fn a_direct_font_dictionary_has_no_object_number() {
    let doc = opened(page_with_font("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>", &[]));
    let fonts = doc.fonts();

    assert_eq!(fonts.len(), 1, "the direct font was not reached at all: {fonts:?}");
    assert_eq!(
        fonts[0].object_id, None,
        "a direct font dictionary was given an object number: {:?}",
        fonts[0]
    );
    assert_eq!(fonts[0].name, "Helvetica", "the wrong dictionary was summarised");
}

/// **An indirect one reports the object that holds it, and that object loads it back.**
///
/// The other half: the number has to be the one a caller can use, which is what
/// `inspect debug` does with it. Asserting it is `Some` would pass on any number at all.
#[test]
fn an_indirect_font_dictionary_reports_the_object_that_holds_it() {
    let doc = opened(page_with_font(
        "4 0 R",
        &["<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>"],
    ));
    let fonts = doc.fonts();

    assert_eq!(fonts.len(), 1, "the font was not reached: {fonts:?}");
    let id = fonts[0].object_id.expect("an indirect font dictionary has an object number");
    let font = doc.get_font(id).expect("the reported object number loads the font");
    assert!(
        font.base_font.as_str().contains("Times"),
        "the object number named something else: {:?}",
        font.base_font
    );
}

/// **The substituted-key fallback could not be reached, and this holds the reason.**
///
/// Five sites read `arena.get_name_by_str(k).unwrap_or(fv)`, where `fv` is the handle for
/// `/Font` — the resource name the font walk starts from. Where the name had never been
/// interned they looked `/Font` up in a font dictionary and reported what they found
/// there as the font's subtype, encoding or name.
///
/// **It never happened, and a behavioural test for it is a test that cannot fail.** This
/// document writes none of `/Encoding`, `/BaseFont`, `/FontDescriptor` or
/// `/DescendantFonts`, and all four are interned by the time `open` returns, because
/// normalisation-at-load builds every font and font construction reads them through
/// `arena.name` — which interns
/// ([ADR-0046](../../../docs/adr/0046-unify-font-construction-paths-at-load.md)). A first
/// version of this test asserted the encoding instead and **survived a mutation restoring
/// the fallback**, which is what sent it here.
///
/// So the fix is on principle — a fallback that answers from the wrong key is wrong
/// whether or not anything reaches it — and what is checked is the reason it was
/// unreachable. The day font construction stops interning these names, this fails and the
/// arm needs a behavioural test.
#[test]
fn the_keys_a_font_summary_reads_are_interned_before_it_reads_them() {
    let doc = opened(page_with_font(
        "4 0 R",
        &["<< /Type /Font /Subtype /TrueType /BaseFont /Helvetica /Font /Bogus >>"],
    ));
    let arena = doc.inner().arena();

    for key in ["Subtype", "Encoding", "FontDescriptor", "DescendantFonts"] {
        assert!(
            arena.get_name_by_str(key).is_some(),
            "/{key} is not interned after open, so the substituted-key fallback this \
             document's /Font key would have been read by is now reachable and wants a \
             test of its own"
        );
    }
    // A name nothing writes and nothing looks up stays uninterned, so the check above is
    // about these keys and not about interning everything.
    assert!(
        arena.get_name_by_str("NeverUsedAnywhere").is_none(),
        "every name is interned, so the assertion above says nothing"
    );

    // And the reading itself is right: the font states no /Encoding, so it has none.
    assert_eq!(
        doc.fonts()[0].encoding,
        "Standard",
        "a font stating no /Encoding was given one from another key"
    );
}

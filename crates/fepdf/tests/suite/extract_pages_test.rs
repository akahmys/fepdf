//! What `extract_pages` actually produces.
//!
//! It had one caller — `fepdf extract` — and no test. `create_empty` was in the same
//! position and had never once produced a well-formed file, so nothing about a function
//! being present and called is evidence that it works.

use fepdf::PdfDocument;

/// A sample from the corpus, or `None` when `samples/` is not in the tree.
fn sample(name: &str) -> Option<PdfDocument> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples").join(name);
    let bytes = std::fs::read(path).ok()?;
    PdfDocument::open(bytes.into()).ok()
}

/// A directory to write into, removed by the caller.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("fepdf-extract-{name}"));
    std::fs::create_dir_all(&dir).expect("a directory to write into");
    dir
}

#[test]
#[ignore = "needs samples/, which the repository does not hold"]
fn the_extracted_document_has_the_pages_asked_for_and_no_others() {
    let Some(doc) = sample("fy05.pdf") else {
        panic!("samples/ is not in the tree, so this has nothing to test")
    };
    let out = doc.extract_pages(vec![2, 4, 6]).expect("three pages come out");
    assert_eq!(out.page_count().expect("it counts"), 3);
}

/// The pages that come out are the pages that went in, not the first three.
#[test]
#[ignore = "needs samples/, which the repository does not hold"]
fn the_pages_are_the_ones_named() {
    let Some(doc) = sample("fy05.pdf") else {
        panic!("samples/ is not in the tree, so this has nothing to test")
    };
    let wanted = [2usize, 4, 6];
    let out = doc.extract_pages(wanted.to_vec()).expect("it extracts");
    for (place, &source) in wanted.iter().enumerate() {
        let before = doc.extract_text(source).expect("the source page has text");
        let after = out.extract_text(place).expect("the extracted page has text");
        assert_eq!(after, before, "extracted page {place} is not source page {source}");
    }
}

/// It must survive being written and read back — an in-memory arena is not a file.
#[test]
#[ignore = "needs samples/, which the repository does not hold"]
fn it_survives_being_written_and_read_back() {
    let Some(doc) = sample("fy05.pdf") else {
        panic!("samples/ is not in the tree, so this has nothing to test")
    };
    let out = doc.extract_pages(vec![2, 4, 6]).expect("it extracts");
    let dir = scratch("round-trip");
    let path = dir.join("three.pdf");
    let _ = out.save_as_version(&path, "2.0").expect("it writes");

    let bytes = std::fs::read(&path).expect("it is on disk");
    let back = PdfDocument::open(bytes.into()).expect("it opens");
    let pages = back.page_count().expect("it counts");
    let decisions = back.decisions();
    let text = back.extract_text(0).expect("the first page has text");
    let source = doc.extract_text(2).expect("the source page has text");
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(pages, 3);
    assert!(decisions.is_empty(), "the written file needed repairing: {decisions:?}");
    assert_eq!(text, source, "the written file's first page is not the page extracted");
}

/// An empty selection is refused rather than answered with an empty document.
#[test]
#[ignore = "needs samples/, which the repository does not hold"]
fn extracting_nothing_is_refused() {
    let Some(doc) = sample("fy05.pdf") else {
        panic!("samples/ is not in the tree, so this has nothing to test")
    };
    assert!(doc.extract_pages(Vec::new()).is_err());
}

/// A page index the document does not have is refused, not skipped.
#[test]
#[ignore = "needs samples/, which the repository does not hold"]
fn a_page_that_is_not_there_is_refused() {
    let Some(doc) = sample("fy05.pdf") else {
        panic!("samples/ is not in the tree, so this has nothing to test")
    };
    let past_the_end = doc.page_count().expect("it counts");
    assert!(doc.extract_pages(vec![0, past_the_end]).is_err());
}

/// `merge` built its document the same way and reported the same nothing.
///
/// Its one caller is `fepdf merge`, which writes the result immediately — so the count
/// being wrong was invisible for as long as nobody asked the returned document anything.
#[test]
#[ignore = "needs samples/, which the repository does not hold"]
fn a_merged_document_knows_how_many_pages_it_has() {
    let Some(first) = sample("print_sample.pdf") else {
        panic!("samples/ is not in the tree, so this has nothing to test")
    };
    let Some(second) = sample("constitution.pdf") else {
        panic!("samples/ is not in the tree, so this has nothing to test")
    };
    let total = first.page_count().expect("it counts") + second.page_count().expect("it counts");
    let merged = PdfDocument::merge(vec![first, second]).expect("they merge");
    assert_eq!(merged.page_count().expect("it counts"), total);
}

/// `InsertFrom` puts every page of the source in, and the document says so afterwards.
///
/// The counts are re-derived with
/// `cargo run --release --example page_counts -p fepdf -- samples/print_sample.pdf samples/constitution.pdf`
/// — 23 and 13 on 2026-09-14.
#[test]
#[ignore = "needs samples/, which the repository does not hold"]
fn inserting_a_document_adds_all_of_its_pages() {
    let Some(mut doc) = sample("print_sample.pdf") else {
        panic!("samples/ is not in the tree, so this has nothing to test")
    };
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/constitution.pdf");
    let Ok(bytes) = std::fs::read(source) else {
        panic!("samples/ is not in the tree, so this has nothing to test")
    };
    let before = doc.page_count().expect("it counts");
    let added = PdfDocument::open(bytes.clone().into()).expect("it opens").page_count().unwrap();

    doc.apply(fepdf::Operation::InsertFrom { source: bytes, at: 2 }).expect("it inserts");

    assert_eq!(doc.page_count().expect("it counts"), before + added);
    // The insertion went in at the position asked for: page 2 is the source's first page.
    let source_first =
        sample("constitution.pdf").expect("it opens").extract_text(0).expect("it reads");
    assert_eq!(doc.extract_text(2).expect("it reads"), source_first);
}

/// A two-page tagged document everything names the second page of: a link on the first,
/// a bookmark, a named destination, the `/OpenAction`, and two structure elements — one
/// whose content is all on it, with `/Alt` and an `/ID` the `/IDTree` names, and one with
/// a mark on each page.
fn named_everywhere() -> PdfDocument {
    let first = "/P << /MCID 0 >> BDC BT /F1 12 Tf 20 100 Td (KEEPME) Tj ET EMC";
    let second = "/P << /MCID 0 >> BDC BT /F1 12 Tf 20 100 Td (SECRETPAGETWO) Tj ET EMC \
                  /P << /MCID 1 >> BDC BT /F1 12 Tf 20 80 Td (SPANNING) Tj ET EMC";
    let stream =
        |content: &str| format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len());
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R /Outlines 9 0 R /Dests << /two [4 0 R /Fit] >> \
         /OpenAction [4 0 R /Fit] /StructTreeRoot 11 0 R >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Contents 5 0 R \
         /Resources << /Font << /F1 7 0 R >> >> /Annots [8 0 R] /StructParents 0 >>"
            .to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Contents 6 0 R \
         /Resources << /Font << /F1 7 0 R >> >> /StructParents 1 >>"
            .to_string(),
        stream(first),
        stream(second),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        "<< /Type /Annot /Subtype /Link /Rect [0 0 50 50] /Dest [4 0 R /Fit] >>".to_string(),
        "<< /Type /Outlines /First 10 0 R /Last 10 0 R /Count 1 >>".to_string(),
        "<< /Title (Two) /Parent 9 0 R /Dest /two >>".to_string(),
        "<< /Type /StructTreeRoot /K [12 0 R 13 0 R] \
         /ParentTree << /Nums [0 [13 0 R] 1 [12 0 R 13 0 R]] >> \
         /IDTree << /Names [(p2) 12 0 R] >> >>"
            .to_string(),
        "<< /Type /StructElem /S /P /P 11 0 R /Pg 4 0 R /ID (p2) /Alt (SECRETALT) /K 0 >>"
            .to_string(),
        "<< /Type /StructElem /S /P /P 11 0 R /Pg 3 0 R \
         /K [0 << /Type /MCR /Pg 4 0 R /MCID 1 >>] >>"
            .to_string(),
    ];
    PdfDocument::open(fepdf_fixtures::assemble(&bodies).into()).expect("the fixture opens")
}

/// `doc` written and read back, and whether any of its objects says `needle` — in a
/// string, or in a stream as written or as hex.
fn written_mentions(doc: &PdfDocument, name: &str, needle: &str) -> (PdfDocument, bool) {
    let dir = scratch(name);
    let path = dir.join("out.pdf");
    doc.save_with_options(&path, "2.0", &fepdf::SaveOptions::default()).expect("it writes");
    let back = PdfDocument::open(std::fs::read(&path).expect("it is there").into())
        .expect("it reads back");
    let _ = std::fs::remove_dir_all(&dir);
    let hex = needle.bytes().fold(String::new(), |mut hex, b| {
        use std::fmt::Write as _;
        let _ = write!(hex, "{b:02x}");
        hex
    });
    let says = |bytes: &[u8]| {
        let text = String::from_utf8_lossy(bytes).to_lowercase();
        text.contains(&needle.to_lowercase()) || text.contains(&hex)
    };
    let arena = back.inner().arena();
    let mentions = (0..arena.object_count()).any(|i| {
        let object = arena.get_object(arena.handle(i));
        object.as_ref().is_some_and(|o| back.inner().decode_stream(o).is_ok_and(|b| says(&b)))
    }) || arena.all_dict_handles().into_iter().any(|d| {
        arena.get_dict(d).unwrap_or_default().values().any(|value| match value {
            fepdf_model::Object::String(b) | fepdf_model::Object::Hex(b) => says(b),
            fepdf_model::Object::Text(t) => says(t.as_bytes()),
            _ => false,
        })
    });
    (back, mentions)
}

/// **A page taken out is not written** (ROADMAP Y-F18): its content, and the `/Alt` of
/// the element that was all on it, are nowhere in the file, though a link, a bookmark, a
/// named destination, the `/OpenAction` and the structure tree all named it. What was
/// on the page kept is still there, the bookmark keeps its title, and the element with a
/// mark on each page keeps the one on the page kept.
#[test]
fn a_removed_page_is_not_written_whatever_named_it() {
    let mut doc = named_everywhere();
    doc.apply(fepdf::Operation::RemovePages(fepdf::operation::PageSelection::Single(1)))
        .expect("the page is removed");
    for needle in ["SECRETPAGETWO", "SECRETALT", "SPANNING"] {
        let (_, mentions) = written_mentions(&doc, "removed", needle);
        assert!(!mentions, "{needle} is still in the written file");
    }
    let (back, _) = written_mentions(&doc, "removed-back", "KEEPME");
    assert_eq!(back.extract_text(0).expect("page one reads").trim(), "KEEPME");
    let (outline, _) = back.outlines();
    assert_eq!(outline.items.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(), ["Two"]);
    // What named the page went with it, rather than staying with its target made null.
    let arena = back.inner().arena();
    let catalog = back.inner().catalog_handle().and_then(|h| back.inner().resolve_to_dict(h).ok());
    let entry = |dict, key: &str| arena.dict_entry(dict, arena.name(key));
    let catalog = catalog.expect("the catalogue reads");
    assert!(entry(catalog, "OpenAction").is_none(), "the /OpenAction to the page stayed");
    let dests = entry(catalog, "Dests").and_then(|d| d.resolve(arena).as_dict_handle());
    assert!(
        dests.is_none_or(|d| arena.get_dict(d).unwrap_or_default().is_empty()),
        "the named destination to the page stayed"
    );
    let page = back.inner().get_page_handle(0).and_then(|h| arena.get_object(h));
    let annots = page.and_then(|p| entry(p.as_dict_handle()?, "Annots"));
    let links = match annots.map(|a| a.resolve(arena)) {
        Some(fepdf_model::Object::Array(a)) => arena.get_array(a).unwrap_or_default().len(),
        _ => 0,
    };
    assert_eq!(links, 0, "the link to the page stayed");
    let first = back.inner().catalog_handle().and_then(|_| entry(catalog, "Outlines"));
    let item = first
        .and_then(|o| entry(o.resolve(arena).as_dict_handle()?, "First"))
        .and_then(|i| i.resolve(arena).as_dict_handle())
        .expect("the bookmark is there");
    assert!(entry(item, "Dest").is_none(), "the bookmark kept a destination to the page");
    let tree = back.extract_struct_tree().expect("the tree is there");
    assert_eq!(tree.children.len(), 1, "the element all on the removed page went");
    assert_eq!(tree.children[0].mcids, vec![0], "the spanning element keeps page one's mark");
}

/// **Nor is a page left out of an extraction**, which cloning the link on the page asked
/// for brought across.
#[test]
fn a_page_left_out_of_an_extraction_is_not_written() {
    let out = named_everywhere().extract_pages(vec![0]).expect("it extracts");
    for needle in ["SECRETPAGETWO", "SPANNING"] {
        let (_, mentions) = written_mentions(&out, "extracted", needle);
        assert!(!mentions, "{needle} is still in the extracted file");
    }
}

/// A one-page document whose outline has one item, `title`, going to its page.
fn outlined(title: &str) -> PdfDocument {
    let bytes = fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>".to_string(),
        "<< /Type /Outlines /First 5 0 R /Last 5 0 R /Count 1 >>".to_string(),
        format!("<< /Title ({title}) /Parent 4 0 R /Dest [3 0 R /Fit] >>"),
    ]);
    PdfDocument::open(bytes.into()).expect("the fixture opens")
}

/// **Each source's outline comes across under an item of its own, in order.**
///
/// The items are linked to each other, so reading the outline back walks `/Next` from the
/// first to the second; and the second source's bookmark goes to the second page.
#[test]
fn a_merged_document_keeps_each_sources_outline_under_its_own_item() {
    let merged = PdfDocument::merge(vec![outlined("One"), outlined("Two")]).expect("they merge");
    let (tree, _) = merged.outlines();
    let shape: Vec<_> = tree
        .items
        .iter()
        .map(|i| {
            let children: Vec<_> =
                i.children.iter().map(|c| (c.title.clone(), c.destination_page)).collect();
            (i.title.clone(), children)
        })
        .collect();
    assert_eq!(
        shape,
        [
            ("Source 1".to_string(), vec![("One".to_string(), 0)]),
            ("Source 2".to_string(), vec![("Two".to_string(), 1)]),
        ]
    );
}

/// **Every source's form fields are in the merged form.**
#[test]
fn a_merged_document_has_every_sources_fields() {
    use fepdf_fixtures::{FormField, acroform};
    let open = |bytes: Vec<u8>| PdfDocument::open(bytes.into()).expect("the fixture opens");
    let first = open(acroform(&[FormField::new("who", "a")], &[]));
    let second = open(acroform(&[FormField::new("when", "b")], &[]));
    let merged = PdfDocument::merge(vec![first, second]).expect("they merge");
    let form = fepdf::form_of(merged.inner());
    let names: Vec<_> = form.terminal.iter().map(|f| f.name.clone()).collect();
    assert_eq!(names, [Some("who".to_string()), Some("when".to_string())]);
}

/// Merging nothing is refused rather than answered with an empty document.
#[test]
fn merging_nothing_is_refused() {
    assert!(PdfDocument::merge(Vec::new()).is_err());
}

/// **A link between two pages both kept is kept.**
///
/// A page was cloned as a dictionary into one object, and the link's destination through
/// the cloner's map into another no page tree held; removing what names an absent page
/// then deleted the link, from a selection that left nothing out.
#[test]
fn a_link_between_two_kept_pages_is_kept() {
    let bytes = fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Annots [5 0 R] >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
        "<< /Type /Annot /Subtype /Link /Rect [0 0 10 10] /Dest [4 0 R /Fit] >>",
    ]);
    let doc = PdfDocument::open(bytes.into()).expect("the fixture opens");
    let out = doc.extract_pages(vec![0, 1]).expect("it extracts");
    let (arena, inner) = (out.inner().arena(), out.inner());
    let first = fepdf_model::Object::Reference(inner.get_page_handle(0).expect("page 0"));
    let second = inner.get_page_handle(1).expect("page 1");

    let annots = fepdf_model::access::items(arena, &first, "Annots");
    assert_eq!(annots.len(), 1, "the link was dropped");
    // The destination's page as written: `items` would resolve it into the page itself.
    let dest = fepdf_model::access::entry(arena, &annots[0], "Dest").and_then(|d| d.as_array());
    let page = dest.and_then(|d| arena.get_array(d)).and_then(|d| d.first().cloned());
    assert_eq!(
        page,
        Some(fepdf_model::Object::Reference(second)),
        "the link goes to a page outside the tree"
    );
}

/// **A page that is not there is one condition, in the type, saying how many there are.**
///
/// It had six spellings across the engine (ADR-0102), and only one of them said how many
/// pages the document had. Three routes that each spelled it differently — reading a page,
/// extracting it, and setting a measurement scale on it — answer the same variant now.
#[test]
#[ignore = "needs samples/, which the repository does not hold"]
fn a_missing_page_says_how_many_pages_there_are() {
    use fepdf::{Missing, PdfError};
    let Some(mut doc) = sample("fy05.pdf") else {
        panic!("samples/ is not in the tree, so this has nothing to test")
    };
    let count = doc.page_count().expect("it counts");
    let missing = |result: Result<(), PdfError>| match result {
        Err(PdfError::NotFound(Missing::Page { index, count })) => Some((index, count)),
        _ => None,
    };

    assert_eq!(missing(doc.inner().get_page(count).map(|_| ())), Some((count, count)));
    assert_eq!(
        missing(doc.extract_pages(vec![0, count + 3]).map(|_| ())),
        Some((count + 3, count))
    );
    let scale = fepdf::Operation::SetMeasurementScale(fepdf::MeasurementScale {
        page: count + 1,
        scale_ratio: 1.0,
        unit_label: "mm".into(),
    });
    assert_eq!(missing(doc.apply(scale)), Some((count + 1, count)));
}

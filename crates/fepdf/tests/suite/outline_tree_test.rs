//! Reading `/Outlines` back out, and writing what was read.
//!
//! The counts below were re-derived on 2026-09-13 with
//! `cargo run --release --example outline_survey -p fepdf`, and each was checked against
//! `fepdf inspect interactive`, whose reader walks the same tree with a queue instead of
//! recursion and shares no code with this one. Both say 135, 3,853, 23 and 1,319.

use fepdf::PdfDocument;
use fepdf_doc::{Operation, read_outlines};
use fepdf_model::document::extensions::{OutlineNode, OutlineTree};

/// `tree` with no item saying which it was read from: what is compared is what the tree
/// says, and a tree read carries where it came from beside it.
fn without_sources(tree: OutlineTree) -> OutlineTree {
    fn clear(nodes: Vec<OutlineNode>) -> Vec<OutlineNode> {
        nodes
            .into_iter()
            .map(|mut node| {
                node.source = None;
                node.children = clear(std::mem::take(&mut node.children));
                node
            })
            .collect()
    }
    OutlineTree { items: clear(tree.items) }
}

/// A document with no `/Outlines` answers an empty tree, not an error.
#[test]
fn a_document_without_bookmarks_reads_as_empty() {
    let doc = PdfDocument::create_empty().expect("a new document opens");
    let (tree, report) = read_outlines(doc.inner());
    assert!(tree.items.is_empty(), "read {} items from a blank document", tree.items.len());
    assert_eq!(report.items, 0);
    assert!(!report.looped);
}

/// What the operation wrote is what comes back.
#[test]
fn what_was_written_is_what_is_read() {
    let mut doc = PdfDocument::create_empty().expect("a new document opens");
    let written = OutlineTree {
        items: vec![
            OutlineNode {
                title: "第一章".into(),
                destination_page: 0,
                children: vec![OutlineNode {
                    title: "1.1 まえがき".into(),
                    destination_page: 0,
                    children: Vec::new(),
                    source: None,
                }],
                source: None,
            },
            OutlineNode {
                title: "Appendix".into(),
                destination_page: 0,
                children: Vec::new(),
                source: None,
            },
        ],
    };
    doc.apply(Operation::UpdateOutlines(written.clone())).expect("the outline is written");

    let (read, report) = read_outlines(doc.inner());
    assert_eq!(without_sources(read), written);
    assert_eq!(report.items, 3, "two roots and one child");
    assert_eq!(report.placeless, 0);
    assert!(!report.looped);
}

/// The tree survives being written to a file and read back from one.
///
/// The title is deliberately outside PDFDocEncoding. `/Title` was being written as raw
/// UTF-8 bytes in a byte string, which 7.9.2.2 does not admit: an ASCII title round-trips
/// through that bug untouched and would have proved nothing.
#[test]
fn the_tree_survives_a_round_trip_through_a_file() {
    let mut doc = PdfDocument::create_empty().expect("a new document opens");
    let written = OutlineTree {
        items: vec![OutlineNode {
            title: "表紙 — Cover".into(),
            destination_page: 0,
            children: Vec::new(),
            source: None,
        }],
    };
    doc.apply(Operation::UpdateOutlines(written.clone())).expect("the outline is written");

    let dir = std::env::temp_dir().join("fepdf-outline-round-trip");
    std::fs::create_dir_all(&dir).expect("a directory to write into");
    let path = dir.join("bookmarked.pdf");
    doc.save_as_version(&path, "2.0").expect("it writes");

    let bytes = std::fs::read(&path).expect("it is on disk");
    let reopened = PdfDocument::open(bytes.into()).expect("it opens");
    let (read, report) = read_outlines(reopened.inner());
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(without_sources(read), written);
    // Without this the test cannot tell a bookmark that resolved to page 0 from one that
    // resolved to nothing, since `destination_page` falls back to 0 either way.
    assert_eq!(report.placeless, 0);
}

/// A `/Next` that points back at its own chain stops the walk instead of hanging it.
///
/// Nothing in the format forbids the cycle and nothing in the corpus contains one, so
/// this is the only thing that exercises the visited set — without it the bound is a
/// comment.
#[test]
fn a_looping_chain_is_reported_rather_than_followed() {
    let doc = PdfDocument::open(looping_outline()).expect("the document opens");
    let (tree, report) = read_outlines(doc.inner());
    assert_eq!(report.items, 2, "the two distinct items are both read");
    assert!(report.looped, "the cycle was not noticed");
    assert_eq!(tree.items.len(), 2);
}

/// Two outline items whose `/Next` links form a ring: 5 → 6 → 5.
fn looping_outline() -> bytes::Bytes {
    use std::fmt::Write as _;
    const BODIES: [&str; 6] = [
        "<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R >>",
        "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
        "<< /Type /Outlines /First 5 0 R /Last 6 0 R /Count 2 >>",
        "<< /Title (first) /Parent 4 0 R /Next 6 0 R /Dest [3 0 R /Fit] >>",
        "<< /Title (second) /Parent 4 0 R /Prev 5 0 R /Next 5 0 R /Dest [3 0 R /Fit] >>",
    ];
    let mut out = String::from("%PDF-2.0\n");
    let mut offsets = Vec::with_capacity(BODIES.len());
    for (n, body) in BODIES.iter().enumerate() {
        offsets.push(out.len());
        let _ = write!(out, "{} 0 obj\n{body}\nendobj\n", n + 1);
    }
    let start_xref = out.len();
    let _ = write!(out, "xref\n0 {}\n0000000000 65535 f \n", BODIES.len() + 1);
    for offset in &offsets {
        let _ = writeln!(out, "{offset:010} 00000 n ");
    }
    let _ = write!(
        out,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{start_xref}\n%%EOF\n",
        BODIES.len() + 1
    );
    bytes::Bytes::from(out.into_bytes())
}

/// Every bookmark in the corpus names a page, and no chain doubles back.
#[test]
fn the_corpus_reads_the_same_numbers_the_other_reader_reports() {
    // (file, items) — `fepdf inspect interactive` reports the same four, 2026-09-13.
    const EXPECTED: [(&str, usize); 4] = [
        ("fy05.pdf", 135),
        ("intel_sdm.pdf", 3853),
        ("print_sample.pdf", 23),
        ("unicode_16.pdf", 1319),
    ];
    let samples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
    if !samples.is_dir() {
        // `.gitignore` excludes `samples/`, so a fresh clone has none.
        return;
    }
    for (name, items) in EXPECTED {
        let Ok(bytes) = std::fs::read(samples.join(name)) else { continue };
        let doc = PdfDocument::open(bytes.into()).expect("the sample opens");
        let (tree, report) = read_outlines(doc.inner());
        assert_eq!(report.items, items, "{name} reported {} items", report.items);
        assert_eq!(report.placeless, 0, "{name} has a bookmark that names no page");
        assert!(!report.looped, "{name}'s outline doubles back");
        assert!(!tree.items.is_empty(), "{name} read no roots");
    }
}

/// A two-page document whose catalogue's `/Outlines` is `outlines`, with `extra` objects
/// after the pages (numbered from 5).
fn outlined(outlines: &str, extra: &[&str]) -> PdfDocument {
    let mut bodies = vec![
        format!("<< /Type /Catalog /Pages 2 0 R /Outlines {outlines} >>"),
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>".to_string(),
    ];
    bodies.extend(extra.iter().map(|b| (*b).to_string()));
    PdfDocument::open(fepdf_fixtures::assemble(&bodies).into()).expect("the fixture opens")
}

fn titles(doc: &PdfDocument) -> Vec<(String, usize)> {
    let (tree, _) = read_outlines(doc.inner());
    tree.items.iter().map(|i| (i.title.clone(), i.destination_page)).collect()
}

/// **An outline written in place is read, and the repair is said.** Table 29 says
/// `/Outlines` shall be an indirect reference; the reader took a direct one's index in the
/// dictionary pool as an object number, and read whatever object shared it — here, no
/// bookmarks at all (ROADMAP Y-F21).
#[test]
fn an_outline_root_written_in_place_is_read() {
    let doc = outlined(
        "<< /Type /Outlines /First 5 0 R /Last 5 0 R /Count 1 >>",
        &["<< /Title (Two) /Dest [4 0 R /Fit] >>"],
    );
    assert_eq!(titles(&doc), [("Two".to_string(), 1)]);
    assert!(
        doc.decisions().iter().any(|d| d.clause == "7.7.2" && d.found.contains("outline")),
        "the direct outline was repaired in silence: {:?}",
        doc.decisions()
    );
}

/// The items too: a `/First` and a `/Next` written in place are two bookmarks, in order.
#[test]
fn outline_items_written_in_place_are_read_in_order() {
    let doc = outlined(
        "5 0 R",
        &["<< /Type /Outlines /First << /Title (One) /Dest [3 0 R /Fit] \
             /Next << /Title (Two) /Dest [4 0 R /Fit] >> >> >>"],
    );
    assert_eq!(titles(&doc), [("One".to_string(), 0), ("Two".to_string(), 1)]);
}

/// **Retitling one bookmark changes nothing else about any** (ROADMAP Y-F11): a bookmark
/// to a web address keeps its address, its colour and its style, and a closed one with a
/// structure element stays closed and keeps it. The tree is written back whole, from what
/// `OutlineNode` carries, and what it does not model is copied from the item it was read
/// from.
#[test]
fn a_retitled_tree_keeps_what_its_items_carried() {
    let bytes = fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R /StructTreeRoot 8 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> >>",
        "<< /Type /Outlines /First 5 0 R /Last 6 0 R /Count 2 >>",
        "<< /Title (Site) /Parent 4 0 R /Next 6 0 R /C [1 0 0] /F 2 \
           /A << /S /URI /URI (https://example.org/) >> >>",
        "<< /Title (Chapter) /Parent 4 0 R /Prev 5 0 R /Dest [3 0 R /Fit] /SE 9 0 R \
           /First 7 0 R /Last 7 0 R /Count -1 >>",
        "<< /Title (Section) /Parent 6 0 R /Dest [3 0 R /Fit] >>",
        "<< /Type /StructTreeRoot /K [9 0 R] >>",
        "<< /Type /StructElem /S /H1 /P 8 0 R >>",
    ]);
    let mut doc = PdfDocument::open(bytes.into()).expect("the fixture opens");
    let (mut tree, _) = doc.outlines();
    tree.items[1].title = "Chapter One".to_string();
    doc.apply(Operation::UpdateOutlines(tree)).expect("it writes the outline");

    let arena = doc.inner().arena();
    let catalog = doc
        .inner()
        .catalog_handle()
        .and_then(|c| doc.inner().resolve_to_dict(c).ok())
        .expect("a catalogue");
    let dict = |object: &fepdf_model::Object| {
        object.resolve(arena).as_dict_handle().expect("a dictionary")
    };
    let root = dict(&arena.dict_entry(catalog, arena.name("Outlines")).expect("an outline"));
    let entry = |d, key: &str| arena.dict_entry(d, arena.name(key));
    let site = dict(&entry(root, "First").expect("a first item"));
    let chapter = dict(&entry(site, "Next").expect("a second item"));

    let action = dict(&entry(site, "A").expect("the site keeps an action"));
    assert_eq!(entry(action, "S").and_then(|s| s.as_name()), Some(arena.name("URI")));
    assert!(entry(site, "Dest").is_none(), "the site was given a page as well");
    assert!(
        entry(site, "C").is_some() && entry(site, "F").is_some(),
        "the site lost its colour or style"
    );
    assert!(
        entry(chapter, "Count").and_then(|c| c.as_integer()).is_some_and(|c| c < 0),
        "the chapter opened"
    );
    assert!(entry(chapter, "SE").is_some(), "the chapter lost its structure element");
}

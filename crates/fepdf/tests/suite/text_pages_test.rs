//! **A PDF made from plain text** (ROADMAP AA-6, ADR-0124): lines broken where UAX #14
//! allows at the face's advances, pages broken where they fill, and each paragraph a
//! blank line ends tagged `/P`.

use fepdf::{Operation, PdfDocument, PdfError, StructureTreeNode, TextSetting};

fn setting() -> TextSetting {
    TextSetting { lang: Some("ja".to_owned()), ..TextSetting::default() }
}

/// The `/P` elements under the tree's `/Document`, in order.
fn paragraphs(doc: &PdfDocument) -> Vec<StructureTreeNode> {
    let root = doc.extract_struct_tree().expect("a structure tree");
    // The reader presents the tree's root as a node of its own, above the element.
    let document = root
        .children
        .iter()
        .find(|n| n.tag == "Document")
        .expect("a /Document element under the root")
        .clone();
    assert!(document.children.iter().all(|p| p.tag == "P"), "only paragraphs");
    document.children
}

/// **A blank line ends a paragraph and a line break does not**: three paragraphs, each a
/// `/P` in one `/Document`, in the language given, and the text reads back.
#[test]
fn a_paragraph_is_what_a_blank_line_ends() {
    let text = "最初の段落です。\n同じ段落の二行目。\n\nThe second paragraph.\n\n\n\n三つ目。";
    let doc = PdfDocument::from_text(text, setting()).expect("it is set");
    assert_eq!(doc.page_count().expect("pages"), 1);
    let found = paragraphs(&doc);
    assert_eq!(found.len(), 3, "{:?}", found.iter().map(|p| &p.mcids).collect::<Vec<_>>());
    assert!(found.iter().all(|p| p.lang.as_deref() == Some("ja")));
    let inner = doc.inner();
    let arena = inner.arena();
    let catalog =
        inner.resolve_to_dict(inner.catalog_handle().expect("a catalogue")).expect("dict");
    let shows_title = arena
        .dict_entry(catalog, arena.name("ViewerPreferences"))
        .and_then(|v| v.resolve(arena).as_dict_handle())
        .and_then(|v| arena.dict_entry(v, arena.name("DisplayDocTitle")))
        .and_then(|d| d.as_bool());
    assert_eq!(shows_title, Some(true), "PDF/UA-2 07-001: a tagged document shows its title");
    let read = doc.extract_text(0).expect("it reads");
    for words in ["最初の段落です。", "同じ段落の二行目。", "The second paragraph.", "三つ目。"]
    {
        assert!(read.contains(words), "{words:?} is not in {read:?}");
    }
}

/// **No line runs past the margins**, in English broken at spaces and in Japanese
/// between characters, and a word with nowhere to break is broken between its letters.
#[test]
fn lines_stay_inside_the_margins() {
    let english = "A plain text file is set in lines no wider than the page allows. ".repeat(12);
    let japanese = "日本語の文章は文字と文字の間で行を分けることができます。".repeat(8);
    let unbroken = "x".repeat(400);
    let text = format!("{english}\n\n{japanese}\n\n{unbroken}");
    let doc = PdfDocument::from_text(&text, TextSetting::default()).expect("it is set");
    let spans = doc.extract_spans(0).expect("spans");
    let (left, right) = (72.0, 595.0 - 72.0);
    let lines: std::collections::BTreeSet<String> =
        spans.iter().map(|s| format!("{:.0}", s.y)).collect();
    assert!(lines.len() > 10, "the three paragraphs were not broken into lines: {}", lines.len());
    for span in &spans {
        assert!(span.x >= left - 0.5, "{:?} starts at {}", span.text, span.x);
        assert!(
            span.x + span.width <= right + 0.5,
            "{:?} ends at {}",
            span.text,
            span.x + span.width
        );
    }
}

/// **A page holds what fits and the paragraph goes on overleaf**, as one `/P` whose
/// marks are on both pages; a form feed starts a page of its own.
#[test]
fn a_long_paragraph_runs_on_to_the_next_page() {
    let lines: Vec<String> = (1..=60).map(|n| format!("line {n}")).collect();
    let text = format!("{}\u{c}after the form feed", lines.join("\n"));
    let doc = PdfDocument::from_text(&text, TextSetting::default()).expect("it is set");
    assert_eq!(doc.page_count().expect("pages"), 3, "two pages of lines and one after \\f");
    let found = paragraphs(&doc);
    assert_eq!(found.len(), 2);
    let first = found.first().expect("the first paragraph");
    let pages: std::collections::BTreeSet<_> = first.mark_pages.iter().flatten().collect();
    assert_eq!(pages.len(), 2, "one paragraph over two pages: {:?}", first.mark_pages);
    assert!(doc.extract_text(1).expect("page 2").contains("line 60"));
    assert!(doc.extract_text(2).expect("page 3").contains("after the form feed"));
}

/// **Into a tagged document, its paragraphs join its tree**: the parent tree's keys go
/// on from where they were.
#[test]
fn text_goes_into_a_tagged_document_and_its_tree() {
    let mut doc = PdfDocument::from_text("one\n\ntwo", setting()).expect("set");
    let english = TextSetting { lang: Some("en".to_owned()), ..TextSetting::default() };
    doc.apply(Operation::InsertText { text: "three\n\nfour".to_owned(), at: 1, setting: english })
        .expect("it goes in");
    assert_eq!(doc.page_count().expect("pages"), 2);
    let found = paragraphs(&doc);
    assert_eq!(found.len(), 4, "the four paragraphs are in one /Document");
    let langs: Vec<Option<&str>> = found.iter().map(|p| p.lang.as_deref()).collect();
    assert_eq!(langs, [Some("ja"), Some("ja"), Some("en"), Some("en")], "each paragraph its own");
    assert!(doc.extract_text(1).expect("page 2").contains("four"));
    let inner = doc.inner();
    let arena = inner.arena();
    let key = |page: usize| {
        let dict = inner.resolve_to_dict(inner.page_handle(page).expect("page")).expect("dict");
        arena.dict_entry(dict, arena.name("StructParents")).and_then(|k| k.as_integer())
    };
    assert_eq!((key(0), key(1)), (Some(0), Some(1)), "each page its own parent tree key");
}

/// What is not text, or has no room, is refused, and nothing changes.
#[test]
fn what_cannot_be_set_is_refused() {
    let mut doc = PdfDocument::from_text("one", TextSetting::default()).expect("set");
    let cramped = TextSetting { margin: 400.0, ..TextSetting::default() };
    for (text, setting) in [(" \n\t\n", TextSetting::default()), ("words", cramped)] {
        let refused = doc.apply(Operation::InsertText { text: text.to_owned(), at: 0, setting });
        assert!(matches!(refused, Err(PdfError::Refused { .. })), "{refused:?}");
    }
    assert_eq!(doc.page_count().expect("pages"), 1);
    assert!(PdfDocument::plain_text(&[0x93, 0xFA, 0x96, 0x7B]).is_err(), "Shift_JIS is refused");
    let utf16: Vec<u8> =
        [0xFF, 0xFE].into_iter().chain("日本".encode_utf16().flat_map(u16::to_le_bytes)).collect();
    assert_eq!(PdfDocument::plain_text(&utf16).expect("UTF-16 with a mark"), "日本");
}

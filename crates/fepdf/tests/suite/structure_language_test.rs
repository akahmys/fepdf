//! What language an element is in (14.9.2), and what its tag stands for (14.8.4.4).
//!
//! The element properties panel showed a constant `en-US` and a constant
//! `Default Mapping` for every element of every document, which is worse than an empty
//! row: a reader checking a document's language found an answer. Neither entry had a
//! reader anywhere in the workspace.

use fepdf::{IngestionOptions, PdfDocument};
use fepdf_fixtures::assemble;

/// A one-page tagged document. `catalogue` and `root` are merged into the catalogue and
/// the `/StructTreeRoot`; objects 6 and 7 are the two structure elements.
fn tagged(catalogue: &str, root: &str, outer: &str, inner: &str) -> PdfDocument {
    let content = "/P << /MCID 0 >> BDC\n10 20 30 40 re f\nEMC\n";
    let bodies = vec![
        format!("<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R {catalogue} >>"),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_string(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        format!("<< /Type /StructTreeRoot /K [6 0 R] {root} >>"),
        outer.to_string(),
        inner.to_string(),
    ];
    PdfDocument::open_with_options(assemble(&bodies).into(), &IngestionOptions::default())
        .expect("the fixture opens")
}

/// The outer element and the one it holds.
fn pair(doc: &PdfDocument) -> (fepdf::StructureTreeNode, fepdf::StructureTreeNode) {
    let root = doc.extract_struct_tree().expect("the document is tagged");
    let outer = root.children[0].clone();
    let inner = outer.children[0].clone();
    (outer, inner)
}

#[test]
fn an_element_with_no_language_is_in_the_documents() {
    // The catalogue's `/Lang` is the document's, and 14.9.2 makes it the answer for
    // everything that does not override it. `volvo_xc90.pdf` places all 23,416 of its
    // elements this way and `fugaku.pdf` all 126.
    let doc = tagged(
        "/Lang (en-US)",
        "",
        "<< /Type /StructElem /S /Sect /P 5 0 R /Pg 3 0 R /K [7 0 R] >>",
        "<< /Type /StructElem /S /P /P 6 0 R /Pg 3 0 R /K [0] >>",
    );
    let (outer, inner) = pair(&doc);
    assert_eq!(outer.lang.as_deref(), Some("en-US"));
    assert_eq!(inner.lang.as_deref(), Some("en-US"));
}

#[test]
fn an_element_that_declares_a_language_is_in_that_one_and_so_is_what_it_holds() {
    // Inheritance runs down, not up: the section overrides the document and the paragraph
    // inside it takes the section's answer. `print_sample.pdf` is written this way — 1,048
    // elements in `ja`, 147 in `en` and 33 in `zh`, in one file.
    let doc = tagged(
        "/Lang (en-US)",
        "",
        "<< /Type /StructElem /S /Sect /P 5 0 R /Pg 3 0 R /Lang (ja) /K [7 0 R] >>",
        "<< /Type /StructElem /S /P /P 6 0 R /Pg 3 0 R /K [0] >>",
    );
    let (outer, inner) = pair(&doc);
    assert_eq!(outer.lang.as_deref(), Some("ja"));
    assert_eq!(inner.lang.as_deref(), Some("ja"));
}

#[test]
fn the_innermost_declaration_wins() {
    let doc = tagged(
        "/Lang (en-US)",
        "",
        "<< /Type /StructElem /S /Sect /P 5 0 R /Pg 3 0 R /Lang (ja) /K [7 0 R] >>",
        "<< /Type /StructElem /S /P /P 6 0 R /Pg 3 0 R /Lang (zh) /K [0] >>",
    );
    let (outer, inner) = pair(&doc);
    assert_eq!(outer.lang.as_deref(), Some("ja"));
    assert_eq!(inner.lang.as_deref(), Some("zh"));
}

#[test]
fn a_document_that_names_no_language_leaves_the_row_empty() {
    // Absent rather than guessed. The row this feeds used to read `en-US` here.
    let doc = tagged(
        "",
        "",
        "<< /Type /StructElem /S /Sect /P 5 0 R /Pg 3 0 R /K [7 0 R] >>",
        "<< /Type /StructElem /S /P /P 6 0 R /Pg 3 0 R /K [0] >>",
    );
    let (outer, inner) = pair(&doc);
    assert_eq!(outer.lang, None);
    assert_eq!(inner.lang, None);
}

#[test]
fn a_role_map_says_what_a_non_standard_tag_stands_for() {
    // 14.8.4.4. `print_sample.pdf` carries the corpus's only `/RoleMap`, and it maps
    // `/Slide` and `/Textbox` onto `/Sect`.
    let doc = tagged(
        "",
        "/RoleMap << /Slide /Sect >>",
        "<< /Type /StructElem /S /Slide /P 5 0 R /Pg 3 0 R /K [7 0 R] >>",
        "<< /Type /StructElem /S /P /P 6 0 R /Pg 3 0 R /K [0] >>",
    );
    let (outer, inner) = pair(&doc);
    assert_eq!(outer.tag, "Slide");
    assert_eq!(outer.role.as_deref(), Some("Sect"));
    assert_eq!(inner.role, None, "a standard tag needs no mapping");
}

#[test]
fn the_corpus_reads_three_languages_and_two_mappings() {
    // The only file that exercises either entry beyond a single inherited value.
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/print_sample.pdf");
    let bytes = std::fs::read(path).expect("the sample is in the tree");
    let doc = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the sample opens");
    let root = doc.extract_struct_tree().expect("the sample is tagged");
    let mut langs = std::collections::BTreeMap::new();
    let mut roles = std::collections::BTreeMap::new();
    tally(&root, &mut langs, &mut roles);
    assert_eq!(langs.get("ja"), Some(&1048));
    assert_eq!(langs.get("en"), Some(&147));
    assert_eq!(langs.get("zh"), Some(&33));
    assert_eq!(roles.get("Slide").map(String::as_str), Some("Sect"));
    assert_eq!(roles.get("Textbox").map(String::as_str), Some("Sect"));
}

/// How many elements are in each language, and what each mapped tag maps to.
fn tally(
    node: &fepdf::StructureTreeNode,
    langs: &mut std::collections::BTreeMap<String, usize>,
    roles: &mut std::collections::BTreeMap<String, String>,
) {
    if let Some(lang) = &node.lang {
        *langs.entry(lang.clone()).or_default() += 1;
    }
    if let Some(role) = &node.role {
        roles.insert(node.tag.clone(), role.clone());
    }
    for child in &node.children {
        tally(child, langs, roles);
    }
}

//! What the MCP tools do to a document, asserted against the document.
//!
//! **This suite asserted `is_ok()` and nothing else until 2026-09-06**, and it skipped
//! itself in silence when `samples/sample.pdf` was absent — which `.gitignore` makes it
//! on every machine but the one that generated it. So on a fresh clone every test here
//! passed without running, and on this machine they passed without checking: the
//! redaction tool was covered by one `assert!(res.is_ok())` and removed the wrong text
//! for as long as it had existed ([ADR-0064]).
//!
//! Two things follow, and both are the point of the rewrite:
//!
//! * **The fixtures are built here.** Nothing reads `samples/`, so nothing can skip, and
//!   what each test needs is visible in the test.
//! * **Every assertion is about the output document**, opened and read back. A tool that
//!   returned `Ok` having done nothing fails here.
//!
//! [ADR-0064]: ../../../docs/adr/0064-redaction-removed-the-second-run-of-a-page-and-no-other.md

use fepdf_mcp::McpError;
use fepdf_mcp::prompts::{prompt_audit_accessibility, prompt_remediate_pdf_ua};
use fepdf_mcp::resources::{
    read_audit_resource, read_metadata_resource, read_struct_tree_resource,
};
use fepdf_mcp::tools::operations::vocabulary::{DuplicatePagesArgs, duplicate_pages_impl};
use fepdf_mcp::tools::{
    AddAnnotationArgs, AddPageDecorationArgs, ApplyBatesNumberingArgs, AuditArgs, ExtractTextArgs,
    OutlineNodeArg, RedactDocumentArgs, RedactionTarget, RemovePagesArgs, ReorderPagesArgs,
    RotatePagesArgs, SetFormFieldValueArgs, UpdateOutlinesArgs, VerifySignaturesArgs,
    add_annotation_impl, add_page_decoration_impl, apply_bates_numbering_impl,
    apply_redaction_impl, audit_document_impl, extract_text_impl, remove_pages_impl,
    reorder_pages_impl, rotate_pages_impl, set_form_field_value_impl, update_outlines_impl,
    verify_signatures_impl,
};
use std::path::PathBuf;

// --- fixtures -------------------------------------------------------------------------

/// `n` pages, page *i* carrying the text `Pi` at a known place, each 612x792.
fn pages(n: usize) -> Vec<u8> {
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 3 + i * 2)).collect();
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!("<< /Type /Pages /Kids [{}] /Count {n} >>", kids.join(" ")),
    ];
    for i in 0..n {
        let content = format!("BT /F1 24 Tf 72 700 Td (P{i}) Tj ET");
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
              /Resources << /Font << /F1 {} 0 R >> >> /Contents {} 0 R >>",
            3 + n * 2,
            4 + i * 2
        ));
        objects.push(format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()));
    }
    objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string());
    fepdf_fixtures::assemble(&objects)
}

/// A path under the temporary directory, holding `bytes`.
fn written(name: &str, bytes: &[u8]) -> String {
    let mut p: PathBuf = std::env::temp_dir();
    p.push(format!("fepdf_mcp_{name}.pdf"));
    std::fs::write(&p, bytes).expect("the fixture is written");
    p.to_string_lossy().to_string()
}

fn out(name: &str) -> String {
    let mut p: PathBuf = std::env::temp_dir();
    p.push(format!("fepdf_mcp_out_{name}.pdf"));
    let _ = std::fs::remove_file(&p);
    p.to_string_lossy().to_string()
}

/// The text of one page of a document on disk — how these tests see what a tool did.
fn text_of(path: &str, page: usize) -> String {
    extract_text_impl(ExtractTextArgs {
        path: path.to_string(),
        page_range: Some(page.to_string()),
    })
    .expect("the output document reads back")
}

// --- tests ----------------------------------------------------------------------------

#[test]
fn an_io_error_keeps_its_message() {
    let io = std::io::Error::new(std::io::ErrorKind::NotFound, "PDF file missing");
    let mcp: McpError = io.into();
    assert!(format!("{mcp}").contains("PDF file missing"));
}

/// Extraction returns the page's own text, not merely a well-formed report.
#[test]
fn extract_text_returns_what_is_on_the_page() {
    let path = written("extract", &pages(2));
    let json = text_of(&path, 0);
    assert!(json.contains("total_pages"), "the report is shaped as documented: {json}");
    assert!(json.contains("P0"), "and carries page 0's text: {json}");
    assert!(!json.contains("P1"), "and only the page asked for: {json}");
}

/// Removing a page removes that page, and the others keep their order.
///
/// **`pages` counts from 1 and `page_range` counts from 0**, on the same surface, and
/// only the second used to say so. `pages: "2"` is the middle page; `page_range: "2"` is
/// the last. This test names both bases in one place so neither can drift into the other
/// unnoticed — which is the only reason the mixture is safe to keep.
///
/// The version this replaces asserted `is_ok()` on a chain of three operations and
/// re-opened none of them.
#[test]
fn remove_pages_counts_from_one_where_extraction_counts_from_zero() {
    let path = written("remove", &pages(3));
    let dest = out("remove");
    remove_pages_impl(RemovePagesArgs {
        input_path: path,
        output_path: dest.clone(),
        pages: "2".into(),
    })
    .expect("the tool runs");

    // "2" removed the middle page, so P0 and P2 are what is left — and `text_of` reaches
    // them by 0-based index, which is the other convention.
    assert!(text_of(&dest, 0).contains("P0"), "the first page stays");
    assert!(text_of(&dest, 1).contains("P2"), "and the last moves up into the gap");
}

/// Reordering moves the page it names.
#[test]
fn reorder_pages_moves_the_page_named() {
    let path = written("reorder", &pages(3));
    let dest = out("reorder");
    reorder_pages_impl(ReorderPagesArgs {
        input_path: path,
        output_path: dest.clone(),
        from: 2,
        to: 0,
    })
    .expect("the tool runs");

    assert!(text_of(&dest, 0).contains("P2"), "the page from the end is now first");
    assert!(text_of(&dest, 1).contains("P0"), "and the one that was first follows it");
}

/// Rotating writes the rotation into the page.
#[test]
fn rotate_pages_writes_the_rotation() {
    let path = written("rotate", &pages(1));
    let dest = out("rotate");
    rotate_pages_impl(RotatePagesArgs {
        input_path: path,
        output_path: dest.clone(),
        selection: Some("all".into()),
        angle: 90,
        relative: Some(true),
    })
    .expect("the tool runs");

    // Not a byte search: objects are packed into 7.5.7 streams by default (ADR-0016), so
    // the names are inside a compressed container. The engine's own reader is what sees
    // them, and a test that greps the file is testing the writer's compression setting.
    // A 90-degree rotation swaps the page's reported width and height, which is the
    // reader's own answer to "was it rotated" and needs no byte search.
    let doc = fepdf::PdfDocument::open(bytes::Bytes::from(
        std::fs::read(&dest).expect("the output exists"),
    ))
    .expect("the output opens");
    let (w, h) = doc.get_page_size(0).expect("the page has a size");
    assert!(w > h, "612x792 rotated a quarter turn reads as landscape, not {w}x{h}");
}

/// A decoration puts its text on the page.
#[test]
fn a_decoration_reaches_the_page() {
    let path = written("decorate", &pages(1));
    let dest = out("decorate");
    add_page_decoration_impl(AddPageDecorationArgs {
        input_path: path,
        output_path: dest.clone(),
        pages: Some("all".into()),
        text: "CONFIDENTIAL".into(),
        position: "top_center".into(),
        layer: None,
    })
    .expect("the tool runs");

    let text = text_of(&dest, 0);
    assert!(text.contains("CONFIDENTIAL"), "the decoration is on the page: {text}");
    assert!(text.contains("P0"), "and the page's own text is still there");
}

/// Bates numbering puts its number on the page.
#[test]
fn bates_numbering_reaches_the_page() {
    let path = written("bates", &pages(2));
    let dest = out("bates");
    apply_bates_numbering_impl(ApplyBatesNumberingArgs {
        input_path: path,
        output_path: dest.clone(),
        pages: None,
        prefix: Some("TEST-".into()),
        start_number: Some(100),
        digits: Some(6),
        position: Some("bottom_right".into()),
    })
    .expect("the tool runs");

    assert!(text_of(&dest, 0).contains("TEST-000100"), "page 0 carries the first number");
    assert!(text_of(&dest, 1).contains("TEST-000101"), "and page 1 the next");
}

/// An annotation reaches the page's `/Annots`.
#[test]
fn an_annotation_reaches_the_page() {
    let path = written("annot", &pages(1));
    let dest = out("annot");
    add_annotation_impl(AddAnnotationArgs {
        input_path: path,
        output_path: dest.clone(),
        page: 0,
        rect: [100.0, 100.0, 200.0, 150.0],
        contents: "Test Comment".into(),
        kind: Some("text".into()),
    })
    .expect("the tool runs");

    let report =
        fepdf::InteractiveReport::survey(&std::fs::read(&dest).expect("the output exists"))
            .expect("the output surveys");
    assert!(!report.is_empty(), "the document is no longer free of interactive features");
    assert_eq!(report.annotations.total, 1, "one annotation, the one asked for");
    assert_eq!(report.annotations.pages_with, 1, "on the one page asked for");
}

/// An outline reaches the catalogue.
#[test]
fn an_outline_reaches_the_catalogue() {
    let path = written("outline", &pages(2));
    let dest = out("outline");
    update_outlines_impl(UpdateOutlinesArgs {
        input_path: path,
        output_path: dest.clone(),
        roots: vec![OutlineNodeArg {
            title: "Chapter 1".into(),
            dest_page: Some(0),
            children: None,
        }],
    })
    .expect("the tool runs");

    let report =
        fepdf::InteractiveReport::survey(&std::fs::read(&dest).expect("the output exists"))
            .expect("the output surveys");
    assert!(report.outline.present, "the catalogue names an outline");
    assert_eq!(report.outline.total, 1, "carrying the one item asked for");
}

/// **Redaction removes the text under the rectangle, and says how many it removed.**
///
/// The version this replaces was one `assert!(res.is_ok())`, and the tool removed the
/// wrong run of a page for as long as it had existed. The count is the second half: it
/// reported `args.targets.len()` under a field documented "Number of redactions
/// successfully scrubbed", so a rectangle over empty space was reported to the caller as
/// a redaction that had happened.
#[test]
fn redaction_removes_the_text_and_reports_what_it_removed() {
    let path = written("redact", &pages(1));

    let dest = out("redact_hit");
    let report = apply_redaction_impl(RedactDocumentArgs {
        input_path: path.clone(),
        output_path: dest.clone(),
        targets: vec![RedactionTarget { page: 0, rect: [0.0, 0.0, 612.0, 792.0] }],
    })
    .expect("the tool runs");

    assert!(!text_of(&dest, 0).contains("P0"), "the page's text is gone");
    assert!(report.contains("\"redacted_count\": 1"), "and the count is what went: {report}");

    let missed = out("redact_miss");
    let report = apply_redaction_impl(RedactDocumentArgs {
        input_path: path,
        output_path: missed.clone(),
        targets: vec![RedactionTarget { page: 0, rect: [0.0, 0.0, 1.0, 1.0] }],
    })
    .expect("the tool runs");

    assert!(text_of(&missed, 0).contains("P0"), "a rectangle over nothing removes nothing");
    assert!(
        report.contains("\"redacted_count\": 0"),
        "and says so, rather than reporting the rectangle it was given: {report}"
    );
}

/// The read-only tools answer about the document rather than merely succeeding.
#[test]
fn the_reporting_tools_answer_about_the_document() {
    let path = written("report", &pages(2));

    let audit = audit_document_impl(AuditArgs { path: path.clone() }).expect("audit runs");
    assert!(audit.contains("\"status\""), "the report carries a verdict: {audit}");
    assert!(
        audit.contains("Structural Tree Root"),
        "and names what an untagged document is missing: {audit}"
    );

    let signatures = verify_signatures_impl(VerifySignaturesArgs { path: path.clone() })
        .expect("verification runs");
    assert!(
        signatures.to_lowercase().contains("no digital signatures"),
        "an unsigned document says so in words rather than returning an empty report: \
         {signatures}"
    );
    // `list_signatures` could not say this: a field with no `/V` is not a signature, so
    // enumerating signatures never counted one. `SignatureReport::survey` does, which is
    // how this assertion tells the two implementations apart.
    assert!(
        signatures.contains("signature field(s) carry none"),
        "the answer accounts for signature fields carrying no signature: {signatures}"
    );

    assert!(read_metadata_resource(&path).expect("metadata reads").contains('{'));
    assert!(read_audit_resource(&path).expect("the audit resource reads").contains('{'));
    // An untagged document has no structure tree, and the resource says `null` rather
    // than an empty object. Asserted as `null` and not merely "it returned", because
    // "there is no tree" and "I could not read the tree" must not look the same to an
    // agent — and here they would.
    assert_eq!(
        read_struct_tree_resource(&path).expect("the struct tree resource reads").trim(),
        "null",
        "an untagged document has no tree, and says so"
    );
}

/// The prompts name what they are for.
#[test]
fn the_prompts_name_their_subject() {
    let path = written("prompts", &pages(1));
    assert!(prompt_audit_accessibility(&path).contains("PDF/UA-2"));
    assert!(prompt_remediate_pdf_ua(&path, "output.pdf").contains("remediation"));
}

// --- page selections ------------------------------------------------------------------

/// How many pages a document on disk has, read back through the extraction report.
fn page_count(path: &str) -> usize {
    let json = extract_text_impl(ExtractTextArgs { path: path.to_string(), page_range: None })
        .expect("the document reads back");
    let marker = "\"total_pages\":";
    let at = json.find(marker).expect("the report states a page count") + marker.len();
    json[at..]
        .trim_start()
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .unwrap_or("0")
        .parse()
        .expect("the page count is a number")
}

/// A selection string nobody can parse is refused, and does not mean "every page".
///
/// **`remove_pages(pages: "foo")` used to remove the whole document.** Both selection
/// parsers ended their match with `_ => PageSelection::All`, so anything unrecognised —
/// a typo, an empty string, or `"2,3"`, which is the first thing a caller reaches for —
/// selected every page. On `remove_pages` that is the entire file, silently, reported as
/// SUCCESS.
#[test]
fn an_unparsable_selection_is_refused_rather_than_meaning_all() {
    for probe in ["foo", "", "2,3", "0", "-1"] {
        let path = written("sel_bad", &pages(3));
        let dest = out("sel_bad");
        let result = remove_pages_impl(RemovePagesArgs {
            input_path: path,
            output_path: dest.clone(),
            pages: probe.to_string(),
        });
        assert!(result.is_err(), "{probe:?} should be refused, not read as a selection");
        assert!(
            !std::path::Path::new(&dest).exists(),
            "{probe:?} should not have written an output document"
        );
    }
}

/// The two page tools read one selection string the same way.
///
/// `duplicate_pages` and `remove_pages` had a `parse_selection` each, differing in one
/// `unwrap_or`: for `"3-"` the first yielded page 3 and the second yielded nothing, so
/// the same string named a page to one tool and no page to the other.
#[test]
fn both_page_tools_read_one_selection_the_same_way() {
    for probe in ["3-", "5-x", "1-"] {
        let dup_path = written("sel_dup", &pages(3));
        let dup_dest = out("sel_dup");
        let dup = duplicate_pages_impl(DuplicatePagesArgs {
            input_path: dup_path,
            output_path: dup_dest,
            pages: probe.to_string(),
        });

        let rm_path = written("sel_rm", &pages(3));
        let rm_dest = out("sel_rm");
        let rm = remove_pages_impl(RemovePagesArgs {
            input_path: rm_path,
            output_path: rm_dest,
            pages: probe.to_string(),
        });

        assert_eq!(
            dup.is_err(),
            rm.is_err(),
            "{probe:?}: duplicate_pages and remove_pages disagree about whether it parses"
        );
    }
}

/// The forms the schema documents still work, and name the pages it says they name.
#[test]
fn the_documented_selection_forms_still_name_their_pages() {
    let path = written("sel_ok", &pages(3));
    let dest = out("sel_ok");
    remove_pages_impl(RemovePagesArgs {
        input_path: path.clone(),
        output_path: dest.clone(),
        pages: "1-2".into(),
    })
    .expect("a documented range parses");
    assert_eq!(page_count(&dest), 1, "1-2 removed two of three pages");
    assert!(text_of(&dest, 0).contains("P2"), "and the one left is the third");

    let single = out("sel_ok_single");
    remove_pages_impl(RemovePagesArgs {
        input_path: path.clone(),
        output_path: single.clone(),
        pages: "2".into(),
    })
    .expect("a documented single page parses");
    assert_eq!(page_count(&single), 2, "2 removed one of three pages");

    let all = out("sel_ok_all");
    remove_pages_impl(RemovePagesArgs {
        input_path: path,
        output_path: all.clone(),
        pages: "all".into(),
    })
    .expect("\"all\" parses");
    assert_eq!(page_count(&all), 0, "all removed every page, because it was asked to");
}

/// `apply_bates_numbering`'s `pages` field is the selection its schema says it is.
///
/// **It used to be read by nothing.** `apply_bates_numbering_impl` opened with
/// `let pages = PageSelection::All;` and never looked at `args.pages`, while the schema
/// told every caller "Selection of pages, **counting from 1**: \"all\", \"1-5\".
/// Default: \"all\"." Asking for a range stamped the whole document.
#[test]
fn bates_numbering_stamps_the_pages_it_was_given() {
    let path = written("bates_sel", &pages(3));
    let dest = out("bates_sel");
    apply_bates_numbering_impl(ApplyBatesNumberingArgs {
        input_path: path,
        output_path: dest.clone(),
        pages: Some("1".into()),
        prefix: Some("B-".into()),
        start_number: Some(1),
        digits: Some(3),
        position: Some("bottom_right".into()),
    })
    .expect("the tool runs");

    assert!(text_of(&dest, 0).contains("B-"), "page 1 was asked for and is stamped");
    assert!(!text_of(&dest, 1).contains("B-"), "page 2 was not asked for");
    assert!(!text_of(&dest, 2).contains("B-"), "nor page 3");
}

/// Page decoration reads the same selection as every other page tool.
///
/// It had a third inline copy of the parser, ending in the same `_ => PageSelection::All`.
#[test]
fn page_decoration_refuses_an_unparsable_selection() {
    let path = written("deco_sel", &pages(3));
    let dest = out("deco_sel");
    let result = add_page_decoration_impl(AddPageDecorationArgs {
        input_path: path,
        output_path: dest.clone(),
        pages: Some("2,3".into()),
        text: "X".into(),
        position: "bottom_right".into(),
        layer: None,
    });
    assert!(result.is_err(), "\"2,3\" should be refused, not read as every page");
    assert!(!std::path::Path::new(&dest).exists(), "and should write no document");
}

// --- what the server tells a client it has -------------------------------------------

/// **A capability this server does not declare is one no client asks for.**
///
/// `prompts` and `resources` were `None` in `ServerCapabilities`, and `serde` drops a
/// `None` capability from the response, so `initialize` answered with a capability object
/// naming tools alone. Two prompt templates and three resource readers existed, were
/// tested, and were unreachable: a conforming client stopped at this response and never
/// sent `prompts/list` or `resources/list`. Nothing here went through the protocol, so
/// nothing noticed — every other test in this file calls the `_impl` functions directly.
#[test]
fn the_server_declares_every_capability_it_serves() {
    use rmcp::handler::server::ServerHandler as _;
    let capabilities = fepdf_mcp::FepdfServer.get_info().capabilities;
    assert!(capabilities.tools.is_some(), "tools");
    assert!(capabilities.prompts.is_some(), "prompts");
    assert!(capabilities.resources.is_some(), "resources");
}

// --- the form's calculations, which the server now runs -------------------------------

/// A form whose `total` is computed from `a` and `b`, with `/CO` naming the order.
fn calculating_form() -> Vec<u8> {
    let script = "event.value = Number\\(this.getField\\('a'\\).value\\) + \
                  Number\\(this.getField\\('b'\\).value\\);";
    fepdf_fixtures::acroform(
        &[
            fepdf_fixtures::FormField::new("a", "2"),
            fepdf_fixtures::FormField::new("b", "3"),
            fepdf_fixtures::FormField::new("total", "0").calculating(script),
        ],
        &[2],
    )
}

/// **Setting a field runs the form's calculation order.**
///
/// It did not. `fepdf-script` executed ECMAScript and had done since Phase R, and nothing
/// called `run_calculations` but its own tests, so every write through this server left
/// the computed fields holding whatever the file had been saved with and recorded a
/// `Violation` of 12.6.3 saying so. Here `total` is `a + b`: writing 10 into `a` makes it
/// 13, and left uncalculated it stays 0.
#[test]
fn setting_a_field_recalculates_what_is_computed_from_it() {
    let input = written("calc_in", &calculating_form());
    let output = out("calc_out");

    set_form_field_value_impl(SetFormFieldValueArgs {
        input_path: input,
        output_path: output.clone(),
        field_name: "a".into(),
        value_text: Some("10".into()),
        value_bool: None,
    })
    .expect("the field is set");

    let saved = fepdf::PdfDocument::open(std::fs::read(&output).expect("written").into())
        .expect("the output opens");
    assert_eq!(
        fepdf::field_value(saved.inner(), "a").as_deref(),
        Some("10"),
        "the value that was written"
    );
    assert_eq!(
        fepdf::field_value(saved.inner(), "total").as_deref(),
        Some("13"),
        "10 + 3, which only a calculation run produces"
    );
}

/// The text editing tool changes the page it is pointed at, through a file.
#[test]
fn edit_text_run_replaces_what_it_was_asked_to() {
    let content = "BT /F1 24 Tf 1 0 0 1 40 700 Tm (ORIGINAL) Tj ET";
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
         /Resources << /Font << /F1 5 0 R >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let input = std::env::temp_dir().join("fepdf_mcp_edit_in.pdf");
    let output = std::env::temp_dir().join("fepdf_mcp_edit_out.pdf");
    std::fs::write(&input, fepdf_fixtures::assemble(&bodies)).expect("the fixture writes");

    let said = fepdf_mcp::tools::edit_text_run_impl(fepdf_mcp::tools::EditTextRunArgs {
        input_path: input.to_string_lossy().to_string(),
        output_path: output.to_string_lossy().to_string(),
        page: 0,
        find: "ORIGINAL".to_string(),
        replace: "CHANGED".to_string(),
    })
    .expect("the tool runs");
    assert!(said.contains("replaced") || said.contains("Text run"), "it said: {said}");

    let written = std::fs::read(&output).expect("the output is there");
    let doc = fepdf::PdfDocument::open(written.into()).expect("it opens");
    let text = doc.extract_text(0).expect("it extracts");
    assert!(text.contains("CHANGED"), "the tool did not change the page: {text:?}");

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&output);
}

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
use fepdf_mcp::tools::{
    SetCalculationOrderArgs, SetTabOrderArgs, set_calculation_order_impl, set_tab_order_impl,
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
        ..annotation_defaults()
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
        fill: None,
    })
    .expect("the tool runs");

    assert!(!text_of(&dest, 0).contains("P0"), "the page's text is gone");
    // Glyphs, since redaction removes glyphs: the page draws `P0`, two of them.
    assert!(report.contains("\"redacted_count\": 2"), "and the count is what went: {report}");

    let missed = out("redact_miss");
    let report = apply_redaction_impl(RedactDocumentArgs {
        input_path: path,
        output_path: missed.clone(),
        targets: vec![RedactionTarget { page: 0, rect: [0.0, 0.0, 1.0, 1.0] }],
        fill: None,
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
    // **The key, not a paraphrase of it.** This read "Structural Tree Root", which is
    // not what the catalogue calls the entry and not what a reader would search the
    // standard for. W-21b renamed the finding and this went with it — and the finding is
    // no longer filed under `00-001`, a failure-condition number the Matterhorn Protocol
    // does not have, but under the clause PDF/UA-1 states the requirement in.
    assert!(
        audit.contains("/StructTreeRoot"),
        "and names what an untagged document is missing: {audit}"
    );
    assert!(
        audit.contains("[UA1:7.1]"),
        "under the clause that requires it, rather than an invented number: {audit}"
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
fn edit_run_replaces_the_run_it_was_asked_for() {
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
    let input = written("edit_run", &fepdf_fixtures::assemble(&bodies));
    let output = out("edit_run");

    let said = fepdf_mcp::tools::edit_run_impl(fepdf_mcp::tools::EditRunArgs {
        input_path: input,
        output_path: output.clone(),
        page: 0,
        run: 0,
        text: "CHANGED".to_string(),
    })
    .expect("the tool runs");
    assert!(said.contains("replaced") || said.contains("Run"), "it said: {said}");

    let bytes = std::fs::read(&output).expect("the output is there");
    let doc = fepdf::PdfDocument::open(bytes.into()).expect("it opens");
    let text = doc.extract_text(0).expect("it extracts");
    assert!(text.contains("CHANGED"), "the tool did not change the page: {text:?}");
}

/// A page drawing two runs, for the tools that name one of them.
fn two_run_page() -> Vec<u8> {
    let content = "BT /F1 24 Tf 1 0 0 1 40 700 Tm (ALPHA) Tj (BETA) Tj ET";
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
         /Resources << /Font << /F1 5 0 R >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    fepdf_fixtures::assemble(&bodies)
}

/// **The number the listing gives is the number the edits take.**
///
/// The three run tools all name a run by an index into a content stream the caller cannot
/// see, and for a day nothing served that listing — so the only way to get a number was to
/// guess one. This asks for the listing and then uses what it said.
#[test]
fn list_runs_gives_the_number_the_other_run_tools_take() {
    let input = written("list_runs", &two_run_page());

    let said = fepdf_mcp::tools::list_runs_impl(fepdf_mcp::tools::ListRunsArgs {
        path: input.clone(),
        page: 0,
    })
    .expect("the tool runs");
    let report: serde_json::Value = serde_json::from_str(&said).expect("it is JSON");
    let runs = report["runs"].as_array().expect("it lists runs");
    assert_eq!(runs.len(), 2, "the listing does not read the page: {said}");
    assert_eq!(runs[1]["text"], "BETA", "the listing is in a different order: {said}");

    let named = usize::try_from(runs[1]["run"].as_u64().expect("a run carries its number"))
        .expect("a run number fits a usize");
    let output = out("list_runs");
    fepdf_mcp::tools::edit_run_impl(fepdf_mcp::tools::EditRunArgs {
        input_path: input,
        output_path: output.clone(),
        page: 0,
        run: named,
        text: "OMEGA".to_string(),
    })
    .expect("the tool runs");

    let bytes = std::fs::read(&output).expect("the output is there");
    let text = fepdf::PdfDocument::open(bytes.into())
        .expect("it opens")
        .extract_text(0)
        .expect("it extracts");
    assert!(text.contains("OMEGA"), "the number the listing gave named another run: {text:?}");
    assert!(text.contains("ALPHA"), "the run that was not named changed: {text:?}");
}

/// Deleting through the server takes the run off the page and leaves the other one.
#[test]
fn delete_run_takes_the_named_run_off_the_page() {
    let input = written("delete_run", &two_run_page());
    let output = out("delete_run");

    fepdf_mcp::tools::delete_run_impl(fepdf_mcp::tools::DeleteRunArgs {
        input_path: input,
        output_path: output.clone(),
        page: 0,
        run: 0,
    })
    .expect("the tool runs");

    let bytes = std::fs::read(&output).expect("the output is there");
    let doc = fepdf::PdfDocument::open(bytes.into()).expect("it opens");
    let text = doc.extract_text(0).expect("it extracts");
    assert!(!text.contains("ALPHA"), "the deleted run is still drawn: {text:?}");
    assert!(text.contains("BETA"), "the run that was not named went too: {text:?}");
    assert_eq!(
        fepdf::text::runs_of_page(doc.inner(), 0).expect("it lists").len(),
        1,
        "the deleted run is still in the listing"
    );
}

/// Splitting through the server leaves the page drawing the same and lists two runs.
#[test]
fn split_run_leaves_the_page_drawing_what_it_drew() {
    let input = written("split_run", &two_run_page());
    let output = out("split_run");

    fepdf_mcp::tools::split_run_impl(fepdf_mcp::tools::SplitRunArgs {
        input_path: input,
        output_path: output.clone(),
        page: 0,
        run: 0,
        after: 2,
    })
    .expect("the tool runs");

    let bytes = std::fs::read(&output).expect("the output is there");
    let doc = fepdf::PdfDocument::open(bytes.into()).expect("it opens");
    let listed = fepdf::text::runs_of_page(doc.inner(), 0).expect("it lists");
    assert_eq!(
        listed.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(),
        vec!["AL", "PHA", "BETA"],
        "the cut did not fall where it was asked for"
    );
    let text = doc.extract_text(0).expect("it extracts");
    assert!(text.contains("ALPHA"), "the page stopped reading as it did: {text:?}");
}

/// Joining through the server makes two runs one, and names what stands in the way.
#[test]
fn merge_runs_joins_two_and_refuses_across_an_operator() {
    let input = written("merge_runs", &two_run_page());
    let output = out("merge_runs");

    fepdf_mcp::tools::merge_runs_impl(fepdf_mcp::tools::MergeRunsArgs {
        input_path: input,
        output_path: output.clone(),
        page: 0,
        run: 0,
    })
    .expect("the tool runs");

    let bytes = std::fs::read(&output).expect("the output is there");
    let doc = fepdf::PdfDocument::open(bytes.into()).expect("it opens");
    let listed = fepdf::text::runs_of_page(doc.inner(), 0).expect("it lists");
    assert_eq!(
        listed.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(),
        vec!["ALPHABETA"],
        "the two runs did not become one"
    );

    let content = "BT /F1 24 Tf 1 0 0 1 40 700 Tm (ALPHA) Tj 100 0 Td (BETA) Tj ET";
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
         /Resources << /Font << /F1 5 0 R >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let apart = written("merge_runs_apart", &fepdf_fixtures::assemble(&bodies));
    let said = fepdf_mcp::tools::merge_runs_impl(fepdf_mcp::tools::MergeRunsArgs {
        input_path: apart,
        output_path: out("merge_runs_apart"),
        page: 0,
        run: 0,
    })
    .expect_err("it refuses")
    .to_string();
    assert!(said.contains("Td"), "the refusal does not name what is in the way: {said}");
}

/// Moving through the server puts the run where it was asked for and leaves the other.
#[test]
fn move_run_puts_the_named_run_where_it_was_asked_for() {
    let input = written("move_run", &two_run_page());
    let output = out("move_run");
    // Where the second run was before anything moved. It is *not* the start of the line:
    // it draws from where the first one ended, which is the whole reason a move has to
    // put back what the run it took away had advanced.
    let before = fepdf::text::runs_of_page(
        fepdf::PdfDocument::open(two_run_page().into()).expect("it opens").inner(),
        0,
    )
    .expect("it lists")[1]
        .origin;

    fepdf_mcp::tools::move_run_impl(fepdf_mcp::tools::MoveRunArgs {
        input_path: input,
        output_path: output.clone(),
        page: 0,
        run: 0,
        x: 300.0,
        y: 120.0,
    })
    .expect("the tool runs");

    let bytes = std::fs::read(&output).expect("the output is there");
    let doc = fepdf::PdfDocument::open(bytes.into()).expect("it opens");
    let listed = fepdf::text::runs_of_page(doc.inner(), 0).expect("it lists");
    assert_eq!(
        listed.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(),
        vec!["ALPHA", "BETA"],
        "the move changed what the page reads or how its runs are numbered"
    );
    assert!(
        (listed[0].origin.0 - 300.0).abs() < 0.1 && (listed[0].origin.1 - 120.0).abs() < 0.1,
        "the run was put at (300, 120) and reads as {:?}",
        listed[0].origin
    );
    assert!(
        (listed[1].origin.0 - before.0).abs() < 0.1 && (listed[1].origin.1 - before.1).abs() < 0.1,
        "the run that was not named moved from {before:?} to {:?}",
        listed[1].origin
    );
}

/// A form of two fields, `subtotal` and `total`, each recalculated by a script, with
/// `/CO` naming them in that order.
fn two_calculating_fields() -> Vec<u8> {
    fepdf_fixtures::acroform(
        &[
            fepdf_fixtures::FormField::new("subtotal", "0").calculating("event.value = 1;"),
            fepdf_fixtures::FormField::new("total", "0").calculating("event.value = 2;"),
        ],
        &[0, 1],
    )
}

fn opened(path: &str) -> fepdf::PdfDocument {
    fepdf::PdfDocument::open(bytes::Bytes::from(std::fs::read(path).expect("the output exists")))
        .expect("the output opens")
}

/// The order given is the order the output recalculates in.
#[test]
fn set_calculation_order_writes_the_order_given() {
    let path = written("calculation", &two_calculating_fields());
    let dest = out("calculation");
    set_calculation_order_impl(SetCalculationOrderArgs {
        input_path: path,
        output_path: dest.clone(),
        fields: vec!["total".into(), "subtotal".into()],
    })
    .expect("the tool runs");
    assert_eq!(fepdf::form_of(opened(&dest).inner()).calculation_order, ["total", "subtotal"]);
}

/// An order leaving out a field that calculates is refused, and nothing is written.
#[test]
fn set_calculation_order_refuses_an_order_that_leaves_one_out() {
    let path = written("calculation_short", &two_calculating_fields());
    let dest = out("calculation_short");
    let refused = set_calculation_order_impl(SetCalculationOrderArgs {
        input_path: path,
        output_path: dest.clone(),
        fields: vec!["total".into()],
    })
    .expect_err("an order without subtotal is refused")
    .to_string();
    assert!(refused.contains("subtotal"), "the refusal does not name the field: {refused}");
    assert!(!std::path::Path::new(&dest).exists(), "a refused order wrote a file");
}

/// `/Tabs` reaches the page named — the second, since the selection counts from 1.
#[test]
fn set_tab_order_writes_the_order_on_the_page() {
    let path = written("tabs", &pages(2));
    let dest = out("tabs");
    set_tab_order_impl(SetTabOrderArgs {
        input_path: path,
        output_path: dest.clone(),
        pages: Some("2".into()),
        order: "structure".into(),
    })
    .expect("the tool runs");
    let doc = opened(&dest);
    let tabs = |page: usize| {
        let inner = doc.inner();
        let arena = inner.arena();
        let dict = inner.resolve_to_dict(inner.get_page(page).expect("a page").obj_handle());
        arena
            .dict_entry(dict.expect("a dictionary"), arena.name("Tabs"))
            .and_then(|tabs| tabs.as_name())
            .and_then(|name| arena.get_name_str(name))
    };
    assert_eq!(tabs(1).as_deref(), Some("S"));
    assert_eq!(tabs(0), None, "a page not named was given an order");
}

/// Every optional field of `AddAnnotationArgs` left out, as a caller who names only the
/// kind and its rectangle does.
fn annotation_defaults() -> AddAnnotationArgs {
    AddAnnotationArgs {
        input_path: String::new(),
        output_path: String::new(),
        page: 0,
        rect: [0.0; 4],
        contents: String::new(),
        kind: None,
        color: None,
        font_size: None,
        points_at: None,
        strokes: None,
        from: None,
        to: None,
        width: None,
        url: None,
        destination_page: None,
        stamp_path: None,
        vertices: None,
        paragraph: None,
        file_path: None,
        mime_type: None,
        parent: None,
        open: None,
        mark: None,
        opacity: None,
        interior_color: None,
    }
}

/// **A kind this tool does not know is refused.** It was read as a note, so `underline`
/// wrote a sticky note and said it had added an annotation.
#[test]
fn an_unknown_annotation_kind_is_refused() {
    let path = written("annot_unknown", &pages(1));
    let dest = out("annot_unknown");
    let refused = add_annotation_impl(AddAnnotationArgs {
        input_path: path,
        output_path: dest.clone(),
        rect: [100.0, 100.0, 200.0, 150.0],
        kind: Some("sticker".into()),
        ..annotation_defaults()
    })
    .expect_err("an unknown kind is refused")
    .to_string();
    assert!(refused.contains("sticker"), "the refusal does not name the kind: {refused}");
    assert!(!std::path::Path::new(&dest).exists(), "a refused annotation wrote a file");
}

/// Each kind the schema names reaches the page as the subtype it is.
#[test]
fn every_kind_the_tool_names_reaches_the_page() {
    let attached = written("annot_attached_file", b"Hello");
    let points = || Some(vec![[110.0, 110.0], [190.0, 110.0], [150.0, 140.0]]);
    let worded = || AddAnnotationArgs { contents: "DRAFT".into(), ..annotation_defaults() };
    let cases: [(&str, &str, AddAnnotationArgs); 14] = [
        ("underline", "Underline", annotation_defaults()),
        ("squiggly", "Squiggly", annotation_defaults()),
        ("rectangle", "Square", annotation_defaults()),
        (
            "line",
            "Line",
            AddAnnotationArgs {
                from: Some([100.0, 100.0]),
                to: Some([200.0, 150.0]),
                ..annotation_defaults()
            },
        ),
        (
            "ink",
            "Ink",
            AddAnnotationArgs {
                strokes: Some(vec![vec![[110.0, 110.0], [190.0, 140.0]]]),
                ..annotation_defaults()
            },
        ),
        ("polygon", "Polygon", AddAnnotationArgs { vertices: points(), ..annotation_defaults() }),
        ("polyline", "PolyLine", AddAnnotationArgs { vertices: points(), ..annotation_defaults() }),
        ("caret", "Caret", worded()),
        (
            "file_attachment",
            "FileAttachment",
            AddAnnotationArgs { file_path: Some(attached), ..annotation_defaults() },
        ),
        ("screen", "Screen", annotation_defaults()),
        ("printer_mark", "PrinterMark", annotation_defaults()),
        ("watermark", "Watermark", worded()),
        ("redact", "Redact", worded()),
        ("projection", "Projection", worded()),
    ];
    for (kind, subtype, args) in cases {
        let path = written(&format!("annot_{kind}"), &pages(1));
        let dest = out(&format!("annot_{kind}"));
        add_annotation_impl(AddAnnotationArgs {
            input_path: path,
            output_path: dest.clone(),
            rect: [100.0, 100.0, 200.0, 150.0],
            kind: Some(kind.into()),
            ..args
        })
        .unwrap_or_else(|e| panic!("{kind}: {e}"));
        let report =
            fepdf::InteractiveReport::survey(&std::fs::read(&dest).expect("the output exists"))
                .expect("the output surveys");
        let said = format!("{:?}", report.annotations);
        assert!(said.contains(subtype), "{kind} did not reach the page as /{subtype}: {said}");
    }
}

/// A one-page document drawing a 2 by 2 grey image 40 by 20 points at (20, 30).
fn page_with_an_image() -> Vec<u8> {
    let content = "q 40 0 0 20 20 30 cm /Im0 Do Q";
    let mut image = b"<< /Type /XObject /Subtype /Image /Width 2 /Height 2 \
                      /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 4 >>\nstream\n"
        .to_vec();
    image.extend_from_slice(&[0, 64, 128, 255]);
    image.extend_from_slice(b"\nendstream");
    fepdf_fixtures::assemble(&[
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R \
          /Resources << /XObject << /Im0 5 0 R >> >> >>"
            .to_vec(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()).into_bytes(),
        image,
    ])
}

/// **An object moved by number is where the listing then says it is.**
#[test]
fn edit_object_moves_what_list_objects_names() {
    use fepdf_mcp::tools::{EditObjectArgs, ListObjectsArgs, edit_object_impl, list_objects_impl};
    let path = written("objects", &page_with_an_image());
    let listed =
        list_objects_impl(ListObjectsArgs { path: path.clone(), page: 0 }).expect("it lists");
    assert!(listed.contains("\"object\": 0") && listed.contains("image"), "{listed}");

    let dest = out("objects");
    edit_object_impl(EditObjectArgs {
        input_path: path,
        output_path: dest.clone(),
        page: 0,
        object: 0,
        move_to: Some((100.0, 120.0)),
        scale: None,
        rotate_degrees: None,
        replace_with: None,
    })
    .expect("the tool runs");
    let moved = list_objects_impl(ListObjectsArgs { path: dest, page: 0 }).expect("it lists");
    let value: serde_json::Value = serde_json::from_str(&moved).expect("json");
    let corner = &value[0]["corners"][0];
    assert!(
        (corner[0].as_f64().unwrap_or(0.0) - 100.0).abs() < 0.01
            && (corner[1].as_f64().unwrap_or(0.0) - 120.0).abs() < 0.01,
        "the object is at {corner} and was put at (100, 120)"
    );
}

/// Two edits at once is refused rather than one of them done.
#[test]
fn edit_object_refuses_two_edits_at_once() {
    use fepdf_mcp::tools::{EditObjectArgs, edit_object_impl};
    let path = written("objects_two", &page_with_an_image());
    let refused = edit_object_impl(EditObjectArgs {
        input_path: path,
        output_path: out("objects_two"),
        page: 0,
        object: 0,
        move_to: Some((1.0, 1.0)),
        scale: Some(2.0),
        rotate_degrees: None,
        replace_with: None,
    })
    .expect_err("refused")
    .to_string();
    assert!(refused.contains("exactly one"), "{refused}");
}

/// **The window an OCR engine is given, and what it hands back**: the page goes out as a
/// picture with the transform back, a box comes back in the picture's pixels, and the word
/// is then in the page's text.
#[test]
fn a_word_read_off_the_picture_is_found_in_the_page() {
    use fepdf_mcp::tools::text_layer::{
        AddTextLayerArgs, PageForOcrArgs, ReadText, add_text_layer_impl, page_for_ocr_impl,
    };
    let path = written("ocr", &page_with_an_image());
    let image = std::env::temp_dir().join(format!("fepdf_ocr_{}.png", std::process::id()));
    let answer = page_for_ocr_impl(PageForOcrArgs {
        path: path.clone(),
        page: 0,
        image_path: image.display().to_string(),
        dpi: Some(144.0),
    })
    .expect("the page goes out");
    let value: serde_json::Value = serde_json::from_str(&answer).expect("json");
    assert!(std::fs::metadata(&image).map_or(0, |m| m.len()) > 0, "no picture was written");
    let _ = std::fs::remove_file(&image);
    let to: Vec<f64> = value["pixel_to_page"]
        .as_array()
        .expect("a transform")
        .iter()
        .filter_map(serde_json::Value::as_f64)
        .collect();
    let pixel_to_page: [f64; 6] = to.try_into().expect("six numbers");

    let dest = out("ocr");
    add_text_layer_impl(AddTextLayerArgs {
        input_path: path,
        output_path: dest.clone(),
        page: 0,
        items: vec![ReadText { text: "Scanned".into(), rect: [40.0, 40.0, 200.0, 70.0] }],
        pixel_to_page: Some(pixel_to_page),
    })
    .expect("the layer is laid");
    let bytes = std::fs::read(&dest).expect("it was written");
    let text =
        fepdf::PdfDocument::open(bytes.into()).expect("it opens").extract_text(0).expect("text");
    assert!(text.contains("Scanned"), "{text:?}");
}

/// **A document is the same as itself, and not as a blank page of the same size**: the
/// picture on it is where the two look different.
#[test]
fn compare_documents_finds_where_two_differ() {
    use fepdf_mcp::tools::compare::{CompareArgs, compare_documents_impl};
    let pictured = written("compare_a", &page_with_an_image());
    let answer = compare_documents_impl(CompareArgs {
        path_a: pictured.clone(),
        path_b: pictured.clone(),
        dpi: None,
    })
    .expect("it compares");
    let value: serde_json::Value = serde_json::from_str(&answer).expect("json");
    assert_eq!(value["differences"].as_array().map(Vec::len), Some(0), "{answer}");

    let blank = fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
    ]);
    let blank = written("compare_b", &blank);
    let answer = compare_documents_impl(CompareArgs { path_a: pictured, path_b: blank, dpi: None })
        .expect("it compares");
    let value: serde_json::Value = serde_json::from_str(&answer).expect("json");
    let regions = value["differences"][0]["regions"].as_array().map_or(0, Vec::len);
    assert!(regions > 0, "the picture was not found to differ: {answer}");
}

/// **A file that opened only by being repaired is not reported as read as written.**
/// The audit said "XRef chain and trailer resolved successfully" of every file it could
/// open, and a file with a wrong `startxref` opens: the reader scans for its objects.
#[test]
fn a_repaired_file_structure_is_reported_as_repaired() {
    let clean = written("audit_clean", &pages(1));
    let report = audit_document_impl(AuditArgs { path: clean }).expect("audit runs");
    assert!(report.contains("nothing was repaired"), "{report}");

    let mut broken = pages(1);
    let at = broken.windows(9).rposition(|w| w == b"startxref").expect("a startxref");
    let tail = String::from_utf8_lossy(&broken[at..]).replace(|c: char| c.is_ascii_digit(), "9");
    broken.truncate(at);
    broken.extend_from_slice(tail.as_bytes());
    let path = written("audit_repaired", &broken);
    let report = audit_document_impl(AuditArgs { path }).expect("audit runs");
    assert!(!report.contains("nothing was repaired"), "a repair was called none: {report}");
    assert!(report.contains("\"Warning\"") && report.contains("7.5"), "{report}");
}

/// **`resize_pages` puts the page on the sheet asked for** (ROADMAP Y-F27): the operation
/// was reachable only through `apply_operation`, with no schema saying it existed.
#[test]
fn resize_pages_puts_the_page_on_the_sheet() {
    use fepdf_mcp::tools::operations::page::{ResizePagesArgs, resize_pages_impl};
    let dest = out("resize");
    resize_pages_impl(ResizePagesArgs {
        input_path: written("resize", &pages(1)),
        output_path: dest.clone(),
        pages: None,
        sheet_width: Some(300.0),
        sheet_height: Some(400.0),
        scale: Some("fit".into()),
        offset_right: None,
        offset_up: None,
    })
    .expect("the tool runs");
    let doc =
        fepdf::PdfDocument::open(std::fs::read(&dest).expect("written").into()).expect("it opens");
    assert_eq!(doc.get_page_size(0).expect("a size"), (300.0, 400.0));
}

/// **`remove_outside` takes the text outside the rectangle off the page.**
#[test]
fn remove_outside_takes_the_text_outside() {
    use fepdf_mcp::tools::operations::page::{RemoveOutsideArgs, remove_outside_impl};
    let dest = out("outside");
    remove_outside_impl(RemoveOutsideArgs {
        input_path: written("outside", &pages(1)),
        output_path: dest.clone(),
        page: 0,
        left: 0.0,
        bottom: 0.0,
        right: 50.0,
        top: 50.0,
    })
    .expect("the tool runs");
    assert!(!text_of(&dest, 0).contains("P0"), "the text outside the rectangle stayed");
}

/// **`apply_redact_annotations` applies the document's own `/Redact` annotations.**
#[test]
fn apply_redact_annotations_removes_what_they_mark() {
    use fepdf_mcp::tools::redact::{ApplyRedactAnnotationsArgs, apply_redact_annotations_impl};
    let content = "BT /F1 24 Tf 72 700 Td (P0) Tj ET";
    let bytes = fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
           /Resources << /Font << /F1 5 0 R >> >> /Annots [6 0 R] >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        "<< /Type /Annot /Subtype /Redact /Rect [60 690 200 730] /IC [0 0 0] >>".to_string(),
    ]);
    let dest = out("redact_annots");
    apply_redact_annotations_impl(ApplyRedactAnnotationsArgs {
        input_path: written("redact_annots", &bytes),
        output_path: dest.clone(),
        pages: None,
    })
    .expect("the tool runs");
    assert!(!text_of(&dest, 0).contains("P0"), "what the annotation marked stayed");
}

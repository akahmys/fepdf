//! What the audit looked at, beside what it found.
//!
//! **A clean report from a check that was never run is the worst answer this engine can
//! give**, and it was the answer it gave. `audit_ua2` returned findings and a caller had
//! no way to tell "nothing is wrong" from "almost nothing was examined": the Matterhorn
//! Protocol 1.1 is 31 checkpoints comprised of 137 failure conditions, and this auditor
//! reports fourteen.
//!
//! `PdfStandard::UA2` writes a conformance claim into the catalogue, and that claim is a
//! statement about 137 things. Saying which fourteen were looked at is not a nicety; it is
//! the difference between a report and an assurance nobody checked.
//!
//! **And the numbers have to be the right ones.** All three were wrong until 2026-09-21,
//! cited from memory: 14-001 is "Headings are not tagged" where this checks that levels
//! are not skipped, which is 14-003, and 13-001 is graphics not tagged as a `<Figure>`
//! where this checks the missing alternative text, which is 13-004. The third — a
//! structure element naming a page that is not there — matched **no** failure condition,
//! because a broken reference is not a way to fail PDF/UA-1, and it went. A fourth
//! survived one level up, in the facade: a document with no structure tree was reported
//! under `00-001`, which is not a number the protocol has either.

use fepdf::{IngestionOptions, PdfDocument};
use fepdf_doc::audit_files::FROM_FILES;
use fepdf_doc::audit_fonts::FROM_FONTS;
use fepdf_doc::audit_objects::FROM_OBJECTS;
use fepdf_doc::matterhorn::LEFT_TO_A_PERSON;
use fepdf_doc::{
    AuditFinding, AuditReport, FROM_CATALOGUE, FROM_CONTENT, FROM_FORM, FROM_STRUCTURE_TREE,
    MatterhornAuditor, NO_STRUCTURE_TREE, Outcome,
};
use std::collections::BTreeSet;

fn opened(bytes: Vec<u8>) -> PdfDocument {
    PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the fixture opens")
}

/// A tagged document that breaks every failure condition this auditor looks at but one.
///
/// **One element or entry per check, so that a failure names which one is not found.**
/// 07-002 is the exception and cannot be here: it fails on `/DisplayDocTitle` being
/// *false*, and 07-001 fails on its being absent, so one document cannot break both.
/// [`display_doc_title_false`] is the other half.
///
/// | | How this document breaks it |
/// | :--- | :--- |
/// | 01-003 | an `/Artifact` sequence opened inside `/P <</MCID 0>>` |
/// | 01-004 | a `/P <</MCID 1>>` opened inside an `/Artifact` |
/// | 01-005 | a `re f` under neither |
/// | 01-007 | `/MarkInfo /Suspects true` |
/// | 07-001 | no `/ViewerPreferences`, so no `/DisplayDocTitle` |
/// | 11-002 | a `<Span>` with `/ActualText` and no `/Lang` reaching it |
/// | 13-004 | a `<Figure>` with neither `/Alt` nor `/ActualText` |
/// | 14-002 | the first numbered heading is `<H2>` |
/// | 14-003 | `<H4>` follows `<H2>` |
/// | 14-006 | a `<Sect>` holding two `<H>` children |
/// | 14-007 | those `<H>`s beside the `<H2>` and `<H4>` |
/// | 17-002 | a `<Formula>` with no `/Alt` |
/// | 28-005 | a form field with no `/TU` |
fn breaks_everything() -> Vec<u8> {
    // Three lines, one condition each. The `f` inside the nested sequences is under a
    // tag or an artefact either way, so the only mark under neither is the first.
    let content = "0 0 5 5 re f\n\
                   /P <</MCID 0>> BDC /Artifact BMC 0 0 5 5 re f EMC EMC\n\
                   /Artifact BMC /P <</MCID 1>> BDC 0 0 5 5 re f EMC EMC";
    fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 12 0 R \
           /MarkInfo << /Marked true /Suspects true >> \
           /AcroForm << /Fields [13 0 R] >> >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 14 0 R >>".to_string(),
        "<< /Type /StructElem /S /H2 /P 12 0 R /Pg 3 0 R >>".to_string(),
        "<< /Type /StructElem /S /H4 /P 12 0 R /Pg 3 0 R >>".to_string(),
        "<< /Type /StructElem /S /Sect /P 12 0 R /K [7 0 R 8 0 R] >>".to_string(),
        "<< /Type /StructElem /S /H /P 6 0 R /Pg 3 0 R >>".to_string(),
        "<< /Type /StructElem /S /H /P 6 0 R /Pg 3 0 R >>".to_string(),
        "<< /Type /StructElem /S /Figure /P 12 0 R /Pg 3 0 R >>".to_string(),
        "<< /Type /StructElem /S /Formula /P 12 0 R /Pg 3 0 R >>".to_string(),
        "<< /Type /StructElem /S /Span /P 12 0 R /Pg 3 0 R /ActualText (ibid.) >>".to_string(),
        "<< /Type /StructTreeRoot /K [4 0 R 5 0 R 6 0 R 9 0 R 10 0 R 11 0 R] >>".to_string(),
        "<< /FT /Tx /T (Given name) >>".to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
    ])
    .into_iter()
    .collect()
}

/// A tagged document that breaks every condition W-21i added, one object per condition.
///
/// | | How this document breaks it |
/// | :--- | :--- |
/// | 02-001 | a `<Custom>` element with no mapping |
/// | 02-003 | `/Loop1` mapped to `/Loop2` and back |
/// | 02-004 | the standard `/P` mapped to `/Span` |
/// | 11-003 | outline items and no catalogue `/Lang` |
/// | 11-004 | a note annotation's `/Contents` with no `/Lang` anywhere |
/// | 11-005 | a field's `/TU` with no `/Lang` anywhere |
/// | 15-003 | a `<TH>` with no `/Scope` in a table with no `/Headers` |
/// | 19-003 | a `<Note>` with no `/ID` |
/// | 19-004 | two `<Note>`s with the `/ID` (n1) |
/// | 20-001 | a `/Configs` configuration with no `/Name` |
/// | 20-002 | a `/D` configuration with no `/Name` |
/// | 20-003 | `/AS` in the `/D` configuration |
/// | 28-004 | a `/Square` annotation with no `/Contents` |
/// | 28-007 | a `/TrapNet` annotation |
/// | 28-008 | page 1 has annotations and no `/Tabs` |
/// | 28-009 | page 2 has annotations and `/Tabs /R` |
/// | 28-012 | a `/Link` annotation with no `/Contents` |
/// | 28-002 | the note, square and trap annotations, in no `<Annot>` |
/// | 28-010 | a widget in no `<Form>` |
/// | 28-011 | the links, in no `<Link>` |
/// | 28-017 | a `/PrinterMark` the parent tree places in the structure |
/// | 30-001 | a form XObject carrying `/Ref` |
fn breaks_the_second_pass() -> Vec<u8> {
    fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R /MarkInfo << /Marked true >> \
           /Outlines 12 0 R /AcroForm << /Fields [14 0 R] >> \
           /OCProperties << /OCGs [15 0 R] /D << /AS [] >> /Configs [<< /Order [] >>] >> >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Annots [16 0 R 17 0 R 18 0 R] \
           /Resources << /XObject << /Fm0 19 0 R >> >> >>"
            .to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Tabs /R \
           /Annots [20 0 R 23 0 R 24 0 R] >>"
            .to_string(),
        "<< /Type /StructTreeRoot /K [6 0 R 7 0 R 8 0 R 9 0 R 10 0 R 25 0 R] \
           /RoleMap << /P /Span /Loop1 /Loop2 /Loop2 /Loop1 >> \
           /ParentTree << /Nums [0 25 0 R] >> >>"
            .to_string(),
        "<< /Type /StructElem /S /Custom /P 5 0 R >>".to_string(),
        "<< /Type /StructElem /S /Note /P 5 0 R >>".to_string(),
        "<< /Type /StructElem /S /Note /P 5 0 R /ID (n1) >>".to_string(),
        "<< /Type /StructElem /S /Note /P 5 0 R /ID (n1) >>".to_string(),
        "<< /Type /StructElem /S /Table /P 5 0 R /K [11 0 R] >>".to_string(),
        "<< /Type /StructElem /S /TR /P 10 0 R /K [21 0 R 22 0 R] >>".to_string(),
        "<< /Type /Outlines /First 13 0 R /Last 13 0 R /Count 1 >>".to_string(),
        "<< /Title (Chapter one) /Parent 12 0 R >>".to_string(),
        "<< /FT /Tx /T (Name) /TU (Your name) >>".to_string(),
        "<< /Type /OCG /Name (Layer) >>".to_string(),
        "<< /Type /Annot /Subtype /Text /Rect [0 0 10 10] /Contents (A note) >>".to_string(),
        "<< /Type /Annot /Subtype /Square /Rect [0 0 10 10] >>".to_string(),
        "<< /Type /Annot /Subtype /TrapNet /Rect [0 0 10 10] /Contents (trap) >>".to_string(),
        "<< /Type /XObject /Subtype /Form /BBox [0 0 1 1] /Ref << /F (other.pdf) /Page 0 >> \
           /Length 0 >>\nstream\n\nendstream"
            .to_string(),
        "<< /Type /Annot /Subtype /Link /Rect [0 0 10 10] >>".to_string(),
        "<< /Type /StructElem /S /TH /P 11 0 R >>".to_string(),
        "<< /Type /StructElem /S /TD /P 11 0 R >>".to_string(),
        // 23: a widget in no <Form>; 24: a printer's mark in the structure; 25: its element.
        "<< /Type /Annot /Subtype /Widget /Rect [0 0 10 10] /FT /Btn /T (Box) >>".to_string(),
        "<< /Type /Annot /Subtype /PrinterMark /Rect [0 0 10 10] /Contents (mark) \
           /StructParent 0 >>"
            .to_string(),
        "<< /Type /StructElem /S /Annot /P 5 0 R >>".to_string(),
    ])
    .into_iter()
    .collect()
}

/// A tagged document that breaks the five conditions W-21h added, each from its table in
/// ISO 32000-1.
///
/// | | How this document breaks it |
/// | :--- | :--- |
/// | 09-004 | a `<Table>` holding a `<P>` (Table 337) |
/// | 09-005 | an `<L>` holding a `<P>` (Table 336) |
/// | 09-006 | a `<TOC>` holding a `<P>` (Table 333) |
/// | 09-007 | a `<Ruby>` holding an `<RT>` alone (Tables 338 and 339) |
/// | 09-008 | a `<Warichu>` holding a `<WT>` alone (Tables 338 and 339) |
fn breaks_the_syntax() -> Vec<u8> {
    fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 4 0 R /MarkInfo << /Marked true >> >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>".to_string(),
        "<< /Type /StructTreeRoot /K [5 0 R 7 0 R 9 0 R 11 0 R 13 0 R] >>".to_string(),
        "<< /Type /StructElem /S /Table /P 4 0 R /K [6 0 R] >>".to_string(),
        "<< /Type /StructElem /S /P /P 5 0 R >>".to_string(),
        "<< /Type /StructElem /S /L /P 4 0 R /K [8 0 R] >>".to_string(),
        "<< /Type /StructElem /S /P /P 7 0 R >>".to_string(),
        "<< /Type /StructElem /S /TOC /P 4 0 R /K [10 0 R] >>".to_string(),
        "<< /Type /StructElem /S /P /P 9 0 R >>".to_string(),
        "<< /Type /StructElem /S /Ruby /P 4 0 R /K [12 0 R] >>".to_string(),
        "<< /Type /StructElem /S /RT /P 11 0 R >>".to_string(),
        "<< /Type /StructElem /S /Warichu /P 4 0 R /K [14 0 R] >>".to_string(),
        "<< /Type /StructElem /S /WT /P 13 0 R >>".to_string(),
    ])
    .into_iter()
    .collect()
}

/// A stream of `bytes` given as hexadecimal, under `extra`.
fn hex_stream(extra: &str, hex: &str) -> String {
    format!(
        "<< {extra} /Filter /ASCIIHexDecode /Length {} >>\nstream\n{hex}>\nendstream",
        hex.len() + 1
    )
}

/// Fonts that break the ten conditions W-21k added, one font per condition or two.
///
/// | | How this document breaks it |
/// | :--- | :--- |
/// | 31-004 | a `CIDFontType2` whose `/CIDToGIDMap` is `/Foo` |
/// | 31-019 | a non-symbolic TrueType font with no `/Encoding` |
/// | 31-020 | a non-symbolic one whose `/Encoding` dictionary has no `/BaseEncoding` |
/// | 31-021 | a non-symbolic one on `/StandardEncoding` |
/// | 31-023 | the second, with `/Differences` and a program with no (3,1) cmap |
/// | 31-024 | a symbolic TrueType font carrying `/Encoding` |
/// | 31-025 | that font's program, which has no cmap |
/// | 31-026 | a symbolic font whose program has two cmaps and no (3,0) |
/// | 31-028 | a `/ToUnicode` mapping a code to U+0000 |
/// | 31-029 | the same map, mapping another to U+FEFF |
/// | 31-022 | a non-symbolic TrueType font whose `/Differences` names `/notaname` |
/// | 31-027 | a Type 1 font with no `/ToUnicode` showing a glyph named `/madeup` |
fn breaks_the_fonts() -> Vec<u8> {
    let descriptor = |flags: u8, program: u8| {
        format!("<< /Type /FontDescriptor /FontName /F /Flags {flags} /FontFile2 {program} 0 R >>")
    };
    let to_unicode = "/CIDInit /ProcSet findresource begin 12 dict begin begincmap \
        1 begincodespacerange <00> <FF> endcodespacerange \
        2 beginbfchar <01> <0000> <02> <FEFF> endbfchar endcmap end end";
    fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 20 0 R \
           /Resources << /Font << /A 4 0 R /B 5 0 R /C 6 0 R /D 7 0 R /E 8 0 R /F 9 0 R \
           /G 10 0 R /H 21 0 R /I 22 0 R >> >> >>"
            .to_string(),
        "<< /Type /Font /Subtype /TrueType /BaseFont /NoEncoding /FontDescriptor 11 0 R >>".to_string(),
        "<< /Type /Font /Subtype /TrueType /BaseFont /NoBase /FontDescriptor 12 0 R \
           /Encoding << /Differences [65 /A] >> >>"
            .to_string(),
        "<< /Type /Font /Subtype /TrueType /BaseFont /Standard /FontDescriptor 11 0 R \
           /Encoding /StandardEncoding >>"
            .to_string(),
        "<< /Type /Font /Subtype /TrueType /BaseFont /SymbolicEncoded /FontDescriptor 13 0 R \
           /Encoding /WinAnsiEncoding >>"
            .to_string(),
        "<< /Type /Font /Subtype /TrueType /BaseFont /TwoCmaps /FontDescriptor 14 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type0 /BaseFont /Cid /Encoding /Identity-H \
           /DescendantFonts [<< /Type /Font /Subtype /CIDFontType2 /BaseFont /Cid \
           /CIDToGIDMap /Foo >>] >>"
            .to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /ToUnicode 18 0 R >>".to_string(),
        descriptor(32, 15),
        descriptor(32, 16),
        descriptor(4, 17),
        descriptor(4, 19),
        hex_stream("", "000100000001001000000000636d6170000000000000001c0000001400000001000100000000000c0000000000000000"),
        hex_stream("", "000100000001001000000000636d6170000000000000001c0000001400000001000100000000000c0000000000000000"),
        hex_stream("", "00010000000100100000000068656164000000000000001c000000080000000000000000"),
        format!("<< /Length {} >>\nstream\n{to_unicode}\nendstream", to_unicode.len()),
        hex_stream("", "000100000001001000000000636d6170000000000000001c0000001c00000002000100000000001400030001000000140000000000000000"),
        // 20: the page's text, in the Type 1 font whose glyph has no listed name.
        {
            let content = "BT /H 12 Tf 20 100 Td (A) Tj ET";
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len())
        },
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica \
           /Encoding << /Differences [65 /madeup] >> >>"
            .to_string(),
        "<< /Type /Font /Subtype /TrueType /BaseFont /Unlisted /FontDescriptor 11 0 R \
           /Encoding << /BaseEncoding /WinAnsiEncoding /Differences [66 /notaname] >> >>"
            .to_string(),
    ])
    .into_iter()
    .collect()
}

/// An `/Encrypt` this engine cannot open, whose `/P` is `permissions` or absent.
///
/// **Encrypted in name only**: the strings and streams are plaintext and `/U` matches no
/// password, so ingestion records the handler, fails to unlock, and reads the objects as
/// they are — which is all 26-001 and 26-002 ask about.
fn encrypt_entry(permissions: Option<i32>) -> String {
    let zeros = "00".repeat(32);
    let p = permissions.map_or_else(String::new, |p| format!("/P {p}"));
    format!("/Encrypt << /Filter /Standard /V 1 /R 2 /O <{zeros}> /U <{zeros}> {p} >>")
}

/// A document carrying what W-21m's conditions are about, each of them broken.
///
/// | | How this document breaks it |
/// | :--- | :--- |
/// | 21-001 | an `/EmbeddedFiles` entry whose specification has `/F` and no `/UF` |
/// | 25-001 | an `/XFA` stream whose `dynamicRender` is `required` |
/// | 26-002 | an `/Encrypt` whose `/P` has bit 10 clear |
/// | 28-014, 28-015 | an `/OpenAction` rendition whose media clip has neither `/CT` nor `/Alt` |
/// | 28-016 | a file attachment annotation whose `/FS` is a string |
/// | 30-002 | a form carrying an MCID, drawn twice |
/// | 31-009 | text in Helvetica, whose program is not embedded |
/// | 31-017 | text in a non-symbolic TrueType font whose program has only a (3,0) cmap |
/// | 31-018 | text `B` in a non-symbolic TrueType font whose (3,1) cmap maps only `A` |
/// | 28-006 | a `/Foo` annotation with no `/Contents`, in no structure element |
/// | 28-018 | a `/PrinterMark` whose appearance fills a rectangle under no `/Artifact` |
/// | 11-006 | no `/Lang` in the catalogue — left for a reader, which is not sound |
fn breaks_the_files() -> Vec<u8> {
    let stream = |extra: &str, content: &str| {
        format!("<< {extra} /Length {} >>\nstream\n{content}\nendstream", content.len())
    };
    fepdf_fixtures::Pdf::new()
        .trailer_entries(&encrypt_entry(Some(-1548)))
        .assemble(&[
            "<< /Type /Catalog /Pages 2 0 R /Names << /EmbeddedFiles << /Names [(a.txt) 4 0 R] >> >> \
               /AcroForm << /Fields [] /XFA 5 0 R >> \
               /OpenAction << /S /Rendition /R << /S /MR /C 6 0 R >> >> >>"
                .to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 7 0 R \
               /Resources << /XObject << /X 8 0 R >> \
               /Font << /T 9 0 R /U 10 0 R /V 18 0 R >> >> /Annots [11 0 R 15 0 R 16 0 R] >>"
                .to_string(),
            "<< /Type /Filespec /F (a.txt) /EF << /F 12 0 R >> >>".to_string(),
            stream(
                "",
                "<xdp:xdp><config><acrobat><acrobat7><dynamicRender>required</dynamicRender>\
                 </acrobat7></acrobat></config></xdp:xdp>",
            ),
            "<< /Type /MediaClip /S /MCD /D (clip.mp4) >>".to_string(),
            stream(
                "",
                "q /X Do Q q /X Do Q BT /T 12 Tf 20 100 Td (A) Tj ET BT /U 12 Tf 20 80 Td (A) Tj ET \
                 BT /V 12 Tf 20 60 Td (B) Tj ET",
            ),
            stream("/Type /XObject /Subtype /Form /BBox [0 0 10 10]", "/P <</MCID 0>> BDC 0 0 5 5 re f EMC"),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_string(),
            "<< /Type /Font /Subtype /TrueType /BaseFont /NoLatinCmap /FontDescriptor 13 0 R \
               /Encoding /WinAnsiEncoding >>"
                .to_string(),
            "<< /Type /Annot /Subtype /FileAttachment /Rect [0 0 10 10] /FS (b.txt) /Contents (b) >>"
                .to_string(),
            stream("/Type /EmbeddedFile", "hello"),
            "<< /Type /FontDescriptor /FontName /NoLatinCmap /Flags 32 /FontFile2 14 0 R >>".to_string(),
            hex_stream("", "000100000001001000000000636d6170000000000000001c0000001400000001000300000000000c0000000000000000"),
            "<< /Type /Annot /Subtype /Foo /Rect [0 0 10 10] >>".to_string(),
            "<< /Type /Annot /Subtype /PrinterMark /Rect [0 0 10 10] /AP << /N 17 0 R >> >>"
                .to_string(),
            stream("/Type /XObject /Subtype /Form /BBox [0 0 10 10]", "0 0 5 5 re f"),
            "<< /Type /Font /Subtype /TrueType /BaseFont /OnlyA /FontDescriptor 19 0 R \
               /Encoding /WinAnsiEncoding >>"
                .to_string(),
            "<< /Type /FontDescriptor /FontName /OnlyA /Flags 32 /FontFile2 20 0 R >>".to_string(),
            hex_stream("", ONLY_A_BY_UNICODE),
        ])
}

/// A TrueType program whose one `cmap` subtable is (3,1), format 4, mapping U+0041 to
/// glyph 1 and nothing else.
const ONLY_A_BY_UNICODE: &str = "000100000001001000000000636d6170000000000000001c0000002c00000001000300010000000c00040020000000040004000100000041ffff00000041ffffffc0000100000000";

/// A TrueType program whose one `cmap` subtable is (1,0), format 0, mapping code 0x41 to
/// glyph 1 and code 219 — `Euro` in Mac OS Roman, `currency` in MacRomanEncoding — to 2.
const A_AND_EURO_BY_MAC_ROMAN: &str = "000100000001001000000000636d6170000000000000001c0000011200000001000100000000000c00000106000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000000000000";

/// 26-001, which [`breaks_the_files`] cannot also break: its `/Encrypt` has a `/P`.
fn encrypted_without_p() -> Vec<u8> {
    fepdf_fixtures::Pdf::new().trailer_entries(&encrypt_entry(None)).assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
    ])
}

/// The one condition [`breaks_everything`] cannot also break: 07-002 wants the entry
/// present and false, where 07-001 wants it absent.
fn display_doc_title_false() -> Vec<u8> {
    fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /ViewerPreferences << /DisplayDocTitle false >> >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
    ])
    .into_iter()
    .collect()
}

/// A tagged document that breaks none of the conditions this auditor looks at.
///
/// Headings `<H1>` then `<H2>` and no `<H>` (14-002, 14-003, 14-007), a figure with its
/// alternative text and a formula with its own (13-004, 17-002), a `/Lang` on the
/// catalogue for the `<Span>`'s `/ActualText` to be read in (11-002), `/Suspects` absent
/// (01-007), `/DisplayDocTitle` true (07-001, 07-002), no form at all (28-005), one
/// `<H>` per node and none of them beside a `<Hn>` (14-006), and a page whose every mark
/// is under either an `/MCID` or an `/Artifact` (01-003, 01-004, 01-005).
fn breaks_nothing() -> Vec<u8> {
    fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 9 0 R /Lang (en-GB) \
           /MarkInfo << /Marked true >> \
           /ViewerPreferences << /DisplayDocTitle true >> >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 10 0 R >>".to_string(),
        "<< /Type /StructElem /S /H1 /P 9 0 R /Pg 3 0 R >>".to_string(),
        "<< /Type /StructElem /S /H2 /P 9 0 R /Pg 3 0 R >>".to_string(),
        "<< /Type /StructElem /S /Figure /P 9 0 R /Pg 3 0 R /Alt (a duck) >>".to_string(),
        "<< /Type /StructElem /S /Formula /P 9 0 R /Pg 3 0 R /Alt (E equals m c squared) >>"
            .to_string(),
        "<< /Type /StructElem /S /Span /P 9 0 R /Pg 3 0 R /ActualText (ibid.) >>".to_string(),
        "<< /Type /StructTreeRoot /K [4 0 R 5 0 R 6 0 R 7 0 R 8 0 R] >>".to_string(),
        // **A page that is actually marked, rather than a page that draws nothing.** An
        // empty `/Contents` passes checkpoint 01 vacuously, which is not the sound case
        // worth having: one sequence carries an `/MCID` and the other is an artefact.
        {
            let content = "/P <</MCID 0>> BDC 0 0 5 5 re f EMC\n\
                           /Artifact BMC 10 10 5 5 re f EMC";
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len())
        },
    ])
    .into_iter()
    .collect()
}

/// A document with a catalogue and no structure tree at all.
fn untagged() -> Vec<u8> {
    fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
    ])
    .into_iter()
    .collect()
}

/// Every condition the report names, whatever came of checking it.
fn reported(report: &AuditReport) -> Vec<&str> {
    report.findings.iter().map(|f| f.checkpoint.as_str()).collect()
}

/// What came of one condition, as many times as the report says it.
fn outcomes(report: &AuditReport, condition: &str) -> Vec<Outcome> {
    report.findings.iter().filter(|f| f.checkpoint == condition).map(|f| f.outcome).collect()
}

/// **The report says how much of the protocol it looked at.**
///
/// Fourteen of 137 failure conditions. A reader told "no findings" and not told that would
/// have been told the document conforms.
#[test]
fn the_report_says_how_much_of_the_protocol_it_checked() {
    let doc = opened(breaks_everything());
    let report = doc.audit_ua2_report().expect("it audits");

    assert_eq!(
        report.scope.in_protocol, 137,
        "the protocol's size is not what this was written about"
    );
    assert_eq!(
        report.scope.checked.len(),
        71,
        "the scope does not name the failure conditions this auditor looks at"
    );
    assert!(
        report.scope.checked.len() < report.scope.in_protocol,
        "the scope claims the whole protocol"
    );
}

/// **The three lists of conditions partition the one the scope is built from.**
///
/// A condition is decided by reading the catalogue, by reading the form, or by walking
/// the structure tree, and which of the three decides it is what says whether it was
/// examined at all in a document with no tree. A condition in `CHECKED` and in none of
/// the three would be promised and never looked at; one in two of them would be reported
/// twice.
#[test]
fn every_checked_condition_is_decided_by_exactly_one_reader() {
    let mut union: Vec<&str> = Vec::new();
    union.extend(FROM_CATALOGUE);
    union.extend(FROM_FORM);
    union.extend(FROM_CONTENT);
    union.extend(FROM_STRUCTURE_TREE);
    union.extend(FROM_OBJECTS);
    union.extend(FROM_FONTS);
    union.extend(FROM_FILES);

    let distinct: BTreeSet<&str> = union.iter().copied().collect();
    assert_eq!(distinct.len(), union.len(), "a condition is in two of the lists: {union:?}");
    assert_eq!(
        distinct,
        MatterhornAuditor::CHECKED.iter().copied().collect::<BTreeSet<&str>>(),
        "the lists and the scope do not name the same conditions"
    );
}

/// **Every condition the audit reports is one the scope names.**
///
/// Adding a check and forgetting to say so is how a scope stops being true, and it is
/// invisible from the outside: the report grows and the promise does not. The one row
/// that is allowed not to be a failure condition is [`NO_STRUCTURE_TREE`], which is a
/// clause rather than an index — and the next test holds it to that.
#[test]
fn the_scope_names_every_checkpoint_reported() {
    for fixture in [
        breaks_everything(),
        breaks_nothing(),
        untagged(),
        display_doc_title_false(),
        breaks_the_second_pass(),
        breaks_the_syntax(),
        breaks_the_fonts(),
        breaks_the_files(),
        encrypted_without_p(),
        subsets(["(/A)", "(/A/B/C)", "00", "64"]),
    ] {
        let doc = opened(fixture);
        let report = doc.audit_ua2_report().expect("it audits");
        assert!(!report.findings.is_empty(), "the audit reported nothing at all");

        for finding in &report.findings {
            if finding.checkpoint == NO_STRUCTURE_TREE {
                continue;
            }
            assert!(
                report.scope.checked.contains(&finding.checkpoint),
                "the audit reported {} and the scope does not name it: {:?}",
                finding.checkpoint,
                report.scope.checked
            );
        }
    }
}

/// **Nothing is reported under a number the protocol does not have.**
///
/// `00-001` was one: the facade filed "not a tagged PDF" under it, and no checkpoint 00
/// exists. A wrong number is not a gap in the audit — it is a finding against a defect
/// that is not there, and whoever looks it up is told something untrue. The row survives
/// as the *clause* PDF/UA-1 states the requirement in, which is a thing that can be
/// looked up.
#[test]
fn nothing_is_reported_under_a_number_the_protocol_does_not_have() {
    let doc = opened(untagged());
    let report = doc.audit_ua2_report().expect("it audits");

    assert!(
        reported(&report).contains(&NO_STRUCTURE_TREE),
        "a document with no structure tree did not say so: {:?}",
        reported(&report)
    );
    for finding in &report.findings {
        assert_ne!(finding.checkpoint, "00-001", "00-001 is not a Matterhorn failure condition");
        let indexed = finding.checkpoint.len() == 6
            && finding.checkpoint.is_char_boundary(2)
            && finding.checkpoint[2..3] == *"-";
        assert!(
            !indexed || MatterhornAuditor::CHECKED.contains(&finding.checkpoint.as_str()),
            "{} looks like an index number and is not one this auditor checks",
            finding.checkpoint
        );
    }
}

/// And every condition the scope names is one the audit can report as broken.
///
/// The other direction, and the stronger one: a scope naming a condition nothing looks at
/// is a promise of work that is not done. **Proved by breaking the thing each check
/// checks** — a fixture where the check merely *runs* would pass this while reporting
/// every condition sound.
#[test]
fn every_condition_the_scope_names_can_be_reported_broken() {
    let mut waiting: BTreeSet<&str> = MatterhornAuditor::CHECKED.iter().copied().collect();

    for fixture in [
        breaks_everything(),
        display_doc_title_false(),
        breaks_the_second_pass(),
        breaks_the_syntax(),
        breaks_the_fonts(),
        breaks_the_files(),
        encrypted_without_p(),
        subsets(["(/A)", "(/A/B/C)", "00", "64"]),
    ] {
        let doc = opened(fixture);
        let report = doc.audit_ua2_report().expect("it audits");
        for finding in &report.findings {
            if finding.outcome != Outcome::Sound {
                waiting.remove(finding.checkpoint.as_str());
            }
        }
    }

    assert!(waiting.is_empty(), "the scope names these and no fixture provokes one: {waiting:?}");
}

/// **A document with no structure tree says which check it failed, not "no findings".**
///
/// It is not a tagged PDF, which is the one thing this can say without looking at a
/// condition — and the scope still says what it would have looked at rather than claiming
/// it did.
#[test]
fn an_untagged_document_is_told_apart_from_a_clean_one() {
    let doc = opened(untagged());
    let report = doc.audit_ua2_report().expect("it audits");

    assert!(!report.found_nothing(), "an untagged document came back with nothing said");
    assert_eq!(report.scope.in_protocol, 137, "the scope forgot the protocol");
}

/// **A catalogue is still a catalogue when there is no structure tree.**
///
/// The three conditions that are properties of the catalogue and the one that is a
/// property of the form do not need a tag in the document. Reporting only "not a tagged
/// PDF" about such a file left four checks promised by the scope and run on nothing —
/// which is the shape this whole item exists to remove, one document short of where it
/// was being removed.
#[test]
fn an_untagged_document_is_still_asked_the_catalogue_conditions() {
    let doc = opened(untagged());
    let report = doc.audit_ua2_report().expect("it audits");
    let named = reported(&report);

    for condition in FROM_CATALOGUE.iter().chain(FROM_FORM.iter()) {
        assert!(
            named.contains(condition),
            "{condition} does not need a structure tree and was not reported: {named:?}"
        );
    }
    // This one has no `/ViewerPreferences`, so one of the four is broken rather than
    // merely asked.
    assert_eq!(
        outcomes(&report, "07-001"),
        vec![Outcome::Broken],
        "07-001 is broken by this document and was not reported so"
    );
}

/// **`found_nothing` could not return true, and nothing noticed.**
///
/// It was `findings.is_empty()`. Once a checked and unbroken condition became a `Sound`
/// finding, a clean document carried one row per condition in `CHECKED`, so the list was
/// never empty and the method always answered `false` — which made the assertion above,
/// that a broken document did not come back silent, pass for every input including a
/// perfect one. This is the document that makes it answer `true`.
#[test]
fn a_document_breaking_nothing_checked_is_said_to_have_broken_nothing() {
    let doc = opened(breaks_nothing());
    let report = doc.audit_ua2_report().expect("it audits");

    assert!(
        report.found_nothing(),
        "a document breaking no checked condition was reported as breaking one: {:?}",
        report
            .findings
            .iter()
            .filter(|f| f.outcome != Outcome::Sound)
            .map(|f| (&f.checkpoint, &f.message))
            .collect::<Vec<_>>()
    );
    // And it is not silent: the sound conditions are still reported, so "nothing to act
    // on" and "nothing was looked at" stay apart.
    assert_eq!(
        report.findings.iter().filter(|f| f.outcome == Outcome::Sound).count(),
        MatterhornAuditor::CHECKED.len(),
        "a clean document did not report the conditions it was checked against"
    );
}

/// **A condition checked and not broken is a result the report carries.**
///
/// A reader is owed what was examined, not only what was wrong. This document breaks
/// 14-003 and not 13-004, so the report says both — one broken, one sound.
#[test]
fn a_condition_that_came_out_sound_is_in_the_report() {
    let doc = opened(
        fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
            // H1 then H3: 14-003 is broken, 14-002 is not.
            "<< /Type /StructElem /S /H1 /P 6 0 R /Pg 3 0 R >>",
            "<< /Type /StructElem /S /H3 /P 6 0 R /Pg 3 0 R >>",
            "<< /Type /StructTreeRoot /K [4 0 R 5 0 R] >>",
        ])
        .into_iter()
        .collect(),
    );
    let report = doc.audit_ua2_report().expect("it audits");

    assert_eq!(
        outcomes(&report, "14-003"),
        vec![Outcome::Broken],
        "the skipped heading level was not reported"
    );
    assert_eq!(
        outcomes(&report, "13-004"),
        vec![Outcome::Sound],
        "a condition checked and not broken is missing from the report"
    );
    assert_eq!(
        outcomes(&report, "14-002"),
        vec![Outcome::Sound],
        "the first numbered heading is <H1> and 14-002 was not reported sound"
    );
}

/// **Sound and not-looked-at are different answers, and the report keeps them apart.**
///
/// Every condition the report calls sound has to be one the scope says was checked. A
/// report that called an unexamined condition sound would say a document conforms on the
/// strength of work nobody did — which is the shape this whole item exists to remove.
#[test]
fn nothing_is_called_sound_that_was_not_checked() {
    for fixture in [breaks_everything(), breaks_nothing(), untagged()] {
        let doc = opened(fixture);
        let report = doc.audit_ua2_report().expect("it audits");

        for finding in &report.findings {
            if finding.outcome == Outcome::Sound {
                assert!(
                    report.scope.checked.contains(&finding.checkpoint),
                    "{} is reported sound and the scope does not say it was checked",
                    finding.checkpoint
                );
            }
        }
        // **A condition is broken or sound, never both** — and this counts rather than
        // collecting into a set, because a set is exactly what hides the defect. A first
        // version of this test gathered the checkpoints into a `BTreeSet` and looked one
        // up with `find`, and a mutation that reported every condition sound *including
        // the broken ones* passed all eight tests: the set collapsed the duplicate and
        // `find` returned the first of the pair.
        //
        // **One sound row per condition and no more**, which is W-21g's rule; a broken
        // one may repeat, because a document with four untagged figures breaks 13-004
        // four times and a reader wants all four.
        for condition in &report.scope.checked {
            let came_to = outcomes(&report, condition);
            let sound = came_to.iter().filter(|o| **o == Outcome::Sound).count();
            assert!(sound <= 1, "{condition} is reported sound {sound} times: {came_to:?}");
            assert!(
                sound == 0 || came_to.len() == 1,
                "{condition} is reported both sound and not: {came_to:?}"
            );
        }
    }
}

/// A document with no structure tree has that tree's conditions examined by nothing.
///
/// Reporting them as sound because the walk found no elements to break them would be the
/// emptiest kind of pass.
#[test]
fn an_untagged_document_has_no_structure_condition_to_call_sound() {
    let doc = opened(untagged());
    let report = doc.audit_ua2_report().expect("it audits");

    for condition in FROM_STRUCTURE_TREE {
        assert!(
            outcomes(&report, condition).is_empty(),
            "{condition} is decided by walking a structure tree this document has not got, \
             and it was reported anyway: {:?}",
            outcomes(&report, condition)
        );
    }
}

/// **A condition waiting for a reader is not "nothing found".**
///
/// 28-005 is the first condition with a producer: it reads "a form field does not have a
/// `TU` entry **and** does not have an alternative description (in the form of an `Alt`
/// entry in the enclosing structure element)", and the second half is reached through an
/// `/OBJR`, which nothing here follows. So a field with no `/TU` is handed to a reader
/// with what was found, and is not counted as nothing to act on.
#[test]
fn a_condition_left_for_a_reader_is_not_nothing_found() {
    let doc = opened(breaks_everything());
    let report = doc.audit_ua2_report().expect("it audits");

    assert_eq!(
        outcomes(&report, "28-005"),
        vec![Outcome::ForAReader],
        "a form field with no /TU was not left for a reader"
    );
    let row =
        report.findings.iter().find(|f| f.checkpoint == "28-005").expect("28-005 is in the report");
    // **A suspicion carries its evidence or it is not shown.** The field's name is what
    // lets a reader agree or disagree; "28-005 suspected" is not something to act on.
    assert!(
        row.message.contains("Given name"),
        "the row does not say which field it is about: {}",
        row.message
    );

    let waiting = AuditReport {
        findings: vec![AuditFinding {
            checkpoint: "28-005".into(),
            severity: "Warning".into(),
            outcome: Outcome::ForAReader,
            message: "the field is yours to judge".into(),
            handle_id: None,
        }],
        scope: MatterhornAuditor::scope(),
    };
    assert!(
        !waiting.found_nothing(),
        "a condition handed to a reader to decide was reported as nothing to act on"
    );
}

/// **A field that states its `/TU` decides the condition rather than deferring it.**
///
/// 28-005 is a conjunction, and its first half is reachable from here: a field with a
/// `/TU` does not break it whatever its enclosing structure element says. Deferring that
/// to a reader would hand over work this engine has already done.
#[test]
fn a_form_field_that_states_its_tu_settles_the_condition() {
    let doc = opened(
        fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R] >> >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
            "<< /FT /Tx /T (Given name) /TU (Your given name, as on your passport) >>",
        ])
        .into_iter()
        .collect(),
    );
    let report = doc.audit_ua2_report().expect("it audits");

    assert_eq!(
        outcomes(&report, "28-005"),
        vec![Outcome::Sound],
        "a form field stating its /TU was not settled"
    );
}

/// **A `/Lang` an ancestor states reaches the element under it (14.9.2.2).**
///
/// The catalogue states none here and the `<Sect>` above the `<Span>` does, so the
/// `/ActualText` has a language and 11-002 is not broken. Checking the element's own
/// `/Lang` alone would report a document that is conforming.
#[test]
fn a_language_stated_on_an_ancestor_reaches_the_element() {
    let doc = opened(
        fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
            "<< /Type /StructElem /S /Sect /P 6 0 R /Lang (cy) /K [5 0 R] >>",
            "<< /Type /StructElem /S /Span /P 4 0 R /Pg 3 0 R /ActualText (ibid.) >>",
            "<< /Type /StructTreeRoot /K [4 0 R] >>",
        ])
        .into_iter()
        .collect(),
    );
    let report = doc.audit_ua2_report().expect("it audits");

    assert_eq!(
        outcomes(&report, "11-002"),
        vec![Outcome::Sound],
        "a /Lang on the enclosing element did not reach the text under it"
    );
}

/// **A `<Figure>` carrying `/ActualText` and no `/Alt` does not break 13-004.**
///
/// The condition is "`<Figure>` tag alternative **or replacement** text missing", and
/// this read `/Alt` alone — so a figure whose replacement text is what 7.3 paragraph 3
/// allows was reported as breaking a condition it does not break. 17-002 really is `/Alt`
/// alone, which is why the two are not one test spelt twice.
#[test]
fn a_figure_with_replacement_text_and_no_alternative_text_is_sound() {
    let doc = opened(
        fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R /Lang (en-GB) >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
            "<< /Type /StructElem /S /Figure /P 6 0 R /Pg 3 0 R /ActualText (Figure 1) >>",
            "<< /Type /StructElem /S /Formula /P 6 0 R /Pg 3 0 R /ActualText (x) >>",
            "<< /Type /StructTreeRoot /K [4 0 R 5 0 R] >>",
        ])
        .into_iter()
        .collect(),
    );
    let report = doc.audit_ua2_report().expect("it audits");

    assert_eq!(
        outcomes(&report, "13-004"),
        vec![Outcome::Sound],
        "a figure with replacement text was reported as having none"
    );
    // And the formula beside it, whose condition names `/Alt` and nothing else, is broken
    // by the same entry that settles the figure.
    assert_eq!(
        outcomes(&report, "17-002"),
        vec![Outcome::Broken],
        "17-002 names an Alt attribute and /ActualText was taken for one"
    );
}

/// **A heading level is a number, not a character.**
///
/// The test for `<Hn>` was `tag.len() == 2`, which stops at `<H9>`: a document going
/// `<H1>` to `<H10>` skipped eight levels and `<H10>` was not a heading at all, so 14-003
/// had nothing to compare. 14-005 is about a seventh level and higher, so a document deep
/// enough to need `<H10>` is one this was written for.
#[test]
fn a_heading_past_the_ninth_level_is_still_a_heading() {
    let doc = opened(
        fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
            "<< /Type /StructElem /S /H1 /P 6 0 R /Pg 3 0 R >>",
            "<< /Type /StructElem /S /H10 /P 6 0 R /Pg 3 0 R >>",
            "<< /Type /StructTreeRoot /K [4 0 R 5 0 R] >>",
        ])
        .into_iter()
        .collect(),
    );
    let report = doc.audit_ua2_report().expect("it audits");

    assert_eq!(
        outcomes(&report, "14-003"),
        vec![Outcome::Broken],
        "<H10> after <H1> skips eight levels and was not reported"
    );
    // And the first numbered heading is still <H1>, so 14-002 is not dragged in with it.
    assert_eq!(outcomes(&report, "14-002"), vec![Outcome::Sound], "14-002 fired on <H1> first");
}

/// A tagged one-page document drawing `content`, with an image and a form to draw.
///
/// The structure tree is one `<P>` claiming `/MCID 0`, so a mark under that sequence is
/// tagged and anything outside it is not.
fn page_drawing(content: &str) -> Vec<u8> {
    fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 8 0 R /Lang (en-GB) >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R \
           /Resources << /XObject << /Im0 5 0 R /Fm0 6 0 R >> \
                         /Properties << /Pr1 << /MCID 4 >> /Pr2 << /Foo 1 >> >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray \
           /BitsPerComponent 8 /Length 1 >>\nstream\n0\nendstream"
            .to_string(),
        "<< /Type /XObject /Subtype /Form /BBox [0 0 10 10] /Length 0 >>\nstream\n\nendstream"
            .to_string(),
        "<< /Type /StructElem /S /P /P 8 0 R /Pg 3 0 R >>".to_string(),
        "<< /Type /StructElem /S /Span /P 8 0 R /Pg 3 0 R >>".to_string(),
        "<< /Type /StructTreeRoot /K [7 0 R] >>".to_string(),
    ])
    .into_iter()
    .collect()
}

/// **A form XObject under neither a tag nor an artefact is a place to look, not a
/// verdict.**
///
/// Its own content stream may open the sequences, and this walk does not descend into it.
/// Reporting 01-005 broken here would be a finding against a file that conforms; saying
/// nothing would be the silence this phase keeps removing.
#[test]
fn a_form_xobject_under_no_tag_is_left_for_a_reader() {
    let doc = opened(page_drawing("q /Fm0 Do Q"));
    let report = doc.audit_ua2_report().expect("it audits");

    assert_eq!(
        outcomes(&report, "01-005"),
        vec![Outcome::ForAReader],
        "a form drawn under neither was decided rather than handed over"
    );
    let row = report.findings.iter().find(|f| f.checkpoint == "01-005").expect("01-005 is there");
    assert!(
        row.message.contains("Fm0"),
        "the row does not say which XObject it is about: {}",
        row.message
    );
}

/// **An image under neither is decided, because an image holds no marks of its own.**
///
/// The same `Do` operator, and the opposite answer: what makes the form undecidable is
/// its content stream, and an image XObject has none. Treating the two alike would either
/// lose a real finding or invent one.
#[test]
fn an_image_under_no_tag_is_decided() {
    let doc = opened(page_drawing("q /Im0 Do Q"));
    let report = doc.audit_ua2_report().expect("it audits");

    assert_eq!(
        outcomes(&report, "01-005"),
        vec![Outcome::Broken],
        "an image drawn under neither a tag nor an /Artifact was not reported"
    );
}

/// **A property list named through `/Properties` carries its `/MCID` all the same.**
///
/// `/P /Pr1 BDC` is the same tagging as `/P <</MCID 4>> BDC`, and reading the name as "no
/// `/MCID`" would report every mark on a page written that way as untagged — a document
/// that conforms, reported broken throughout.
#[test]
fn a_named_property_list_tags_what_is_under_it() {
    let tagged = opened(page_drawing("/P /Pr1 BDC 0 0 5 5 re f EMC"));
    assert_eq!(
        outcomes(&tagged.audit_ua2_report().expect("it audits"), "01-005"),
        vec![Outcome::Sound],
        "a named property list carrying an /MCID did not tag what was under it"
    );

    // And one that carries no `/MCID` tags nothing, which is what makes the lookup a
    // lookup rather than a way of passing anything with a name in it.
    let untagged = opened(page_drawing("/P /Pr2 BDC 0 0 5 5 re f EMC"));
    assert_eq!(
        outcomes(&untagged.audit_ua2_report().expect("it audits"), "01-005"),
        vec![Outcome::Broken],
        "a named property list with no /MCID was taken for tagging"
    );
}

/// **A sequence with no `/MCID` tags nothing.**
///
/// A structure element reaches content by `/MCID` and by nothing else, so a sequence
/// without one is not "tagged as real content" however much it looks like a tag. This is
/// the arm that makes 01-005 fire on a file that looks well formed.
///
/// **Both well-formed spellings, because the malformed one proves nothing.** This was
/// `/Span BDC` — `BDC` takes a tag *and* a property list, so with one operand the
/// sublimator finds no tag and emits no sequence at all. The mark came out untagged
/// because the operator was dropped, not because the arm under test said so: a mutation
/// making that arm report `Tagged` left this test passing. `BMC` takes the tag alone and
/// is the well-formed way to open a sequence without a property list.
#[test]
fn a_sequence_with_no_mcid_tags_nothing() {
    for content in ["/Span BMC 0 0 5 5 re f EMC", "/Span <</Foo 1>> BDC 0 0 5 5 re f EMC"] {
        let doc = opened(page_drawing(content));
        assert_eq!(
            outcomes(&doc.audit_ua2_report().expect("it audits"), "01-005"),
            vec![Outcome::Broken],
            "a sequence with no /MCID was taken for tagging: {content}"
        );
    }
}

/// **The two nesting conditions are told apart, and from the third.**
///
/// 01-003 and 01-004 are the same question asked in both directions, which is exactly the
/// shape that gets implemented once and reported twice.
#[test]
fn the_nesting_conditions_are_told_apart() {
    let artifact_in_tagged =
        opened(page_drawing("/P <</MCID 0>> BDC /Artifact BMC 0 0 5 5 re f EMC EMC"));
    let report = artifact_in_tagged.audit_ua2_report().expect("it audits");
    assert_eq!(outcomes(&report, "01-003"), vec![Outcome::Broken], "01-003 did not fire");
    assert_eq!(outcomes(&report, "01-004"), vec![Outcome::Sound], "01-004 fired the wrong way");

    let tagged_in_artifact =
        opened(page_drawing("/Artifact BMC /P <</MCID 0>> BDC 0 0 5 5 re f EMC EMC"));
    let report = tagged_in_artifact.audit_ua2_report().expect("it audits");
    assert_eq!(outcomes(&report, "01-004"), vec![Outcome::Broken], "01-004 did not fire");
    assert_eq!(outcomes(&report, "01-003"), vec![Outcome::Sound], "01-003 fired the wrong way");

    // Neither is broken by the two side by side, which is the ordinary shape of a tagged
    // page: content under its tag, furniture under an artefact.
    let beside =
        opened(page_drawing("/P <</MCID 0>> BDC 0 0 5 5 re f EMC /Artifact BMC 9 9 5 5 re f EMC"));
    let report = beside.audit_ua2_report().expect("it audits");
    for condition in FROM_CONTENT {
        assert_eq!(
            outcomes(&report, condition),
            vec![Outcome::Sound],
            "{condition} fired on a page whose marks are each under one sequence"
        );
    }
}

/// **14-006 counts a node's children, not its descendants.**
///
/// Two `<Sect>`s under one `<Sect>`, each with a heading of its own, is two nodes with one
/// heading each. Counting `<H>` anywhere beneath would report the outer one as holding
/// two, which is how a check about a node becomes a check about a document.
#[test]
fn one_heading_per_node_counts_children_not_descendants() {
    let doc = opened(
        fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 9 0 R /Lang (en-GB) >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
            "<< /Type /StructElem /S /Sect /P 9 0 R /K [5 0 R 7 0 R] >>",
            "<< /Type /StructElem /S /Sect /P 4 0 R /K [6 0 R] >>",
            "<< /Type /StructElem /S /H /P 5 0 R /Pg 3 0 R >>",
            "<< /Type /StructElem /S /Sect /P 4 0 R /K [8 0 R] >>",
            "<< /Type /StructElem /S /H /P 7 0 R /Pg 3 0 R >>",
            "<< /Type /StructTreeRoot /K [4 0 R] >>",
        ])
        .into_iter()
        .collect(),
    );
    let report = doc.audit_ua2_report().expect("it audits");

    assert_eq!(
        outcomes(&report, "14-006"),
        vec![Outcome::Sound],
        "a heading in each of two child nodes was counted as two in one node"
    );
    // And no numbered heading anywhere, so 14-007 has nothing to pair the <H>s with.
    assert_eq!(outcomes(&report, "14-007"), vec![Outcome::Sound], "14-007 fired on <H> alone");
}

/// **Checkpoint 06 is left out because ingestion answers it, not because it is hard.**
///
/// 06-001 is "document does not contain an XMP metadata stream" and 06-003 is "XMP
/// metadata stream does not contain dc:title". `metadata::settle` writes a packet into
/// the catalogue as a file is ingested, and promotes `/Info`'s `/Title` into it — so the
/// document this auditor reads is one where both have already been repaired, because a
/// document here is one normalised state and not the bytes it came from (ADR-0013).
/// 06-001 would answer sound for every file this engine can open, and 06-003 would answer
/// sound for a file whose only title is in the deprecated dictionary, which is a file that
/// breaks it.
///
/// **A check that cannot fail is what this phase has spent itself removing**, so neither
/// is claimed. This test holds that reason to the code: the day ingestion stops writing
/// the packet, it fails, and the two conditions become checkable.
#[test]
fn checkpoint_06_is_left_out_because_ingestion_answers_it() {
    for condition in ["06-001", "06-003"] {
        assert!(
            !MatterhornAuditor::CHECKED.contains(&condition),
            "{condition} is claimed, and what it asks about has been answered before the \
             auditor is called"
        );
    }
    let file = untagged();
    // The file states no `/Metadata`. `survey` reads the bytes rather than the ingested
    // document, which is the whole of the difference being measured here.
    let raw = fepdf::CatalogReport::survey(&file).expect("the catalogue reads");
    assert!(
        raw.entries.iter().all(|entry| entry.key != "Metadata"),
        "the fixture states a /Metadata of its own, so this measures nothing"
    );
    // The document ingested from it does.
    let ingested = opened(file);
    assert!(
        ingested.inner().catalog().expect("the catalogue reads").metadata.is_some(),
        "ingestion no longer writes an XMP packet, so 06-001 and 06-003 can be answered \
         about the file and belong in CHECKED"
    );
}

/// **The report says which conditions the protocol expects a person to answer.**
///
/// A reader told "fourteen of 137 checked" and nothing else cannot tell the rest apart:
/// a condition nobody has implemented and a condition the protocol *assigns to a human
/// auditor* are the same silence, and only one of them is work this engine could do.
#[test]
fn the_scope_says_which_conditions_are_left_to_a_person() {
    let doc = opened(breaks_nothing());
    let report = doc.audit_ua2_report().expect("it audits");

    assert_eq!(
        report.scope.left_to_a_person.len(),
        48,
        "the protocol marks 48 of its failure conditions H"
    );
    for entry in &report.scope.left_to_a_person {
        assert_eq!(entry.condition.len(), 6, "{} is not an index number", entry.condition);
        assert!(
            entry.wording.len() > 20,
            "{} is listed for a person with nothing to read: {:?}",
            entry.condition,
            entry.wording
        );
    }
    // **The same list for every document**, which is the whole reason it is scope rather
    // than a finding: a row per document would be 48 findings saying nothing about the
    // document they are attached to.
    let other = opened(breaks_everything()).audit_ua2_report().expect("it audits");
    assert_eq!(
        report.scope.left_to_a_person, other.scope.left_to_a_person,
        "the conditions left to a person changed with the document"
    );
}

/// **A condition is checked here or left to a person, never both.**
///
/// The protocol's `How` is advice — "not determinative … the realistic best-practice
/// approach at the present time" — so an `H` condition may well be decided here one day.
/// What must not happen is its staying on the list of questions for a reader after this
/// engine has started answering it: that is a report asking someone to do work it has
/// already done.
#[test]
fn nothing_is_both_checked_here_and_left_to_a_person() {
    let left: BTreeSet<&str> = LEFT_TO_A_PERSON.iter().map(|(condition, _)| *condition).collect();
    let checked: BTreeSet<&str> = MatterhornAuditor::CHECKED.iter().copied().collect();
    let both: Vec<&&str> = left.intersection(&checked).collect();
    assert!(both.is_empty(), "checked here and still listed for a reader to decide: {both:?}");

    assert_eq!(left.len(), LEFT_TO_A_PERSON.len(), "a condition is listed for a person twice");
    assert!(
        checked.len() + left.len() <= MatterhornAuditor::IN_PROTOCOL,
        "the two lists together claim more conditions than the protocol has: {} and {} of {}",
        checked.len(),
        left.len(),
        MatterhornAuditor::IN_PROTOCOL
    );
}

/// The Index, Type and How columns of one row of the protocol's tables.
///
/// **The `How` is not at a fixed place.** The Section, Type and How columns interrupt the
/// *first line* of the Failure Condition text, wherever that line happens to end, so they
/// are found by looking for the Section — the one column whose value has a shape.
fn row_of(line: &str) -> Option<(&str, &str, &str)> {
    let number = line.get(..6)?;
    let bytes = number.as_bytes();
    if !bytes[..2].iter().all(u8::is_ascii_digit)
        || bytes[2] != b'-'
        || !bytes[3..].iter().all(u8::is_ascii_digit)
    {
        return None;
    }
    let words: Vec<&str> = line.split_whitespace().collect();
    let at = words.iter().position(|word| word.starts_with("UA1:"))?;
    let kind = *words.get(at + 1)?;
    if !["Doc", "Page", "Object", "JS", "All"].contains(&kind) {
        return None;
    }
    // Everything between the number and the Section column is the first line of the
    // condition's own text.
    let said = line.get(6..line.find(words[at])?)?.trim();
    Some((number, words.get(at + 2)?, said))
}

/// **Every condition listed for a person is one the protocol marks `H`, worded as it
/// words it.**
///
/// Two ways this goes wrong and neither is visible from inside: handing a reader an `M`
/// condition, which is work this engine promised and did not do, and quoting a condition
/// as something it does not say. The protocol is untracked, so the wording is checked in
/// — and a table that is checked in is a table that can drift.
#[test]
fn every_condition_left_to_a_person_is_one_the_protocol_marks_h() {
    let text = protocol_text();
    let mut marked_h = BTreeSet::new();
    let mut said: std::collections::BTreeMap<&str, &str> = std::collections::BTreeMap::new();
    for line in text.lines() {
        if let Some((number, how, first_line)) = row_of(line) {
            if how == "H" {
                marked_h.insert(number);
            }
            said.entry(number).or_insert(first_line);
        }
    }

    let listed: BTreeSet<&str> = LEFT_TO_A_PERSON.iter().map(|(condition, _)| *condition).collect();
    assert_eq!(
        listed, marked_h,
        "the conditions listed for a person are not the ones the protocol marks H"
    );
    assert_eq!(marked_h.len(), 48, "the protocol no longer marks 48 of its conditions H");

    for (condition, wording) in LEFT_TO_A_PERSON {
        let opening = said.get(condition).unwrap_or_else(|| panic!("{condition} has no row"));
        // The stored wording is the row's text with the line breaks taken out, so it
        // begins with exactly what the row's first line says before the columns cut in.
        assert!(
            wording.starts_with(opening),
            "{condition} is quoted as something the protocol does not say:\n  stored: \
             {wording}\n  protocol: {opening}"
        );
    }
}

/// The protocol, as text, for the two tests that read it.
///
/// It is `docs/specs/Matterhorn-Protocol-1-1.pdf`, which is untracked
/// (`docs/specs/README.md` says where to get it at no cost). Without it these cannot
/// check anything, and they say so rather than passing.
fn protocol_text() -> String {
    let path = "../../docs/specs/Matterhorn-Protocol-1-1.pdf";
    let Ok(bytes) = std::fs::read(path) else {
        panic!("{path} is not in this working copy; docs/specs/README.md says where it is");
    };
    let protocol = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the protocol opens");
    let pages = protocol.page_count().expect("it counts");
    (0..pages)
        .map(|page| protocol.extract_text(page).unwrap_or_default())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Where the Index column states a number, as the rest of that table row.
///
/// **Not the first occurrence in the document.** 01-007 and 28-005 are named in the
/// Document History before their own tables — "moved 08-003 to 01-007" and "Failure
/// conditions 28-002, 28-004, 28-005 specified more precisely" — so looking for the first
/// occurrence lands on a sentence *about* the number rather than the row that defines it,
/// and the two conditions would have failed a test that is checking the right thing.
/// Index is the table's first column, so a row starts its line.
fn index_rows<'a>(text: &'a str, number: &str) -> Vec<&'a str> {
    text.match_indices(number)
        .filter(|(at, _)| *at == 0 || text.as_bytes()[at - 1] == b'\n')
        .map(|(at, _)| &text[at..])
        .collect()
}

/// **Every number this auditor reports says, in the protocol, what this auditor checks.**
///
/// All three were wrong until 2026-09-21 and nothing noticed, because nothing compared
/// them to the document. A wrong number is not a gap in the audit — it is a finding filed
/// against a different defect, and a reader or a tool that looks the number up is told
/// something untrue.
#[test]
fn every_number_reported_means_in_the_protocol_what_it_is_used_for() {
    let text = protocol_text();

    // What each number has to be followed by in the protocol's own words. Short enough to
    // survive the line breaks a two-column table puts in — the Section, Type and How
    // columns interrupt the first line of every row — and long enough to be that
    // condition and no other.
    let says = [
        ("01-003", "Content marked as Artifact is present inside tagged content"),
        ("01-004", "Tagged content is present inside content marked as Artifact"),
        ("01-005", "Content is neither marked as Artifact nor tagged as real"),
        ("01-007", "Suspects entry has a value of true"),
        ("07-001", "does not contain a DisplayDocTitle"),
        ("07-002", "contains a DisplayDocTitle entry with a"),
        ("11-002", "Natural language for text in Alt, ActualText and"),
        ("13-004", "alternative or replacement text missing"),
        ("14-002", "Does use numbered headings, but the first"),
        ("14-003", "Numbered heading levels in descending"),
        ("14-006", "A node contains more than one <H> tag"),
        ("14-007", "Document uses both <H> and <H#> tags"),
        ("17-002", "<Formula> tag is missing an Alt attribute"),
        ("28-005", "A form field does not have a TU entry and does not"),
        ("02-001", "One or more non-standard tag"),
        ("02-003", "A circular mapping exists"),
        ("02-004", "One or more standard types are remapped"),
        ("11-003", "Natural language in the Outline entries"),
        ("11-004", "Natural language in the Contents entry for"),
        ("11-005", "Natural language in the TU entry for form"),
        ("15-003", "In a table not organized with Headers"),
        ("19-003", "ID entry of the <Note> tag is not present"),
        ("19-004", "ID entry of the <Note> tag is non-unique"),
        ("20-001", "Name entry is missing or has an empty string"),
        ("20-002", "Name entry is missing or has an empty string"),
        ("20-003", "An AS entry appears in an Optional Content"),
        ("28-004", "An annotation, other than of subtype Widget,"),
        ("28-007", "An annotation of subtype TrapNet exists"),
        ("28-008", "A page containing an annotation does not"),
        ("28-009", "A page containing an annotation has a Tabs"),
        ("28-012", "A link annotation does not include an"),
        ("30-001", "A reference XObject is present"),
        ("31-004", "A Type 2 CID font contains neither a stream nor"),
        ("31-019", "The font dictionary for a non-symbolic TrueType"),
        ("31-020", "The font dictionary for a non-symbolic TrueType"),
        ("31-021", "The value for either the Encoding entry or the"),
        ("31-022", "The Differences array in the Encoding entry in a"),
        ("31-023", "The Differences array is present in the Encoding"),
        ("31-027", "A font dictionary does not contain the ToUnicode"),
        ("31-024", "The Encoding entry is present in the font"),
        ("31-025", "The embedded font program for a symbolic"),
        ("31-026", "The embedded font program for a symbolic"),
        ("31-028", "One or more Unicode values specified in the"),
        ("31-029", "One or more Unicode values specified in the"),
        ("28-002", "An annotation, other than of subtype Widget,"),
        ("28-010", "A widget annotation is not nested within a"),
        ("28-011", "A link annotation is not nested within a"),
        ("28-017", "A PrinterMark annotation is included in the"),
        ("09-004", "A table-related structure element is used in a"),
        ("09-005", "A list-related structure element is used in a way"),
        ("09-006", "A TOC-related structure element is used in a way"),
        ("09-007", "A Ruby-related structure element is used in a way"),
        ("09-008", "A Warichu-related structure element is used in"),
        ("21-001", "The file specification dictionary for an"),
        ("25-001", "File contains the dynamicRender element with"),
        ("26-001", "The file is encrypted but does not contain a P"),
        ("26-002", "The file is encrypted and does contain a P entry"),
        ("28-014", "CT entry is missing from the media clip data"),
        ("28-015", "Alt entry is missing from the media clip data"),
        ("28-016", "File attachment annotations do not conform"),
        ("30-002", "Form XObject contains MCIDs and is referenced"),
        ("31-009", "For a font used by text intended to be rendered"),
        ("31-017", "A non-symbolic TrueType font is used for"),
        ("11-006", "Natural language for document metadata cannot"),
        ("28-006", "An annotation with subtype undefined in ISO"),
        ("28-018", "The appearance stream of a PrinterMark"),
        ("31-018", "A non-symbolic TrueType font is used for rendering, but"),
        ("31-012", "font contains a CharSet string, but at least"),
        ("31-013", "font contains a CharSet string, but at least"),
        ("31-014", "font contains a CIDSet string, but at least one"),
        ("31-015", "font contains a CIDSet string, but at least"),
    ];
    assert_eq!(
        says.len(),
        MatterhornAuditor::CHECKED.len(),
        "a failure condition was added to the auditor and not to this list"
    );

    for (number, words) in says {
        assert!(
            MatterhornAuditor::CHECKED.contains(&number),
            "{number} is checked for here and the auditor does not name it"
        );
        let rows = index_rows(&text, number);
        assert!(!rows.is_empty(), "the protocol has no table row for {number}");
        assert!(
            rows.iter().any(|row| row.chars().take(160).collect::<String>().contains(words)),
            "the protocol says {number} is something else: {:?}",
            rows.iter().map(|row| row.chars().take(160).collect::<String>()).collect::<Vec<_>>()
        );
    }

    // And what it is a protocol for, which is the reason none of this measures UA-2.
    assert!(
        text.contains("specified in PDF/UA-1"),
        "the protocol no longer says which standard it is about"
    );
}

/// **The protocol's tables enumerate 137 failure conditions and its prose says 136.**
///
/// `IN_PROTOCOL` is the denominator of every "N of M checked" this engine prints, so what
/// it counts has to be the conditions that exist rather than a sentence about them. The
/// sentence is version 1.02's: 1.1's own Document History records "Failure condition
/// 13-008 added", 13-008 is marked `H`, and counting the `How` column gives 87 `M` and 48
/// `H` beside the 2 with no test — one more `H` than the sentence's 47.
///
/// The prose is asserted too, so that an edition correcting the sentence is noticed here
/// rather than silently agreed with.
#[test]
fn every_failure_condition_in_the_protocol_is_counted() {
    let text = protocol_text();

    let numbers: BTreeSet<&str> = text
        .lines()
        .filter_map(|line| line.get(..6))
        .filter(|head| {
            let bytes = head.as_bytes();
            bytes[..2].iter().all(u8::is_ascii_digit)
                && bytes[2] == b'-'
                && bytes[3..].iter().all(u8::is_ascii_digit)
        })
        .collect();

    // **Contiguous from 001, checkpoint by checkpoint**, which is what makes counting
    // distinct numbers a count of the conditions rather than of whatever matched: a gap
    // would mean the extraction lost a row, and a stray match would show as a checkpoint
    // whose highest index exceeds how many it has.
    let mut counted = 0;
    for checkpoint in 1..=31 {
        let here: Vec<&str> = numbers
            .iter()
            .copied()
            .filter(|n| n.starts_with(&format!("{checkpoint:02}-")))
            .collect();
        assert!(!here.is_empty(), "checkpoint {checkpoint:02} has no failure conditions");
        for (index, number) in here.iter().enumerate() {
            assert_eq!(
                *number,
                format!("{checkpoint:02}-{:03}", index + 1),
                "checkpoint {checkpoint:02} is not numbered from 001 without a gap: {here:?}"
            );
        }
        counted += here.len();
    }
    assert_eq!(numbers.len(), counted, "a number matched outside checkpoints 01 to 31");

    assert_eq!(counted, 137, "the protocol's tables no longer enumerate what this counts");
    assert_eq!(MatterhornAuditor::IN_PROTOCOL, counted, "the stated total drifted from the tables");

    // The sentence that disagrees, and the change that explains it.
    assert!(
        text.contains("136 failure conditions"),
        "the protocol's prose no longer says 136, so the reason IN_PROTOCOL disagrees with \
         it has gone and the record explaining it needs rereading"
    );
    assert!(
        text.contains("Failure condition 13-008 added"),
        "the Document History no longer records the condition version 1.1 added, which is \
         the whole of why the tables and the prose differ by one"
    );
}

/// **Each condition W-21i added comes out sound where the document meets it** — a
/// mapped custom tag, a table whose header states its scope, notes with distinct IDs,
/// named configurations, described annotations on a page with `/Tabs /S` — so none of them
/// fires on the mere presence of what it is about.
#[test]
fn the_second_pass_comes_out_sound_where_it_is_met() {
    let doc = opened(
        fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 4 0 R /Lang (en) \
               /MarkInfo << /Marked true >> /Outlines 10 0 R /AcroForm << /Fields [12 0 R] >> \
               /OCProperties << /OCGs [13 0 R] /D << /Name (Default) >> \
               /Configs [<< /Name (Print) >>] >> >>"
                .to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Tabs /S \
               /Annots [14 0 R 15 0 R] /Contents 37 0 R \
               /Resources << /Font << /F1 28 0 R /F2 33 0 R /F3 38 0 R >> >> >>"
                .to_string(),
            "<< /Type /StructTreeRoot /K [5 0 R 6 0 R 7 0 R 8 0 R 16 0 R 19 0 R 21 0 R 24 0 R \
               31 0 R 32 0 R] /RoleMap << /Custom /P >> \
               /ParentTree << /Nums [0 31 0 R 1 32 0 R] >> >>"
                .to_string(),
            "<< /Type /StructElem /S /Custom /P 4 0 R >>".to_string(),
            "<< /Type /StructElem /S /Note /P 4 0 R /ID (n1) >>".to_string(),
            "<< /Type /StructElem /S /Note /P 4 0 R /ID (n2) >>".to_string(),
            "<< /Type /StructElem /S /Table /P 4 0 R /K [9 0 R] >>".to_string(),
            "<< /Type /StructElem /S /TR /P 8 0 R /K [29 0 R] >>".to_string(),
            "<< /Type /Outlines /First 11 0 R /Last 11 0 R /Count 1 >>".to_string(),
            "<< /Title (Chapter one) /Parent 10 0 R >>".to_string(),
            "<< /FT /Tx /T (Name) /TU (Your name) >>".to_string(),
            "<< /Type /OCG /Name (Layer) >>".to_string(),
            "<< /Type /Annot /Subtype /Link /Rect [0 0 10 10] /Contents (Go to chapter one) \
               /StructParent 0 >>"
                .to_string(),
            "<< /Type /Annot /Subtype /Text /Rect [0 0 10 10] /Contents (A note) /StructParent 1 >>"
                .to_string(),
            // 16: a list, an item, its label and body (Table 336).
            "<< /Type /StructElem /S /L /P 4 0 R /K [17 0 R] >>".to_string(),
            "<< /Type /StructElem /S /LI /P 16 0 R /K [18 0 R 30 0 R] >>".to_string(),
            "<< /Type /StructElem /S /Lbl /P 17 0 R >>".to_string(),
            // 19: a table of contents and its item (Table 333).
            "<< /Type /StructElem /S /TOC /P 4 0 R /K [20 0 R] >>".to_string(),
            "<< /Type /StructElem /S /TOCI /P 19 0 R >>".to_string(),
            // 21: ruby, base then annotation text (Table 339).
            "<< /Type /StructElem /S /Ruby /P 4 0 R /K [22 0 R 23 0 R] >>".to_string(),
            "<< /Type /StructElem /S /RB /P 21 0 R >>".to_string(),
            "<< /Type /StructElem /S /RT /P 21 0 R >>".to_string(),
            // 24: warichu, punctuation either side of the text.
            "<< /Type /StructElem /S /Warichu /P 4 0 R /K [25 0 R 26 0 R 27 0 R] >>".to_string(),
            "<< /Type /StructElem /S /WP /P 24 0 R >>".to_string(),
            "<< /Type /StructElem /S /WT /P 24 0 R >>".to_string(),
            "<< /Type /StructElem /S /WP /P 24 0 R >>".to_string(),
            // 28: a Type 0 font on a CMap Table 118 lists.
            "<< /Type /Font /Subtype /Type0 /BaseFont /Listed /Encoding /Identity-H \
               /DescendantFonts [<< /Type /Font /Subtype /CIDFontType2 /BaseFont /Listed \
               /CIDSystemInfo << /Registry (Adobe) /Ordering (Japan1) /Supplement 6 >> >>] >>"
                .to_string(),
            "<< /Type /StructElem /S /TH /P 9 0 R /A << /O /Table /Scope /Column >> >>".to_string(),
            "<< /Type /StructElem /S /LBody /P 17 0 R >>".to_string(),
            // 31, 32: the elements the link and the note belong to.
            "<< /Type /StructElem /S /Link /P 4 0 R /K [<< /Type /OBJR /Obj 14 0 R >>] >>"
                .to_string(),
            "<< /Type /StructElem /S /Annot /P 4 0 R /K [<< /Type /OBJR /Obj 15 0 R >>] >>"
                .to_string(),
            // 33: a non-symbolic TrueType font as UA-1 wants one, with a sound /ToUnicode.
            "<< /Type /Font /Subtype /TrueType /BaseFont /Sound /FontDescriptor 34 0 R \
               /Encoding << /BaseEncoding /WinAnsiEncoding /Differences [65 /A] >> \
               /ToUnicode 36 0 R >>"
                .to_string(),
            "<< /Type /FontDescriptor /FontName /Sound /Flags 32 /FontFile2 35 0 R >>".to_string(),
            hex_stream("", "000100000001001000000000636d6170000000000000001c0000001400000001000300010000000c0000000000000000"),
            {
                let map = "/CIDInit /ProcSet findresource begin 12 dict begin begincmap \
                    1 begincodespacerange <00> <FF> endcodespacerange \
                    1 beginbfchar <41> <0041> endbfchar endcmap end end";
                format!("<< /Length {} >>\nstream\n{map}\nendstream", map.len())
            },
            // 37: text in a Type 1 font whose one renamed glyph is in Adobe's list.
            {
                let content = "BT /F3 12 Tf 20 100 Td (AB) Tj ET";
                format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len())
            },
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica \
               /Encoding << /Differences [65 /eacute] >> >>"
                .to_string(),
        ])
        .into_iter()
        .collect(),
    );
    let report = doc.audit_ua2_report().expect("it audits");
    for condition in [
        "02-001", "02-003", "02-004", "11-003", "11-004", "11-005", "15-003", "19-003", "19-004",
        "20-001", "20-002", "20-003", "28-004", "28-007", "28-008", "28-009", "28-012", "30-001",
        "09-004", "09-005", "09-006", "09-007", "09-008", "28-002", "28-010", "28-011", "28-017",
        "31-004", "31-019", "31-020", "31-021", "31-023", "31-024", "31-025", "31-026", "31-028",
        "31-029", "31-022", "31-027",
    ] {
        assert_eq!(
            outcomes(&report, condition),
            vec![Outcome::Sound],
            "{condition} did not come out sound on a document that meets it: {:?}",
            report
                .findings
                .iter()
                .filter(|f| f.checkpoint == condition)
                .map(|f| &f.message)
                .collect::<Vec<_>>()
        );
    }
}

/// **31-006 and 31-008 are left out, because ingestion answers them.**
///
/// Both are about the CMap a Type 0 font names: one in ISO 32000-1's Table 118 or
/// embedded (31-006), and an embedded one using no other (31-008). `refine::font` rewrites
/// every Type 0 font's `/Encoding` to `Identity-H` or `Identity-V` as it reads the file,
/// so the document this auditor reads names a listed CMap whatever the file said — and
/// both conditions would come out sound for every file this engine opens. It is checkpoint
/// 06's position ([ADR-0094](../../../docs/adr/0094-the-auditor-reads-the-ingested-document-so-ingestion-answers-checkpoint-06.md)),
/// and this test holds the reason to the code: were ingestion to stop rewriting, the
/// assertion on `/Encoding` fails and the two can be checked.
#[test]
fn the_cmap_conditions_are_left_out_because_ingestion_answers_them() {
    let doc = opened(
        fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] \
               /Resources << /Font << /F1 4 0 R >> >> >>",
            "<< /Type /Font /Subtype /Type0 /BaseFont /Madeup /Encoding /Made-Up-H \
               /DescendantFonts [] >>",
        ])
        .into_iter()
        .collect(),
    );
    let arena = doc.inner().arena();
    let font = arena
        .get_object(fepdf::Handle::new(4))
        .and_then(|o| o.as_dict_handle())
        .expect("the font is there");
    let encoding = arena
        .dict_entry(font, arena.name("Encoding"))
        .and_then(|e| e.as_name())
        .and_then(|n| arena.get_name(n))
        .map(|n| n.as_str().to_string());
    assert_eq!(
        encoding.as_deref(),
        Some("Identity-H"),
        "ingestion no longer rewrites a Type 0 font's CMap, so 31-006 and 31-008 can be checked"
    );
    for condition in ["31-006", "31-008"] {
        assert!(
            !MatterhornAuditor::CHECKED.contains(&condition),
            "{condition} is checked on a document whose CMaps ingestion has already rewritten"
        );
    }
}

/// **A Type 1 font is judged by the glyphs its text shows.** `/madeup` is in neither
/// Adobe's list nor the Symbol font: a page showing it breaks 31-027, and a page showing
/// only `B` — StandardEncoding's `B`, which the list has — does not, from the same font.
#[test]
fn a_type_1_font_is_judged_by_the_glyphs_its_text_shows() {
    let showing = |text: &str| {
        let content = format!("BT /F1 12 Tf 20 100 Td ({text}) Tj ET");
        opened(
            fepdf_fixtures::assemble(&[
                "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
                "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R \
                   /Resources << /Font << /F1 5 0 R >> >> >>"
                    .to_string(),
                format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
                "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica \
                   /Encoding << /Differences [65 /madeup] >> >>"
                    .to_string(),
            ])
            .into_iter()
            .collect(),
        )
        .audit_ua2_report()
        .expect("it audits")
    };
    assert_eq!(outcomes(&showing("A"), "31-027"), vec![Outcome::Broken]);
    assert_eq!(outcomes(&showing("B"), "31-027"), vec![Outcome::Sound]);
}

/// **Each condition W-21m added comes out sound where the document meets it**: an
/// embedded file named both ways, a static XFA form, an encryption that lets assistive
/// technology in, a described media clip, an attached file named both ways, a marked form
/// drawn once, and text rendered only in fonts that are embedded — Helvetica is shown too,
/// in mode 3, which 7.21.4.1 NOTE 2 exempts.
#[test]
fn the_files_come_out_sound_where_they_are_met() {
    let stream = |extra: &str, content: &str| {
        format!("<< {extra} /Length {} >>\nstream\n{content}\nendstream", content.len())
    };
    let doc = opened(
        fepdf_fixtures::Pdf::new().trailer_entries(&encrypt_entry(Some(-4))).assemble(&[
            "<< /Type /Catalog /Pages 2 0 R /Names << /EmbeddedFiles << /Names [(a.txt) 4 0 R] >> >> \
               /AcroForm << /Fields [] /XFA 5 0 R >> \
               /OpenAction << /S /Rendition /R << /S /MR /C 6 0 R >> >> >>"
                .to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 7 0 R \
               /Resources << /XObject << /X 8 0 R >> /Font << /T 9 0 R /U 10 0 R >> >> \
               /Annots [11 0 R] >>"
                .to_string(),
            "<< /Type /Filespec /F (a.txt) /UF (a.txt) /EF << /F 12 0 R >> >>".to_string(),
            stream(
                "",
                "<xdp:xdp><config><acrobat><acrobat7><dynamicRender>interactive</dynamicRender>\
                 </acrobat7></acrobat></config></xdp:xdp>",
            ),
            "<< /Type /MediaClip /S /MCD /D (clip.mp4) /CT (video/mp4) /Alt [() (A clip)] >>"
                .to_string(),
            stream("", "/X Do BT 3 Tr /T 12 Tf 20 100 Td (A) Tj ET BT /U 12 Tf 20 80 Td (A) Tj ET"),
            stream("/Type /XObject /Subtype /Form /BBox [0 0 10 10]", "/P <</MCID 0>> BDC 0 0 5 5 re f EMC"),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_string(),
            "<< /Type /Font /Subtype /TrueType /BaseFont /LatinCmap /FontDescriptor 13 0 R \
               /Encoding /WinAnsiEncoding >>"
                .to_string(),
            "<< /Type /Annot /Subtype /FileAttachment /Rect [0 0 10 10] /Contents (b) \
               /FS << /Type /Filespec /F (b.txt) /UF (b.txt) /EF << /F 12 0 R >> >> >>"
                .to_string(),
            stream("/Type /EmbeddedFile", "hello"),
            "<< /Type /FontDescriptor /FontName /LatinCmap /Flags 32 /FontFile2 14 0 R >>".to_string(),
            hex_stream("", ONLY_A_BY_UNICODE),
        ]),
    );
    let report = doc.audit_ua2_report().expect("it audits");
    let mut conditions: Vec<&str> = FROM_FILES.to_vec();
    conditions.extend(["31-009", "31-017", "31-018"]);
    for condition in conditions {
        assert_eq!(
            outcomes(&report, condition),
            vec![Outcome::Sound],
            "{condition} did not come out sound on a document that meets it: {:?}",
            report
                .findings
                .iter()
                .filter(|f| f.checkpoint == condition)
                .map(|f| &f.message)
                .collect::<Vec<_>>()
        );
    }
}

/// A page whose `/X` form draws `form` and whose own content is `content`, both able to
/// show text in `/F1`, Helvetica, whose program is not embedded.
fn helvetica_page(content: &str, form: &str) -> AuditReport {
    let stream = |extra: &str, content: &str| {
        format!("<< {extra} /Length {} >>\nstream\n{content}\nendstream", content.len())
    };
    opened(
        fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R \
               /Resources << /Font << /F1 5 0 R >> /XObject << /X 6 0 R >> >> >>"
                .to_string(),
            stream("", content),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
                .to_string(),
            stream(
                "/Type /XObject /Subtype /Form /BBox [0 0 10 10] \
                 /Resources << /Font << /F1 5 0 R >> >>",
                form,
            ),
        ])
        .into_iter()
        .collect(),
    )
    .audit_ua2_report()
    .expect("it audits")
}

/// **A font is used for rendering unless its every glyph is shown in mode 3.** ISO
/// 14289-1 7.21.4.1 NOTE 2 exempts mode 3 alone, whose glyphs are neither painted nor
/// clip; mode 7 clips with them, so it renders. A `q`…`Q` restores the mode it saved.
#[test]
fn only_invisible_text_needs_no_embedded_program() {
    let text = "BT /F1 12 Tf 20 100 Td (A) Tj ET";
    for (content, outcome) in [
        (text.to_string(), Outcome::Broken),
        (format!("3 Tr {text}"), Outcome::Sound),
        (format!("7 Tr {text}"), Outcome::Broken),
        (format!("3 Tr q 0 Tr Q {text}"), Outcome::Sound),
        (format!("q 3 Tr Q {text}"), Outcome::Broken),
    ] {
        assert_eq!(outcomes(&helvetica_page(&content, ""), "31-009"), vec![outcome], "{content}");
    }
}

/// **Text a form draws is text the page draws.** The same Helvetica text, in a form the
/// page draws, breaks 31-009; in a form the page never draws, it does not — and the mode
/// the page is in when it draws the form is the one the form's text is shown in.
#[test]
fn the_text_a_form_draws_is_read() {
    let text = "BT /F1 12 Tf 20 100 Td (A) Tj ET";
    assert_eq!(outcomes(&helvetica_page("/X Do", text), "31-009"), vec![Outcome::Broken]);
    assert_eq!(outcomes(&helvetica_page("", text), "31-009"), vec![Outcome::Sound]);
    assert_eq!(outcomes(&helvetica_page("3 Tr /X Do", text), "31-009"), vec![Outcome::Sound]);
}

/// A page with `annotations` as objects 4 onwards, in no structure element and with no
/// `/Contents`, on a crop box of 0 0 100 100; and whether each came out broken under
/// 28-002 and 28-004.
fn annotation_page(annotations: &[&str]) -> AuditReport {
    let kids: Vec<String> = (0..annotations.len()).map(|i| format!("{} 0 R", i + 4)).collect();
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R /Lang (en) >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /CropBox [0 0 100 100] \
               /Tabs /S /Annots [{}] >>",
            kids.join(" ")
        ),
    ];
    objects.extend(annotations.iter().map(|a| (*a).to_string()));
    opened(fepdf_fixtures::assemble(&objects)).audit_ua2_report().expect("it audits")
}

/// **7.18.1 does not apply to a pop-up, a hidden annotation, or one outside the crop box**,
/// so none of those is asked 28-002 or 28-004 — and the same annotation inside the crop
/// box, shown, and of another subtype is.
#[test]
fn what_7_18_1_does_not_apply_to_is_not_asked_it() {
    for (annotation, asked) in [
        ("<< /Type /Annot /Subtype /Text /Rect [10 10 20 20] >>", true),
        ("<< /Type /Annot /Subtype /Popup /Rect [10 10 20 20] >>", false),
        ("<< /Type /Annot /Subtype /Text /Rect [10 10 20 20] /F 2 >>", false),
        ("<< /Type /Annot /Subtype /Text /Rect [10 10 20 20] /F 4 >>", true),
        ("<< /Type /Annot /Subtype /Text /Rect [150 150 160 160] >>", false),
        ("<< /Type /Annot /Subtype /Text /Rect [95 95 160 160] >>", true),
    ] {
        let report = annotation_page(&[annotation]);
        let outcome = if asked { Outcome::Broken } else { Outcome::Sound };
        for condition in ["28-002", "28-004"] {
            assert_eq!(outcomes(&report, condition), vec![outcome], "{condition}: {annotation}");
        }
    }
}

/// **28-006 is about the subtypes ISO 32000-1 does not define.** A `/Foo` annotation that
/// breaks 7.18.1 breaks it; a `/Text` one breaking it the same way does not, and neither
/// does a `/Foo` annotation meeting it.
#[test]
fn an_undefined_subtype_is_held_to_7_18_1() {
    let foo = "<< /Type /Annot /Subtype /Foo /Rect [10 10 20 20] >>";
    let text = "<< /Type /Annot /Subtype /Text /Rect [10 10 20 20] >>";
    let hidden_foo = "<< /Type /Annot /Subtype /Foo /Rect [10 10 20 20] /F 2 >>";
    assert_eq!(outcomes(&annotation_page(&[foo]), "28-006"), vec![Outcome::Broken]);
    assert_eq!(outcomes(&annotation_page(&[text]), "28-006"), vec![Outcome::Sound]);
    assert_eq!(outcomes(&annotation_page(&[hidden_foo]), "28-006"), vec![Outcome::Sound]);
}

/// **A printer's mark whose appearance is all `/Artifact` meets 28-018**, and one that
/// paints a single rectangle outside it — or inside a sequence that is not one — does not.
#[test]
fn a_printer_marks_appearance_is_an_artifact() {
    let with_appearance = |content: &str| {
        annotation_page(&[
            "<< /Type /Annot /Subtype /PrinterMark /Rect [10 10 20 20] /AP << /N 5 0 R >> >>",
            &format!(
                "<< /Type /XObject /Subtype /Form /BBox [0 0 10 10] /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            ),
        ])
    };
    assert_eq!(
        outcomes(&with_appearance("/Artifact BMC 0 0 5 5 re f EMC"), "28-018"),
        vec![Outcome::Sound]
    );
    assert_eq!(
        outcomes(&with_appearance("/Artifact BMC 0 0 5 5 re f EMC 0 0 1 1 re f"), "28-018"),
        vec![Outcome::Broken]
    );
    // Under a sequence that is not an artefact is under none; inside one that is, is.
    assert_eq!(
        outcomes(&with_appearance("/Span BMC 0 0 1 1 re f EMC"), "28-018"),
        vec![Outcome::Broken]
    );
    assert_eq!(
        outcomes(&with_appearance("/Artifact BMC /Span BMC 0 0 1 1 re f EMC EMC"), "28-018"),
        vec![Outcome::Sound]
    );
}

/// **11-006 is left for a reader when the catalogue states no `/Lang`, because ingestion
/// rewrites the packet that could have said.** The fixture's `dc:title` is in English by
/// its own `xml:lang`; the packet the ingested document carries has lost that, and the day
/// it keeps it, this fails and 11-006 can be decided from the packet.
#[test]
fn the_metadata_language_is_left_for_a_reader_without_a_lang() {
    let packet = r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?><x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title><rdf:Alt><rdf:li xml:lang="en">A title</rdf:li></rdf:Alt></dc:title></rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end="r"?>"#;
    let doc = opened(fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /Metadata 4 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>".to_string(),
        format!(
            "<< /Type /Metadata /Subtype /XML /Length {} >>\nstream\n{packet}\nendstream",
            packet.len()
        ),
    ]));
    let arena = doc.inner().arena();
    let catalogue = doc
        .inner()
        .catalog_handle()
        .and_then(|c| arena.get_object(c))
        .and_then(|c| c.as_dict_handle())
        .expect("a catalogue");
    let stream = arena.dict_entry(catalogue, arena.name("Metadata")).expect("a packet");
    let read = doc.inner().decode_stream(&stream.resolve(arena)).expect("it decodes");
    assert!(
        !String::from_utf8_lossy(&read).contains(r#"xml:lang="en""#),
        "ingestion kept the packet's own language, so 11-006 can be decided from it"
    );
    assert_eq!(
        outcomes(&doc.audit_ua2_report().expect("it audits"), "11-006"),
        vec![Outcome::ForAReader]
    );
    assert_eq!(outcomes(&annotation_page(&[]), "11-006"), vec![Outcome::Sound]);
}

/// A page showing `text` in a non-symbolic TrueType font whose `/Encoding` is `encoding`
/// and whose program is `program`, as hexadecimal.
fn true_type_page(encoding: &str, program: &str, text: &str) -> AuditReport {
    let content = format!("BT /F1 12 Tf 20 100 Td {text} Tj ET");
    opened(fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R \
           /Resources << /Font << /F1 5 0 R >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        format!(
            "<< /Type /Font /Subtype /TrueType /BaseFont /Test /FontDescriptor 6 0 R \
               /Encoding {encoding} >>"
        ),
        "<< /Type /FontDescriptor /FontName /Test /Flags 32 /FontFile2 7 0 R >>".to_string(),
        hex_stream("", program),
    ]))
    .audit_ua2_report()
    .expect("it audits")
}

/// **31-018 follows 9.6.6.4 and nothing else.** Through (3,1), a code is its name's
/// Unicode value: `A` reaches glyph 1 and `B` reaches nothing, and so does `A` renamed by
/// `/Differences` to a name Adobe's list does not have — while `B` renamed `A` reaches
/// `A`'s glyph. Text in mode 3 is not rendered and is not asked.
#[test]
fn a_true_type_code_is_looked_up_through_the_unicode_cmap_by_name() {
    let win = "/WinAnsiEncoding";
    let renamed = "<< /BaseEncoding /WinAnsiEncoding /Differences [65 /madeup] >>";
    let as_a = "<< /BaseEncoding /WinAnsiEncoding /Differences [66 /A] >>";
    for (encoding, text, outcome) in [
        (win, "(A)", Outcome::Sound),
        (win, "(B)", Outcome::Broken),
        (renamed, "(A)", Outcome::Broken),
        (as_a, "(B)", Outcome::Sound),
        (win, "3 Tr (B)", Outcome::Sound),
    ] {
        let report = true_type_page(encoding, ONLY_A_BY_UNICODE, text);
        assert_eq!(outcomes(&report, "31-018"), vec![outcome], "{encoding} {text}");
    }
}

/// **Without a (3,1) subtable, the name goes to (1,0) by its Mac OS Roman code**, which is
/// MacRomanEncoding with Table 115's differences: WinAnsi's 0x80 is `Euro`, and `Euro` is
/// 219 there, where MacRomanEncoding's 219 is `currency`, which Mac OS Roman does not have.
#[test]
fn a_true_type_code_is_looked_up_through_the_mac_cmap_by_mac_os_roman() {
    for (encoding, text, outcome) in [
        ("/WinAnsiEncoding", "(A)", Outcome::Sound),
        ("/WinAnsiEncoding", "<80>", Outcome::Sound),
        ("/MacRomanEncoding", "<DB>", Outcome::Broken),
    ] {
        let report = true_type_page(encoding, A_AND_EURO_BY_MAC_ROMAN, text);
        assert_eq!(outcomes(&report, "31-018"), vec![outcome], "{encoding} {text}");
    }
}

/// A raw Type 1 program: 60 cleartext bytes, then `/CharStrings` for `.notdef`, `A` and
/// `B` under eexec.
const TYPE_1_A_B: &str = "2521466f6e7454797065312d312e303a20546573740a2f466f6e744e616d65202f54657374206465660a63757272656e7466696c652065657865630ad9d66f633b846a989b9974b0179fc6cc4452954d3a4fc272596999ba876cc6961876a36a3e0691600d27978f3466dac5e6c0328e74b316219e55bef8b40bfa7097977a0ae48a5588a9eb85fa1944834b491163b221bbd8dcd4f7c18ab788697941010806744caceebc5ad9be2c2b7fab5e09aca140d90d8d8444595afff597d945510082af7873f1a9d72848220a";

/// A name-keyed CFF program: `.notdef`, then `A` and `B` by their standard SIDs.
const CFF_A_B: &str = "01000401000101010554657374000101010d1d000000220f1d0000002711000000000000220023000301010203040e0e0e";

/// An SFNT program of three glyph slots, of which only glyph 1 has an outline.
const THREE_SLOTS: &str = "000100000004004000020000676c7966000000000000004c0000000a686561640000000000000058000000366c6f63610000000000000090000000086d6178700000000000000098000000060000000000000000000000000001000000000000000000005f0f3cf5000003e800000000000000000000000000000000000000000000000000000000000000000000000000000000000500050000500000030000";

/// Four embedded fonts, each with the claim `claims` gives it: a Type 1 font and a
/// `/Type1C` one, each holding `A` and `B`, under a `/CharSet`; a TrueType CIDFont whose
/// program has glyph 1 only outlined, and a CFF one holding CIDs 1 and 2, under a
/// `/CIDSet` given as hexadecimal.
fn subsets(claims: [&str; 4]) -> Vec<u8> {
    let [type_1, cff, true_type, cid_cff] = claims;
    let cid_font = |subtype: &str, descriptor: usize| {
        format!(
            "<< /Type /Font /Subtype /Type0 /BaseFont /Cid /Encoding /Identity-H \
               /DescendantFonts [<< /Type /Font /Subtype /{subtype} /BaseFont /Cid \
               /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> \
               /FontDescriptor {descriptor} 0 R /CIDToGIDMap /Identity >>] >>"
        )
    };
    fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] \
           /Resources << /Font << /A 4 0 R /B 7 0 R /C 10 0 R /D 14 0 R >> >> >>"
            .to_string(),
        // 4: Type 1, `/FontFile`.
        "<< /Type /Font /Subtype /Type1 /BaseFont /T1 /FontDescriptor 5 0 R >>".to_string(),
        format!(
            "<< /Type /FontDescriptor /FontName /T1 /Flags 32 /CharSet {type_1} /FontFile 6 0 R >>"
        ),
        hex_stream("/Length1 60 /Length2 142 /Length3 0", TYPE_1_A_B),
        // 7: Type 1, `/FontFile3 /Type1C`.
        "<< /Type /Font /Subtype /Type1 /BaseFont /C1 /FontDescriptor 8 0 R >>".to_string(),
        format!(
            "<< /Type /FontDescriptor /FontName /C1 /Flags 32 /CharSet {cff} /FontFile3 9 0 R >>"
        ),
        hex_stream("/Subtype /Type1C", CFF_A_B),
        // 10: a TrueType CIDFont.
        cid_font("CIDFontType2", 11),
        "<< /Type /FontDescriptor /FontName /Cid /Flags 4 /FontFile2 12 0 R /CIDSet 13 0 R >>"
            .to_string(),
        hex_stream("", THREE_SLOTS),
        hex_stream("", true_type),
        // 14: a CFF CIDFont.
        cid_font("CIDFontType0", 15),
        "<< /Type /FontDescriptor /FontName /Cid /Flags 4 /FontFile3 16 0 R /CIDSet 17 0 R >>"
            .to_string(),
        hex_stream("/Subtype /CIDFontType0C", CFF_A_B),
        hex_stream("", cid_cff),
    ])
}

/// **A `/CharSet` or `/CIDSet` that says what its program holds meets 31-012 to 31-015**,
/// and each claim that says more or less breaks the one condition about that direction.
/// The TrueType CIDFont's empty glyph 2 may be listed or not: a slot is not evidence
/// either way.
#[test]
fn a_subsets_claim_is_held_to_its_program() {
    let sound = ["(/A/B)", "(/B/A)", "40", "60"];
    let also_sound = ["(/A /B)", "(/A/B)", "60", "60"];
    for claims in [sound, also_sound] {
        let report = opened(subsets(claims)).audit_ua2_report().expect("it audits");
        for condition in ["31-012", "31-013", "31-014", "31-015"] {
            assert_eq!(
                outcomes(&report, condition),
                vec![Outcome::Sound],
                "{condition}: {claims:?}"
            );
        }
    }
    for (claims, condition, times) in [
        (["(/A)", "(/A/B)", "40", "60"], "31-012", 1),
        (["(/A/B)", "(/A)", "40", "60"], "31-012", 1),
        (["(/A/B/C)", "(/A/B)", "40", "60"], "31-013", 1),
        (["(/A/B)", "(/A/B)", "00", "20"], "31-014", 2),
        (["(/A/B)", "(/A/B)", "44", "64"], "31-015", 2),
    ] {
        let report = opened(subsets(claims)).audit_ua2_report().expect("it audits");
        assert_eq!(outcomes(&report, condition), vec![Outcome::Broken; times], "{claims:?}");
    }
}

//! **A form's values out to FDF and XFDF and back in, by name** (ROADMAP AA-3b,
//! ADR-0125): values set as filling the form sets them, flags as Table 249 says, and
//! nothing made that the form does not have.

use fepdf::{FormFieldSpec, FormValue, Operation, PdfDocument, PdfError};

/// A form of a text field, a nested one, a check box, a choice and a signature.
fn form() -> PdfDocument {
    let widget = "/Type /Annot /Subtype /Widget /P 3 0 R";
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R 5 0 R 7 0 R 8 0 R 9 0 R] /DA (/Helv 0 Tf 0 g) >> >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Resources << >> /Annots [4 0 R 6 0 R 7 0 R 8 0 R 9 0 R] >>".to_owned(),
        format!("<< {widget} /FT /Tx /T (name) /Rect [10 260 290 280] >>"),
        "<< /T (address) /Kids [6 0 R] >>".to_owned(),
        format!("<< {widget} /FT /Tx /T (city) /Parent 5 0 R /Rect [10 230 290 250] >>"),
        format!("<< {widget} /FT /Btn /T (agree) /AS /Off /Rect [10 200 30 220] >>"),
        format!("<< {widget} /FT /Ch /Ff 131072 /T (colour) /Opt [(red) (green)] /Rect [10 170 290 190] >>"),
        format!("<< {widget} /FT /Sig /T (sig) /V (forged) /Rect [10 140 290 160] >>"),
    ];
    let bodies: Vec<&str> = bodies.iter().map(String::as_str).collect();
    PdfDocument::open(fepdf_fixtures::assemble(&bodies).into()).expect("the form opens")
}

fn filled() -> PdfDocument {
    let mut doc = form();
    for (name, value) in [
        ("name", FormValue::Text("田中 花子".to_owned())),
        ("address.city", FormValue::Text("Kyoto".to_owned())),
        ("agree", FormValue::Boolean(true)),
        ("colour", FormValue::Choice("green".to_owned())),
    ] {
        doc.apply(Operation::SetFormFieldValue(FormFieldSpec { name: name.to_owned(), value }))
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
    }
    doc
}

/// Each terminal field's value as `form_of` reads it.
fn values(doc: &PdfDocument) -> Vec<(String, Option<String>)> {
    fepdf::form_of(doc.inner())
        .terminal
        .into_iter()
        .map(|f| (f.qualified_name.unwrap_or_default(), f.value))
        .collect()
}

fn said(doc: &PdfDocument, what: &str) -> bool {
    doc.decisions().iter().any(|d| d.found.contains(what))
}

/// **FDF out of a filled form and into an empty one gives the same values**, the nested
/// field nested under its parent and the check box by its state.
#[test]
fn form_values_go_out_as_fdf_and_come_back() {
    let fdf = filled().export_fdf_form().expect("it exports");
    let text = String::from_utf8_lossy(&fdf).into_owned();
    assert!(text.contains("/Fields"), "{text}");
    assert!(text.contains("/Kids"), "address.city is a kid of address: {text}");
    assert!(text.contains("/V /Yes"), "the check box's state: {text}");
    let mut empty = form();
    empty.apply(Operation::ImportFdf { fdf }).expect("it imports");
    let unsigned =
        |doc: &PdfDocument| values(doc).into_iter().filter(|(n, _)| n != "sig").collect::<Vec<_>>();
    assert_eq!(unsigned(&empty), unsigned(&filled()));
    // A signature's value is not exported (ISO 19444-1 6.3.3), though this one has one.
    assert!(values(&empty).iter().any(|(n, v)| n == "sig" && v.as_deref() == Some("forged")));
    // Four values out — name, city, the check box and the choice — and no fifth, though
    // the signature has one.
    assert_eq!(text.matches("/V ").count(), 4, "a signature's value went out: {text}");
}

/// **XFDF does the same by `<field>` and `<value>`** (6.3), a hierarchy by nesting.
#[test]
fn form_values_go_out_as_xfdf_and_come_back() {
    let xfdf = filled().export_xfdf_form().expect("it exports");
    assert!(
        xfdf.contains("<field name=\"address\"><field name=\"city\"><value>Kyoto</value>"),
        "{xfdf}"
    );
    assert!(xfdf.contains("<field name=\"agree\"><value>Yes</value>"), "{xfdf}");
    let mut empty = form();
    empty.apply(Operation::ImportXfdf { xfdf: xfdf.into_bytes() }).expect("it imports");
    assert_eq!(values(&empty), values(&filled()));
}

/// **Flags are set and cleared as Table 249 says**, and what is not imported — a field
/// the form lacks, an appearance, a second choice — is said.
#[test]
fn flags_apply_and_what_is_not_imported_is_said() {
    let fdf = fepdf_fixtures::assemble(&[
        "<< /FDF << /Fields [ << /T (name) /V (x) /SetFf 1 /SetF 2 /AP << >> >> << /T (missing) /V (y) >> << /T (colour) /V [(red) (green)] >> ] >> >>",
    ]);
    let fdf = String::from_utf8_lossy(&fdf).replacen("%PDF-2.0", "%FDF-1.2", 1).into_bytes();
    let mut doc = form();
    doc.apply(Operation::ImportFdf { fdf }).expect("it imports");
    let name = fepdf::form_of(doc.inner())
        .terminal
        .into_iter()
        .find(|f| f.qualified_name.as_deref() == Some("name"))
        .expect("the field");
    assert_eq!(name.flags.map(|f| f & 1), Some(1), "SetFf set read-only");
    assert_eq!(name.value.as_deref(), Some("x"));
    assert!(said(&doc, "\"missing\""), "the field the form lacks: {:?}", doc.decisions());
    assert!(said(&doc, "/AP"), "the appearance not imported");
    assert!(said(&doc, "2 choices"), "the second choice");
    let inner = doc.inner();
    let arena = inner.arena();
    let page = inner.resolve_to_dict(inner.page_handle(0).expect("page")).expect("dict");
    let first = arena
        .dict_entry(page, arena.name("Annots"))
        .and_then(|a| a.resolve(arena).as_array())
        .and_then(|a| arena.get_array(a))
        .and_then(|a| a.first().cloned())
        .and_then(|w| w.resolve(arena).as_dict_handle())
        .expect("the name field's widget");
    let f = arena.dict_entry(first, arena.name("F")).and_then(|f| f.as_integer());
    assert_eq!(f.map(|f| f & 2), Some(2), "SetF set hidden on the widget");
}

/// `sample_02c.pdf`, whose fields are named in Japanese.
fn japanese_form() -> PdfDocument {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/sample_02c.pdf");
    PdfDocument::open(std::fs::read(path).expect("the sample").into()).expect("it opens")
}

/// An FDF whose `/Encoding` is `encoding`, filling the field named `name` with `value`,
/// both given as hexadecimal bytes in that encoding.
fn encoded_fdf(encoding: &str, name: &str, value: &str) -> Vec<u8> {
    let body = format!(
        "<< /FDF << /Encoding /{encoding} /Fields [ << /T <{name}> /V <{value}> >> ] >> >>"
    );
    let fdf = fepdf_fixtures::assemble(&[body.as_str()]);
    String::from_utf8_lossy(&fdf).replacen("%PDF-2.0", "%FDF-1.2", 1).into_bytes()
}

fn value_of(doc: &PdfDocument, name: &str) -> Option<String> {
    values(doc).into_iter().find(|(n, _)| n == name).and_then(|(_, v)| v)
}

/// **Shift_JIS, as Windows writes it, is read as its `/Encoding` says** (Table 246):
/// 住所 named in Shift_JIS finds the field named in UTF-16, and the value keeps 髙 and ①,
/// which only Windows' extensions of Shift_JIS have.
#[test]
fn a_shift_jis_fdf_is_decoded() {
    let mut doc = japanese_form();
    let fdf = encoded_fdf("Shift_JIS", "8F5A8F8A", "8B9E93738E738DB68B9E8BE620EEE08BB48740");
    doc.apply(Operation::ImportFdf { fdf }).expect("it imports");
    assert_eq!(value_of(&doc, "住所").as_deref(), Some("京都市左京区 髙橋①"));
}

/// UTF-8, the other encoding Table 246 names that is not a PDF text string.
#[test]
fn a_utf_8_fdf_is_decoded() {
    let mut doc = japanese_form();
    let fdf = encoded_fdf("utf_8", "E5908DE5898DE6BCA2E5AD97", "E794B0E4B8AD");
    doc.apply(Operation::ImportFdf { fdf }).expect("it imports");
    assert_eq!(value_of(&doc, "名前漢字").as_deref(), Some("田中"));
}

/// **An encoding Table 246 does not name is refused**, before anything changes.
#[test]
fn an_unnamed_encoding_is_refused() {
    let mut doc = japanese_form();
    let before = values(&doc);
    let refused =
        doc.apply(Operation::ImportFdf { fdf: encoded_fdf("EUC-JP", "8F5A8F8A", "93639286") });
    assert!(matches!(refused, Err(PdfError::Refused { .. })), "{refused:?}");
    assert_eq!(values(&doc), before, "nothing was filled");
}

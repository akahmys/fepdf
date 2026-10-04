//! **After one redaction and a save, nothing inside the region can be recovered from the
//! file** (12.5.6.23, ROADMAP Y-10's *done when*).
//!
//! One page carries a marker string in every place the redaction reaches: its text, a
//! form's text, an image's pixels, an annotation with its popup and reply, marked content's
//! `/ActualText`, an element's `/Alt`, a text field's value merged with its widget and
//! one held by a parent with kids, a choice field's options, the form's `/XFA`, the page's
//! thumbnail, and text in an optional content group that is off. One region covers all of
//! them. The saved file is opened again and every string and every decoded stream in it
//! is searched, as text, as hexadecimal digits and as UTF-16.

use fepdf::{Operation, PdfDocument, Redaction, SaveOptions};

/// Every marker the fixture plants inside the region.
const MARKERS: [&str; 15] = [
    "MARKERTEXT",
    "MARKERFORM",
    "MARKERIMAGE",
    "MARKERANNOT",
    "MARKERREPLY",
    "MARKERACTUAL",
    "MARKERALT",
    "MARKERFIELD",
    "MARKERPARENT",
    "MARKEROPTION",
    "MARKERXFA",
    "MARKERTHUMB",
    "MARKERHIDDEN",
    "MARKERNAMED",
    "MARKERDOCUMENT",
];

/// The fixture: everything inside x 0–300, y 0–500, and KEEPME outside it.
fn everywhere() -> PdfDocument {
    let content = "/P <</MCID 0 /ActualText (MARKERACTUAL)>> BDC BT /F1 12 Tf 50 100 Td (MARKERTEXT) Tj ET EMC \
                   q 1 0 0 1 50 150 cm /Fm0 Do Q \
                   q 120 0 0 1 50 300 cm /Im0 Do Q \
                   /OC /Off BDC BT /F1 12 Tf 50 420 Td (MARKERHIDDEN) Tj ET EMC \
                   /Span /PL0 BDC BT /F1 12 Tf 50 450 Td (X) Tj ET EMC \
                   BT /F1 12 Tf 400 700 Td (KEEPME) Tj ET";
    let form = "BT /F1 12 Tf 0 0 Td (MARKERFORM) Tj ET";
    let image: String = "MARKERIMAGE".repeat(33).chars().take(360).collect();
    let thumb = "MARKERTHUMB";
    let xfa = "<field>MARKERXFA</field>";
    let objects: Vec<String> = vec![
        // 1 catalogue, 2 pages, 3 page
        "<< /Type /Catalog /Pages 2 0 R /AcroForm 20 0 R /StructTreeRoot 30 0 R \
            /MarkInfo << /Marked true >> /OCProperties << /OCGs [40 0 R] /D << /OFF [40 0 R] >> >> >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /StructParents 0 \
            /Thumb 9 0 R /Annots [10 0 R 11 0 R 12 0 R 13 0 R 15 0 R 16 0 R 17 0 R] \
            /Resources << /Font << /F1 5 0 R >> /XObject << /Fm0 6 0 R /Im0 7 0 R >> \
            /Properties << /Off 40 0 R /PL0 << /ActualText (MARKERNAMED) >> >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 200 50] /Resources << /Font << /F1 5 0 R >> >> \
                /Length {} >>\nstream\n{form}\nendstream",
            form.len()
        ),
        format!(
            "<< /Type /XObject /Subtype /Image /Width 120 /Height 1 /ColorSpace /DeviceRGB \
                /BitsPerComponent 8 /Length {} >>\nstream\n{image}\nendstream",
            image.len()
        ),
        "<< >>".to_string(),
        format!(
            "<< /Width 11 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length {} >>\nstream\n{thumb}\nendstream",
            thumb.len()
        ),
        // 10 note, 11 its popup, 12 its reply
        "<< /Type /Annot /Subtype /Text /Rect [20 20 40 40] /Contents (MARKERANNOT) /Popup 11 0 R >>".to_string(),
        "<< /Type /Annot /Subtype /Popup /Rect [400 400 500 500] /Parent 10 0 R >>".to_string(),
        "<< /Type /Annot /Subtype /Text /Rect [450 600 470 620] /IRT 10 0 R /Contents (MARKERREPLY) >>".to_string(),
        // 13 a text field merged with its widget
        "<< /Type /Annot /Subtype /Widget /Rect [50 350 200 370] /FT /Tx /T (merged) /V (MARKERFIELD) >>".to_string(),
        // 14 a parent field, 15 and 16 its widgets
        "<< /FT /Tx /T (parent) /V (MARKERPARENT) /Kids [15 0 R 16 0 R] >>".to_string(),
        "<< /Type /Annot /Subtype /Widget /Rect [50 380 200 395] /Parent 14 0 R >>".to_string(),
        "<< /Type /Annot /Subtype /Widget /Rect [50 400 200 415] /Parent 14 0 R >>".to_string(),
        // 17 a choice field
        "<< /Type /Annot /Subtype /Widget /Rect [50 470 200 490] /FT /Ch /T (pick) /Opt [(MARKEROPTION)] >>".to_string(),
        "<< >>".to_string(),
        "<< >>".to_string(),
        // 20 the form, 21 its XFA
        "<< /Fields [13 0 R 14 0 R 17 0 R] /XFA 21 0 R >>".to_string(),
        format!("<< /Length {} >>\nstream\n{xfa}\nendstream", xfa.len()),
        "<< >>".to_string(),
        "<< >>".to_string(),
        "<< >>".to_string(),
        "<< >>".to_string(),
        "<< >>".to_string(),
        "<< >>".to_string(),
        "<< >>".to_string(),
        "<< >>".to_string(),
        // 30 the structure tree's root, 31 the Document element, 32 the paragraph
        "<< /Type /StructTreeRoot /K [31 0 R] /ParentTree << /Nums [0 [32 0 R]] >> >>".to_string(),
        "<< /Type /StructElem /S /Document /P 30 0 R /K [32 0 R] /Alt (MARKERDOCUMENT) >>".to_string(),
        "<< /Type /StructElem /S /P /P 31 0 R /Pg 3 0 R /K 0 /Alt (MARKERALT) >>".to_string(),
        "<< >>".to_string(),
        "<< >>".to_string(),
        "<< >>".to_string(),
        "<< >>".to_string(),
        "<< >>".to_string(),
        "<< >>".to_string(),
        "<< >>".to_string(),
        // 40 the optional content group that is off
        "<< /Type /OCG /Name (Hidden) >>".to_string(),
    ];
    PdfDocument::open(fepdf_fixtures::assemble(&objects).into()).expect("the fixture opens")
}

/// Every string and every decoded stream the saved file holds, as bytes.
fn everything_in(file: Vec<u8>) -> Vec<Vec<u8>> {
    let doc = PdfDocument::open(file.into()).expect("the saved file opens");
    let arena = doc.inner().arena();
    let mut found = Vec::new();
    let mut strings = |value: &fepdf_model::Object| {
        if let Some(bytes) = value.as_string() {
            found.push(bytes.to_vec());
        } else if let Some(text) = value.as_text() {
            found.push(text.as_bytes().to_vec());
        }
    };
    for handle in arena.all_dict_handles() {
        arena.get_dict(handle).unwrap_or_default().values().for_each(&mut strings);
    }
    for handle in arena.all_array_handles() {
        arena.get_array(handle).unwrap_or_default().iter().for_each(&mut strings);
    }
    for index in 0..arena.object_count() {
        let Some(object) = arena.get_object(arena.handle(index)) else { continue };
        if let Ok(bytes) = doc.inner().decode_stream(&object) {
            found.push(bytes.to_vec());
        }
    }
    found
}

/// Whether `haystack` holds `marker` as text, as hexadecimal digits or as UTF-16BE.
fn holds(haystack: &[u8], marker: &str) -> bool {
    let utf16: Vec<u8> = marker.encode_utf16().flat_map(u16::to_be_bytes).collect();
    let hex: String = marker.bytes().map(|b| format!("{b:02X}")).collect::<Vec<_>>().concat();
    let upper = String::from_utf8_lossy(haystack).to_uppercase();
    haystack.windows(marker.len()).any(|w| w == marker.as_bytes())
        || haystack.windows(utf16.len()).any(|w| w == utf16)
        || upper.contains(&hex)
}

/// **Nothing the region covered is left in the file, and what lies outside it is.**
#[test]
fn nothing_inside_the_region_is_left_in_the_file() {
    let mut doc = everywhere();
    let redaction = Redaction { page: 0, regions: vec![(0.0, 0.0, 300.0, 500.0)], fill: None };
    doc.apply(Operation::Redact(redaction)).expect("it redacts");

    let path =
        std::env::temp_dir().join(format!("fepdf-redact-complete-{}.pdf", std::process::id()));
    doc.save_with_options(&path, "2.0", &SaveOptions::default()).expect("it writes");
    let file = std::fs::read(&path).expect("it is there");
    let _ = std::fs::remove_file(&path);

    let everything = everything_in(file);
    let left: Vec<&str> = MARKERS
        .iter()
        .copied()
        .filter(|m| everything.iter().any(|bytes| holds(bytes, m)))
        .collect();
    assert!(left.is_empty(), "these are still in the file: {left:?}");
    assert!(
        everything.iter().any(|bytes| holds(bytes, "KEEPME")),
        "what lies outside the region went"
    );
}

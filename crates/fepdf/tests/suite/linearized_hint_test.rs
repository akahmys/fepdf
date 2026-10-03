//! How much of a linearised file's hint stream is reserve nobody wrote into.

use fepdf::{PdfDocument, SaveOptions};

/// Forty pages, each drawing with its own font and its neighbour's, so every font but the
/// two at the ends is shared by two pages and no page names more than two of them.
fn neighbours_share_fonts() -> PdfDocument {
    const PAGES: usize = 40;
    let kids: Vec<String> = (0..PAGES).map(|i| format!("{} 0 R", 3 + i)).collect();
    let mut bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!("<< /Type /Pages /Kids [{}] /Count {PAGES} >>", kids.join(" ")),
    ];
    let font = |i: usize| 3 + PAGES + i;
    for i in 0..PAGES {
        bodies.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] \
               /Resources << /Font << /A {} 0 R /B {} 0 R >> >> >>",
            font(i),
            font(i + 1)
        ));
    }
    for i in 0..=PAGES {
        bodies.push(format!("<< /Type /Font /Subtype /Type1 /BaseFont /F{i} >>"));
    }
    PdfDocument::open(fepdf_fixtures::assemble(&bodies).into()).expect("the fixture opens")
}

/// The hint stream's `/Length`: the bytes reserved for it, written into or not.
///
/// Read as bytes: a lossy conversion to text moves every offset after the first binary
/// stream, which is how a first version of this read the wrong slice.
fn hint_length(file: &[u8]) -> usize {
    let at =
        file.windows(4).position(|w| w == b" /S ").expect("a linearised file has a hint stream");
    let length_at = file[..at]
        .windows(8)
        .rposition(|w| w == b"/Length ")
        .expect("the hint stream states its length");
    let digits = String::from_utf8_lossy(&file[length_at + 8..at]).trim().to_string();
    digits.parse().expect("a length")
}

/// **The reserve is sized from what each page names, not from every page naming every
/// shared object.** That was `pages × shared` entries, and `samples/intel_sdm.pdf`
/// linearised to 129 MB against a 25.6 MB plain save, 71 MB of it zeros (ROADMAP Y-0b).
/// Here the old sizing reserved 5,111 bytes for a table this one reserves 659 for.
#[test]
fn the_hint_stream_reserves_what_its_pages_name() {
    let doc = neighbours_share_fonts();
    let path = std::env::temp_dir().join(format!("fepdf-hint-{}.pdf", std::process::id()));
    let _ = doc.save_linearized(&path, "2.0", &SaveOptions::default()).expect("it linearises");
    let file = std::fs::read(&path).expect("it was written");
    let _ = std::fs::remove_file(&path);

    let length = hint_length(&file);
    assert!(length < 1024, "the hint stream reserves {length} bytes for forty pages");
    let back = PdfDocument::open(file.into()).expect("the linearised file opens");
    assert_eq!(back.page_count().expect("it counts"), 40);
}

/// **The first page comes before the named destinations** (Annex F F.3.5): part 4 holds
/// the catalogue and what F.3.5 names, and a destination is part 9's. Every object the
/// catalogue reached went before the first page, and `intel_sdm.pdf`'s 279,508 named
/// destinations took half the file before it (ROADMAP Y-F30).
#[test]
fn the_first_page_comes_before_the_named_destinations() {
    let bytes = fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /Names << /Dests 4 0 R >> >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
        "<< /Names [(there) 5 0 R] >>",
        "<< /D [3 0 R /Fit] >>",
    ]);
    let doc = PdfDocument::open(bytes.into()).expect("the fixture opens");
    let path = std::env::temp_dir().join(format!("fepdf-order-{}.pdf", std::process::id()));
    let _ = doc.save_linearized(&path, "2.0", &SaveOptions::default()).expect("it linearises");
    let file = std::fs::read(&path).expect("it was written");
    let _ = std::fs::remove_file(&path);
    let at = |needle: &[u8]| file.windows(needle.len()).position(|w| w == needle);
    let page = at(b"/Type /Page\r").or_else(|| at(b"/Type /Page ")).expect("the page is written");
    let destination = at(b"/D [").expect("the destination is written");
    assert!(page < destination, "the destination at {destination} precedes the page at {page}");
}

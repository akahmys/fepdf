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

/// Options that write every object where it can be found by its bytes: the order tests
/// below are about which part an object is in, and packed, part 9 is out of sight.
fn written_directly() -> SaveOptions {
    SaveOptions { obj_stm: false, compress: false, ..SaveOptions::default() }
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
    let _ = doc.save_linearized(&path, "2.0", &written_directly()).expect("it linearises");
    let file = std::fs::read(&path).expect("it was written");
    let _ = std::fs::remove_file(&path);
    let at = |needle: &[u8]| file.windows(needle.len()).position(|w| w == needle);
    let page = at(b"/Type /Page\r").or_else(|| at(b"/Type /Page ")).expect("the page is written");
    let destination = at(b"/D [").expect("the destination is written");
    assert!(page < destination, "the destination at {destination} precedes the page at {page}");
}

/// **The information dictionary comes after the first page** (F.3.5 names it among what
/// part 9 holds); it was written beside the catalogue, before the page.
#[test]
fn the_information_dictionary_comes_after_the_first_page() {
    let bytes = fepdf_fixtures::Pdf::new().trailer_entries("/Info 4 0 R").assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
        "<< /CreationDate (D:20200101000000Z) >>",
    ]);
    let doc = PdfDocument::open(bytes.into()).expect("the fixture opens");
    let path = std::env::temp_dir().join(format!("fepdf-info-{}.pdf", std::process::id()));
    let _ = doc.save_linearized(&path, "2.0", &written_directly()).expect("it linearises");
    let file = std::fs::read(&path).expect("it was written");
    let _ = std::fs::remove_file(&path);
    let text = String::from_utf8_lossy(&file);
    let page = text.find("/Type /Page\r").or_else(|| text.find("/Type /Page ")).expect("a page");
    let info = text.find("/CreationDate").expect("the information dictionary is written");
    assert!(page < info, "the information dictionary at {info} precedes the page at {page}");
}

/// The object numbers a file writes directly (`N 0 obj`), and those its object streams
/// hold, read from each stream's header. Needs the streams uncompressed.
fn direct_and_packed(file: &[u8]) -> (Vec<u32>, Vec<u32>) {
    let text = String::from_utf8_lossy(file);
    let direct = text
        .split("\r\n")
        .filter_map(|line| line.strip_suffix(" 0 obj"))
        .filter_map(|n| n.parse().ok())
        .collect();
    let mut packed = Vec::new();
    for (at, _) in text.match_indices("/Type /ObjStm") {
        let dict = &text[at..at + text[at..].find(">>").expect("the dictionary closes")];
        let count: usize = number_after(dict, "/N ");
        let body = at + text[at..].find("stream\r\n").expect("a stream") + 8;
        let header: Vec<u32> = text[body..]
            .split_whitespace()
            .take(2 * count)
            .filter_map(|n| n.parse().ok())
            .collect();
        packed.extend(header.iter().step_by(2));
    }
    (direct, packed)
}

/// The integer after `key` in `dict`.
fn number_after(dict: &str, key: &str) -> usize {
    let rest = &dict[dict.find(key).expect("the key is there") + key.len()..];
    rest.split(|c: char| !c.is_ascii_digit()).next().and_then(|n| n.parse().ok()).expect("a number")
}

/// **What an object stream holds carries the highest object numbers** (Annex F F.3.1).
/// Part 9 was packed with numbers below the first page's and the main cross-reference
/// stream's, and qpdf reported every linearised sample (ROADMAP Y-F22).
#[test]
fn packed_objects_carry_the_highest_numbers() {
    let bytes = fepdf_fixtures::Pdf::new().trailer_entries("/Info 6 0 R").assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /Names << /Dests 4 0 R >> >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
        "<< /Names [(there) 5 0 R] >>",
        "<< /D [3 0 R /Fit] >>",
        "<< /CreationDate (D:20200101000000Z) >>",
    ]);
    let doc = PdfDocument::open(bytes.into()).expect("the fixture opens");
    let path = std::env::temp_dir().join(format!("fepdf-packed-{}.pdf", std::process::id()));
    let options = SaveOptions { compress: false, ..SaveOptions::default() };
    let _ = doc.save_linearized(&path, "2.0", &options).expect("it linearises");
    let file = std::fs::read(&path).expect("it was written");
    let _ = std::fs::remove_file(&path);

    let (direct, packed) = direct_and_packed(&file);
    assert!(!packed.is_empty(), "nothing was packed: {direct:?}");
    let highest_direct = direct.iter().max().expect("something is written directly");
    let lowest_packed = packed.iter().min().expect("something is packed");
    assert!(
        lowest_packed > highest_direct,
        "packed {packed:?} is numbered below what is written directly {direct:?}"
    );
    let types = main_xref_types(&file);
    let first_packed = types.iter().position(|t| *t == 2).expect("the main section lists packed");
    assert!(
        types[first_packed..].iter().all(|t| *t == 2),
        "the main cross-reference stream lists {types:?}: an entry not compressed after one that is"
    );
    let back = PdfDocument::open(file.into()).expect("the linearised file opens");
    assert_eq!(back.page_count().expect("it counts"), 1);
}

/// The type of each entry the main cross-reference stream lists, in order — the last
/// `/Type /XRef` in the file, written unfiltered with `/W [1 4 2]`.
fn main_xref_types(file: &[u8]) -> Vec<u8> {
    let at = file.windows(11).rposition(|w| w == b"/Type /XRef").expect("a cross-reference stream");
    let body = at + file[at..].windows(8).position(|w| w == b"stream\r\n").expect("its data") + 8;
    let length = number_after(
        String::from_utf8_lossy(&file[..body]).rsplit("<<").next().unwrap_or_default(),
        "/Length ",
    );
    file[body..body + length].chunks(7).map(|entry| entry[0]).collect()
}

/// **The first-page cross-reference reserves room for the first page's entries**, not
/// for every object numbered after them. Packed part 9 is numbered last, and sized from
/// the total, `intel_sdm.pdf` carried 5.7 MB of spaces after the linearisation
/// dictionary (ROADMAP Y-F22). Here, five hundred destinations reserved 10 KB.
#[test]
fn the_first_page_cross_reference_reserves_the_first_pages_entries() {
    const DESTINATIONS: usize = 500;
    let names: Vec<String> =
        (0..DESTINATIONS).map(|i| format!("(d{i:03}) {} 0 R", 5 + i)).collect();
    let mut bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R /Names << /Dests 4 0 R >> >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>".to_string(),
        format!("<< /Names [{}] >>", names.join(" ")),
    ];
    bodies.extend((0..DESTINATIONS).map(|_| "<< /D [3 0 R /Fit] >>".to_string()));
    let doc = PdfDocument::open(fepdf_fixtures::assemble(&bodies).into()).expect("it opens");
    let path = std::env::temp_dir().join(format!("fepdf-reserve-{}.pdf", std::process::id()));
    let _ = doc.save_linearized(&path, "2.0", &SaveOptions::default()).expect("it linearises");
    let file = std::fs::read(&path).expect("it was written");
    let _ = std::fs::remove_file(&path);

    let eof = file.windows(5).position(|w| w == b"%%EOF").expect("the first-page trailer ends");
    let padding = file[eof + 5..].iter().take_while(|b| b.is_ascii_whitespace()).count();
    assert!(padding < 1024, "{padding} bytes of padding follow the first-page trailer");
}

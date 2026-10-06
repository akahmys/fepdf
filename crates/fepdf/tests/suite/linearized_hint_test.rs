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

/// Three pages, each with `LINKS` link annotations of its own.
fn pages_with_links() -> PdfDocument {
    const LINKS: usize = 3;
    let mut bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 >>".to_string(),
    ];
    for page in 0..3 {
        let annots: Vec<String> =
            (0..LINKS).map(|i| format!("{} 0 R", 6 + page * LINKS + i)).collect();
        bodies.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Annots [{}] >>",
            annots.join(" ")
        ));
    }
    for _ in 0..3 * LINKS {
        bodies.push(
            "<< /Type /Annot /Subtype /Link /Rect [0 0 10 10] /Border [0 0 0] \
               /A << /S /URI /URI (https://example.org/) >> >>"
                .to_string(),
        );
    }
    PdfDocument::open(fepdf_fixtures::assemble(&bodies).into()).expect("the fixture opens")
}

/// Where `N 0 obj` begins, for each object written directly.
fn direct_offsets(file: &[u8]) -> Vec<(u32, usize)> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(found) = file[at..].windows(6).position(|w| w == b" 0 obj") {
        let end = at + found;
        let start = file[..end].iter().rposition(|b| !b.is_ascii_digit()).map_or(0, |p| p + 1);
        if let Ok(number) = String::from_utf8_lossy(&file[start..end]).parse() {
            out.push((number, start));
        }
        at = end + 6;
    }
    out
}

/// Table F.4's Item 1 for every page: how many objects the hint table says it has.
fn hinted_object_counts(file: &[u8], pages: usize) -> Vec<u32> {
    let at = file.windows(4).position(|w| w == b" /S ").expect("a hint stream");
    let body = at + file[at..].windows(8).position(|w| w == b"stream\r\n").expect("its data") + 8;
    let least = u32::from_be_bytes(file[body..body + 4].try_into().expect("four bytes"));
    // Table F.3 is 36 bytes; Item 1 then gives each page 16 bits, from Item 3.
    (0..pages)
        .map(|i| {
            let at = body + 36 + 2 * i;
            least + u32::from(u16::from_be_bytes([file[at], file[at + 1]]))
        })
        .collect()
}

/// **A page's own objects are packed into an object stream in its section, and the hint
/// table counts the stream and not what it holds** (Annex F F.3.1, ROADMAP Y-0b). Part 7
/// was written directly, so `intel_sdm.pdf`'s 22,619 link annotations were most of what a
/// linearised save carried over its plain one. The page objects stay direct, as F.3.1
/// requires, and each page's count is what lies directly in its section.
#[test]
fn a_pages_own_objects_are_packed_in_its_section() {
    let doc = pages_with_links();
    let path = std::env::temp_dir().join(format!("fepdf-part7-{}.pdf", std::process::id()));
    let options = SaveOptions { compress: false, ..SaveOptions::default() };
    let _ = doc.save_linearized(&path, "2.0", &options).expect("it linearises");
    let file = std::fs::read(&path).expect("it was written");
    let _ = std::fs::remove_file(&path);

    // Offsets in bytes: the hint stream is binary, and a lossy conversion moves them.
    let find_all = |needle: &[u8]| -> Vec<usize> {
        file.windows(needle.len())
            .enumerate()
            .filter(|(_, w)| *w == needle)
            .map(|(i, _)| i)
            .collect()
    };
    let holds = |range: std::ops::Range<usize>, needle: &[u8]| {
        file[range].windows(needle.len()).any(|w| w == needle)
    };
    let starts = find_all(b"/Type /Page\r");
    assert_eq!(starts.len(), 3, "{starts:?}");
    let objects = direct_offsets(&file);
    let page_start = |at: usize| {
        objects.iter().rev().find(|(_, start)| *start < at).map(|(_, s)| *s).expect("a page")
    };
    for &at in &starts {
        assert!(
            !holds(page_start(at)..at, b"stream"),
            "a page object is inside an object stream, where F.3.1 does not allow it"
        );
    }
    let (second, third) = (page_start(starts[1]), page_start(starts[2]));
    assert!(holds(second..third, b"/Type /ObjStm"), "page 2's section packs nothing");
    let direct_here: Vec<usize> =
        objects.iter().map(|(_, s)| *s).filter(|s| (second..third).contains(s)).collect();
    for &start in &direct_here {
        let end = start + file[start..].windows(6).position(|w| w == b"endobj").expect("it ends");
        assert!(
            holds(start..end, b"/ObjStm") || !holds(start..end, b"/Subtype /Link"),
            "a link of page 2 was written directly"
        );
    }

    let in_section = direct_here.len();
    let counts = hinted_object_counts(&file, 3);
    assert_eq!(counts[1] as usize, in_section, "page 2's hint count {counts:?}");

    let back = PdfDocument::open(file.into()).expect("the linearised file opens");
    assert_eq!(back.page_count().expect("it counts"), 3);
}

/// Table F.5's Items 3 and 4: the shared-object hint table's entries for the first page,
/// and in all. The table starts `/S` bytes into the hint stream's data.
fn shared_entry_counts(file: &[u8]) -> (u32, u32) {
    let at = file.windows(4).position(|w| w == b" /S ").expect("a hint stream");
    let digits: String =
        file[at + 4..].iter().take_while(|b| b.is_ascii_digit()).map(|b| char::from(*b)).collect();
    let offset: usize = digits.parse().expect("/S is a number");
    let body = at + file[at..].windows(8).position(|w| w == b"stream\r\n").expect("its data") + 8;
    let word = |i: usize| {
        let from = body + offset + 4 * i;
        u32::from_be_bytes(file[from..from + 4].try_into().expect("four bytes"))
    };
    // Items 1 and 2 are 32 bits each; Items 3 and 4 follow.
    (word(2), word(3))
}

/// Table F.6's length of shared-object entry `entry`: Table F.5's Item 6 plus the entry's
/// difference, at Item 7's width.
fn shared_entry_length(file: &[u8], entry: usize) -> u32 {
    let at = file.windows(4).position(|w| w == b" /S ").expect("a hint stream");
    let digits: String =
        file[at + 4..].iter().take_while(|b| b.is_ascii_digit()).map(|b| char::from(*b)).collect();
    let table = at
        + file[at..].windows(8).position(|w| w == b"stream\r\n").expect("its data")
        + 8
        + digits.parse::<usize>().expect("/S is a number");
    let least = u32::from_be_bytes(file[table + 18..table + 22].try_into().expect("four bytes"));
    let width = usize::from(u16::from_be_bytes([file[table + 22], file[table + 23]]));
    let start = (table + 24) * 8 + entry * width;
    least
        + (0..width).fold(0, |value, bit| {
            let at = start + bit;
            (value << 1) | u32::from(file[at / 8] >> (7 - at % 8) & 1)
        })
}

/// Table F.4's Item 3 for every page: how many shared objects each references, read at
/// the width Table F.3's Item 10 gives, after Items 1 and 2.
fn shared_references(file: &[u8], pages: usize) -> Vec<u32> {
    let at = file.windows(4).position(|w| w == b" /S ").expect("a hint stream");
    let body = at + file[at..].windows(8).position(|w| w == b"stream\r\n").expect("its data") + 8;
    let width = usize::from(u16::from_be_bytes([file[body + 28], file[body + 29]]));
    let start = (body + 36 + 2 * pages + 4 * pages) * 8;
    (0..pages)
        .map(|i| {
            (0..width).fold(0, |value, bit| {
                let at = start + i * width + bit;
                (value << 1) | u32::from(file[at / 8] >> (7 - at % 8) & 1)
            })
        })
        .collect()
}

/// **The later pages' shared objects are packed, and Table F.6 names the stream holding
/// them** (F.3.1, ROADMAP Y-F35). Forty pages share thirty-eight fonts beyond the first
/// page's two: those two stay with the first page, and the thirty-eight go into one
/// object stream, which is the table's one entry after the first page's. The last page's
/// own font is alone, and is written directly: a stream holding one object costs more.
#[test]
fn later_shared_objects_are_packed_and_named_by_their_stream() {
    let doc = neighbours_share_fonts();
    let path = std::env::temp_dir().join(format!("fepdf-part8-{}.pdf", std::process::id()));
    let options = SaveOptions { compress: false, ..SaveOptions::default() };
    let _ = doc.save_linearized(&path, "2.0", &options).expect("it linearises");
    let file = std::fs::read(&path).expect("it was written");
    let _ = std::fs::remove_file(&path);

    let objects = direct_offsets(&file);
    let direct_fonts = objects
        .iter()
        .filter(|(_, start)| {
            let end = start + file[*start..].windows(6).position(|w| w == b"endobj").expect("ends");
            let object = &file[*start..end];
            !object.windows(7).any(|w| w == b"/ObjStm")
                && object.windows(11).any(|w| w == b"/Type /Font")
        })
        .count();
    assert_eq!(direct_fonts, 3, "the first page's fonts and the last page's own are direct");
    let (first_page, all) = shared_entry_counts(&file);
    assert_eq!(all - first_page, 1, "Table F.6 lists {all} entries, {first_page} the first page's");
    // Its length is the bytes the stream takes, up to the object after it (Table F.6).
    let stream_at = objects
        .iter()
        .position(|(_, start)| {
            let end = start + file[*start..].windows(6).position(|w| w == b"endobj").expect("ends");
            file[*start..end].windows(7).any(|w| w == b"/ObjStm")
                && file[*start..end].windows(11).filter(|w| *w == b"/Type /Font").count() > 1
        })
        .expect("the fonts' stream is written");
    let taken = objects[stream_at + 1].1 - objects[stream_at].1;
    assert_eq!(shared_entry_length(&file, first_page as usize) as usize, taken);
    // Each later page names its fonts' stream once, Table F.4's Item 3; the second page
    // also names the font it shares with the first, which is the first page's.
    let references = shared_references(&file, 40);
    assert_eq!(references[1], 2, "{references:?}");
    assert_eq!(references[2..], [1; 38], "a page's references miss the stream");

    let back = PdfDocument::open(file.into()).expect("the linearised file opens");
    assert_eq!(back.page_count().expect("it counts"), 40);
}

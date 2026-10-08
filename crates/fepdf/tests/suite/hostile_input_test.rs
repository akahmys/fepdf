//! What a hostile file does to the translator (ROADMAP Z-2).
//!
//! Each fixture restates an input that broke another implementation, PrintCraft, as its
//! `vendor/README.md` describes it (ADR-0113): what the input was and what it did, not its
//! code. The other implementation's parser and renderer are not this one, so each is a
//! lead and not a result. What is held here is the translator's promise: a document opens,
//! or is refused, and drawing, extracting and saving it each finish.

use fepdf::{PdfDocument, SaveOptions};
use fepdf_fixtures::assemble;
use fepdf_fixtures::recorder::Recorder;
use kurbo::Affine;
use std::sync::mpsc;
use std::time::Duration;

/// How long one document may take through open, draw, extract and save, in a debug build.
/// Every fixture here is a few hundred bytes; a conforming file of that size takes
/// milliseconds.
const DEADLINE: Duration = Duration::from_secs(20);

/// Opens `pdf`, draws its first page, extracts its text and saves it, on a thread of its
/// own, and fails if that has not finished by the deadline. A refusal is an answer; an
/// error from any step is not a failure here, only a hang or a panic is.
fn finishes(name: &str, pdf: Vec<u8>) {
    let (done, finished) = mpsc::channel();
    let path = std::env::temp_dir().join(format!("fepdf_hostile_{name}.pdf"));
    let worker = std::thread::spawn(move || {
        if let Ok(doc) = PdfDocument::open(pdf.into()) {
            let mut recorder = Recorder::new();
            let _ = doc.render_page(0, &mut recorder, Affine::IDENTITY);
            let _ = doc.extract_text(0);
            let _ = doc.save_with_options(&path, "2.0", &SaveOptions::default());
            let _ = std::fs::remove_file(&path);
        }
        let _ = done.send(());
    });
    match finished.recv_timeout(DEADLINE) {
        Ok(()) => assert!(worker.join().is_ok(), "{name}: a step panicked"),
        Err(mpsc::RecvTimeoutError::Disconnected) => panic!("{name}: a step panicked"),
        Err(mpsc::RecvTimeoutError::Timeout) => panic!("{name}: still running after {DEADLINE:?}"),
    }
}

/// A one-page document whose page draws `content` with `resources`, and whatever further
/// objects the fixture needs from object 5 on.
fn page(content: &str, resources: &str, more: &[Vec<u8>]) -> Vec<u8> {
    let mut bodies: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R \
             /Resources {resources} >>"
        )
        .into_bytes(),
        stream("", content.as_bytes()),
    ];
    bodies.extend(more.iter().cloned());
    assemble(&bodies)
}

/// A stream object with `dict` entries besides its `/Length`.
fn stream(dict: &str, data: &[u8]) -> Vec<u8> {
    let mut body = format!("<< {dict} /Length {} >>\nstream\n", data.len()).into_bytes();
    body.extend_from_slice(data);
    body.extend_from_slice(b"\nendstream");
    body
}

/// `data` as a zlib stream of one stored block (RFC 1950, RFC 1951 3.2.4), so a fixture
/// can carry Flate-encoded bytes without a compressor.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let len = u16::try_from(data.len()).expect("one stored block");
    let mut out = vec![0x78, 0x01, 0x01];
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&(!len).to_le_bytes());
    out.extend_from_slice(data);
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    out.extend_from_slice(&((b << 16) | a).to_be_bytes());
    out
}

/// PrintCraft (its parser and hayro-syntax): `/Columns 4294967295` made a PNG predictor
/// allocate two 4 GiB rows before looking at the data, and `9223372036854775807` wrapped
/// to a 2^61-byte allocation.
#[test]
fn predictor_rows_wider_than_the_data_finish() {
    for (name, columns) in [("columns_u32", "4294967295"), ("columns_i64", "9223372036854775807")] {
        let image = stream(
            &format!(
                "/Type /XObject /Subtype /Image /Width 4 /Height 4 /ColorSpace /DeviceGray \
                 /BitsPerComponent 8 /Filter /FlateDecode \
                 /DecodeParms << /Predictor 12 /Columns {columns} >>"
            ),
            &zlib_stored(&[2, 1, 2, 3, 4]),
        );
        let pdf = page("q 100 0 0 100 0 0 cm /Im Do Q", "<< /XObject << /Im 5 0 R >> >>", &[image]);
        finishes(name, pdf);
    }
}

/// PrintCraft (hayro, hayro-syntax): an image mask and a CCITT image of 4294967295 by
/// 4294967295 samples allocated 4 GiB, and resampling one hung.
#[test]
fn images_of_absurd_dimensions_finish() {
    let huge = "/Width 4294967295 /Height 4294967295";
    let mask = stream(
        &format!("/Type /XObject /Subtype /Image {huge} /ImageMask true /BitsPerComponent 1"),
        &[0xFF; 16],
    );
    finishes(
        "image_mask",
        page("q 100 0 0 100 0 0 cm /Im Do Q", "<< /XObject << /Im 5 0 R >> >>", &[mask]),
    );
    let fax = stream(
        &format!(
            "/Type /XObject /Subtype /Image {huge} /ColorSpace /DeviceGray /BitsPerComponent 1 \
             /Filter /CCITTFaxDecode \
             /DecodeParms << /K -1 /Columns 4294967295 /Rows 4294967295 >>"
        ),
        &[0x00; 16],
    );
    finishes(
        "ccitt",
        page("q 100 0 0 100 0 0 cm /Im Do Q", "<< /XObject << /Im 5 0 R >> >>", &[fax]),
    );
}

/// PrintCraft (hayro-jbig2): a JBIG2 region of 65535 by 65535 pixels decoded for minutes.
/// One immediate lossless generic region segment (7.4.6), embedded with no file header.
#[test]
fn a_jbig2_region_of_billions_of_pixels_finishes() {
    let mut segment = vec![0, 0, 0, 0, 38, 0, 1];
    let mut data = Vec::new();
    data.extend_from_slice(&0xFFFF_u32.to_be_bytes()); // width
    data.extend_from_slice(&0xFFFF_u32.to_be_bytes()); // height
    data.extend_from_slice(&[0; 8]); // x, y
    data.push(0); // combination operator
    data.push(0); // generic region flags: MMR 0, template 0
    data.extend_from_slice(&[3, 0xFF, 0xFD, 0xFF, 2, 0xFE, 0xFE, 0xFE]); // AT pixels
    data.extend_from_slice(&[0x00; 32]);
    segment.extend_from_slice(&u32::try_from(data.len()).expect("small").to_be_bytes());
    segment.extend_from_slice(&data);
    let image = stream(
        "/Type /XObject /Subtype /Image /Width 65535 /Height 65535 /ColorSpace /DeviceGray \
         /BitsPerComponent 1 /Filter /JBIG2Decode",
        &segment,
    );
    finishes(
        "jbig2",
        page("q 100 0 0 100 0 0 cm /Im Do Q", "<< /XObject << /Im 5 0 R >> >>", &[image]),
    );
}

/// PrintCraft (hayro): `/LW 9223372036854775807` allocated 10 GB expanding the stroke.
#[test]
fn a_stroke_of_absurd_width_finishes() {
    finishes("line_width_operator", page("9223372036854775807 w 0 0 m 100 100 l S", "<< >>", &[]));
    finishes(
        "line_width_extgstate",
        page(
            "/G gs 0 0 m 100 100 l S",
            "<< /ExtGState << /G 5 0 R >> >>",
            &[b"<< /Type /ExtGState /LW 9223372036854775807 >>".to_vec()],
        ),
    );
}

/// PrintCraft (hayro-syntax): a `/Kids` loop overflowed the stack and aborted. Here the
/// loop is among plain objects; PrintCraft's ran through object streams.
#[test]
fn a_page_tree_that_loops_finishes() {
    let pdf = assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
        "<< /Type /Pages /Parent 2 0 R /Kids [2 0 R 4 0 R] /Count 2 >>",
    ]);
    finishes("kids_loop", pdf);
}

/// PrintCraft (hayro-syntax): fifty inline images with `EI` inside their data ran for over
/// ten minutes, the end-of-data search retrying from each candidate.
#[test]
fn inline_images_with_ei_in_their_data_finish() {
    let mut content = Vec::new();
    for _ in 0..50 {
        content.extend_from_slice(b"q 10 0 0 10 0 0 cm BI /W 100 /H 100 /BPC 8 /CS /G ID ");
        for _ in 0..2_000 {
            content.extend_from_slice(b"EI x ");
        }
        content.extend_from_slice(b"\nEI Q\n");
    }
    let mut bodies: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_vec(),
    ];
    bodies.push(stream("", &content));
    finishes("inline_ei", assemble(&bodies));
}

/// PrintCraft (hayro-interpret): a Type 3 glyph showing eight glyphs of its own font,
/// sixteen deep, hung.
#[test]
fn a_type3_glyph_that_shows_itself_finishes() {
    let glyph = "1000 0 0 0 1000 1000 d1 BT /F 1 Tf (aaaaaaaa) Tj ET";
    let font = "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 1000 1000] \
                /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs << /a 6 0 R >> \
                /Encoding << /Differences [97 /a] >> /FirstChar 97 /LastChar 97 \
                /Widths [1000] /Resources << /Font << /F 5 0 R >> >> >>";
    let pdf = page(
        "BT /F 12 Tf 10 10 Td (aaaaaaaa) Tj ET",
        "<< /Font << /F 5 0 R >> >>",
        &[font.as_bytes().to_vec(), stream("", glyph.as_bytes())],
    );
    finishes("type3_fan_out", pdf);
}

/// PrintCraft (hayro-interpret): a CID width range `0 4294967295 w` in `/W` hung, read one
/// CID at a time.
#[test]
fn a_cid_width_range_over_every_cid_finishes() {
    let pdf = page(
        "BT /F 12 Tf 10 10 Td <00410042> Tj ET",
        "<< /Font << /F 5 0 R >> >>",
        &[
            b"<< /Type /Font /Subtype /Type0 /BaseFont /X /Encoding /Identity-H \
              /DescendantFonts [6 0 R] >>"
                .to_vec(),
            b"<< /Type /Font /Subtype /CIDFontType2 /BaseFont /X \
              /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> \
              /FontDescriptor 7 0 R /W [0 4294967295 500] /W2 [0 4294967295 -1000 500 880] >>"
                .to_vec(),
            b"<< /Type /FontDescriptor /FontName /X /Flags 4 /FontBBox [0 0 1000 1000] \
              /ItalicAngle 0 /Ascent 800 /Descent -200 /CapHeight 700 /StemV 80 >>"
                .to_vec(),
        ],
    );
    finishes("cid_widths", pdf);
}

/// PrintCraft (hayro): a tiling pattern with an `/XStep` far beyond its `/BBox` allocated
/// 2 GB for its cell; and one painting with itself overflowed the stack.
#[test]
fn tiling_patterns_of_absurd_steps_and_self_reference_finish() {
    let fill = "/Pattern cs /P scn 0 0 200 200 re f";
    let resources = "<< /Pattern << /P 5 0 R >> >>";
    let wide = stream(
        "/Type /Pattern /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 10 10] \
         /XStep 1000000000 /YStep 1000000000 /Resources << >>",
        b"0 0 10 10 re f",
    );
    finishes("pattern_step", page(fill, resources, &[wide]));
    let tiny = stream(
        "/Type /Pattern /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 10 10] \
         /XStep 0.0001 /YStep 0.0001 /Resources << >>",
        b"0 0 10 10 re f",
    );
    finishes("pattern_tiny_step", page(fill, resources, &[tiny]));
    let itself = stream(
        "/Type /Pattern /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 10 10] \
         /XStep 10 /YStep 10 /Resources << /Pattern << /P 5 0 R >> >>",
        fill.as_bytes(),
    );
    finishes("pattern_self", page(fill, resources, &[itself]));
}

/// A form that draws itself, and one that draws itself eight times at every level, the
/// shape PrintCraft's Type 3 glyph had, as forms rather than glyphs.
#[test]
fn a_form_that_draws_itself_finishes() {
    for (name, body) in [
        ("form_self", "/X Do"),
        ("form_fan_out", "/X Do /X Do /X Do /X Do /X Do /X Do /X Do /X Do"),
    ] {
        let form = stream(
            "/Type /XObject /Subtype /Form /BBox [0 0 10 10] /Resources << /XObject << /X 5 0 R >> >>",
            body.as_bytes(),
        );
        finishes(name, page("/X Do", "<< /XObject << /X 5 0 R >> >>", &[form]));
    }
}

/// Where nesting stops, the document records it once, naming Annex C: what is past the
/// limit is not drawn, and a caller can tell that from a page that drew everything.
#[test]
fn the_nesting_limit_is_recorded_once() {
    let form = stream(
        "/Type /XObject /Subtype /Form /BBox [0 0 10 10] /Resources << /XObject << /X 5 0 R >> >>",
        b"/X Do",
    );
    let doc = PdfDocument::open(page("/X Do", "<< /XObject << /X 5 0 R >> >>", &[form]).into())
        .expect("it opens");
    doc.render_page(0, &mut Recorder::new(), Affine::IDENTITY).expect("the page interprets");
    let recorded: Vec<String> =
        doc.decisions().into_iter().filter(|d| d.clause == "C.2").map(|d| d.found).collect();
    assert_eq!(recorded.len(), 1, "{recorded:?}");
    assert!(recorded[0].contains("form XObject /X"), "{recorded:?}");
}

/// Drawing a page writes nothing into the document (ROADMAP Y-11), even where an operator
/// reaches the interpreter as a raw one carrying an inline dictionary. `DP` always does,
/// so this is conforming content: `/Tag << /MCID 0 >> DP` committed its property list
/// into the sealed arena, which a debug build refuses. The fuzzer found it (Z-1), and
/// behind it the reason `DP` never worked at all.
#[test]
fn a_raw_operator_with_an_inline_dictionary_writes_nothing() {
    let pdf = page("/Tag << /MCID 0 /Note [(a) (b)] >> DP 0 0 10 10 re f", "<< >>", &[]);
    let doc = PdfDocument::open(pdf.into()).expect("it opens");
    let mut recorder = Recorder::new();
    doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page interprets");
    // And what follows it is drawn. The parser kept only `DP`'s property list, so the
    // interpreter found no tag, and that error ended the page before the fill.
    assert_eq!(recorder.fills().len(), 1, "the rectangle after DP is filled");
}

/// A `/Parent` chain that loops finishes opening. Inherited resources are gathered by
/// following `/Parent` up from each page, and nothing stopped at a node already seen: a
/// page tree node naming the page as its own parent never finished opening. The fuzzer
/// found it (Z-1); the `/Kids` loop above is a different walk, and was bounded already.
#[test]
fn a_parent_chain_that_loops_finishes() {
    let pdf = assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /Parent 3 0 R >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
    ]);
    finishes("parent_loop", pdf);
}

/// An inline image whose dictionary never reaches `ID` finishes opening. The lexer
/// answers `EOF` once the stream is spent, and reading the dictionary skipped any token
/// that was not a name and asked again, for ever. The fuzzer found it (Z-1).
#[test]
fn an_inline_image_with_no_id_finishes() {
    finishes("bi_without_id", page("q BI /W 1 /H 1", "<< >>", &[]));
}

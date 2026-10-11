//! **A PDF made from pictures** (ROADMAP AA-5, ADR-0123): a JPEG carried as it is, a PNG
//! with its alpha as a soft mask, each page of a TIFF, and each page the picture's size.
//! The pictures are made here, by the encoders the engine decodes with.

use fepdf::{Object, Operation, PdfDocument, PdfError, SublimatedData};
use fepdf_fixtures::recorder::Recorder;
use kurbo::Affine;

/// A `w` by `h` picture of two colours, encoded by `image` as `format`.
fn encoded(w: u32, h: u32, format: image::ImageFormat) -> Vec<u8> {
    let picture = image::RgbImage::from_fn(w, h, |x, _| {
        if x < w / 2 { image::Rgb([200, 30, 30]) } else { image::Rgb([30, 30, 200]) }
    });
    let mut out = std::io::Cursor::new(Vec::new());
    picture.write_to(&mut out, format).expect("it encodes");
    out.into_inner()
}

/// A JPEG `w` by `h` at `dpi` dots to the inch, by JFIF's density.
fn jpeg(w: u32, h: u32, dpi: u16) -> Vec<u8> {
    let picture = image::RgbImage::from_pixel(w, h, image::Rgb([90, 140, 60]));
    let mut out = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90);
    encoder.set_pixel_density(image::codecs::jpeg::PixelDensity::dpi(dpi));
    encoder.encode_image(&picture).expect("it encodes");
    out
}

/// `jpeg` with an EXIF `APP1` saying orientation `orientation`, put after SOI.
fn turned(jpeg: &[u8], orientation: u16) -> Vec<u8> {
    let mut tiff = b"MM\0\x2a\0\0\0\x08\0\x01".to_vec();
    tiff.extend_from_slice(&[0x01, 0x12, 0, 3, 0, 0, 0, 1]);
    tiff.extend_from_slice(&orientation.to_be_bytes());
    tiff.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    let mut segment = b"Exif\0\0".to_vec();
    segment.extend_from_slice(&tiff);
    let length = u16::try_from(segment.len() + 2).expect("short");
    let mut out = jpeg.get(..2).expect("SOI").to_vec();
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(&segment);
    out.extend_from_slice(jpeg.get(2..).expect("the rest"));
    out
}

/// The image XObject page `page` draws, and its dictionary entry `key`.
fn image_entry(doc: &PdfDocument, page: usize, key: &str) -> Option<Object> {
    let inner = doc.inner();
    let arena = inner.arena();
    let page = inner.resolve_to_dict(inner.page_handle(page).ok()?).ok()?;
    let resources =
        arena.dict_entry(page, arena.name("Resources"))?.resolve(arena).as_dict_handle()?;
    let xobjects =
        arena.dict_entry(resources, arena.name("XObject"))?.resolve(arena).as_dict_handle()?;
    let image = arena.dict_entry(xobjects, arena.name("Im0"))?.resolve(arena);
    let Object::Stream(dict, _) = image else { return None };
    arena.dict_entry(dict, arena.name(key)).map(|v| v.resolve(arena))
}

fn name(doc: &PdfDocument, page: usize, key: &str) -> Option<String> {
    let found = image_entry(doc, page, key)?.as_name()?;
    doc.inner().arena().get_name_str(found)
}

fn number(doc: &PdfDocument, page: usize, key: &str) -> Option<f64> {
    image_entry(doc, page, key)?.as_f64()
}

/// The page's width and height, in points.
fn size(doc: &PdfDocument, page: usize) -> (f64, f64) {
    let page = doc.get_page_box(page).expect("a box");
    (page.x2 - page.x1, page.y2 - page.y1)
}

fn saved_and_read(doc: &PdfDocument, name: &str) -> PdfDocument {
    let path =
        std::env::temp_dir().join(format!("fepdf-pictures-{name}-{}.pdf", std::process::id()));
    doc.save_with_options(&path, "2.0", &fepdf::SaveOptions::default()).expect("it saves");
    let bytes = std::fs::read(&path).expect("it was written");
    let _ = std::fs::remove_file(&path);
    PdfDocument::open(bytes.into()).expect("it reads back")
}

/// **A JPEG is carried byte for byte under `/DCTDecode`, through a save, and its page is
/// its size at its resolution**: 80 by 40 dots at 144 to the inch is 40 by 20 points.
#[test]
fn a_jpeg_is_carried_as_it_is() {
    let original = jpeg(80, 40, 144);
    let doc = PdfDocument::from_images(vec![original.clone()], None).expect("it is made");
    assert_eq!(doc.page_count().expect("pages"), 1, "the empty document's page is gone");
    assert_eq!(size(&doc, 0), (40.0, 20.0));
    let read = saved_and_read(&doc, "jpeg");
    assert_eq!(name(&read, 0, "Filter").as_deref(), Some("DCTDecode"));
    let inner = read.inner();
    let arena = inner.arena();
    let page = inner.resolve_to_dict(inner.page_handle(0).expect("page")).expect("dict");
    let carried = arena
        .dict_entry(page, arena.name("Resources"))
        .and_then(|r| r.resolve(arena).as_dict_handle())
        .and_then(|r| arena.dict_entry(r, arena.name("XObject")))
        .and_then(|x| x.resolve(arena).as_dict_handle())
        .and_then(|x| arena.dict_entry(x, arena.name("Im0")))
        .map(|i| i.resolve(arena));
    let Some(Object::Stream(_, data)) = carried else { panic!("no image") };
    let SublimatedData::Raw(bytes) = &*data else { panic!("the JPEG was not kept raw") };
    assert_eq!(bytes.as_ref(), original.as_slice(), "the JPEG's bytes changed");
    let mut recorder = Recorder::new();
    read.render_page(0, &mut recorder, Affine::IDENTITY).expect("it draws");
    assert_eq!(recorder.count("image"), 1, "the picture is drawn");
}

/// **EXIF orientation 6 — a phone held upright — turns the page**: what is stored 80
/// across is shown 80 up, and drawn by a matrix that turns it a quarter.
#[test]
fn a_turned_jpeg_makes_an_upright_page() {
    let doc = PdfDocument::from_images(vec![turned(&jpeg(80, 40, 72), 6)], None).expect("made");
    assert_eq!(size(&doc, 0), (40.0, 80.0), "the page is the picture as shown");
    let inner = doc.inner();
    let arena = inner.arena();
    let page = inner.resolve_to_dict(inner.page_handle(0).expect("page")).expect("dict");
    let contents = arena
        .dict_entry(page, arena.name("Contents"))
        .map(|c| c.resolve(arena))
        .and_then(|c| {
            if let Object::Stream(_, data) = c { arena.get_stream_bytes(&data).ok() } else { None }
        })
        .expect("a content stream");
    let drawn = String::from_utf8_lossy(&contents);
    assert!(drawn.contains("0.0000 -80.0000 40.0000 0.0000 0.0000 80.0000 cm"), "{drawn}");
}

/// An Adobe CMYK JPEG holds its samples inverted, and says so by `APP14`; only the
/// markers are read, so these are all there is of one.
#[test]
fn an_adobe_cmyk_jpeg_is_decoded_inverted() {
    let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xEE, 0, 14];
    bytes.extend_from_slice(b"Adobe\0\x64\0\0\0\0\x02");
    bytes.extend_from_slice(&[0xFF, 0xC0, 0, 20, 8, 0, 2, 0, 2, 4]);
    bytes.extend_from_slice(&[1, 0x11, 0, 2, 0x11, 0, 3, 0x11, 0, 4, 0x11, 0]);
    bytes.extend_from_slice(&[0xFF, 0xDA, 0, 2, 0xFF, 0xD9]);
    let doc = PdfDocument::from_images(vec![bytes], None).expect("made");
    assert_eq!(name(&doc, 0, "ColorSpace").as_deref(), Some("DeviceCMYK"));
    let decode = image_entry(&doc, 0, "Decode").and_then(|d| d.as_array());
    let decode = decode.and_then(|d| doc.inner().arena().get_array(d)).expect("/Decode");
    let values: Vec<f64> = decode.iter().filter_map(Object::as_f64).collect();
    assert_eq!(values, [1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0]);
}

/// **A PNG's alpha is a soft mask, and one with none, or opaque throughout, has none.**
#[test]
fn a_pngs_alpha_is_its_soft_mask() {
    let opaque = image::RgbaImage::from_pixel(20, 10, image::Rgba([10, 20, 30, 255]));
    let mut opaque_png = std::io::Cursor::new(Vec::new());
    opaque.write_to(&mut opaque_png, image::ImageFormat::Png).expect("it encodes");
    let see_through = image::RgbaImage::from_fn(20, 10, |x, _| {
        image::Rgba([0, 0, 0, if x < 10 { 0 } else { 255 }])
    });
    let mut bytes = std::io::Cursor::new(Vec::new());
    see_through.write_to(&mut bytes, image::ImageFormat::Png).expect("it encodes");
    let doc = PdfDocument::from_images(
        vec![bytes.into_inner(), encoded(20, 10, image::ImageFormat::Png), opaque_png.into_inner()],
        None,
    )
    .expect("made");
    assert!(matches!(image_entry(&doc, 0, "SMask"), Some(Object::Stream(..))), "no mask");
    assert!(image_entry(&doc, 1, "SMask").is_none(), "a PNG with no alpha was given a mask");
    assert!(image_entry(&doc, 2, "SMask").is_none(), "an opaque alpha was kept as a mask");
    assert_eq!(name(&doc, 1, "ColorSpace").as_deref(), Some("DeviceRGB"));
    let mut recorder = Recorder::new();
    doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("it draws");
    assert_eq!(recorder.count("image"), 1);
}

/// **Sixteen bits stay sixteen, and `pHYs` sets the page**: 300 dots to the inch is
/// 11811 to the metre, so 600 dots across is two inches, 144 points.
#[test]
fn a_sixteen_bit_png_keeps_its_depth_and_resolution() {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 600, 30);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Sixteen);
        encoder.set_pixel_dims(Some(png::PixelDimensions {
            xppu: 11_811,
            yppu: 11_811,
            unit: png::Unit::Meter,
        }));
        let mut writer = encoder.write_header().expect("a header");
        writer.write_image_data(&vec![0x80; 600 * 30 * 2]).expect("the samples");
    }
    let doc = PdfDocument::from_images(vec![bytes], None).expect("made");
    assert_eq!(number(&doc, 0, "BitsPerComponent"), Some(16.0));
    assert_eq!(name(&doc, 0, "ColorSpace").as_deref(), Some("DeviceGray"));
    let (w, h) = size(&doc, 0);
    assert!((w - 144.0).abs() < 0.05 && (h - 7.2).abs() < 0.05, "{w} by {h}");
}

/// **Each page of a TIFF is a page, and its thumbnail is not.**
#[test]
fn a_tiffs_pages_are_pages_and_its_thumbnail_is_not() {
    use tiff::encoder::{TiffEncoder, colortype};
    let mut bytes = std::io::Cursor::new(Vec::new());
    {
        let mut encoder = TiffEncoder::new(&mut bytes).expect("an encoder");
        encoder.write_image::<colortype::Gray8>(16, 8, &[60; 16 * 8]).expect("page one");
        let mut thumb = encoder.new_image::<colortype::Gray8>(4, 2).expect("a thumbnail");
        thumb.encoder().write_tag(tiff::tags::Tag::NewSubfileType, 1u32).expect("marked");
        thumb.write_data(&[0; 8]).expect("written");
        encoder.write_image::<colortype::RGB8>(10, 20, &[200; 10 * 20 * 3]).expect("page two");
    }
    let doc = PdfDocument::from_images(vec![bytes.into_inner()], None).expect("made");
    assert_eq!(doc.page_count().expect("pages"), 2, "two pages, and no thumbnail");
    assert_eq!(name(&doc, 0, "ColorSpace").as_deref(), Some("DeviceGray"));
    assert_eq!(name(&doc, 1, "ColorSpace").as_deref(), Some("DeviceRGB"));
    assert_eq!(size(&doc, 1), (10.0, 20.0), "72 to the inch where none is said");
}

/// **A one-bit scan whose zero is white stays one-bit, and is drawn the right way
/// round**: a TIFF written byte by byte, uncompressed, `PhotometricInterpretation` 0.
#[test]
fn a_one_bit_white_is_zero_scan_is_inverted_by_decode() {
    // Eight by two, one bit, one strip of two bytes at offset 8; then nine entries.
    let mut bytes = b"II\x2a\0\x0a\0\0\0".to_vec();
    bytes.extend_from_slice(&[0xF0, 0x0F]);
    let entries: [(u16, u16, u32); 9] = [
        (256, 3, 8), // ImageWidth
        (257, 3, 2), // ImageLength
        (258, 3, 1), // BitsPerSample
        (259, 3, 1), // Compression: none
        (262, 3, 0), // PhotometricInterpretation: WhiteIsZero
        (273, 4, 8), // StripOffsets
        (277, 3, 1), // SamplesPerPixel
        (278, 3, 2), // RowsPerStrip
        (279, 4, 2), // StripByteCounts
    ];
    bytes.extend_from_slice(&u16::try_from(entries.len()).expect("few").to_le_bytes());
    for (tag, kind, value) in entries {
        bytes.extend_from_slice(&tag.to_le_bytes());
        bytes.extend_from_slice(&kind.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    let doc = PdfDocument::from_images(vec![bytes], None).expect("made");
    assert_eq!(number(&doc, 0, "BitsPerComponent"), Some(1.0));
    assert!(image_entry(&doc, 0, "Decode").is_some(), "zero is white, and /Decode says so");
}

/// **Into a document, at a place, on a sheet**: a picture twice as wide as high on an A4
/// sheet fills its width and sits in the middle.
#[test]
fn pictures_go_into_a_document_where_asked_and_fit_the_sheet() {
    let mut doc = PdfDocument::from_images(vec![jpeg(10, 10, 72), jpeg(10, 10, 72)], None)
        .expect("two pages");
    let a4 = [595.0, 842.0];
    doc.apply(Operation::InsertImages {
        images: vec![encoded(200, 100, image::ImageFormat::Png)],
        at: 1,
        sheet: Some(a4),
    })
    .expect("it goes in");
    assert_eq!(doc.page_count().expect("pages"), 3);
    assert_eq!(size(&doc, 1), (595.0, 842.0), "the new page is the sheet");
    assert_eq!(size(&doc, 2), (10.0, 10.0), "the page after it is the one that was second");
}

/// **What is not a picture this reads is refused, saying which, and nothing changes.**
#[test]
fn what_is_not_a_picture_is_refused() {
    let mut doc = PdfDocument::from_images(vec![jpeg(10, 10, 72)], None).expect("one page");
    let lossless = vec![0xFF, 0xD8, 0xFF, 0xC3, 0, 11, 8, 0, 1, 0, 1, 1, 1, 0x11, 0, 0xFF, 0xDA];
    for (images, sheet) in [
        (vec![jpeg(10, 10, 72), b"GIF89a....".to_vec()], None),
        (vec![lossless], None),
        (vec![], None),
        (vec![jpeg(10, 10, 72)], Some([0.0, 100.0])),
    ] {
        let refused = doc.apply(Operation::InsertImages { images, at: 0, sheet });
        assert!(matches!(refused, Err(PdfError::Refused { .. })), "{refused:?}");
    }
    assert_eq!(doc.page_count().expect("pages"), 1, "a refusal changed the document");
    let named = doc.apply(Operation::InsertImages {
        images: vec![jpeg(10, 10, 72), b"GIF89a".to_vec()],
        at: 0,
        sheet: None,
    });
    assert!(format!("{named:?}").contains("picture 2"), "{named:?}");
}

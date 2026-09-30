//! The images a crop puts outside the sheet, cut rather than carried (ROADMAP W-G1-b,
//! ADR-0088).
//!
//! **An image half on the kept side was the whole image in the file.** The text a crop put
//! outside was removed glyph by glyph; an image went on carrying every pixel, so a drawing
//! split into two sheets sent both halves of every picture on it. What is asked here is
//! what the page draws after the cut, from the renderer: which pixels, how many.

use fepdf::{
    CropRegion, IngestionOptions, Operation, PageSelection, PdfDocument, WhatFallsOutside,
};
use fepdf_fixtures::recorder::{Event, Recorder};
use kurbo::Affine;

/// Four columns of red, 10 to 40, over two rows — the second with green in it — so that a
/// cut says which columns and which rows it kept.
fn pixels() -> Vec<u8> {
    let mut out = Vec::new();
    for green in [0u8, 99] {
        for red in [10u8, 20, 30, 40] {
            out.extend_from_slice(&[red, green, 0]);
        }
    }
    out
}

/// A 200-point square page with the image drawn over x 0–200, y 50–150, and `extra` in
/// its dictionary.
fn page_with_image(image: &[u8], dict: &str, extra: &[String]) -> PdfDocument {
    let content = "q 200 0 0 100 0 50 cm /Im0 Do Q";
    let mut stream =
        format!("<< /Type /XObject /Subtype /Image {dict} /Length {} >>\nstream\n", image.len())
            .into_bytes();
    stream.extend_from_slice(image);
    stream.extend_from_slice(b"\nendstream");
    let mut bodies: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R \
          /Resources << /XObject << /Im0 5 0 R >> >> >>"
            .to_vec(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()).into_bytes(),
        stream,
    ];
    bodies.extend(extra.iter().map(|b| b.clone().into_bytes()));
    PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&bodies).into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens")
}

fn rgb_page() -> PdfDocument {
    page_with_image(&pixels(), "/Width 4 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8", &[])
}

fn cut(doc: &mut PdfDocument, page: usize, keep: (f64, f64, f64, f64)) {
    doc.apply(Operation::CropPages(
        PageSelection::Single(page),
        CropRegion { keep, outside: WhatFallsOutside::Goes },
    ))
    .expect("the crop applies");
}

/// Every image the page draws: its size and its samples.
fn drawn(doc: &PdfDocument, page: usize) -> Vec<(u32, u32, Vec<u8>)> {
    let mut recorder = Recorder::new();
    doc.render_page(page, &mut recorder, Affine::IDENTITY).expect("the page interprets");
    recorder
        .events
        .into_iter()
        .filter_map(|e| {
            if let Event::Image { samples, width, height, .. } = e {
                Some((width, height, samples))
            } else {
                None
            }
        })
        .collect()
}

/// **The left half keeps the left two columns**, every row of them, and nothing else.
#[test]
fn a_crop_keeps_the_columns_on_its_side() {
    let mut doc = rgb_page();
    cut(&mut doc, 0, (0.0, 0.0, 100.0, 200.0));
    let images = drawn(&doc, 0);
    assert_eq!(images.len(), 1, "one image is drawn: {images:?}");
    let (width, height, samples) = &images[0];
    assert_eq!((*width, *height), (2, 2), "the image was not cut to two columns");
    assert_eq!(samples, &[10, 0, 0, 20, 0, 0, 10, 99, 0, 20, 99, 0], "the wrong pixels stayed");
}

/// **And the top half keeps the top row.** Row 0 is the top of the picture (8.9.4).
#[test]
fn a_crop_keeps_the_rows_on_its_side() {
    let mut doc = rgb_page();
    cut(&mut doc, 0, (0.0, 100.0, 200.0, 200.0));
    let (width, height, samples) = drawn(&doc, 0).remove(0);
    assert_eq!((width, height), (4, 1));
    assert_eq!(samples, [10, 0, 0, 20, 0, 0, 30, 0, 0, 40, 0, 0]);
}

/// An image the crop leaves entirely outside is not drawn, and one it leaves entirely
/// inside is drawn as it was.
#[test]
fn an_image_wholly_outside_goes_and_one_wholly_inside_stays() {
    let mut doc = rgb_page();
    cut(&mut doc, 0, (0.0, 0.0, 200.0, 40.0));
    assert!(drawn(&doc, 0).is_empty(), "an image wholly outside the crop is still drawn");

    let mut doc = rgb_page();
    cut(&mut doc, 0, (0.0, 0.0, 200.0, 200.0));
    let (width, height, samples) = drawn(&doc, 0).remove(0);
    assert_eq!((width, height, samples), (4, 2, pixels()), "an image wholly inside changed");
}

/// **A page split in two gives each sheet its own half of the picture**, which is the
/// case ADR-0088 was written for.
#[test]
fn a_split_gives_each_sheet_its_half() {
    let mut doc = rgb_page();
    doc.apply(Operation::SplitPage {
        page: 0,
        into: fepdf::PageDivision::Grid { columns: 2, rows: 1 },
    })
    .expect("the split applies");
    let left = drawn(&doc, 0).remove(0);
    let right = drawn(&doc, 1).remove(0);
    assert_eq!(left.2, [10, 0, 0, 20, 0, 0, 10, 99, 0, 20, 99, 0]);
    assert_eq!(right.2, [30, 0, 0, 40, 0, 0, 30, 99, 0, 40, 99, 0]);
}

/// **A one-bit mask is cut bit by bit**, its rows starting on a byte (8.9.3).
#[test]
fn a_one_bit_mask_is_cut_by_the_bit() {
    let mut doc =
        page_with_image(&[0b1011_0011, 0b0100_1100], "/Width 8 /Height 2 /ImageMask true", &[]);
    cut(&mut doc, 0, (0.0, 0.0, 100.0, 200.0));
    let (width, height, samples) = drawn(&doc, 0).remove(0);
    assert_eq!((width, height), (4, 2));
    // Packed as the file packs it: each row of four bits starts a byte.
    assert_eq!(samples, [0b1011_0000, 0b0100_0000], "the kept bits are not the left four");
}

/// **A soft mask is cut to the same part of the picture.**
#[test]
fn a_soft_mask_is_cut_with_its_image() {
    let mask_bytes = [0u8, 64, 128, 255, 0, 64, 128, 255];
    let mut mask = b"<< /Type /XObject /Subtype /Image /Width 4 /Height 2 /ColorSpace /DeviceGray \
          /BitsPerComponent 8 /Length 8 >>\nstream\n"
        .to_vec();
    mask.extend_from_slice(&mask_bytes);
    mask.extend_from_slice(b"\nendstream");
    let mut doc = page_with_image(
        &pixels(),
        "/Width 4 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8 /SMask 6 0 R",
        &[String::from_utf8_lossy(&mask).into_owned()],
    );
    cut(&mut doc, 0, (100.0, 0.0, 200.0, 200.0));
    let mut recorder = Recorder::new();
    doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page interprets");
    let smask = recorder
        .events
        .iter()
        .find_map(|e| if let Event::Image { smask, .. } = e { smask.clone() } else { None });
    let smask = smask.expect("the cut image still has its soft mask");
    assert_eq!((smask.width, smask.height), (2, 2), "the soft mask was not cut with the image");
}

/// The images a document written and read back holds, as their widths and heights.
fn written_images(doc: &PdfDocument) -> Vec<(i64, i64)> {
    // A path of its own each call: the tests run side by side in one process.
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let nth = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("fepdf-crop-image-{}-{nth}.pdf", std::process::id()));
    doc.save_with_options(&path, "2.0", &fepdf::SaveOptions::default()).expect("it writes");
    let back = PdfDocument::open(std::fs::read(&path).expect("it is there").into())
        .expect("it reads back");
    let _ = std::fs::remove_file(&path);
    let arena = back.inner().arena();
    let entry = |dict, key: &str| arena.dict_entry(dict, arena.name(key));
    (0..arena.object_count())
        .filter_map(|i| match arena.get_object(arena.handle(i))? {
            fepdf_model::Object::Stream(dict, _)
                if entry(dict, "Subtype").and_then(|s| s.as_name())
                    == Some(arena.name("Image")) =>
            {
                Some((entry(dict, "Width")?.as_integer()?, entry(dict, "Height")?.as_integer()?))
            }
            _ => None,
        })
        .collect()
}

/// **What a crop cuts off an image is not in the file.** The page drew the cut, and the
/// whole image stayed in its resources under its old name, so the writer wrote every
/// pixel of it: the drawing was cut and the file was not. The same for an image the crop
/// leaves wholly outside, whose `Do` went and whose samples did not, and for a split,
/// where each sheet drew its half and carried the whole.
#[test]
fn what_a_crop_cuts_off_an_image_is_not_written() {
    let mut half = rgb_page();
    cut(&mut half, 0, (0.0, 0.0, 100.0, 200.0));
    assert_eq!(written_images(&half), [(2, 2)], "the whole image is still in the file");

    let mut none = rgb_page();
    cut(&mut none, 0, (0.0, 0.0, 200.0, 40.0));
    assert_eq!(written_images(&none), [], "an image wholly outside is still in the file");

    let mut split = rgb_page();
    split
        .apply(Operation::SplitPage {
            page: 0,
            into: fepdf::PageDivision::Grid { columns: 2, rows: 1 },
        })
        .expect("the split applies");
    assert_eq!(written_images(&split), [(2, 2), (2, 2)], "a split sheet carries the whole");
}

/// **An image another page still draws stays**, though the page cropped no longer does:
/// resources inherited from the page tree are one dictionary for both pages, and taking
/// the image out of it would take it off the page nobody cropped.
#[test]
fn an_image_a_page_not_cropped_draws_stays() {
    let content = "q 200 0 0 100 0 50 cm /Im0 Do Q";
    let image = pixels();
    let mut stream = format!(
        "<< /Type /XObject /Subtype /Image /Width 4 /Height 2 /ColorSpace /DeviceRGB \
         /BitsPerComponent 8 /Length {} >>\nstream\n",
        image.len()
    )
    .into_bytes();
    stream.extend_from_slice(&image);
    stream.extend_from_slice(b"\nendstream");
    let bodies: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 /Resources << /XObject << /Im0 5 0 R >> >> >>"
            .to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_vec(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()).into_bytes(),
        stream,
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_vec(),
    ];
    let mut doc = PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&bodies).into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens");
    cut(&mut doc, 0, (0.0, 0.0, 100.0, 200.0));
    assert_eq!(drawn(&doc, 1).remove(0).0, 4, "the page not cropped lost its image");
    let mut written = written_images(&doc);
    written.sort_unstable();
    assert_eq!(written, [(2, 2), (4, 2)]);
}

/// **And so does one a form draws through the page's resources**, having none of its own
/// (7.8.3): the page never names the image, and the form does.
#[test]
fn an_image_a_form_draws_through_the_page_stays() {
    let form = "q 200 0 0 100 0 50 cm /Im0 Do Q";
    let form = format!(
        "<< /Type /XObject /Subtype /Form /BBox [0 0 200 200] /Length {} >>\nstream\n{form}\nendstream",
        form.len()
    );
    let mut doc = page_with_image(
        &pixels(),
        "/Width 4 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8",
        &[form, "<< /Length 7 >>\nstream\n/Fm0 Do\nendstream".to_string()],
    );
    // The page draws the form, and names the image nowhere of its own.
    let arena = doc.inner().arena();
    let page = doc.inner().get_page_handle(0).and_then(|h| arena.get_object(h));
    let page = page.and_then(|p| p.as_dict_handle()).expect("the page is there");
    let mut dict = arena.get_dict(page).expect("it reads");
    dict.insert(arena.name("Contents"), fepdf_model::Object::Reference(arena.handle(7)));
    let resources =
        dict.get(&arena.name("Resources")).and_then(|r| r.resolve(arena).as_dict_handle());
    arena.set_dict(page, dict);
    let xobjects = resources
        .and_then(|r| arena.dict_entry(r, arena.name("XObject")))
        .and_then(|x| x.resolve(arena).as_dict_handle())
        .expect("the page names XObjects");
    let mut names = arena.get_dict(xobjects).expect("it reads");
    names.insert(arena.name("Fm0"), fepdf_model::Object::Reference(arena.handle(6)));
    arena.set_dict(xobjects, names);

    cut(&mut doc, 0, (0.0, 0.0, 100.0, 200.0));
    assert_eq!(written_images(&doc), [(4, 2)], "the image the form draws went");
}

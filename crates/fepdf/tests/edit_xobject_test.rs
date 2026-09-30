//! Moving, scaling, turning and replacing an object a page draws (ROADMAP W-E5).
//!
//! **The check is the entry's own: the page rasterised after the edit is the page that
//! would have been drawn with the object where it was asked to go.** Each expected page
//! writes the object's matrix out by hand, so the comparison is between the edit and
//! arithmetic done here, through a renderer neither wrote.

use fepdf::xobject::objects_of_page;
use fepdf::{IngestionOptions, Operation, PdfDocument, Rasteriser, XObjectEdit};

/// A 2 by 2 image — red, green over blue, white — so a turn shows which way it went.
const PIXELS: [u8; 12] = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];

/// A 200-point page drawing the image with `matrix`, then a form XObject: a black square.
fn page(matrix: &str) -> PdfDocument {
    let content = format!("q {matrix} cm /Im0 Do Q q 1 0 0 1 120 120 cm /Fm0 Do Q");
    let mut image = b"<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceRGB \
          /BitsPerComponent 8 /Length 12 >>\nstream\n"
        .to_vec();
    image.extend_from_slice(&PIXELS);
    image.extend_from_slice(b"\nendstream");
    let form = "0 0 30 30 re f";
    let bodies: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R \
          /Resources << /XObject << /Im0 5 0 R /Fm0 6 0 R >> >> >>"
            .to_vec(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()).into_bytes(),
        image,
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 30 30] /Length {} >>\nstream\n{form}\nendstream",
            form.len()
        )
        .into_bytes(),
    ];
    PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&bodies).into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens")
}

/// The image drawn 40 wide and 20 high with its lower left at (20, 30).
fn original() -> PdfDocument {
    page("40 0 0 20 20 30")
}

fn raster(doc: &PdfDocument) -> Vec<u8> {
    let path = std::env::temp_dir().join(format!(
        "fepdf_edit_xobject_{}_{:?}.png",
        std::process::id(),
        std::thread::current().id()
    ));
    doc.render_page_to_file_with(0, &path, Rasteriser::Cpu).expect("the page renders");
    let pixels = image::open(&path).expect("what was written reads").to_rgba8().into_raw();
    let _ = std::fs::remove_file(&path);
    pixels
}

/// How many pixels differ by more than a little between two rasterisations.
fn differing(one: &[u8], other: &[u8]) -> usize {
    one.chunks(4)
        .zip(other.chunks(4))
        .filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 8))
        .count()
}

fn edited(edit: XObjectEdit) -> PdfDocument {
    let mut doc = original();
    doc.apply(Operation::EditXObject { page: 0, object: 0, edit }).expect("the edit applies");
    doc
}

/// Each edit, and the matrix that draws the image where it should end up.
#[test]
fn each_edit_draws_what_the_expected_page_draws() {
    let cases = [
        ("move", XObjectEdit::Move { to: (100.0, 120.0) }, "40 0 0 20 100 120"),
        // Twice the size about the centre (40, 40): 80 by 40 from (0, 20).
        ("scale", XObjectEdit::Scale { by: 2.0 }, "80 0 0 40 0 20"),
        // A quarter turn anticlockwise about (40, 40): (u, v) goes to (50 - 20v, 20 + 40u).
        ("rotate", XObjectEdit::Rotate { degrees: 90.0 }, "0 40 -20 0 50 20"),
    ];
    for (name, edit, expected) in cases {
        let got = raster(&edited(edit));
        let want = raster(&page(expected));
        let wrong = differing(&got, &want);
        assert!(wrong == 0, "{name}: {wrong} pixels differ from the expected page");
        // And it is not the page it started as, or the comparison proves nothing.
        assert!(differing(&got, &raster(&original())) > 100, "{name} changed nothing");
    }
}

/// **The listing says what is drawn and where**, and a form is listed by its `/BBox`.
#[test]
fn the_listing_names_each_object_and_where_it_is() {
    let listed = objects_of_page(original().inner(), 0).expect("it lists");
    assert_eq!(listed.len(), 2);
    assert!(listed[0].image && !listed[1].image);
    assert_eq!(listed[0].bounds(), (20.0, 30.0, 60.0, 50.0));
    assert_eq!(listed[1].bounds(), (120.0, 120.0, 150.0, 150.0));
}

/// **A form is moved like an image**, and the image beside it stays where it was.
#[test]
fn a_form_is_moved_and_nothing_else_is() {
    let mut doc = original();
    doc.apply(Operation::EditXObject {
        page: 0,
        object: 1,
        edit: XObjectEdit::Move { to: (10.0, 150.0) },
    })
    .expect("the edit applies");
    let listed = objects_of_page(doc.inner(), 0).expect("it lists");
    assert_eq!(listed[1].bounds(), (10.0, 150.0, 40.0, 180.0), "the form is not where it was put");
    assert_eq!(listed[0].bounds(), (20.0, 30.0, 60.0, 50.0), "the image moved as well");
}

/// **A replaced image is the picture it was given**, over the same square.
#[test]
fn a_replaced_image_draws_the_new_picture_in_the_same_place() {
    let doc = edited(XObjectEdit::Replace { jpeg: fepdf_fixtures::red_jpeg() });
    let listed = objects_of_page(doc.inner(), 0).expect("it lists");
    assert_eq!(
        listed[0].bounds(),
        (20.0, 30.0, 60.0, 50.0),
        "the picture is not where the old one was"
    );
    assert_ne!(listed[0].name, "Im0", "the shared image was edited rather than replaced");
}

/// What cannot be done is refused, naming why.
#[test]
fn what_cannot_be_done_is_refused() {
    let mut doc = original();
    for (edit, object, said) in [
        (XObjectEdit::Replace { jpeg: fepdf_fixtures::red_jpeg() }, 1, "form"),
        (XObjectEdit::Scale { by: 0.0 }, 0, "scale"),
        (XObjectEdit::Move { to: (0.0, 0.0) }, 5, "no object 5"),
    ] {
        let error =
            doc.apply(Operation::EditXObject { page: 0, object, edit }).expect_err("refused");
        assert!(error.to_string().contains(said), "the refusal does not say why: {error}");
    }
}

/// **A replaced image is not written.** The picture that replaced it was drawn, and the
/// old one stayed in the page's resources, so the file carried both — a reader replacing
/// a photograph to take it out of the document had sent it anyway. Here the old image is
/// the only one without `/DCTDecode`.
#[test]
fn a_replaced_image_is_not_in_the_file() {
    let doc = edited(XObjectEdit::Replace { jpeg: fepdf_fixtures::red_jpeg() });
    let path = std::env::temp_dir().join(format!("fepdf_replaced_{}.pdf", std::process::id()));
    doc.save_with_options(&path, "2.0", &fepdf::SaveOptions::default()).expect("it writes");
    let back = PdfDocument::open(std::fs::read(&path).expect("it is there").into())
        .expect("it reads back");
    let _ = std::fs::remove_file(&path);
    let arena = back.inner().arena();
    let entry = |dict, key: &str| arena.dict_entry(dict, arena.name(key)).map(|v| v.resolve(arena));
    let unfiltered = (0..arena.object_count())
        .filter_map(|i| match arena.get_object(arena.handle(i))? {
            fepdf_model::Object::Stream(dict, _) => Some(dict),
            _ => None,
        })
        .filter(|dict| {
            entry(*dict, "Subtype").and_then(|s| s.as_name()) == Some(arena.name("Image"))
        })
        .filter(|dict| {
            entry(*dict, "Filter").and_then(|f| f.as_name()) != Some(arena.name("DCTDecode"))
        })
        .count();
    assert_eq!(unfiltered, 0, "the image that was replaced is still in the file");
}

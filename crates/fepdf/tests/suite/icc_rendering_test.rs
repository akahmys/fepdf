//! An `[/ICCBased …]` colour reaches the backend through its profile (10.3).
//!
//! **The reachability test, and it is the one that was missing.** A first attempt at
//! colour management put the transform on `color::ColorSpace`, a type nothing outside its
//! own module referred to: three unit tests passed and not one page changed. That type
//! has since been deleted. What catches such a thing is not a test of the function — it
//! is a test that renders a page and looks at the colour the backend was handed.
//!
//! Display P3 rather than sRGB, because sRGB through sRGB is the identity and would pass
//! whether the profile was consulted or not.

use fepdf::PdfDocument;
use fepdf_fixtures::assemble;
use fepdf_fixtures::recorder::{Event, Recorder};
use kurbo::Affine;

/// A page that fills a square with `0.2 0.4 0.6` in an `[/ICCBased …]` space.
fn page_in_icc_space(profile: &[u8]) -> Vec<u8> {
    let content = "/CS0 cs 0.2 0.4 0.6 scn 0 0 100 100 re f\n";
    let mut bodies: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R \
           /Resources << /ColorSpace << /CS0 [/ICCBased 5 0 R] >> >> >>"
            .to_vec(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()).into_bytes(),
    ];
    let mut stream = format!("<< /N 3 /Length {} >>\nstream\n", profile.len()).into_bytes();
    stream.extend_from_slice(profile);
    stream.extend_from_slice(b"\nendstream");
    bodies.push(stream);
    assemble(&bodies)
}

/// The colour of the one fill the page makes.
fn filled(file: Vec<u8>) -> (f64, f64, f64) {
    let doc = PdfDocument::open(file.into()).expect("the fixture opens");
    let mut recorder = Recorder::new();
    doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page interprets");
    let colour = recorder
        .events
        .iter()
        .find_map(|e| if let Event::Fill { color, .. } = e { Some(*color) } else { None })
        .expect("the page filled something");
    // `to_rgb` answers in RGB for every space this fixture can be in; the other three
    // arms are here because RR-15 Rule 5 forbids a wildcard over a domain enum, and a
    // colour that arrives as one of them is a defect worth the panic saying which.
    let fepdf_content::Color::Rgb(r, g, b) = colour.to_rgb() else {
        panic!("a fill came back as {:?}, which `to_rgb` should not produce", colour.to_rgb());
    };
    (r, g, b)
}

/// **The proof of reachability**: the same components in a P3 space and in the device
/// reading are different colours, and the page shows the managed one.
#[test]
fn a_display_p3_space_reaches_the_backend_converted() {
    let p3 = moxcms::ColorProfile::new_display_p3().encode().expect("P3 encodes");
    let (r, g, b) = filled(page_in_icc_space(&p3));

    // The device reading would be the components unchanged. P3's primaries are wider than
    // sRGB's, so the same numbers name a more saturated colour and converting them back
    // pushes at least one channel away from where it started.
    let distance = (r - 0.2).abs() + (g - 0.4).abs() + (b - 0.6).abs();
    assert!(
        distance > 0.02,
        "the profile was not consulted: got ({r:.3}, {g:.3}, {b:.3}) for 0.2 0.4 0.6"
    );
    for channel in [r, g, b] {
        assert!((-0.1..=1.1).contains(&channel), "a channel left the range: {channel}");
    }
}

/// And sRGB is the control: through its own profile a colour is itself, so the transform
/// is a conversion rather than a distortion.
#[test]
fn an_srgb_space_comes_back_where_it_started() {
    let srgb = moxcms::ColorProfile::new_srgb().encode().expect("sRGB encodes");
    let (r, g, b) = filled(page_in_icc_space(&srgb));
    for (out, want) in [(r, 0.2), (g, 0.4), (b, 0.6)] {
        assert!((out - want).abs() < 0.02, "sRGB through sRGB moved a channel: {out} for {want}");
    }
}

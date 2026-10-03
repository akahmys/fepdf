//! The two content-stream readers, held to the same conclusions.
//!
//! **There are two.** `fepdf-model`'s `Sublimator` lexes a content stream into
//! `Command`s during ingestion; `fepdf-content`'s `Interpreter` executes, either from
//! those `Command`s or — when ingestion did not refine the stream — by lexing the raw
//! bytes itself. Measured 2026-09-09, they handle **66 of the same operators**, the
//! `Sublimator` five more (`BMC`, `BDC`, `EMC`, `BX`, `EX`) and the `Interpreter` one the
//! other does not (`I`, an inline-image key rather than an operator).
//!
//! Which path a document takes depends only on `IngestionOptions::active_refinement`, so
//! the same bytes reach a different reader for a reason that has nothing to do with the
//! document. This file is the net for merging them: it asserts that they agree *before*
//! anything is moved, which is the only order in which a merge can be shown not to have
//! changed what a page draws.
//!
//! `crates/fepdf/tests/form_xobject_test.rs` asks the same question of the two form
//! paths, which were 66 identical lines out of 70 and are one function now.

use fepdf::{IngestionOptions, PdfDocument};
use fepdf_content::Color;
use fepdf_fixtures::assemble;
use fepdf_fixtures::recorder::{Event, Recorder};
use kurbo::{Affine, Shape};

/// Every call the page made, with the detail this file compares.
///
/// Rounded to one decimal so that a matrix differing in the last bit of an `f64` does not
/// read as two readers disagreeing.
fn calls(drawn: &Recorder) -> Vec<String> {
    drawn.events.iter().map(describe).collect()
}

fn describe(event: &Event) -> String {
    let shade = |color: &Color| match *color {
        Color::Rgb(r, g, b) => format!("rgb {r:.1},{g:.1},{b:.1}"),
        Color::Gray(l) => format!("gray {l:.1}"),
        Color::Cmyk(c, m, y, k) => format!("cmyk {c:.1},{m:.1},{y:.1},{k:.1}"),
        Color::Lab(l, a, b) => format!("lab {l:.1},{a:.1},{b:.1}"),
    };
    let boxed = |path: &kurbo::BezPath| {
        let b = path.bounding_box();
        format!("({:.1},{:.1},{:.1},{:.1})", b.x0, b.y0, b.x1, b.y1)
    };
    if let Event::Fill { path, color, rule, .. } = event {
        return format!("fill[{}|{rule:?}]{}", shade(color), boxed(path));
    }
    if let Event::Stroke { path, color, style, .. } = event {
        return format!("stroke[{}|w{:.1}]{}", shade(color), style.width, boxed(path));
    }
    if let Event::PushClip { path, rule, .. } = event {
        return format!("push_clip[{rule:?}]{}", boxed(path));
    }
    if let Event::Transform(t) | Event::SetTransform(t) = event {
        let c = t.as_coeffs();
        return format!(
            "{}({:.1},{:.1},{:.1},{:.1},{:.1},{:.1})",
            event.name(),
            c[0],
            c[1],
            c[2],
            c[3],
            c[4],
            c[5]
        );
    }
    if let Event::Text { glyphs, size, .. } = event {
        let text: String = glyphs.iter().map(|g| g.unicode.as_str()).collect();
        return format!("text[{size:.1}]({text})");
    }
    if let Event::FillColor(c) | Event::StrokeColor(c) = event {
        return format!("{}[{}]", event.name(), shade(c));
    }
    if let Event::FillAlpha(a) | Event::StrokeAlpha(a) = event {
        return format!("{}({a:.2})", event.name());
    }
    if let Event::Blend(mode) = event {
        return format!("blend({mode:?})");
    }
    if let Event::SetFont(name) = event {
        return format!("set_font({name})");
    }
    if let Event::CharSpacing(v) | Event::WordSpacing(v) = event {
        return format!("{}({v:.2})", event.name());
    }
    event.name().to_string()
}

/// Draws page 0 of `bytes`, with ingestion refining the stream or not.
fn draw(bytes: Vec<u8>, refine: bool) -> Recorder {
    let options = IngestionOptions { active_refinement: refine, ..IngestionOptions::default() };
    let doc = PdfDocument::open_with_options(bytes.into(), &options).expect("the fixture opens");
    let mut recorder = Recorder::new();
    doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page interprets");
    recorder
}

/// A one-page document whose content stream is `content`.
fn page(content: &str) -> Vec<u8> {
    assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R \
          /Resources << /Font << /F1 5 0 R >> /ExtGState << /G1 6 0 R >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        "<< /Type /ExtGState /ca 0.5 /CA 0.25 /BM /Multiply /LW 3 >>".to_string(),
    ])
}

/// Asserts the two readers agree, and that they were given something to disagree about.
fn both_paths_agree(what: &str, content: &str) {
    let file = page(content);
    let refined = draw(file.clone(), true);
    let raw = draw(file, false);
    assert!(
        refined.events.len() >= 2,
        "{what}: the refined path drew almost nothing, so agreeing means little: {:?}",
        calls(&refined)
    );
    assert_eq!(calls(&refined), calls(&raw), "{what}: the two content-stream readers disagree");
}

#[test]
fn the_two_readers_agree_on_path_construction_and_painting() {
    both_paths_agree(
        "paths",
        "0 0 1 rg 1 0 0 RG 2 w\n\
         10 10 m 90 10 l 90 90 l h f\n\
         20 20 m 80 80 l S\n\
         10 120 60 40 re f*\n\
         30 30 m 40 60 50 60 60 30 c 70 10 80 10 90 30 v s\n",
    );
}

#[test]
fn the_two_readers_agree_on_the_graphics_state() {
    both_paths_agree(
        "state",
        "q 2 0 0 2 10 10 cm 0.5 g 0 0 50 50 re f Q\n\
         q /G1 gs 0.2 0.4 0.6 rg 0 0 20 20 re f Q\n\
         q 1 J 1 j 4 M [3 2] 0 d 5 w 0 0 m 100 100 l S Q\n",
    );
}

#[test]
fn the_two_readers_agree_on_text() {
    both_paths_agree(
        "text",
        "BT /F1 12 Tf 2 Tc 1 Tw 100 Tz 3 Tr 2 Ts 14 TL\n\
         10 100 Td (First) Tj T* (Second) Tj\n\
         1 0 0 1 10 40 Tm [(Kerned) -200 (run)] TJ\n\
         (Quoted) ' 1 2 (Double) \"\n\
         ET\n",
    );
}

#[test]
fn the_two_readers_agree_on_colour_spaces() {
    both_paths_agree(
        "colour",
        "/DeviceRGB cs 0.1 0.2 0.3 sc 0 0 10 10 re f\n\
         /DeviceCMYK CS 0.1 0.2 0.3 0.4 SC 0 20 10 10 re S\n\
         0.9 0.8 0.7 0.6 k 0 40 10 10 re f\n\
         0.4 G 0 60 10 10 re S\n\
         0.3 0.6 0.9 sc 0 80 10 10 re f\n",
    );
}

#[test]
fn the_two_readers_agree_on_clipping_and_marked_content() {
    both_paths_agree(
        "clip",
        "q 0 0 100 100 re W n 0 g 0 0 200 200 re f Q\n\
         q 0 0 50 50 re W* n 0 0 200 200 re f Q\n\
         /Span BMC 0 0 10 10 re f EMC\n\
         BX /Unknown unk EX 0 20 10 10 re f\n",
    );
}

/// Two files this comparison does not open, and what that costs.
///
/// **A cost decision, measured rather than assumed.** Opening a document twice — once
/// refined, once not — is what this test pays for, and when these two were excluded they
/// were nine tenths of it: with them the binary ran 46 seconds and without them 4.3.
/// Re-introducing the `/Filter` defect this test was written for is still caught without
/// them, by `fugaku.pdf`, so the ninety per cent bought no detection of anything this has
/// found. Both are compared page for page against PDFKit by
/// `scripts/test/crosscheck_reading_order.sh`, which is where their size earns its keep.
///
/// **The 4.3 is now 9.1** — measured 2026-09-10, seven samples — because samples were
/// added afterwards, and the per-sample cost is worth having here because **file size
/// does not predict it**:
///
/// ```text
/// unicode_16.pdf     12M   3.43s
/// volvo_xc90.pdf     27M   1.88s
/// print_sample.pdf  3.5M   0.78s
/// ```
///
/// `volvo_xc90.pdf` is the largest file in `samples/` and costs half what a file less
/// than half its size does, so "exclude the largest" is not the rule this list follows.
/// The rule is the one above: measured cost against measured detection. Nothing has been
/// excluded on size, and `unicode_16.pdf` in particular stays — a test comparing two
/// readers of operators should not drop the sample richest in them.
///
/// Deleting these two names is how to put them back.
const TOO_SLOW_IN_A_DEBUG_BUILD: [&str; 2] = ["intel_sdm.pdf", "fy05.pdf"];

/// The samples, which is where an operator this file did not think of lives.
#[test]
fn the_two_readers_agree_on_the_sample_corpus() {
    let mut checked = 0;
    for entry in std::fs::read_dir("../../samples").expect("the sample directory") {
        let path = entry.expect("a directory entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("pdf") {
            continue;
        }
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
        if TOO_SLOW_IN_A_DEBUG_BUILD.contains(&file_name.as_str()) {
            continue;
        }
        let bytes = std::fs::read(&path).expect("the sample reads");
        let name = file_name;

        eprintln!("--- {name} refined");
        let refined = draw(bytes.clone(), true);
        eprintln!("--- {name} raw");
        let raw = draw(bytes, false);
        let (a, b) = (calls(&refined), calls(&raw));
        if a.len() != b.len() {
            let first =
                a.iter().zip(b.iter()).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
            eprintln!("{name}: refined={} raw={} first difference at {first}", a.len(), b.len());
            let from = first.saturating_sub(8);
            eprintln!("  refined[{from}..]: {:#?}", &a[from..(first + 3).min(a.len())]);
            eprintln!("  raw[{from}..]:     {:#?}", &b[from..(first + 3).min(b.len())]);
        }
        assert_eq!(a.len(), b.len(), "{name}: the two readers made a different number of calls");
        for (index, (left, right)) in a.iter().zip(b.iter()).enumerate() {
            assert_eq!(left, right, "{name}: call {index} differs");
        }
        checked += 1;
    }
    assert!(checked >= 7, "only {checked} samples were checked");
}

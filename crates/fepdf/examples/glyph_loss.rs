//! Which font loses which character codes, and how many glyphs that is (9.10.2).
//!
//! **The denominator lived nowhere.** `ROADMAP.md` §9 quotes an extraction loss of
//! *n* glyphs of 16,321,270, and the second number could not be re-derived from
//! anything the engine printed: `TextExtractionBackend` records its 9.10.2 violation
//! only on pages that lost something, so summing those messages counts the glyphs on
//! lossy pages and not the ones on the rest. This walks every page of every file given
//! and totals both.
//!
//! **And a count is not a direction.** "1,350 glyphs" says a document is lossy;
//! `20  DEEEKF+EdiF-uSKWqUMLei3cKo-01B  0x0003  c037  encoding  fy05.pdf p176` says where
//! to go and look. The route each glyph reached is carried beside it (`UnicodeSource`), so
//! a code the encoding answered with nothing and a code no route saw at all stay apart:
//! those are different defects, and separating `fy05.pdf`'s 1,350 that way is what
//! [ADR-0041] and [ADR-0042] are.
//!
//! ```text
//! cargo run --release -p fepdf --example glyph_loss -- samples/fy05.pdf
//! ```
//!
//! `--codes` adds one line per losing site: the font, the character code, the glyph name
//! the encoding gave it, the route that failed, and the first page it appears on.
//!
//! [ADR-0041]: ../../../docs/adr/0041-a-character-collection-is-declared-not-guessed.md
//! [ADR-0042]: ../../../docs/adr/0042-a-glyph-name-that-looks-like-a-character-code-is-not-one.md

use fepdf::{IngestionOptions, PdfDocument};
use fepdf_content::{
    BlendMode, Color, FallbackFontType, Paint, PixelFormat, RenderBackend, SMaskData, ShadingSpec,
    StrokeStyle, TextGlyph, TextState, WindingRule,
};
use fepdf_model::font::UnicodeSource;
use fepdf_model::graphics::TextRenderingMode;
use kurbo::{Affine, BezPath};
use std::collections::BTreeMap;
use std::sync::Arc;

/// One losing site: the font selected, the code drawn, the glyph name the encoding gave
/// it, and the route that failed.
///
/// The glyph name is in the key because it is the answer, not a detail: a code the
/// encoding answered with a name this engine could not turn into a character is a
/// different defect from one the encoding never named, and only the name says which.
type Site = (String, u32, Option<String>, &'static str);

/// Counts glyphs drawn and glyphs that reached no Unicode, by font and code.
#[derive(Default)]
struct Loss {
    seen: u64,
    lost: u64,
    replaced: u64,
    /// Resource name to `/BaseFont`, filled by `define_font` and read by `set_font`.
    base_names: BTreeMap<String, String>,
    current: String,
    sites: BTreeMap<Site, u64>,
    /// Whether an `/ActualText` section is open (14.9.4).
    ///
    /// **Without this the probe and the extractor disagree by 2,106.** A glyph inside a
    /// section that declares its own text is not lost when it cannot be named — the
    /// section says what it means, and `volvo_xc90.pdf` draws eight pages of Chinese and
    /// Thai as `.notdef` on exactly that understanding. Counting those would report a
    /// loss the extracted text does not have.
    replacing: usize,
    /// The page being drawn, so a site can name one to look at. A count says how much
    /// is lost; a page number is what makes it checkable against another reader.
    page: usize,
    first_page: BTreeMap<Site, usize>,
}

impl RenderBackend for Loss {
    fn show_text(
        &mut self,
        glyphs: &[TextGlyph],
        _size: f64,
        _transform: Affine,
        _state: TextState,
        _op_index: usize,
    ) {
        for glyph in glyphs {
            self.seen = self.seen.saturating_add(1);
            if self.replacing > 0 {
                self.replaced = self.replaced.saturating_add(1);
                continue;
            }
            if !glyph.unicode.is_empty() {
                continue;
            }
            self.lost = self.lost.saturating_add(1);
            let site =
                (self.current.clone(), glyph.char_code, glyph.name.clone(), glyph.source.name());
            self.first_page.entry(site.clone()).or_insert(self.page);
            *self.sites.entry(site).or_default() += 1;
        }
    }

    fn define_font(
        &mut self,
        name: &str,
        base_name: Option<&str>,
        _data: Option<Arc<Vec<u8>>>,
        _index: Option<usize>,
        _cid_to_gid_map: Option<BTreeMap<u32, u32>>,
        _fallback_type: FallbackFontType,
        _is_cid_keyed: bool,
    ) {
        self.base_names.insert(name.to_string(), base_name.unwrap_or(name).to_string());
    }

    fn begin_actual_text(&mut self, _text: &str) {
        self.replacing = self.replacing.saturating_add(1);
    }

    fn end_actual_text(&mut self) {
        self.replacing = self.replacing.saturating_sub(1);
    }

    fn set_font(&mut self, name: &str) {
        self.current = self.base_names.get(name).cloned().unwrap_or_else(|| name.to_string());
    }

    fn transform(&mut self, _transform: Affine) {}
    fn set_transform(&mut self, _transform: Affine) {}
    fn push_state(&mut self) {}
    fn pop_state(&mut self) {}
    fn fill_path(&mut self, _path: &BezPath, _color: &Color, _rule: WindingRule) {}
    fn stroke_path(&mut self, _path: &BezPath, _color: &Color, _style: &StrokeStyle) {}
    fn push_clip(&mut self, _path: &BezPath, _rule: WindingRule) {}
    fn pop_clip(&mut self) {}
    fn set_fill_alpha(&mut self, _alpha: f64) {}
    fn set_stroke_alpha(&mut self, _alpha: f64) {}
    fn set_fill_color(&mut self, _color: Color) {}
    fn set_stroke_color(&mut self, _color: Color) {}
    fn set_fill_paint(&mut self, _paint: &Paint) {}
    fn set_stroke_paint(&mut self, _paint: &Paint) {}
    fn paint_shading(&mut self, _shading: &ShadingSpec) {}
    fn set_blend_mode(&mut self, _mode: BlendMode) {}
    fn draw_image(
        &mut self,
        _image: &[u8],
        _width: u32,
        _height: u32,
        _format: PixelFormat,
        _smask: Option<SMaskData>,
    ) {
    }
    fn set_text_render_mode(&mut self, _mode: TextRenderingMode) {}
    fn set_char_spacing(&mut self, _spacing: f64) {}
    fn set_word_spacing(&mut self, _spacing: f64) {}
}

/// Every route, so a route that lost nothing prints a zero rather than vanishing.
const ROUTES: [UnicodeSource; 6] = [
    UnicodeSource::ToUnicode,
    UnicodeSource::Encoding,
    UnicodeSource::CidCollection,
    UnicodeSource::AsciiGuess,
    UnicodeSource::Withheld,
    UnicodeSource::Unmapped,
];

/// What one file lost, or `None` when it could not be opened at all.
fn measure(path: &str) -> Option<Loss> {
    let data = match std::fs::read(path) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("{path}: {e}");
            return None;
        }
    };
    let doc = match PdfDocument::open_with_options(data.into(), &IngestionOptions::default()) {
        Ok(doc) => doc,
        Err(e) => {
            eprintln!("{path}: {e:?}");
            return None;
        }
    };
    let pages = match doc.page_count() {
        Ok(pages) => pages,
        Err(e) => {
            eprintln!("{path}: {e:?}");
            return None;
        }
    };
    let mut loss = Loss::default();
    for index in 0..pages {
        loss.page = index + 1;
        // A page that will not interpret has still drawn what it drew, and the error is
        // not this probe's subject — `inspect text` is where it is reported.
        let _ = doc.render_page(index, &mut loss, Affine::IDENTITY);
    }
    Some(loss)
}

/// One line per file, and one for the total.
fn report(name: &str, lost: u64, seen: u64, replaced: u64) {
    println!("{name:<44} lost {lost:>9} of {seen:>11}   ({replaced} replaced under 14.9.4)");
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let codes = args.iter().any(|a| a == "--codes");
    args.retain(|a| a != "--codes");

    let (mut all_seen, mut all_lost, mut all_replaced) = (0_u64, 0_u64, 0_u64);
    let mut all_sites: BTreeMap<Site, u64> = BTreeMap::new();
    let mut all_pages: BTreeMap<Site, (String, usize)> = BTreeMap::new();

    for path in &args {
        let Some(loss) = measure(path) else { continue };
        report(&short(path), loss.lost, loss.seen, loss.replaced);
        all_seen = all_seen.saturating_add(loss.seen);
        all_lost = all_lost.saturating_add(loss.lost);
        all_replaced = all_replaced.saturating_add(loss.replaced);
        for (site, count) in loss.sites {
            let page = loss.first_page.get(&site).copied().unwrap_or(0);
            all_pages.entry(site.clone()).or_insert((short(path), page));
            *all_sites.entry(site).or_default() += count;
        }
    }

    report("TOTAL", all_lost, all_seen, all_replaced);
    println!();
    for route in ROUTES {
        let count: u64 =
            all_sites.iter().filter(|((_, _, _, r), _)| *r == route.name()).map(|(_, c)| c).sum();
        println!("  {:<16} {count:>9}", route.name());
    }

    if !codes {
        return;
    }
    println!();
    let mut ranked: Vec<(Site, u64)> = all_sites.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    for (site, count) in ranked {
        let (file, page) = all_pages.get(&site).cloned().unwrap_or_default();
        let (font, code, name, route) = site;
        let name = name.map_or_else(|| "-".to_string(), |n| n.trim_start_matches('/').to_string());
        println!("  {count:>7}  {font:<46} 0x{code:04X}  {name:<16} {route:<16} {file} p{page}");
    }
}

/// The last two path components, so a corpus listing stays readable.
fn short(path: &str) -> String {
    let mut parts: Vec<&str> = path.rsplit('/').take(2).collect();
    parts.reverse();
    parts.join("/")
}

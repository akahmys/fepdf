//! Any bytes, taken the way a caller takes a document: opened, its first page drawn, its
//! text extracted and the result saved. A refusal is an answer; a panic, a hang or an
//! allocation the machine cannot meet is a finding (ROADMAP Z-1).
#![no_main]

use fepdf_content::{
    BlendMode, Color, FallbackFontType, PixelFormat, RenderBackend, SMaskData, StrokeStyle,
    TextGlyph, TextState, WindingRule,
};
use fepdf_model::graphics::TextRenderingMode;
use kurbo::{Affine, BezPath};
use libfuzzer_sys::fuzz_target;
use std::sync::Arc;

/// The page is drawn into this, which counts calls and keeps nothing.
///
/// The fixtures' `Recorder` keeps every call, 168 bytes each, and a page nesting Type 3
/// glyphs up to the limit makes about six million: the run that ended out of memory was
/// measuring the recorder. What the engine does is what this measures.
#[derive(Default)]
struct Counter(u64);

impl RenderBackend for Counter {
    fn transform(&mut self, _: Affine) {
        self.0 += 1;
    }
    fn set_transform(&mut self, _: Affine) {
        self.0 += 1;
    }
    fn push_state(&mut self) {
        self.0 += 1;
    }
    fn pop_state(&mut self) {
        self.0 += 1;
    }
    fn fill_path(&mut self, _: &BezPath, _: &Color, _: WindingRule) {
        self.0 += 1;
    }
    fn stroke_path(&mut self, _: &BezPath, _: &Color, _: &StrokeStyle) {
        self.0 += 1;
    }
    fn push_clip(&mut self, _: &BezPath, _: WindingRule) {
        self.0 += 1;
    }
    fn pop_clip(&mut self) {
        self.0 += 1;
    }
    fn set_fill_alpha(&mut self, _: f64) {
        self.0 += 1;
    }
    fn set_stroke_alpha(&mut self, _: f64) {
        self.0 += 1;
    }
    fn set_fill_color(&mut self, _: Color) {
        self.0 += 1;
    }
    fn set_stroke_color(&mut self, _: Color) {
        self.0 += 1;
    }
    fn set_blend_mode(&mut self, _: BlendMode) {
        self.0 += 1;
    }
    fn draw_image(&mut self, _: &[u8], _: u32, _: u32, _: PixelFormat, _: Option<SMaskData>) {
        self.0 += 1;
    }
    fn define_font(
        &mut self,
        _: &str,
        _: Option<&str>,
        _: Option<Arc<Vec<u8>>>,
        _: Option<usize>,
        _: Option<std::collections::BTreeMap<u32, u32>>,
        _: FallbackFontType,
        _: bool,
    ) {
        self.0 += 1;
    }
    fn set_font(&mut self, _: &str) {
        self.0 += 1;
    }
    fn set_text_render_mode(&mut self, _: TextRenderingMode) {
        self.0 += 1;
    }
    fn set_char_spacing(&mut self, _: f64) {
        self.0 += 1;
    }
    fn set_word_spacing(&mut self, _: f64) {
        self.0 += 1;
    }
    fn show_text(&mut self, _: &[TextGlyph], _: f64, _: Affine, _: TextState, _: Option<usize>) {
        self.0 += 1;
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok(doc) = fepdf::PdfDocument::open(data.to_vec().into()) else { return };
    let _ = doc.render_page(0, &mut Counter::default(), Affine::IDENTITY);
    let _ = doc.extract_text(0);
    let path = std::env::temp_dir().join(format!("fepdf_fuzz_{}.pdf", std::process::id()));
    let _ = doc.save_with_options(&path, "2.0", &fepdf::SaveOptions::default());
    let _ = std::fs::remove_file(&path);
});

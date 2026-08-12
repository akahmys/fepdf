//! Renders sample pages for visual inspection.

use fepdf_render::VelloBackend;
use fepdf_sdk::{Interpreter, PdfDocument};
use kurbo::Affine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = std::fs::read("samples/fy05.pdf")?;
    let mut doc = PdfDocument::open(bytes::Bytes::from(data))?;

    // Load system fonts for fallback
    let system_fonts = VelloBackend::load_system_fonts();
    doc.set_system_fonts((*system_fonts).clone());

    let page = doc.get_page(0)?;

    println!("--- Rendering Page 1 of fy05.pdf ---");
    let mut backend = VelloBackend::new(system_fonts);

    // Page is fepdf_model::document::page::Page
    // We need its resources and contents.

    let mut interpreter =
        Interpreter::new(&mut backend, doc.inner(), page.resources_handle(), Affine::IDENTITY);

    for ch in page.contents_handles() {
        interpreter.execute(ch)?;
    }

    println!("--- Rendering Complete ---");

    Ok(())
}

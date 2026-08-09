//! Example to inspect fonts in sample PDF documents.

use fepdf_sdk::PdfDocument;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let samples_dir = Path::new("samples");
    for entry in std::fs::read_dir(samples_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("pdf") {
            println!("Inspecting {}...", path.display());
            let data = std::fs::read(&path)?;
            let doc = PdfDocument::open(data.into())?;

            if doc.page_count()? > 0 {
                let i = 0;
                let page = doc.get_page(i)?;
                let arena = doc.inner().arena();
                let resources_h = page.resources_handle();
                if let Some(dict) = arena.get_dict(resources_h)
                    && let Some(fonts_obj) = dict.get(&arena.name("Font"))
                    && let fepdf_core::Object::Dictionary(dh) = fonts_obj.resolve(arena)
                    && let Some(font_dict) = arena.get_dict(dh)
                {
                    for (name_h, _) in font_dict {
                        if let Some(name) = arena.get_name(name_h) {
                            println!("  Page {}: Font {}", i, name.as_str());
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

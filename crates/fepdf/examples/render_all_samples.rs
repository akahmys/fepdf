//! Example demonstrating rendering of all PDF samples in the samples directory.

use fepdf::Interpreter;
use fepdf::PdfDocument;
use fepdf_render::VelloBackend;
use fepdf_render::headless::render_to_image;
use image::ImageFormat;
use std::path::Path;

/// Main function for running the multi-sample rendering example.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let samples_dir = Path::new("samples");
    let entries = std::fs::read_dir(samples_dir)?;

    // Ensure the output directory exists
    let output_dir = Path::new("artifacts/renders");
    std::fs::create_dir_all(output_dir)?;

    let system_fonts = VelloBackend::load_system_fonts();

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("pdf") {
            let filename = path.file_name().unwrap().to_str().unwrap();
            println!("Rendering {filename}...");

            let data = std::fs::read(&path)?;
            let doc = PdfDocument::open(data.into())?;

            if doc.page_count()? > 0 {
                let page = doc.get_page(0)?;
                let mut backend = VelloBackend::new(system_fonts.clone());

                // Get media box
                let media_box = page.media_box();
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let width = media_box.width() as u32;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let height = media_box.height() as u32;

                if width == 0 || height == 0 {
                    println!("  Skipping {filename} due to zero dimension");
                    continue;
                }

                // PDF coordinates are bottom-up, Vello is top-down.
                let transform = kurbo::Affine::scale_non_uniform(1.0, -1.0)
                    * kurbo::Affine::translate((0.0, -f64::from(height)));

                let mut interpreter =
                    Interpreter::new(&mut backend, doc.inner(), page.resources_handle(), transform);
                for contents_h in page.contents_handles() {
                    let _ = interpreter.execute(contents_h);
                }

                let output_path = output_dir.join(format!("{filename}.png"));
                render_to_image(backend.scene(), width, height, &output_path, ImageFormat::Png)
                    .await?;
                println!("  Saved to {}", output_path.display());
            }
        }
    }

    Ok(())
}

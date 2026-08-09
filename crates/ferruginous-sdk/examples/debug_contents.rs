//! Example to debug contents of sample PDF documents.

use ferruginous_sdk::PdfDocument;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pdfs = ["samples/fy05.pdf", "samples/intel_sdm.pdf"];

    for pdf_path in pdfs {
        println!("Checking {pdf_path}...");
        let data = std::fs::read(pdf_path)?;
        let doc = PdfDocument::open(data.into())?;

        if doc.page_count()? > 0 {
            let page = doc.get_page(0)?;
            let handles = page.contents_handles();
            println!("  Page 0 content handles: {handles:?}");

            for (i, h) in handles.iter().enumerate() {
                let obj = doc.inner().arena().get_object(*h).unwrap();
                println!("  Handle {i}: {obj:?}");

                if let Some(sublimated) = doc.inner().arena().get_sublimated_data(*h) {
                    match &*sublimated {
                        ferruginous_core::object::SublimatedData::Commands { items: cmds } => {
                            println!("    Sublimated as Commands: {} commands", cmds.len());
                        }
                        ferruginous_core::object::SublimatedData::Raw(bytes) => {
                            println!("    Sublimated as Raw: {} bytes", bytes.len());
                        }
                        ferruginous_core::object::SublimatedData::Image { .. }
                        | ferruginous_core::object::SublimatedData::Compressed { .. } => {
                            println!("    Sublimated as Other: {sublimated:?}");
                        }
                    }
                } else {
                    println!("    No sublimated data found!");
                }
            }
        }
    }

    Ok(())
}

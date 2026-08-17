//! Example demonstrating how to dump text commands from a PDF.

use fepdf::PdfDocument;
use fepdf_model::object::sublimation::{Command, TextArrayItem};

/// Main function for running the text command dumping example.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let path_str = args.get(1).map_or("samples/bokutokitan.pdf", |s| s.as_str());

    println!("Checking {path_str}...");
    let data = std::fs::read(path_str)?;
    let doc = PdfDocument::open(data.into())?;
    doc.inner().arena().sublimate_all()?;

    if doc.page_count()? > 0 {
        let page = doc.get_page(0)?;
        let handles = page.contents_handles();

        for h in handles {
            if let Some(sublimated) = doc.inner().arena().get_sublimated_data(h)
                && let fepdf_model::object::SublimatedData::Commands { items: cmds } = &*sublimated
            {
                for cmd in cmds {
                    match cmd {
                        Command::ShowText(bytes) => {
                            println!("Tj: {bytes:?}");
                        }
                        Command::ShowTextArray(items) => {
                            print!("TJ: [");
                            for item in items {
                                match item {
                                    TextArrayItem::Text(b) => print!("{b:?}, "),
                                    TextArrayItem::Offset(o) => print!("{o}, "),
                                }
                            }
                            println!("]");
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    Ok(())
}

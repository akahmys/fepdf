//! Linearises each input into a directory, for `scripts/test/check_linearization.sh` to
//! hold against `qpdf --check-linearization` (ROADMAP Y-F31).
//!
//! `cargo run --release -p fepdf --example linearize -- OUT_DIR input.pdf...`

use fepdf::{PdfDocument, SaveOptions};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let out = args.next().ok_or("usage: linearize OUT_DIR input.pdf...")?;
    std::fs::create_dir_all(&out)?;
    // One fixed moment, so two runs over one input write the same bytes.
    let options = SaveOptions { stamped_at: Some(1_790_985_600), ..SaveOptions::default() };
    for input in args {
        let name = Path::new(&input).file_name().ok_or("an input names a file")?;
        let doc = PdfDocument::open(std::fs::read(&input)?.into())?;
        let target = Path::new(&out).join(name);
        if let Err(e) = doc.save_linearized(&target, "2.0", &options) {
            eprintln!("{input}: {e}");
        }
    }
    Ok(())
}

//! What the engine produces from each input, written to a directory so that two runs can be
//! compared file by file. `scripts/test/golden_outputs.sh` drives it (ROADMAP Y-0).
//!
//! Usage: `golden <out-dir> <input.pdf>...`
//!
//! Per input it writes the bytes of three saves and four readings. A step that fails
//! writes its error instead, so a change in *how* something fails is a difference too.
//!
//! Every save is stamped with one fixed moment (`SaveOptions::stamped_at`), which the XMP
//! `InstanceID` is derived from. One thing still varies by design: an encrypted save
//! draws its file key and salts at random (7.6.4.4), so it is opened again with its
//! password and saved plain, and those bytes are what is kept.

use fepdf::{IngestionOptions, PdfDocument, SaveOptions};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

const PASSWORD: &str = "golden";

/// The moment every save here is stamped with, so that two runs agree byte for byte.
const STAMP: u64 = 1_790_000_000;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().ok_or("usage: golden <out-dir> <input.pdf>...")?);
    std::fs::create_dir_all(&out)?;
    // One per process: the driver runs several at once into the same directory.
    let scratch = out.join(format!(".scratch-{}", std::process::id()));
    std::fs::create_dir_all(&scratch)?;
    for input in args {
        let input = PathBuf::from(input);
        let name = label(&input);
        let dir = out.join(&name);
        std::fs::create_dir_all(&dir)?;
        // A panic is an outcome like any other, and one input's must not hide the rest.
        let run = std::panic::catch_unwind(|| record(&input, &dir, &scratch.join(&name)));
        match run {
            Ok(written) => written?,
            Err(_) => std::fs::write(dir.join("panic.txt"), "panicked\n")?,
        }
    }
    std::fs::remove_dir_all(&scratch)?;
    Ok(())
}

/// The input's path with separators flattened, so two files of one name in two
/// directories stay two entries.
fn label(input: &Path) -> String {
    input.to_string_lossy().replace(['/', '\\'], "__")
}

fn record(input: &Path, dir: &Path, scratch: &Path) -> std::io::Result<()> {
    let data = std::fs::read(input)?;
    let doc = match PdfDocument::open(data.clone().into()) {
        Ok(doc) => doc,
        Err(e) => return std::fs::write(dir.join("open.err"), format!("{e:?}\n")),
    };
    std::fs::write(dir.join("decisions.txt"), format!("{:#?}\n", doc.decisions()))?;
    std::fs::write(dir.join("fonts.txt"), format!("{:#?}\n", doc.fonts()))?;
    std::fs::write(dir.join("text.txt"), text(&doc))?;
    let coverage = fepdf::Coverage::of(&data).map(|c| c.axes());
    std::fs::write(dir.join("coverage.txt"), format!("{coverage:#?}\n"))?;

    let options = SaveOptions { stamped_at: Some(STAMP), ..SaveOptions::default() };
    let plain = scratch.with_extension("plain.pdf");
    keep(doc.save_with_options(&plain, "2.0", &options), &plain, &dir.join("plain.pdf"))?;
    let lin = scratch.with_extension("lin.pdf");
    keep(doc.save_linearized(&lin, "2.0", &options), &lin, &dir.join("linearized.pdf"))?;
    encrypted(&doc, scratch, &dir.join("encrypted-reopened.pdf"))
}

/// Every page's text, each under its number, and a page that fails says how.
fn text(doc: &PdfDocument) -> String {
    let Ok(count) = doc.page_count() else { return "no page count\n".into() };
    let mut out = String::new();
    for page in 0..count {
        out.push_str("=== page ");
        out.push_str(&page.to_string());
        out.push('\n');
        match doc.extract_text(page) {
            Ok(text) => out.push_str(&text),
            // A `write!` into a `String` cannot fail.
            Err(e) => {
                let _ = write!(out, "error: {e:?}");
            }
        }
        out.push('\n');
    }
    out
}

/// Saved with a password, opened again with it, and saved plain.
fn encrypted(doc: &PdfDocument, scratch: &Path, to: &Path) -> std::io::Result<()> {
    let options = SaveOptions {
        password: Some(PASSWORD.into()),
        stamped_at: Some(STAMP),
        ..SaveOptions::default()
    };
    let locked = scratch.with_extension("enc.pdf");
    if let Err(e) = doc.save_with_options(&locked, "2.0", &options) {
        return std::fs::write(to.with_extension("err"), format!("save: {e:?}\n"));
    }
    let opening = IngestionOptions { password: Some(PASSWORD.into()), ..Default::default() };
    let reopened = match PdfDocument::open_with_options(std::fs::read(&locked)?.into(), &opening) {
        Ok(doc) => doc,
        Err(e) => return std::fs::write(to.with_extension("err"), format!("reopen: {e:?}\n")),
    };
    let plain = scratch.with_extension("reopened.pdf");
    let options = SaveOptions { stamped_at: Some(STAMP), ..SaveOptions::default() };
    let saved = reopened.save_with_options(&plain, "2.0", &options);
    keep(saved, &plain, to)
}

/// The saved bytes, or the error.
fn keep<T>(result: fepdf::PdfResult<T>, written: &Path, to: &Path) -> std::io::Result<()> {
    match result {
        Ok(_) => std::fs::copy(written, to).map(|_| ()),
        Err(e) => std::fs::write(to.with_extension("err"), format!("{e:?}\n")),
    }
}

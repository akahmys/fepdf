use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Common options for PDF ingestion/reading
// Each bool is one `--flag` that clap parses for us. Grouping them into a
// sub-struct would only move the same flags one level down and break the
// flattened command line.
#[allow(clippy::struct_excessive_bools)]
#[derive(clap::Args, Debug, Clone)]
pub struct IngestArgs {
    /// Disable active 2-pass refinement (UTF-8 normalization)
    #[arg(long)]
    pub no_refinement: bool,
    // Visible again: ADR-0007 hid this because nothing read `sublime_metadata`, and
    // said un-hiding was the last step of implementing it rather than the first.
    // ADR-0013 implemented it.
    /// Keep /Info and the metadata stream as the file has them, rather than settling
    /// them into one state and reporting where they disagree (14.3.3)
    #[arg(long)]
    pub no_metadata_recovery: bool,
    // Visible again: ADR-0007 hid this because nothing read `color_policy`.
    // Color space validation (Clause 8.6) in active refinement now reads it.
    /// Use relaxed color validation policy
    #[arg(long)]
    pub relaxed_color: bool,
    /// Force fallback to system fonts if embedded font parsing fails
    #[arg(long)]
    pub force_fallback: bool,
    // `IngestionOptions::password` was hardcoded `None` here, so every command that
    // opens a document could only ever open one with an empty user password.
    /// Password to open an encrypted document with
    #[arg(long)]
    pub password: Option<String>,
    /// DER certificate a public-key encrypted document (7.6.5) was addressed to
    #[arg(long = "recipient-certificate", requires = "recipient_key")]
    pub recipient_certificate: Option<PathBuf>,
    /// DER PKCS#8 private key for that certificate
    #[arg(long = "recipient-key", id = "recipient_key", requires = "recipient_certificate")]
    pub recipient_key: Option<PathBuf>,
}

impl From<IngestArgs> for fepdf::IngestionOptions {
    fn from(args: IngestArgs) -> Self {
        Self {
            active_refinement: !args.no_refinement,
            sublime_metadata: !args.no_metadata_recovery,
            color_policy: if args.relaxed_color {
                fepdf::ColorPolicy::Relaxed
            } else {
                fepdf::ColorPolicy::Strict
            },
            force_fallback: args.force_fallback,
            password: args.password,
            // Read here rather than in the engine: a path that does not exist is the
            // frontend's problem, and a certificate that does not parse is the
            // engine's. Keeping the two apart means the message names the right one.
            recipient: match (args.recipient_certificate, args.recipient_key) {
                (Some(certificate), Some(key)) => Some((
                    std::fs::read(&certificate)
                        .unwrap_or_else(|e| panic!("cannot read {}: {e}", certificate.display())),
                    std::fs::read(&key)
                        .unwrap_or_else(|e| panic!("cannot read {}: {e}", key.display())),
                )),
                _ => None,
            },
            progress_callback: None,
        }
    }
}

/// Common options for PDF writing/optimization
// Each bool is one `--flag` that clap parses for us. Grouping them into a
// sub-struct would only move the same flags one level down and break the
// flattened command line.
#[allow(clippy::struct_excessive_bools)]
#[derive(clap::Args, Debug, Clone)]
pub struct SaveArgs {
    // Was `--compress`, opting *in*. The GUI has always defaulted to compressing, so
    // the same operation behaved differently depending on which frontend asked for it —
    // and `publish upgrade` wrote 27 MB from a 15 MB source. The default lives in
    // `SaveOptions` now and this inverts it.
    /// Write streams uncompressed (FlateDecode is applied by default)
    #[arg(long)]
    pub no_compress: bool,
    // Hidden because it names a choice that does not exist. The writer traces from the
    // catalogue and writes only what it reaches, so an unreferenced object is dropped
    // whether this is passed or not: `samples/fy05.pdf` goes from a highest object
    // number of 4,680 to 4,575 either way. Unlike the flags below, the behaviour is
    // there — what is missing is the option to decline it.
    /// Remove unreachable objects
    #[arg(long, hide = true)]
    pub vacuum: bool,
    /// Strip descriptive metadata
    #[arg(long)]
    pub strip: bool,
    // Hidden, and renamed so it stops colliding with the one that works.
    //
    // The `id` matters as much as the flag. Both this and `IngestArgs::password`
    // defaulted their clap id to the field name, so every command flattening both —
    // `publish upgrade` and `publish sign` — panicked at startup. Only in debug builds,
    // because clap's duplicate check is a `debug_assert`, which is why release-only
    // verification never saw it.
    /// Encrypt the output with a password (AES-256)
    #[arg(long = "encrypt-password", id = "encrypt_password")]
    pub password: Option<String>,
    /// Password carrying owner rights, if it differs from the one that opens the file
    #[arg(long = "owner-password", id = "owner_password", requires = "encrypt_password")]
    pub owner_password: Option<String>,
    /// Encrypt the output to this DER certificate (7.6.5). Repeat for more recipients.
    /// Only the certificate is needed — encrypting to someone uses their public half
    #[arg(long = "encrypt-to", id = "encrypt_to", conflicts_with = "encrypt_password")]
    pub encrypt_to: Vec<PathBuf>,
    /// Write loose objects and a classic cross-reference table, instead of packing
    /// objects into object streams (7.5.7). Larger, and readable in a text editor
    #[arg(long = "no-obj-stm")]
    pub no_obj_stm: bool,
    /// Set the document's natural language, a BCP 47 tag (e.g. "en-US", "ja")
    #[arg(long)]
    pub lang: Option<String>,
    /// Override document title
    #[arg(long)]
    pub title: Option<String>,
    /// Override document author
    #[arg(long)]
    pub author: Option<String>,
    /// Set the copyright notice, written as `dc:rights` in the XMP packet
    #[arg(long)]
    pub copyright: Option<String>,
    // No longer hidden, and it was hidden for a reason that has gone: permissions live
    // in `/Encrypt` (7.6.4.2), and this engine wrote no `/Encrypt`. It writes one now,
    // so `/P` has somewhere to go. Without `--encrypt-password` there is still nowhere,
    // which is what `requires` says.
    // No clap constraint: `requires` takes one argument and `/P` needs *either*
    // encryption flag. `required_unless_present_any` says the opposite of what it reads
    // like — it made `--permissions` mandatory on every save — so the check is below,
    // where the message can say why.
    /// Grant only these permissions: print, modify, copy, annotate, forms,
    /// accessibility, assemble, print-high. Everything unnamed is denied. Needs one of
    /// --encrypt-password or --encrypt-to, since /P lives in /Encrypt
    #[arg(long)]
    pub permissions: Option<String>,
    /// Text string encoding for non-ASCII characters (utf16be, utf8)
    #[arg(long, default_value = "utf16be")]
    pub string_encoding: String,
    /// Perform simulation without writing output file
    #[arg(long)]
    pub dry_run: bool,
}

impl SaveArgs {
    /// Rejects a combination clap cannot express.
    ///
    /// `/P` lives in `/Encrypt` (7.6.4.2), so asking for permissions without asking for
    /// encryption is asking for a value with nowhere to go — which is the state
    /// `--permissions` was hidden in for as long as nothing wrote an `/Encrypt` at all.
    pub fn check(&self) -> Result<()> {
        if self.permissions.is_some() && self.password.is_none() && self.encrypt_to.is_empty() {
            anyhow::bail!(
                "--permissions needs --encrypt-password or --encrypt-to: /P lives in \
                 /Encrypt, and an unencrypted file has none"
            );
        }
        Ok(())
    }
}

impl From<SaveArgs> for fepdf::SaveOptions {
    fn from(args: SaveArgs) -> Self {
        Self {
            compress: !args.no_compress,
            compression_level: 9,
            vacuum: args.vacuum,
            strip: args.strip,
            password: args.password,
            owner_password: args.owner_password,
            recipients: args
                .encrypt_to
                .iter()
                .map(|p| {
                    std::fs::read(p).unwrap_or_else(|e| panic!("cannot read {}: {e}", p.display()))
                })
                .collect(),
            obj_stm: !args.no_obj_stm,
            lang: args.lang,
            title: args.title,
            author: args.author,
            copyright: args.copyright,
            permissions: args.permissions,
            string_encoding: match args.string_encoding.to_lowercase().as_str() {
                "utf8" => fepdf::StringEncoding::Utf8,
                _ => fepdf::StringEncoding::Utf16BE,
            },
            creation_date: None,
            dry_run: args.dry_run,
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "fepdf")]
#[command(author = "fepdf Developers")]
#[command(version)]
#[command(about = "fepdf: The Universal PDF Toolkit for Compliance, Optimization, and Manipulation", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Inspect document characteristics (Read-Only)
    Inspect {
        #[command(subcommand)]
        sub: InspectSubcommands,
    },
    /// Edit document pages and structure (Interactive & Structural Edit)
    Edit {
        #[command(subcommand)]
        sub: EditSubcommands,
    },
    /// Publish final compliance-certified outputs
    Publish {
        #[command(subcommand)]
        sub: PublishSubcommands,
    },
    /// Low-level debugging and inspection tools
    Debug {
        #[command(subcommand)]
        sub: DebugSubcommands,
    },
    /// Display open source credits and licenses
    Credits,
}

#[derive(Subcommand, Debug)]
pub enum InspectSubcommands {
    /// Display document information and font summary
    Info {
        /// Input PDF file
        input: PathBuf,
        /// Output format (text, json, markdown)
        #[arg(short, long, default_value = "text")]
        format: String,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
    },
    /// Perform detailed compliance audit (UA-2, ISO 32000-2)
    Audit {
        /// Input PDF file
        input: PathBuf,
        /// Output format (text, json, markdown)
        #[arg(short, long, default_value = "text")]
        format: String,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
    },
    /// Extract text content
    Text {
        /// Input PDF file
        input: PathBuf,
        /// Pages to extract text from (comma-separated or range, e.g., 1-5)
        #[arg(short, long)]
        pages: Option<String>,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
    },
    /// Report every catalogue entry (7.7.2), and what the engine can make of it
    Catalog {
        /// Input PDF file
        input: PathBuf,
        /// Output format (text, json, markdown)
        #[arg(short, long, default_value = "text")]
        format: String,
        /// List every Table 29 key, including those this file does not carry
        #[arg(long)]
        all: bool,
    },
    /// Report what protects the document (7.6), and how far the engine conforms
    Encryption {
        /// Input PDF file
        input: PathBuf,
        /// Output format (text, json, markdown)
        #[arg(short, long, default_value = "text")]
        format: String,
        /// Password to open the document with
        #[arg(long, default_value = "")]
        password: String,
        /// DER certificate a public-key encrypted document (7.6.5) was addressed to
        #[arg(long, requires = "key")]
        certificate: Option<PathBuf>,
        /// DER PKCS#8 private key for that certificate
        #[arg(long, id = "key", requires = "certificate")]
        private_key: Option<PathBuf>,
    },
    /// Report interactive features: annotations, form fields, actions, outline
    Interactive {
        /// Input PDF file
        input: PathBuf,
        /// Output format (text, json, markdown)
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Report file layout: revisions, cross-reference form, object storage, decisions
    Structure {
        /// Input PDF file
        input: PathBuf,
        /// Output format (text, json, markdown)
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Dump hierarchical logical structure tree
    Tree {
        /// Input PDF file
        input: PathBuf,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
    },
    /// Report how much of what these files present the engine reads the contents of
    Coverage {
        /// Input PDF files. More than one is the point: the figure is over a corpus.
        inputs: Vec<PathBuf>,
        /// Output format (text, json, markdown)
        #[arg(short, long, default_value = "text")]
        format: String,
        /// List every construct that has no reader, per axis
        #[arg(long)]
        unread: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum EditSubcommands {
    /// Merge multiple PDF files into one
    Merge {
        /// Input PDF files
        inputs: Vec<PathBuf>,
        /// Output PDF file
        #[arg(short, long)]
        output: PathBuf,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
        /// Output optimization options
        #[command(flatten)]
        save: SaveArgs,
    },
    /// Split or extract pages from a PDF
    Split {
        /// Input PDF file
        input: PathBuf,
        /// Output directory or file pattern
        #[arg(short, long)]
        output: PathBuf,
        /// Page range (e.g., 1-5, 10)
        #[arg(long)]
        pages: Option<String>,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
        /// Output optimization options
        #[command(flatten)]
        save: SaveArgs,
    },
    /// Rotate specific pages in the document
    Rotate {
        /// Input PDF file
        input: PathBuf,
        /// Output PDF file
        #[arg(short, long)]
        output: PathBuf,
        /// Pages to rotate (comma-separated, e.g., 1,3-5) (default: all)
        #[arg(short, long)]
        pages: Option<String>,
        /// Rotation angle (90, 180, 270)
        #[arg(short, long)]
        angle: i32,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
        /// Output optimization options
        #[command(flatten)]
        save: SaveArgs,
    },
    /// Attempt to repair a corrupted PDF document
    Repair {
        /// Input corrupted PDF file
        input: PathBuf,
        /// Output repaired PDF file
        output: PathBuf,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
        /// Output optimization options
        #[command(flatten)]
        save: SaveArgs,
    },
    /// Heuristically re-tag the document logical structure for UA-2
    Tag {
        /// Input PDF file
        input: PathBuf,
        /// Output repaired PDF file (Explicitly required)
        #[arg(short, long)]
        output: PathBuf,
        /// Enable interactive Wizard Mode
        #[arg(short, long)]
        wizard: bool,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
        /// Output optimization options
        #[command(flatten)]
        save: SaveArgs,
    },
    /// Create a PDF Portfolio / Collection
    Portfolio {
        /// Output PDF portfolio file
        #[arg(short, long)]
        output: PathBuf,
        /// Input files to embed into portfolio
        #[arg(short, long)]
        files: Vec<PathBuf>,
        /// Optional cover page PDF
        #[arg(long)]
        cover: Option<PathBuf>,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
        /// Output optimization options
        #[command(flatten)]
        save: SaveArgs,
    },
    /// Apply Bates numbering to PDF pages
    Bates {
        /// Input PDF file
        input: PathBuf,
        /// Output PDF file
        #[arg(short, long)]
        output: PathBuf,
        /// Bates prefix (e.g. "CONFIDENTIAL-")
        #[arg(long, default_value = "")]
        prefix: String,
        /// Starting number
        #[arg(long, default_value_t = 1)]
        start_number: u64,
        /// Total digits count for padding (e.g. 6)
        #[arg(long, default_value_t = 6)]
        digits: usize,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
        /// Output optimization options
        #[command(flatten)]
        save: SaveArgs,
    },
    /// Attach an Associated File (/AF) to PDF
    Attach {
        /// Input PDF file
        input: PathBuf,
        /// Output PDF file
        #[arg(short, long)]
        output: PathBuf,
        /// File to attach
        #[arg(long)]
        file: PathBuf,
        /// Semantic relationship (Source, Data, Supplement, Alternative)
        #[arg(long, default_value = "Data")]
        relationship: String,
        /// MIME type (e.g. "text/xml")
        #[arg(long, default_value = "application/octet-stream")]
        mime_type: String,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
        /// Output optimization options
        #[command(flatten)]
        save: SaveArgs,
    },
    /// Set page numbering labels (/PageLabels)
    PageLabel {
        /// Input PDF file
        input: PathBuf,
        /// Output PDF file
        #[arg(short, long)]
        output: PathBuf,
        /// Label style (decimal, lower-roman, upper-roman, lower-alpha, upper-alpha)
        #[arg(long, default_value = "decimal")]
        style: String,
        /// Optional label prefix
        #[arg(long)]
        prefix: Option<String>,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
        /// Output optimization options
        #[command(flatten)]
        save: SaveArgs,
    },
    /// Set GIS geographic anchor (/Geo)
    Geo {
        /// Input PDF file
        input: PathBuf,
        /// Output PDF file
        #[arg(short, long)]
        output: PathBuf,
        /// Latitude in degrees
        #[arg(long)]
        lat: f64,
        /// Longitude in degrees
        #[arg(long)]
        lon: f64,
        /// Coordinate Reference System WKT
        #[arg(long, default_value = "GEOGCS[\"WGS 84\"]")]
        crs: String,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
        /// Output optimization options
        #[command(flatten)]
        save: SaveArgs,
    },
}

#[derive(Subcommand, Debug)]
pub enum PublishSubcommands {
    /// Upgrade document to PDF 2.0 and modern standards (A-4, X-6, UA-2)
    Upgrade {
        /// Input PDF file
        input: PathBuf,
        /// Output PDF file
        output: PathBuf,
        /// Target standard (a4, x6, ua2)
        #[arg(long)]
        standard: Option<String>,
        /// Optional ICC color profile path
        #[arg(long)]
        icc_profile: Option<PathBuf>,
        /// Opt-in for Fast Web View (Linearization)
        #[arg(long)]
        linearize: bool,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
        /// Output optimization options
        #[command(flatten)]
        save: SaveArgs,
    },
    /// Render a PDF page to an image (PNG, JPEG)
    Render {
        /// Input PDF file
        input: PathBuf,
        /// Output image file (format detected from extension)
        output: PathBuf,
        /// Page number to render (default 1)
        #[arg(short, long, default_value_t = 1)]
        page: usize,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
    },
    /// Sign this engine's output with a certificate and key
    ///
    /// What is signed is the file this command writes, not the input: the engine
    /// normalises a document at load, so the input's bytes no longer exist by the time
    /// there is anything to sign (ADR-0014). Both files must be DER — convert PEM with
    /// `openssl x509 -outform der` and `openssl pkcs8 -topk8 -nocrypt -outform der`.
    Sign {
        /// Input PDF file
        input: PathBuf,
        /// Output signed PDF file
        output: PathBuf,
        /// DER-encoded X.509 certificate
        #[arg(long)]
        certificate: PathBuf,
        /// DER-encoded PKCS#8 private key
        #[arg(long)]
        private_key: PathBuf,
        /// Reason for signing
        #[arg(long)]
        reason: Option<String>,
        /// Location of signing
        #[arg(long)]
        location: Option<String>,
        /// Signer name, used only if the certificate states none
        #[arg(long)]
        name: Option<String>,
        /// Page number carrying the signature field (default 1)
        #[arg(long, default_value_t = 1)]
        page: usize,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
        /// Output optimization options
        #[command(flatten)]
        save: SaveArgs,
    },
    /// Check every signature a file carries
    ///
    /// Reports whether each signature verifies over the bytes its `/ByteRange` names,
    /// and whether that range is the whole file. It does not decide whether the
    /// certificate should be trusted — see the note it prints.
    VerifySignature {
        /// Input PDF file
        input: PathBuf,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
    },
}

#[derive(Subcommand, Debug)]
pub enum DebugSubcommands {
    /// Dump a specific PDF object
    Dump {
        /// Input PDF file
        input: PathBuf,
        /// Object ID to dump
        #[arg(long)]
        obj: u32,
        /// Gen number (default 0)
        #[arg(long, default_value_t = 0)]
        gen_num: u16,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
    },
    /// Display arena memory and object statistics
    Stats {
        /// Input PDF file
        input: PathBuf,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
    },
    /// Extract raw font data
    FontExtract {
        /// Input PDF file
        input: PathBuf,
        /// Object ID of the font
        obj_num: u32,
        /// Output file path
        output: PathBuf,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
    },
    /// Trace glyph mapping for a specific character
    TraceGlyph {
        /// Input PDF file
        input: PathBuf,
        /// Unicode character or hex code (e.g., "A" or "U+0041")
        #[arg(short, long)]
        unicode: String,
        /// Specific font name to trace (optional, scans all if omitted)
        #[arg(short, long)]
        font: Option<String>,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Covers the mapping from flags to options and nothing beyond it.
    ///
    /// `color_policy` is asserted here and read by nobody (ADR-0007). This test passed
    /// throughout, because setting a field correctly is all it ever claimed. Whether an
    /// option *does* anything is a question about ingestion, not about `From`, and it
    /// is answered by varying one flag and comparing the documents that come out —
    /// which for `sublime_metadata`, now that ADR-0013 has made it live, the tests in
    /// `fepdf_model::metadata` do.
    #[test]
    fn test_ingest_args_conversion() {
        let args = IngestArgs {
            no_refinement: true,
            no_metadata_recovery: false,
            relaxed_color: true,
            force_fallback: false,
            password: None,
            recipient_certificate: None,
            recipient_key: None,
        };
        let opts: fepdf::IngestionOptions = args.into();
        assert!(!opts.active_refinement);
        assert!(opts.sublime_metadata);
        assert_eq!(opts.color_policy, fepdf::ColorPolicy::Relaxed);
    }
}

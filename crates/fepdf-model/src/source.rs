//! Where documents come from.
//!
//! [`DocumentSource`] is the boundary between "read some bytes" and "have a
//! normalised [`Document`]". Today there is exactly one implementation,
//! [`PdfSource`]; the trait exists so that the file-format knowledge it needs stays
//! on one side of that line rather than inside `Document` itself.
//!
//! # Scope
//!
//! This is deliberately the smallest thing that names the boundary. It is **not** a
//! plugin system: there is no registry, no dynamic dispatch, and no format feature
//! flags, because an interface designed against a single implementation is almost
//! always wrong for the second one. When a second source exists, its real
//! requirements can reshape this.
//!
//! # What a second source would owe
//!
//! A source must hand back a `Document` whose arena already holds a catalogue, a page
//! tree, content streams and font resources. For a format like DOCX that means
//! resolving styles, breaking lines, paginating and generating content streams — a
//! layout engine. Implementing this trait is the small part of that work; almost
//! nothing but font handling is shared with reading PDF.

use crate::document::Document;
use crate::error::PdfResult;
use bytes::Bytes;

/// Turns bytes of some document format into a normalised [`Document`].
pub trait DocumentSource {
    /// Options this source understands.
    ///
    /// Associated rather than shared: `password` and `color_policy` mean something
    /// to PDF and nothing to a word-processor format, and the reverse would hold for
    /// page size or margin defaults.
    type Options: Default;

    /// Short name of the format, for diagnostics.
    const FORMAT: &'static str;

    /// Whether `bytes` look like this format.
    ///
    /// Cheap and heuristic: it decides which source to try, not whether the input is
    /// valid. A source that accepts the sniff may still fail to load.
    fn sniff(bytes: &[u8]) -> bool;

    /// Builds a document, recording any interpretation decisions on the way.
    fn load(bytes: Bytes, options: &Self::Options) -> PdfResult<Document>;
}

/// Reads PDF files (ISO 32000-2, and earlier versions on a best-effort basis).
pub struct PdfSource;

/// How far into a file the `%PDF-` header may appear.
///
/// Files routinely arrive with bytes prepended by mail gateways and scanners, and
/// readers are expected to scan for the header rather than demand it at offset zero.
const HEADER_SEARCH_WINDOW: usize = 1024;

impl DocumentSource for PdfSource {
    type Options = crate::ingest::IngestionOptions;

    const FORMAT: &'static str = "PDF";

    fn sniff(bytes: &[u8]) -> bool {
        let window = &bytes[..bytes.len().min(HEADER_SEARCH_WINDOW + 5)];
        window.windows(5).any(|w| w == b"%PDF-")
    }

    fn load(bytes: Bytes, options: &Self::Options) -> PdfResult<Document> {
        Document::open(bytes, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_accepts_a_header_at_the_start() {
        assert!(PdfSource::sniff(b"%PDF-2.0\n1 0 obj"));
    }

    #[test]
    fn sniff_accepts_a_header_preceded_by_junk() {
        // Prepended bytes are common in the wild; the header is still there.
        let mut data = vec![b'x'; 300];
        data.extend_from_slice(b"%PDF-1.7\n");
        assert!(PdfSource::sniff(&data));
    }

    #[test]
    fn sniff_gives_up_beyond_the_search_window() {
        let mut data = vec![b'x'; HEADER_SEARCH_WINDOW + 64];
        data.extend_from_slice(b"%PDF-1.7\n");
        assert!(!PdfSource::sniff(&data));
    }

    #[test]
    fn sniff_rejects_other_formats() {
        assert!(!PdfSource::sniff(b"PK\x03\x04"), "a zip container is not a PDF");
        assert!(!PdfSource::sniff(b""));
        assert!(!PdfSource::sniff(b"%PD"));
    }
}

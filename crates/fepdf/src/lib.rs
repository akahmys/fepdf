#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::redundant_field_names,
    clippy::collapsible_if,
    clippy::match_like_matches_macro,
    clippy::cast_possible_wrap,
    clippy::assign_op_pattern,
    clippy::too_many_arguments
)]
//! fepdf SDK: High-level PDF processing library.
//!
//! This crate provides a high-level, easy-to-use interface for PDF document
//! manipulation, rendering, and auditing, abstracting away the low-level
//! complexities of the core type system and document model.

use crate::remediation::HeuristicEngine;
use bytes::Bytes;
pub use fepdf_content::FallbackFontType;
pub use fepdf_model::document::fallback_fonts;
pub use fepdf_model::font::{GlyphTrace, TraceContext};
// Re-exported so frontends need no dependency on fepdf-model at all: with the model
// unreachable by name, ARCHITECTURE.md Rule A is enforced by Cargo rather than by
// review.
pub use fepdf_model::catalog::{CatalogEntry, CatalogReport, Support};
pub use fepdf_model::cms::RecipientIdentity;
pub use fepdf_model::decrypt::Credentials;
pub use fepdf_model::destination::{Destination, Lookup, NamedDestinations, Target, View};
// The whole of `/ViewerPreferences`, not just the struct: its fields are public and
// typed, so a frontend that cannot name `Duplex` cannot read `duplex` — and naming
// `fepdf_model` to get it is what Rule A forbids.
pub use fepdf_model::actions::{ActionReport, Capability, ReachableAction, Says, Trigger};
pub use fepdf_model::catalog::ABSENT_FROM_BOTH_CORPORA as CATALOGUE_KEYS_NO_CORPUS_CARRIES;
pub use fepdf_model::coverage::{AXES as COVERAGE_AXES, AxisCoverage, Coverage};
pub use fepdf_model::document::{
    Direction, Duplex, PageBoundary, PageLayout, PageMode, PrintScaling, ViewerPreferences,
};
pub use fepdf_model::encryption::{
    Conformance, CryptFilter, EncryptedPayload, EncryptionReport, Permission, permission_keywords,
};
pub use fepdf_model::file_structure::{
    FileStructure, FilterUse, ObjectCensus, ObjectStream, Revision,
};
pub use fepdf_model::font::FontResource;
pub use fepdf_model::graphics::Rect;
pub use fepdf_model::ingest::{ColorPolicy, IngestionOptions};
pub use fepdf_model::interactive::{
    AnnotationCensus, AnnotationEntry, ChoiceOption, DestinationCensus, FormField, FormFields,
    InteractiveReport, Outline as OutlineSummary, SubtypeCensus,
};
pub use fepdf_model::interactive::{calculation_order, field_value, form_of};
pub use fepdf_model::interpretation::{Decision, DecisionLog, Severity, Strictness};
pub use fepdf_model::optional_content::{LayerId, LayerPanel, LayerRow};
pub use fepdf_model::security::{Access, AesV5Spec, SecurityHandler};
pub use fepdf_model::signature::{SignatureCheck, SignatureReport};
pub use fepdf_model::{
    AFRelationship, AnnotationKind, AnnotationSpec, ArticleBead, ArticleThread, AssociatedFile,
    CollectionViewMode, Document, FormFieldSpec, FormValue, GeoSpatialAnchor, Handle, LayerGroup,
    MeasurementScale, Missing, Object, OptionalContentProperties, OutlineNode, OutlineTree,
    OutputIntent, Page, PageLabelSpec, PageLabelStyle, PdfAction, PdfArena, PdfError, PdfName,
    PdfResult, PortfolioCollection, PortfolioItem, ShapeForm, SublimatedData, TransitionSpec,
    TransitionStyle, UnencryptedWrapperSpec, UserProperty, UserPropertyValue, VisibilityState,
};
pub use fepdf_model::{DocumentSource, PdfSource};
#[cfg(feature = "render")]
pub use fepdf_render::budget;
#[cfg(feature = "render")]
pub use fepdf_render::{VelloBackend, headless::Rasteriser};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Arc;

/// The object cloning module for object migration (owned by `fepdf-doc`).
pub mod cloning {
    pub use fepdf_doc::cloning::*;
}
/// The interpreter module for processing content streams (owned by `fepdf-content`).
pub mod interpreter {
    pub use fepdf_content::interpreter::*;
}
pub use fepdf_content::{Interpreter, Type3Advance};
/// Unified operation vocabulary for canonical document mutations (owned by `fepdf-doc`).
pub mod operation {
    pub use fepdf_doc::operation::*;
}
/// The remediation module for structural repair (owned by `fepdf-doc`).
pub mod remediation {
    pub use fepdf_doc::remediation::*;
}
/// Reading the runs of a page, which are what a text edit names (owned by `fepdf-doc`).
///
/// A frontend that changes text has to show a caller the runs to choose between first;
/// `edit_run` was reachable through the MCP server for a day with no way to get a run
/// number, which is naming a thing by guessing at its index.
pub mod text {
    pub use fepdf_doc::apply::text::*;
}
/// The box on the page that a rectangle in the page's picture shows.
///
/// Left, top, right, bottom in pixels, through `pixel_to_page` — the inverse of
/// [`PdfDocument::page_to_pixels`], as six matrix numbers — into left, bottom, right,
/// top in points.
///
/// Every corner is taken through, and the upright rectangle round them answered, so a
/// page turned by `/Rotate` gives the box it should.
#[must_use]
pub fn pixel_box_on_page(rect: [f64; 4], pixel_to_page: [f64; 6]) -> [f64; 4] {
    let to = kurbo::Affine::new(pixel_to_page);
    let corners = [(rect[0], rect[1]), (rect[2], rect[1]), (rect[2], rect[3]), (rect[0], rect[3])]
        .map(|(x, y)| to * kurbo::Point::new(x, y));
    let low = |v: [f64; 4]| v.iter().copied().fold(f64::MAX, f64::min);
    let high = |v: [f64; 4]| v.iter().copied().fold(f64::MIN, f64::max);
    let (xs, ys) = (corners.map(|p| p.x), corners.map(|p| p.y));
    [low(xs), low(ys), high(xs), high(ys)]
}

/// Comparing two documents, page by page (ROADMAP W-18).
pub mod compare;
/// A page as pixels: rendered to a file, to a region, or to an image of a given
/// resolution.
#[cfg(feature = "render")]
mod raster;
/// Drawing a page: its content interpreted, and the annotations a reader would show over
/// it.
mod rendering;
/// Writing a document out: the copy a save takes, its metadata, encryption, linearisation
/// and signing.
mod saving;
/// The scale a drawing declares for measuring on it (12.9, ROADMAP W-16).
pub mod measure {
    pub use fepdf_doc::measure::{Fraction, NumberFormat, Scale, format, holding, polygon_area};
}
/// The text of a tagged document in reading order, for a synthesiser (ROADMAP W-19a).
pub mod reading {
    pub use fepdf_doc::reading::{Passage, Reading, Spoken};
}
/// The objects a page draws with `Do`, and where (ROADMAP W-E5).
pub mod xobject {
    pub use fepdf_doc::apply::xobject::*;
}
/// The structure module for UA-2 logical tree handling (owned by `fepdf-audit`).
pub mod structure {
    pub use fepdf_audit::structure::*;
}
/// Logical structure tree visitor and presentation data (owned by `fepdf-doc`).
pub mod struct_tree {
    pub use fepdf_doc::struct_tree::*;
}
pub use fepdf_audit::Outcome;
pub use fepdf_audit::{
    AuditFinding,
    // The protocol's own wording for the conditions it leaves to a person, which the
    // window shows beside what was checked (W-21d).
    LeftToAPerson,
    MatterhornAuditor,
    StructureVisitor,
};
/// What a redaction will remove, read before it is applied.
pub use fepdf_doc::apply::redact::Removal;
pub use fepdf_doc::operation::{
    CropRegion, FieldKind, NewField, PageArrangement, PageDivision, Redaction, TabOrder,
    TextLayerItem, WhatFallsOutside, XObjectEdit,
};
pub use fepdf_doc::{
    Align,
    AttributeValue,
    ContentScale,
    DecorationPosition,
    Operation,
    OutlineReport,
    PageResize,
    PageSelection,
    // `PdfStandard` moved here from this file when `Operation::Upgrade` came to carry it:
    // a type an operation holds has to live with the vocabulary.
    PdfStandard,
    // The same reason as `PdfStandard` below: `Operation::MoveStructElem` carries it.
    Placement,
    Quarter,
    RotateMode,
    StructAttribute,
    StructElemMove,
    StructElemUpdate,
    StructElemWrap,
    StructureTreeNode,
    StructureTreeVisitor,
    apply_operation,
};
/// The internal writer module for generating PDF files.
pub mod writer;

/// Supported text string encodings for PDF output.
pub use fepdf_model::StringEncoding;

/// Options for saving a PDF document.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub struct SaveOptions {
    /// Whether to compress streams using FlateDecode.
    pub compress: bool,
    /// The compression level to use (0-9).
    pub compression_level: u32,
    /// Whether to strip descriptive metadata.
    pub strip: bool,
    /// Encrypt the output, and the password that opens it.
    pub password: Option<String>,
    /// The password that carries owner rights, when it differs from the one that opens
    /// the document. Ignored unless `password` is set.
    pub owner_password: Option<String>,
    /// DER certificates to encrypt the output to (7.6.5), one entry per recipient.
    ///
    /// Certificates and no keys: encrypting to someone needs their public half and
    /// nothing of yours. Mutually exclusive with `password` — a document takes one
    /// handler, and 7.6.4 and 7.6.5 are different handlers.
    pub recipients: Vec<Vec<u8>>,
    /// Pack objects into object streams (7.5.7), with the cross-reference stream
    /// (7.5.8) they require. **On by default**: it makes every corpus file smaller and
    /// `samples/intel_sdm.pdf` less than half the size, and two independent readers get
    /// the same text out either way ([ADR-0016]).
    ///
    /// [ADR-0016]: ../../../../docs/adr/0016-objects-are-packed-by-default.md
    pub obj_stm: bool,
    /// The catalogue's `/Lang` (14.9.2.1) — a BCP 47 tag such as `en-GB` or `ja`.
    pub lang: Option<String>,
    /// Override document title.
    pub title: Option<String>,
    /// Override document author.
    pub author: Option<String>,
    /// `dc:rights` in the XMP packet.
    pub copyright: Option<String>,
    /// Override creation date.
    pub creation_date: Option<String>,
    /// PDF permission flags (e.g., "print,copy").
    pub permissions: Option<String>,
    /// Preferred text string encoding for non-ASCII characters.
    pub string_encoding: StringEncoding,
    /// Simulate saving and report results without writing to disk.
    pub dry_run: bool,
    /// The moment this save speaks for, in seconds since the Unix epoch; the clock when
    /// `None`. The XMP `InstanceID` is derived from it, so one document saved twice with
    /// the same value is the same bytes, and `SOURCE_DATE_EPOCH` has somewhere to go.
    pub stamped_at: Option<u64>,
}

impl Default for SaveOptions {
    /// Streams are compressed.
    ///
    /// This was `#[derive(Default)]`, so `compress` was `false`, and the two frontends
    /// disagreed about the same operation: `fepdf-gui` sets `export_compress: true`
    /// while `fepdf-cli` passed its `--compress` flag straight through, defaulting off.
    /// Nothing recorded a reason for either. `ARCHITECTURE.md` §4 has the rotate case as
    /// the previous instance of this exact shape, which is what Rule D exists to stop.
    ///
    /// The default matters: `publish upgrade` on `samples/fy05.pdf` wrote 27 MB from a
    /// 15 MB source, and 8.4 MB with compression on. A tool whose ordinary output is
    /// larger than its input has picked the wrong default, whatever the flag says.
    fn default() -> Self {
        Self {
            compress: true,
            compression_level: 9,
            strip: false,
            password: None,
            owner_password: None,
            recipients: Vec::new(),
            obj_stm: true,
            lang: None,
            title: None,
            author: None,
            copyright: None,
            creation_date: None,
            permissions: None,
            string_encoding: StringEncoding::default(),
            dry_run: false,
            stamped_at: None,
        }
    }
}

impl SaveOptions {
    /// The moment this save speaks for: `stamped_at`, or the clock.
    fn stamp(&self) -> u64 {
        self.stamped_at.unwrap_or_else(fepdf_model::metadata::seconds_now)
    }
}

/// Where a page's box lands in a picture of it.
///
/// The transform from default user space to the picture, `scale` pixels to a unit, origin
/// at the picture's top left and y running down; and the picture's width and height in
/// units, which are the box's, swapped when the page is turned a quarter.
///
/// **Turned, not reflected, and from the box's own corner.** `/Rotate` turns the page
/// clockwise as it is shown (Table 31), so a quarter turn puts the box's bottom left at
/// the picture's top left and its top left at the top right. This table was written twice,
/// here and in the window, and both copies drew `90` and `270` as mirror images and took
/// every box to start at `(0, 0)`, until 2026-09-29. A rotation that is not a multiple of
/// 90 is not one Table 31 allows, and is drawn upright.
#[must_use]
pub fn page_display_transform(
    r: fepdf_model::graphics::Rect,
    rotation: i32,
    scale: f64,
) -> (kurbo::Affine, f64, f64) {
    let (x0, y0) = (r.x1.min(r.x2), r.y1.min(r.y2));
    let (w, h) = ((r.x2 - r.x1).abs(), (r.y2 - r.y1).abs());
    let s = scale;
    match rotation.rem_euclid(360) {
        // x' = s(y - y0), y' = s(x - x0)
        90 => (kurbo::Affine::new([0.0, s, s, 0.0, -s * y0, -s * x0]), h, w),
        // x' = s(w - (x - x0)), y' = s(y - y0)
        180 => (kurbo::Affine::new([-s, 0.0, 0.0, s, s * (w + x0), -s * y0]), w, h),
        // x' = s(h - (y - y0)), y' = s(w - (x - x0))
        270 => (kurbo::Affine::new([0.0, -s, -s, 0.0, s * (h + y0), s * (w + x0)]), h, w),
        // x' = s(x - x0), y' = s(h - (y - y0))
        _ => (kurbo::Affine::new([s, 0.0, 0.0, -s, -s * x0, s * (h + y0)]), w, h),
    }
}

/// Refuses to write a header for any version but 2.0.
///
/// **This engine writes PDF 2.0 and chose not to write anything else** (ROADMAP, the
/// subsets: a PDF writer, for 2.0 only). What it writes is 2.0 whatever the header says —
/// AES-256 R6, an XMP packet, object streams — and the header was whatever string the
/// caller passed, so the window's export wrote `%PDF-1.7` over 2.0 content whenever its
/// "Upgrade to PDF 2.0" box was cleared: a file claiming a version it is not.
fn written_version(version: &str) -> PdfResult<()> {
    if version == "2.0" {
        return Ok(());
    }
    Err(PdfError::refused(
        "save",
        format!("this engine writes PDF 2.0 only, and {version:?} was asked for"),
    ))
}

/// `/M`, the time of signing, in the form 7.9.4 defines.
///
/// Local time with its offset, because that is what the clause asks for and what a
/// reader shows; `%:z` would give `+09:00` where PDF writes `+09'00`.
fn pdf_now() -> String {
    let now = chrono::Local::now();
    let offset = now.offset().local_minus_utc();
    let (sign, seconds) = if offset < 0 { ('-', -offset) } else { ('+', offset) };
    format!(
        "D:{}{sign}{:02}'{:02}",
        now.format("%Y%m%d%H%M%S"),
        seconds / 3600,
        seconds % 3600 / 60
    )
}

/// Options for digitally signing a PDF document.
#[derive(Debug, Clone, Default)]
pub struct SignOptions {
    /// Reason for signing.
    pub reason: Option<String>,
    /// Location of signing.
    pub location: Option<String>,
    /// Contact information for the signer.
    pub contact_info: Option<String>,
    /// Common Name (CN) of the signer.
    pub name: Option<String>,
    /// DER-encoded certificate (X.509).
    pub certificate: Option<Vec<u8>>,
    /// PEM or DER encoded private key.
    pub private_key: Option<Vec<u8>>,
    /// Page index (0-based) to place the signature widget.
    pub page_index: usize,
}

/// Summary of document properties and structural health.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DocumentSummary {
    /// The PDF version string.
    pub version: String,
    /// The total number of pages.
    pub page_count: usize,
    /// Extracted document metadata.
    pub metadata: MetadataInfo,
    /// List of fonts used in the document.
    pub fonts: Vec<FontSummary>,
    /// Summary of structural compliance issues.
    pub compliance: ComplianceSummary,
    /// What the engine decided where the input departed from the standard, with the
    /// severities it assigned (ARCHITECTURE.md §4.3). Present in every output format,
    /// not only the audit's prose.
    pub decisions: Vec<Decision>,
}

pub use fepdf_model::font::FontSummary;
pub use fepdf_model::metadata::MetadataInfo;

/// Summary of a digital signature present in a document.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SignatureSummary {
    /// Object ID index of the signature dictionary.
    pub object_id: u32,
    /// Signer name if specified.
    pub signer_name: Option<String>,
}

/// Structural compliance overview.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ComplianceSummary {
    /// List of compliance issues found.
    pub issues: Vec<ComplianceIssue>,
    /// List of ISO 32000-2 clauses validated in the document.
    pub iso_clauses: Vec<String>,
}

/// A specific compliance violation or observation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ComplianceIssue {
    /// The standard being checked (e.g., "PDF/UA-2").
    pub standard: String,
    /// The severity of the issue.
    pub severity: IssueSeverity,
    /// A descriptive message.
    pub message: String,
}

/// Severity of a compliance issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum IssueSeverity {
    /// Information or observation.
    Info,
    /// Potential issue but not a strict violation.
    Warning,
    /// Violation of a core requirement.
    Error,
    /// Critical violation making the document invalid or inaccessible.
    Critical,
}

/// High-level entry point for interacting with a PDF document.
pub struct PdfDocument {
    inner: Document,
}

impl PdfDocument {
    /// Creates a new PDF 2.0 document holding one blank Letter page.
    ///
    /// **The offsets are counted, not typed.** This was a byte string with its
    /// cross-reference table written out by hand, and all four numbers in it were wrong:
    /// object 2 was declared at 60 and lay at 58, object 3 at 120 and lay at 115, and
    /// `startxref` pointed one byte before the table. It opened because ingestion repaired
    /// it — four decisions every time, ending with the catalogue being found by scanning
    /// for `/Type /Catalog` because the trailer had been lost with the rest — so a
    /// function eleven callers use to start from nothing started them from a salvage.
    ///
    /// A file this small is not worth a writer, but it is worth a loop that adds up the
    /// bytes it has emitted.
    pub fn create_empty() -> PdfResult<Self> {
        Self::open_with_options(blank_document(), &fepdf_model::ingest::IngestionOptions::default())
    }

    /// Opens a PDF document from a byte buffer with default ingestion options.
    pub fn open(data: Bytes) -> PdfResult<Self> {
        Self::open_with_options(data, &fepdf_model::ingest::IngestionOptions::default())
    }

    /// Opens a PDF document with custom ingestion options.
    pub fn open_with_options(
        data: Bytes,
        options: &fepdf_model::ingest::IngestionOptions,
    ) -> PdfResult<Self> {
        Ok(Self::sealed(Document::open(data, options)?))
    }

    /// A document built through the model layer, handed to the facade: from here it
    /// changes only through [`Self::apply`], as one opened here does. What was done to
    /// it before is the builder's (ROADMAP Y-11).
    #[must_use]
    pub fn from_document(inner: Document) -> Self {
        Self::sealed(inner)
    }

    /// The facade's document over `inner`, its arena sealed: from here it changes only
    /// through [`Self::apply`], and a write anywhere else panics in a debug build
    /// (ROADMAP Y-11).
    fn sealed(inner: Document) -> Self {
        inner.arena().seal();
        Self { inner }
    }

    /// Returns the internal document.
    pub fn inner(&self) -> &Document {
        &self.inner
    }

    /// What an interactive processor should present for optional content, and in what
    /// order (6.3.2.3, 8.11.4.3).
    ///
    /// Empty when the document declares no layers *and* when its default configuration
    /// carries no `/Order` — the clause makes those the same answer, because `/Order`
    /// decides membership and defaults to an empty array.
    #[must_use]
    pub fn layers(&self) -> LayerPanel {
        let state = fepdf_model::optional_content::OptionalContentState::read(&self.inner);
        LayerPanel::read(&self.inner, &state)
    }

    /// Turns a layer on or off for viewing, honouring `/Locked` and `/RBGroups`.
    ///
    /// Returns `false` and changes nothing when the group is locked. **Not a document
    /// change**: `save` writes the same bytes either way, which is why this takes `&self`
    /// and is not an `Operation` (Rule D is about editing a document; a reader turning a
    /// layer off is not).
    pub fn set_layer_visible(&self, panel: &LayerPanel, layer: LayerId, on: bool) -> bool {
        panel.set(&self.inner, layer, on)
    }

    /// Forgets every layer toggle, returning to what the configuration says.
    pub fn reset_layer_visibility(&self) {
        self.inner.reset_layer_visibility();
    }

    /// What the engine decided where the input departed from the standard.
    ///
    /// Reaching these previously meant asking for a whole [`DocumentSummary`], which
    /// walks the fonts and runs the compliance audit. A caller that only wants to know
    /// whether the file was conforming should not have to pay for that.
    ///
    /// A snapshot, not a borrow: the log grows while the document is used — an image
    /// skipped during text extraction is recorded when it happens — so what this
    /// returns is what had been decided by the time it was called (ADR-0018).
    #[must_use]
    pub fn decisions(&self) -> Vec<Decision> {
        self.inner.decisions.entries()
    }

    /// Returns the total number of pages.
    pub fn page_count(&self) -> PdfResult<usize> {
        self.inner.page_count()
    }

    /// Retrieves a specific page by its 0-based index.
    pub fn get_page(&self, index: usize) -> PdfResult<fepdf_model::document::page::Page<'_>> {
        self.inner.get_page(index)
    }

    // --- Rule D: there are no other document mutators here, and that is the check. ---
    //
    // Ten used to sit in this block: `add_ltv_info`, `swap_pages`, `reorder_page`,
    // `reorder_pages_batch`, `remove_page`, `duplicate_page`, `insert_pages_from`,
    // `upgrade_to_standard`, `retag_document` and `set_page_rotation`. Each exposed a
    // mutation the `Operation` vocabulary either already carried or should have, so a
    // frontend could leave the vocabulary without re-implementing anything — which is not
    // the escape ARCHITECTURE §4.1's Rule D was written to block, and it was taken at
    // eight call sites across `fepdf-gui` and `fepdf-cli`.
    //
    // §7 called Rule D "enforced by construction" while nothing enforced it. It is
    // enforced by construction now: `apply` is the only way in, so the alternative is
    // unrepresentable rather than discouraged — the same move `RotateMode` and `Quarter`
    // made for the rotate divergence that created the rule.
    //
    // `status.sh` counts the `&mut self` methods in this file that are not `apply` and
    // not the four that configure saving. The row expects 0. Adding a mutating method
    // here is what makes it fail, which is the point: the check reads one file rather
    // than grepping four frontends for method names, and so cannot be fooled by a
    // frontend that happens to define a method of the same name.

    /// Sets the system fallback fonts for the document (Phase 4).
    pub fn set_system_fonts(&mut self, fonts: BTreeMap<FallbackFontType, Arc<Vec<u8>>>) {
        self.inner.system_fonts = Arc::new(fonts);
        self.inner.normalize_resources();
    }

    /// Attempts to open and repair a PDF document with custom options.
    pub fn open_and_repair_with_options(
        data: Bytes,
        options: &fepdf_model::ingest::IngestionOptions,
    ) -> PdfResult<Self> {
        Ok(Self::sealed(Document::open_repair(data, options)?))
    }

    /// Merges multiple documents into a new one.
    pub fn merge(sources: Vec<PdfDocument>) -> PdfResult<Self> {
        let inners: Vec<&Document> = sources.iter().map(|s| &s.inner).collect();
        Ok(Self::sealed(fepdf_doc::assembly::merge(&inners)?))
    }

    /// Extracts specific pages into a new document.
    pub fn extract_pages(&self, indices: Vec<usize>) -> PdfResult<Self> {
        Ok(Self::sealed(fepdf_doc::assembly::extract_pages(&self.inner, &indices)?))
    }

    /// Returns the physical viewport of the page (MediaBox).
    pub fn get_page_box(&self, index: usize) -> PdfResult<fepdf_model::graphics::Rect> {
        let page = self.inner.get_page(index)?;
        let box_obj =
            page.resolve_attribute("CropBox").or_else(|| page.resolve_attribute("MediaBox"));

        if let Some(mb) = box_obj
            && let Some(arr_handle) = mb.as_array()
            && let Some(arr) = self.inner.arena().get_array(arr_handle)
            && let [x1, y1, x2, y2, ..] = arr.as_slice()
        {
            let x1 = x1.resolve(self.inner.arena()).as_f64().unwrap_or(0.0);
            let y1 = y1.resolve(self.inner.arena()).as_f64().unwrap_or(0.0);
            let x2 = x2.resolve(self.inner.arena()).as_f64().unwrap_or(595.0);
            let y2 = y2.resolve(self.inner.arena()).as_f64().unwrap_or(842.0);
            return Ok(fepdf_model::graphics::Rect::new(x1, y1, x2, y2));
        }
        Ok(fepdf_model::graphics::Rect::new(0.0, 0.0, 595.0, 842.0)) // Default A4
    }

    /// Returns the physical dimensions of the page (Width, Height), accounting for page rotation.
    pub fn get_page_size(&self, index: usize) -> PdfResult<(f64, f64)> {
        let r = self.get_page_box(index)?;
        let w = (r.x2 - r.x1).abs();
        let h = (r.y2 - r.y1).abs();
        let rot = self.get_page_rotation(index)?;
        if rot == 90 || rot == 270 { Ok((h, w)) } else { Ok((w, h)) }
    }

    /// Returns a list of potential structural remediations for the document.
    pub fn get_remediation_candidates(
        &self,
    ) -> PdfResult<Vec<crate::remediation::RemediationCandidate>> {
        let engine = HeuristicEngine::new();
        engine.infer_structure(&self.inner)
    }

    /// Extracts Unicode text from a specific page.
    pub fn extract_text(&self, index: usize) -> PdfResult<String> {
        let mut backend = crate::remediation::TextExtractionBackend::new();
        self.render_page(index, &mut backend, kurbo::Affine::IDENTITY)?;
        Ok(backend.finish())
    }

    /// Extracts TextSpans from a specific page.
    pub fn extract_spans(&self, index: usize) -> PdfResult<Vec<crate::remediation::TextSpan>> {
        let mut collector = crate::remediation::CollectorBackend::new();
        self.render_page(index, &mut collector, kurbo::Affine::IDENTITY)?;
        Ok(collector.spans)
    }

    /// Where each `/MCID` on a page drew, in default user space (14.7.4.2).
    ///
    /// The page is interpreted to find out. There is no cheaper answer: a mark's box is
    /// the union of what was drawn between its `BDC` and its `EMC`, and only running the
    /// content stream says what that was.
    pub fn marked_content_boxes(
        &self,
        index: usize,
    ) -> PdfResult<std::collections::BTreeMap<u32, kurbo::Rect>> {
        let mut backend = fepdf_doc::marked_content::MarkBoundsBackend::new();
        self.render_page(index, &mut backend, kurbo::Affine::IDENTITY)?;
        Ok(backend.into_bounds())
    }

    /// The scale that holds at `point` on `page`, in default user space, when a viewport
    /// there declares a rectilinear one (12.9.1).
    ///
    /// A distance is measured in the scale of its first point, as 12.9.1 says.
    #[must_use]
    pub fn scale_at(&self, page: usize, point: (f64, f64)) -> Option<measure::Scale> {
        fepdf_doc::measure::scale_at(&self.inner, page, point)
    }

    /// Every rectilinear scale `page` declares, in its viewport order (12.9.1); pick the
    /// one for a point with [`measure::holding`].
    #[must_use]
    pub fn scales_on(&self, page: usize) -> Vec<measure::Scale> {
        fepdf_doc::measure::scales_on(&self.inner, page)
    }

    /// The document as a synthesiser reads it (ROADMAP W-19a).
    ///
    /// The structure tree's passages in its order, each in its language, and the
    /// pronunciation lexicons its root names.
    ///
    /// **A read, not an `Operation`**, like `extract_text`: nothing about the document
    /// changes. An untagged document has no reading order to give and answers no passages
    /// — reading it in drawing order would be a guess this engine would be making for it.
    ///
    /// Each page an element's marks sit on is interpreted once.
    #[must_use]
    pub fn reading(&self) -> reading::Reading {
        let lexicons = fepdf_doc::reading::lexicons(&self.inner);
        let Some(root) = self.extract_struct_tree() else {
            return reading::Reading { lexicons, passages: Vec::new() };
        };
        let plan = fepdf_doc::reading::Plan::of(&root);
        let mut composed = std::collections::BTreeMap::new();
        for index in plan.pages() {
            let mut backend = fepdf_doc::reading::MarkTextBackend::new(plan.passages_on(index));
            let _ = self.render_page(index, &mut backend, kurbo::Affine::IDENTITY);
            composed.insert(index, backend.into_text());
        }
        reading::Reading { lexicons, passages: plan.passages(&composed) }
    }

    /// Gives the tree's elements the rectangles their marked content drew in.
    ///
    /// **A second call, not part of `extract_struct_tree`.** The tree is read out of the
    /// arena and costs nothing; this interprets every page the tree mentions, and a
    /// caller that only wants the tags should not pay for geometry it will not draw.
    ///
    /// Pages are interpreted once each, in order, and only those an element with an
    /// `/MCID` actually sits on.
    ///
    /// **A page that will not interpret is skipped rather than failing the walk**, and
    /// that is not a swallowed error: the same page fails to draw, where the reader can
    /// see it, and `render_page` records what it could not run. The alternative is a
    /// document whose every element loses its rectangle because one page of forty is
    /// malformed.
    pub fn fill_structure_boxes(&self, root: &mut StructureTreeNode) {
        let mut wanted = std::collections::BTreeSet::new();
        collect_marked_pages(root, &mut wanted);
        let mut boxes = std::collections::BTreeMap::new();
        for index in wanted {
            if let Ok(found) = self.marked_content_boxes(index) {
                boxes.insert(index, found);
            }
        }
        fepdf_doc::marked_content::fill_boxes(root, &boxes);
    }

    /// Prints a textual representation of the logical structure tree.
    pub fn print_structure(&self) -> PdfResult<String> {
        let Some(root) = self.extract_struct_tree() else {
            return Ok("No logical structure found.".into());
        };
        let mut out = String::new();
        write_struct_node(&root, 0, &mut out);
        Ok(out)
    }

    /// Renders one indirect object as human-readable text, for `fepdf debug dump`.
    ///
    /// Takes the object number rather than an arena handle so that callers need no
    /// knowledge of the storage model (ARCHITECTURE.md Rule A). Streams are reported
    /// with their dictionary, raw length, and decoded payload where it is small
    /// enough to be useful.
    pub fn describe_object(&self, obj_id: u32) -> PdfResult<String> {
        let arena = self.inner.arena();
        match Object::Reference(Handle::new(obj_id)).resolve(arena) {
            Object::Dictionary(h) => Ok(arena.get_dict(h).map_or_else(
                || format!("Object {obj_id}: dictionary not present in arena"),
                |dict| format!("Type: Dictionary\n{}", describe_dict_entries(arena, &dict, true)),
            )),
            Object::Stream(h, data) => Ok(arena.get_dict(h).map_or_else(
                || format!("Object {obj_id}: stream dictionary not present in arena"),
                |dict| {
                    let mut out =
                        format!("Type: Stream\n{}", describe_dict_entries(arena, &dict, false));
                    out.push_str(&describe_stream_payload(arena, &data, &dict));
                    out
                },
            )),
            other => Ok(format!("{other:?}")),
        }
    }

    /// Returns the rotation angle (0, 90, 180, 270) of a specific page.
    pub fn get_page_rotation(&self, index: usize) -> PdfResult<i32> {
        let page = self.inner.get_page(index)?;
        if let Some(Object::Integer(angle)) = page.resolve_attribute("Rotate") {
            #[allow(clippy::cast_possible_truncation)]
            let normalized = (angle % 360) as i32;
            return Ok(normalized.rem_euclid(360));
        }
        Ok(0)
    }

    /// The size of this page's default user space unit, in multiples of 1/72 inch (14.11.2,
    /// Table 31).
    ///
    /// **It does not change the coordinate system, only what a coordinate means.** The
    /// content is still drawn in the units the boxes are written in; a `/UserUnit` of 10
    /// says each of those units is ten seventy-seconds of an inch, so a 400 by 400 page is
    /// 55 inches square rather than 5.5. It is what lets a drawing exceed the 14,400-unit
    /// limit a box can express.
    ///
    /// **Not inheritable.** Table 31 lists it in the page dictionary, and unlike
    /// `/MediaBox` or `/Rotate` it is not among the inheritable attributes of Table 30, so
    /// this reads the page's own entry and does not walk up the page tree.
    ///
    /// A value that is absent, unreadable or not positive gives 1.0, which Table 31 makes
    /// the default.
    pub fn get_page_user_unit(&self, index: usize) -> PdfResult<f64> {
        let page = self.inner.get_page(index)?;
        let dict_handle = self.inner.resolve_to_dict(page.obj_handle())?;
        let arena = self.inner.arena();
        let Some(dict) = arena.get_dict(dict_handle) else { return Ok(1.0) };
        let Some(entry) = dict.get(&arena.name("UserUnit")) else { return Ok(1.0) };
        let unit = match entry.resolve(arena) {
            Object::Integer(i) => i as f64,
            Object::Real(r) => r,
            _ => return Ok(1.0),
        };
        Ok(if unit.is_finite() && unit > 0.0 { unit } else { 1.0 })
    }

    /// Extracts the presentation-ready logical structure tree.
    pub fn extract_struct_tree(&self) -> Option<StructureTreeNode> {
        StructureTreeVisitor::extract(&self.inner)
    }

    /// Reads the bookmark tree (12.3.3), with a count of what the read cost.
    ///
    /// The counterpart of `Operation::UpdateOutlines`, which had none: an editor could
    /// only ever write an outline over whatever a document already had. The report says
    /// how many items were read, how many named no page of this document, and whether a
    /// `/Next` chain doubled back.
    #[must_use]
    pub fn outlines(&self) -> (OutlineTree, OutlineReport) {
        fepdf_doc::read_outlines(&self.inner)
    }

    /// Applies a canonical mutation operation to the document.
    pub fn apply(&mut self, op: Operation) -> PdfResult<()> {
        apply_operation(&mut self.inner, op)
    }

    /// Performs a structural health audit for PDF/UA-2.
    ///
    /// **What it looked at is in the report beside what it found**, because an empty list
    /// of findings from fourteen failure conditions of 137 says almost nothing and used to
    /// be indistinguishable from a document that conforms. Use
    /// [`fepdf_audit::AuditReport::found_nothing`] and read the scope; do not read
    /// `findings.is_empty()` as "conforms".
    ///
    /// **The whole decision is the auditor's.** This assembled a report of its own for a
    /// document with no structure tree, under the checkpoint number `00-001` — a number
    /// the protocol does not have, which is the defect W-21e took out of the auditor,
    /// surviving here because the scope test only ever read a document that had a tree.
    /// It also meant the five conditions that are properties of the catalogue went
    /// unasked about a file that has a catalogue like any other.
    ///
    /// # Errors
    /// Fails when the structure tree cannot be read.
    pub fn audit_ua2_report(&self) -> PdfResult<fepdf_audit::AuditReport> {
        MatterhornAuditor::new(&self.inner).audit_report()
    }

    /// The findings alone, for a caller that has read the scope elsewhere.
    ///
    /// # Errors
    /// Fails when the structure tree cannot be read.
    pub fn audit_ua2(&self) -> PdfResult<Vec<AuditFinding>> {
        Ok(self.audit_ua2_report()?.findings)
    }

    /// The version the document is, as `M.m`: its header's, or the catalogue's `/Version`
    /// where that is later (7.7.2).
    ///
    /// **Read, not assumed.** The summary said `2.0` of every document — `let pdf_20 =
    /// true`, commented as an inference — so `inspect info` and the window's document
    /// panel reported `samples/constitution.pdf`, whose header is 1.2, as a PDF 2.0 file.
    /// What this engine *writes* is 2.0; what it read is what this answers.
    #[must_use]
    pub fn effective_version(&self) -> String {
        let parsed = self.inner.header_version.as_deref().and_then(|v| {
            let (major, minor) = v.split_once('.')?;
            Some((major.trim().parse::<u32>().ok()?, minor.trim().parse::<u32>().ok()?))
        });
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let from_header = parsed.unwrap_or_else(|| {
            let tenths = (self.inner.arena().version() * 10.0).round() as u32;
            (tenths / 10, tenths % 10)
        });
        let declared = self
            .inner
            .catalog()
            .ok()
            .and_then(|catalog| catalog.version)
            .and_then(|version| version.numbers())
            .map(|(major, minor)| (u32::from(major), u32::from(minor)));
        let (major, minor) = declared.filter(|d| *d > from_header).unwrap_or(from_header);
        format!("{major}.{minor}")
    }

    /// Returns a comprehensive summary of the document.
    pub fn get_summary(&self) -> PdfResult<DocumentSummary> {
        let findings = self.audit_ua2()?;
        let mut issues = Vec::new();
        for f in findings {
            // **A condition examined and not broken is not an issue.** Since W-21g made
            // it a row of the report, every clean condition arrived here as a
            // `ComplianceIssue` of severity `Warning` — the `_` arm below reads "Pass"
            // that way — so a conforming document listed one warning per check that
            // passed. What a summary's issue list answers is what is wrong.
            if f.outcome == fepdf_audit::Outcome::Sound {
                continue;
            }
            issues.push(ComplianceIssue {
                standard: "PDF/UA-2".into(),
                severity: match f.severity.as_str() {
                    "Error" => IssueSeverity::Error,
                    "Critical" => IssueSeverity::Critical,
                    _ => IssueSeverity::Warning,
                },
                message: format!("[{}] {}", f.checkpoint, f.message),
            });
        }

        // Run structural ISO compliance audit
        let auditor = fepdf_model::audit::compliance::ComplianceAuditor::new(&self.inner);
        let report = auditor.audit();
        let mut iso_clauses: Vec<String> =
            report.clauses_encountered.iter().map(|&s| s.to_string()).collect();
        iso_clauses.sort();

        for issue in report.issues {
            issues.push(ComplianceIssue {
                standard: "ISO 32000-2".into(),
                severity: IssueSeverity::Warning,
                message: issue,
            });
        }

        Ok(DocumentSummary {
            version: self.effective_version(),
            page_count: self.page_count()?,
            metadata: self.inner.metadata(),
            fonts: self.inner.fonts(),
            compliance: ComplianceSummary { issues, iso_clauses },
            decisions: self.inner.decisions.entries(),
        })
    }

    /// Returns the document's refined metadata.
    pub fn metadata(&self) -> fepdf_model::metadata::MetadataInfo {
        self.inner.metadata()
    }

    /// Returns the active security handler method name if encrypted.
    pub fn security_method(&self) -> String {
        self.inner.security_method.clone()
    }

    /// What is lost by writing this document out, when its `/P` said not to.
    ///
    /// Reports rather than refuses; see `Document::permissions_lost_on_write`.
    #[must_use]
    pub fn permissions_lost_on_write(&self) -> Option<Decision> {
        self.inner.permissions_lost_on_write()
    }

    /// Everything a write costs that the caller must know: permissions the source
    /// declared, and signatures it carried.
    fn write_decisions(&self) -> Vec<Decision> {
        self.inner
            .permissions_lost_on_write()
            .into_iter()
            .chain(self.inner.signatures_lost_on_write())
            .collect()
    }

    /// Returns user permissions granted for this document.
    pub fn permissions(&self) -> Option<i32> {
        self.inner.permissions
    }

    /// Returns all font summaries in the document.
    pub fn fonts(&self) -> Vec<FontSummary> {
        self.inner.fonts()
    }

    /// Returns memory arena allocation statistics.
    pub fn arena_stats(&self) -> fepdf_model::arena::ArenaStats {
        self.inner.arena().get_stats()
    }

    /// How the document asks to be presented (`/ViewerPreferences`, 12.2).
    ///
    /// `None` means the catalogue carries no such dictionary. An *empty* one — which
    /// `samples/fy05.pdf` has — comes back as `Some` with every field `None`, because
    /// declaring nothing and declaring nothing at all are different facts.
    pub fn viewer_preferences(&self) -> Option<ViewerPreferences> {
        self.inner.catalog().ok()?.viewer_preferences
    }

    /// Returns the reading direction specified in ViewerPreferences, if any.
    pub fn viewer_direction(&self) -> Option<String> {
        Some(self.viewer_preferences()?.direction?.as_name().to_string())
    }

    /// Returns the natural language identifier (`/Lang`) of the document, if specified.
    pub fn language(&self) -> Option<String> {
        self.inner.catalog().ok()?.lang
    }

    /// What `redaction` would remove, with nothing written: for a frontend to show before
    /// the reader commits to it, and for a caller to report what was removed rather than
    /// what was asked for (ADR-0064). `Operation::Redact` applies it (ROADMAP Y-10).
    ///
    /// # Errors
    /// Refuses a redaction naming no region or a region with no area, and fails when the
    /// page is not there or its content cannot be read.
    pub fn what_redaction_removes(&self, redaction: &Redaction) -> PdfResult<Removal> {
        fepdf_doc::apply::redact::what_redaction_removes(&self.inner, redaction)
    }

    /// Retrieves a font resource by the object number of its font dictionary.
    ///
    /// Takes a number rather than an arena handle so the facade exposes no storage
    /// types (ARCHITECTURE.md Rule A).
    pub fn get_font(
        &self,
        obj_id: u32,
    ) -> PdfResult<std::sync::Arc<fepdf_model::font::FontResource>> {
        self.inner.get_font(Handle::new(obj_id))
    }

    /// Finds all digital signatures (/Type /Sig) present in the document.
    pub fn list_signatures(&self) -> Vec<SignatureSummary> {
        let arena = self.inner.arena();
        let mut signatures = Vec::new();
        for i in 0..arena.object_count() {
            let handle = Handle::new(i);
            if let Some(Object::Dictionary(dh)) = arena.get_object(handle)
                && let Some(dict) = arena.get_dict(dh)
            {
                let type_key = arena.name("Type");
                if let Some(val) = dict.get(&type_key)
                    && let Some(name_h) = val.resolve(arena).as_name()
                    && let Some(name) = arena.get_name(name_h)
                    && name.as_str() == "Sig"
                {
                    let name_key = arena.name("Name");
                    let signer_name = dict.get(&name_key).and_then(|n_val| {
                        let resolved = n_val.resolve(arena);
                        resolved.as_string().and_then(|b| String::from_utf8(b.to_vec()).ok())
                    });
                    signatures.push(SignatureSummary { object_id: i, signer_name });
                }
            }
        }
        signatures
    }
}

/// Formats a dictionary's entries one per line.
///
/// `resolve_names` renders name values as `Name(/Foo)` rather than debug output,
/// which is what the dictionary dump wants and the stream dump does not.
/// One structure element per line, indented by depth.
///
/// **`print_structure` printed one line until 2026-09-06** — `Structure Tree Root found:
/// Handle<Object>(93)` on `samples/fugaku.pdf`, which is tagged — under a CLI subcommand
/// documented "Dump hierarchical logical structure tree". It asked for the root handle
/// and formatted it; nothing walked. `StructureTreeVisitor` is the walk it now uses, and
/// the GUI and `fepdf-mcp`'s struct-tree resource had been reading it all along.
///
/// A handle's `Debug` was also the whole of the old output, which put the storage model
/// into a printed report — the vocabulary Rule A keeps out of frontends, arriving by a
/// door that rule does not watch.
///
/// The tag is what the element *is*; `/Alt` and the page follow it because they are what
/// someone reading an accessibility tree is looking for, and a tree showing neither would
/// be a list of tag names.
fn write_struct_node(node: &StructureTreeNode, depth: usize, out: &mut String) {
    use std::fmt::Write as _;
    let _ = write!(out, "{:indent$}{}", "", node.tag, indent = depth * 2);
    if let Some(page) = node.page_index {
        let _ = write!(out, "  page {}", page + 1);
    }
    if let Some(alt) = &node.alt_text {
        let _ = write!(out, "  /Alt {alt:?}");
    }
    out.push('\n');
    for child in &node.children {
        write_struct_node(child, depth + 1, out);
    }
}

fn describe_dict_entries(
    arena: &PdfArena,
    dict: &BTreeMap<Handle<PdfName>, Object>,
    resolve_names: bool,
) -> String {
    let mut out = String::new();
    for (k, v) in dict {
        let name =
            arena.get_name(*k).map_or_else(|| format!("Unknown_{k:?}"), |n| n.as_str().to_string());
        let val = match v {
            Object::Name(vh) if resolve_names => arena
                .get_name(*vh)
                .map_or_else(|| format!("{v:?}"), |n| format!("Name(/{})", n.as_str())),
            other => format!("{other:?}"),
        };
        let _ = writeln!(out, "  /{name} -> {val}");
    }
    out
}

/// Reports a stream's raw length and, when small enough to read, its decoded bytes.
fn describe_stream_payload(
    arena: &PdfArena,
    data: &std::sync::Arc<SublimatedData>,
    dict: &BTreeMap<Handle<PdfName>, Object>,
) -> String {
    /// Longest decoded payload reproduced in full.
    const PREVIEW: usize = 2000;

    let raw = arena.get_stream_bytes(data).unwrap_or_default();
    let mut out = String::new();
    let _ = writeln!(out, "Raw Length: {} bytes", raw.len());

    let Ok(decoded) = arena.process_filters(&raw, dict) else {
        return out;
    };
    let _ = writeln!(out, "Decoded Length: {} bytes", decoded.len());
    if decoded.len() < PREVIEW {
        let _ = write!(out, "\n--- [ DECODED CONTENT ] ---\n{}", String::from_utf8_lossy(&decoded));
    } else {
        let _ = write!(
            out,
            "\n--- [ DECODED CONTENT (PREVIEW) ] ---\n{}\n... (truncated)",
            String::from_utf8_lossy(decoded.get(..PREVIEW).unwrap_or(&decoded))
        );
    }
    out
}

/// Helper function to perform structural re-tagging on a document.
pub fn retag_document(doc: &mut Document) -> PdfResult<()> {
    let engine = HeuristicEngine::new();
    let _ = engine.infer_structure(doc)?;
    // Automatic application logic would follow
    Ok(())
}

/// `count` numbers out of an array entry, for the rectangles and matrices 12.5.5 needs.
fn read_numbers(
    arena: &fepdf_model::arena::PdfArena,
    entry: Option<&Object>,
    count: usize,
) -> Option<Vec<f64>> {
    let Object::Array(handle) = entry?.resolve(arena) else { return None };
    let items = arena.get_array(handle)?;
    let numbers: Vec<f64> = items.iter().filter_map(|item| item.resolve(arena).as_f64()).collect();
    (numbers.len() >= count).then_some(numbers)
}

/// The bytes of a one-page PDF 2.0 file, with a cross-reference table that agrees with
/// them.
///
/// Written here rather than parsed from a literal so that the offsets are whatever the
/// bytes turn out to be: see [`PdfDocument::create_empty`] for what the typed ones cost.
fn blank_document() -> Bytes {
    // `write!` into a `String` cannot fail — `fmt::Write for String` is infallible, and
    // the `Result` exists only because the trait is shared with `io::Write`.
    use std::fmt::Write as _;

    const BODIES: [&str; 3] = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
        // `/Resources`, empty: Table 31 requires it, and without it opening a new
        // document recorded a repair (ROADMAP Y-11).
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << >> >>",
    ];

    let mut out = String::from("%PDF-2.0\n");
    let mut offsets = Vec::with_capacity(BODIES.len());
    for (n, body) in BODIES.iter().enumerate() {
        offsets.push(out.len());
        let _ = write!(out, "{} 0 obj\n{body}\nendobj\n", n + 1);
    }

    let start_xref = out.len();
    let _ = write!(out, "xref\n0 {}\n0000000000 65535 f \n", BODIES.len() + 1);
    for offset in &offsets {
        let _ = writeln!(out, "{offset:010} 00000 n ");
    }
    let _ = write!(
        out,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{start_xref}\n%%EOF\n",
        BODIES.len() + 1
    );
    Bytes::from(out.into_bytes())
}

/// The pages that carry marked content some structure element claims.
fn collect_marked_pages(node: &StructureTreeNode, into: &mut std::collections::BTreeSet<usize>) {
    if !node.mcids.is_empty()
        && let Some(index) = node.page_index
    {
        into.insert(index);
    }
    for child in &node.children {
        collect_marked_pages(child, into);
    }
}

#[cfg(feature = "render")]
/// How many pixels across and down a region of `wide` by `tall` points is at `scale`.
///
/// **Rounded up, never to nothing.** A region a third of a point wide is a region the
/// reader dragged, and answering it zero pixels would be answering that they dragged
/// nothing. An image with no area is one no rasteriser will make.
fn pixels_across(wide: f64, tall: f64, scale: f64) -> PdfResult<(u32, u32)> {
    let across = (wide * scale).ceil();
    let down = (tall * scale).ceil();
    let fits = |side: f64| {
        (side.is_finite() && side >= 1.0 && side <= f64::from(u32::MAX))
            .then_some(side)
            .and_then(|side| u32::try_from(side as u64).ok())
    };
    match (fits(across), fits(down)) {
        (Some(across), Some(down)) => Ok((across, down)),
        _ => Err(PdfError::refused(
            "render",
            format!(
                "a region of {wide} by {tall} points at {scale} is {across} by {down} pixels, which is no image"
            ),
        )),
    }
}

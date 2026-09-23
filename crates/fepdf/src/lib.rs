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
    Conformance, CryptFilter, EncryptedPayload, EncryptionReport, Permission,
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
    MeasurementScale, MeshShadingSpec, MeshShadingType, Object, OptionalContentProperties,
    OutlineNode, OutlineTree, OutputIntent, Page, PageLabelSpec, PageLabelStyle, PdfAction,
    PdfArena, PdfError, PdfName, PdfResult, PortfolioCollection, PortfolioItem,
    PublicKeyRecipientSpec, SublimatedData, TransitionSpec, TransitionStyle,
    UnencryptedWrapperSpec, UserProperty, UserPropertyValue, VisibilityState,
};
pub use fepdf_model::{DocumentSource, PdfSource};
#[cfg(feature = "render")]
pub use fepdf_render::budget;
#[cfg(feature = "render")]
pub use fepdf_render::{VelloBackend, headless::Rasteriser};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;
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
/// The structure module for UA-2 logical tree handling (owned by `fepdf-doc`).
pub mod structure {
    pub use fepdf_doc::structure::*;
}
/// Logical structure tree visitor and presentation data (owned by `fepdf-doc`).
pub mod struct_tree {
    pub use fepdf_doc::struct_tree::*;
}
pub use fepdf_doc::Outcome;
pub use fepdf_doc::operation::{
    CropRegion, FieldKind, NewField, PageArrangement, PageDivision, WhatFallsOutside,
};
pub use fepdf_doc::{
    Align,
    AuditFinding,
    ContentScale,
    DecorationPosition,
    // The protocol's own wording for the conditions it leaves to a person, which the
    // window shows beside what was checked (W-21d).
    LeftToAPerson,
    MatterhornAuditor,
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
    StructElemMove,
    StructElemUpdate,
    StructureTreeNode,
    StructureTreeVisitor,
    StructureVisitor,
    apply_operation,
    apply_physical_redaction_to_page,
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
        }
    }
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
        let inner = Document::open(data, options)?;
        Ok(Self { inner })
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
        let inner = Document::open_repair(data, options)?;
        Ok(Self { inner })
    }

    /// Merges multiple documents into a new one.
    pub fn merge(sources: Vec<PdfDocument>) -> PdfResult<Self> {
        if sources.is_empty() {
            return Err(PdfError::Other("No sources to merge".into()));
        }

        let target_arena = PdfArena::new();
        let pages_root_dict_h = target_arena.alloc_dict(std::collections::BTreeMap::new());
        let pages_root_h = target_arena.alloc_object(Object::Dictionary(pages_root_dict_h));

        let mut target_pages = Vec::new();
        let mut merged_fields = Vec::new();
        let mut merged_outlines = Vec::new();

        for (idx, source) in sources.iter().enumerate() {
            let mut cloner = cloning::ObjectCloner::new(source.inner.arena(), &target_arena);
            Self::merge_clone_pages(
                source,
                &target_arena,
                pages_root_h,
                &mut target_pages,
                &mut cloner,
            )?;
            Self::merge_clone_acro_form(source, &target_arena, &mut merged_fields, &mut cloner);
            Self::merge_clone_outlines(
                source,
                idx + 1,
                &target_arena,
                &mut merged_outlines,
                &mut cloner,
            );
        }

        Self::merge_assemble(
            target_arena,
            pages_root_h,
            pages_root_dict_h,
            target_pages,
            merged_fields,
            merged_outlines,
        )
    }

    fn merge_clone_pages(
        source: &PdfDocument,
        target_arena: &PdfArena,
        pages_root_h: Handle<Object>,
        target_pages: &mut Vec<Object>,
        cloner: &mut cloning::ObjectCloner,
    ) -> PdfResult<()> {
        let parent_key = target_arena.name("Parent");
        let count = source.page_count()?;
        for i in 0..count {
            let source_page = source.inner.get_page(i)?;
            let source_dh = source.inner.resolve_to_dict(source_page.obj_handle())?;
            let cloned = cloner.clone_complete(&Object::Dictionary(source_dh))?;
            if let Object::Dictionary(dh) = cloned {
                let mut dict = target_arena.get_dict(dh).unwrap_or_default();
                dict.insert(parent_key, Object::Reference(pages_root_h));
                target_arena.set_dict(dh, dict);
                let target_page_h = target_arena.alloc_object(Object::Dictionary(dh));
                target_pages.push(Object::Reference(target_page_h));
            }
        }
        Ok(())
    }

    fn merge_clone_acro_form(
        source: &PdfDocument,
        target_arena: &PdfArena,
        merged_fields: &mut Vec<Object>,
        cloner: &mut cloning::ObjectCloner,
    ) {
        if let Some(cah) = source.inner.catalog_handle()
            && let Ok(cadh) = source.inner.resolve_to_dict(cah)
            && let Some(af_obj) = source
                .inner
                .arena()
                .get_dict(cadh)
                .and_then(|c| c.get(&target_arena.name("AcroForm")).cloned())
            && let Some(afh) = af_obj.resolve(source.inner.arena()).as_dict_handle()
            && let Some(af_dict) = source.inner.arena().get_dict(afh)
            && let Some(fields_obj) = af_dict.get(&target_arena.name("Fields"))
            && let Some(fah) = fields_obj.resolve(source.inner.arena()).as_array()
            && let Some(fields) = source.inner.arena().get_array(fah)
        {
            for field in fields {
                if let Ok(cloned_field) = cloner.clone_complete(&field) {
                    merged_fields.push(cloned_field);
                }
            }
        }
    }

    fn merge_clone_outlines(
        source: &PdfDocument,
        idx: usize,
        target_arena: &PdfArena,
        merged_outlines: &mut Vec<Object>,
        cloner: &mut cloning::ObjectCloner,
    ) {
        if let Some(cah) = source.inner.catalog_handle()
            && let Ok(cadh) = source.inner.resolve_to_dict(cah)
            && let Some(outlines_obj) = source
                .inner
                .arena()
                .get_dict(cadh)
                .and_then(|c| c.get(&target_arena.name("Outlines")).cloned())
            && let Some(oh) = outlines_obj.resolve(source.inner.arena()).as_dict_handle()
            && let Some(o_dict) = source.inner.arena().get_dict(oh)
            && let Some(first_obj) = o_dict.get(&target_arena.name("First"))
            && let Ok(cloned_first) = cloner.clone_complete(first_obj)
        {
            let mut source_outline_dict = std::collections::BTreeMap::new();
            source_outline_dict
                .insert(target_arena.name("Title"), Object::String(format!("Source {idx}").into()));
            source_outline_dict.insert(target_arena.name("First"), cloned_first);
            let source_outline_h = target_arena.alloc_dict(source_outline_dict);
            merged_outlines.push(Object::Reference(
                target_arena.alloc_object(Object::Dictionary(source_outline_h)),
            ));
        }
    }

    fn merge_assemble(
        target_arena: PdfArena,
        pages_root_h: Handle<Object>,
        pages_root_dict_h: Handle<std::collections::BTreeMap<Handle<fepdf_model::PdfName>, Object>>,
        target_pages: Vec<Object>,
        merged_fields: Vec<Object>,
        merged_outlines: Vec<Object>,
    ) -> PdfResult<Self> {
        let type_key = target_arena.name("Type");
        let pages_root_key = target_arena.name("Pages");
        let catalog_key = target_arena.name("Catalog");

        // Finalize Pages root
        let mut pages_dict = std::collections::BTreeMap::new();
        pages_dict.insert(type_key, Object::Name(pages_root_key));
        #[allow(clippy::cast_possible_wrap)]
        pages_dict.insert(target_arena.name("Count"), Object::Integer(target_pages.len() as i64));
        pages_dict.insert(
            target_arena.name("Kids"),
            Object::Array(target_arena.alloc_array(target_pages)),
        );
        target_arena.set_dict(pages_root_dict_h, pages_dict);

        // Create Catalog
        let mut catalog_dict = std::collections::BTreeMap::new();
        catalog_dict.insert(type_key, Object::Name(catalog_key));
        catalog_dict.insert(pages_root_key, Object::Reference(pages_root_h));

        if !merged_fields.is_empty() {
            let mut af_dict = std::collections::BTreeMap::new();
            af_dict.insert(
                target_arena.name("Fields"),
                Object::Array(target_arena.alloc_array(merged_fields)),
            );
            catalog_dict.insert(
                target_arena.name("AcroForm"),
                Object::Dictionary(target_arena.alloc_dict(af_dict)),
            );
        }

        if !merged_outlines.is_empty() {
            Self::merge_link_outlines(&target_arena, &merged_outlines, &mut catalog_dict);
        }

        let catalog_h =
            target_arena.alloc_object(Object::Dictionary(target_arena.alloc_dict(catalog_dict)));
        let mut inner = Document::new(target_arena, catalog_h, None);
        // Without this the document reports no pages at all: the page index is
        // `Document::new`'s empty vector until something walks the tree.
        inner.index_pages();
        Ok(Self { inner })
    }

    fn merge_link_outlines(
        target_arena: &PdfArena,
        merged_outlines: &[Object],
        catalog_dict: &mut std::collections::BTreeMap<Handle<fepdf_model::PdfName>, Object>,
    ) {
        let mut outline_handles = Vec::new();
        for item in merged_outlines {
            if let Object::Reference(h) = item {
                outline_handles.push(*h);
            }
        }

        for (i, &current_h) in outline_handles.iter().enumerate() {
            if let Object::Dictionary(dh) =
                target_arena.get_object(current_h).unwrap_or(Object::Null)
            {
                let mut dict = target_arena.get_dict(dh).unwrap_or_default();
                if i > 0 {
                    dict.insert(
                        target_arena.name("Prev"),
                        Object::Reference(outline_handles[i - 1]),
                    );
                }
                if i + 1 < outline_handles.len() {
                    dict.insert(
                        target_arena.name("Next"),
                        Object::Reference(outline_handles[i + 1]),
                    );
                }
                target_arena.set_dict(dh, dict);
            }
        }

        let mut outlines_root = std::collections::BTreeMap::new();
        outlines_root
            .insert(target_arena.name("Type"), Object::Name(target_arena.name("Outlines")));
        if let Some(first_h) = outline_handles.first() {
            outlines_root.insert(target_arena.name("First"), Object::Reference(*first_h));
        }
        if let Some(last_h) = outline_handles.last() {
            outlines_root.insert(target_arena.name("Last"), Object::Reference(*last_h));
        }
        #[allow(clippy::cast_possible_wrap)]
        outlines_root
            .insert(target_arena.name("Count"), Object::Integer(outline_handles.len() as i64));
        catalog_dict.insert(
            target_arena.name("Outlines"),
            Object::Dictionary(target_arena.alloc_dict(outlines_root)),
        );
    }

    /// Extracts specific pages into a new document.
    pub fn extract_pages(&self, indices: Vec<usize>) -> PdfResult<Self> {
        if indices.is_empty() {
            return Err(PdfError::Other("No indices to extract".into()));
        }

        let target_arena = PdfArena::new();
        let mut target_pages = Vec::new();

        let pages_root_key = target_arena.name("Pages");
        let type_key = target_arena.name("Type");
        let parent_key = target_arena.name("Parent");
        let kids_key = target_arena.name("Kids");
        let count_key = target_arena.name("Count");
        let catalog_key = target_arena.name("Catalog");

        // 1. Create target Pages root (placeholder)
        let pages_root_dict_handle = target_arena.alloc_dict(std::collections::BTreeMap::new());
        let pages_root_handle =
            target_arena.alloc_object(Object::Dictionary(pages_root_dict_handle));

        let mut cloner = cloning::ObjectCloner::new(self.inner.arena(), &target_arena);

        for i in indices {
            let source_page = self.inner.get_page(i)?;
            let source_dh = self.inner.resolve_to_dict(source_page.obj_handle())?;

            let cloned_page_dict_obj = cloner.clone_complete(&Object::Dictionary(source_dh))?;

            if let Object::Dictionary(dh) = cloned_page_dict_obj {
                let mut dict = target_arena.get_dict(dh).unwrap_or_default();
                // Update parent to the new Pages root
                dict.insert(parent_key, Object::Reference(pages_root_handle));
                target_arena.set_dict(dh, dict);

                // Allocate as an indirect object
                let target_page_handle = target_arena.alloc_object(Object::Dictionary(dh));
                target_pages.push(Object::Reference(target_page_handle));
            }
        }

        // 2. Finalize Pages root
        let mut pages_dict = std::collections::BTreeMap::new();
        pages_dict.insert(type_key, Object::Name(pages_root_key));
        #[allow(clippy::cast_possible_wrap)]
        pages_dict.insert(count_key, Object::Integer(target_pages.len() as i64));
        pages_dict.insert(kids_key, Object::Array(target_arena.alloc_array(target_pages)));
        target_arena.set_dict(pages_root_dict_handle, pages_dict);

        // 3. Create Catalog
        let mut catalog_dict = std::collections::BTreeMap::new();
        catalog_dict.insert(type_key, Object::Name(catalog_key));
        catalog_dict.insert(pages_root_key, Object::Reference(pages_root_handle));
        let catalog_handle =
            target_arena.alloc_object(Object::Dictionary(target_arena.alloc_dict(catalog_dict)));

        let mut inner = Document::new(target_arena, catalog_handle, None);
        // Without this the document reports no pages at all: the page index is
        // `Document::new`'s empty vector until something walks the tree.
        inner.index_pages();
        Ok(Self { inner })
    }

    /// Writes the document, returning what the write cost that the caller must know.
    ///
    /// The `Vec<Decision>` is not decoration. Decryption drops `/Encrypt`, so a
    /// document whose `/P` forbade modification produces output declaring nothing —
    /// and that used to happen in silence. Returning the decisions makes the compiler
    /// ask every caller what it intends to do with them, which is the mechanism
    /// ADR-0005 prefers over remembering: `let _ =` in a test is a caller saying it
    /// does not assert on this, which is honest; a frontend ignoring it is visible in
    /// review because it had to write the discard.
    pub fn save_as_version(&self, output_path: &Path, version: &str) -> PdfResult<Vec<Decision>> {
        self.save_with_options(output_path, version, &SaveOptions::default())
    }

    /// The document copied into an arena of its own, for a writer to consume.
    ///
    /// **The one place two arenas are live**, and the reason it is a function rather than
    /// four lines written twice. `root` and `info` index the arena this returns;
    /// `self.inner`'s handles index a different one, and a `Handle<Object>` does not say
    /// which — the pools are separated by handle *type*, not by arena (ROADMAP W-A3). Two
    /// copies of this sat four lines above a `writer.finish(root, info)` where
    /// `*self.inner.root_handle()` would have compiled and written a different object.
    ///
    /// It does not make that unwritable. What it does is stop the two from sharing a
    /// scope, and stop the shape from being maintained in two places.
    fn cloned_for_output(&self) -> PdfResult<(PdfArena, Handle<Object>, Option<Handle<Object>>)> {
        let target = PdfArena::new();
        let mut cloner = crate::cloning::ObjectCloner::new(self.inner.arena(), &target);
        let root = cloner.clone_handle(*self.inner.root_handle())?;
        let info = self.inner.info_handle().map(|h| cloner.clone_handle(h)).transpose()?;
        Ok((target, root, info))
    }

    /// Saves the document with custom options.
    pub fn save_with_options(
        &self,
        output_path: &Path,
        version: &str,
        options: &SaveOptions,
    ) -> PdfResult<Vec<Decision>> {
        self.write_out(output_path, version, options, None)
    }

    /// Applies the metadata the options ask for, returning what stripping cost.
    fn settle_metadata(
        &self,
        options: &SaveOptions,
    ) -> PdfResult<fepdf_model::interpretation::DecisionLog> {
        let mut metadata = self.inner.metadata();

        if let Some(v) = &options.title {
            metadata.title = Some(v.clone());
        }
        if let Some(v) = &options.author {
            metadata.author = Some(v.clone());
        }
        if let Some(v) = &options.lang {
            metadata.language = Some(v.clone());
        }
        if let Some(v) = &options.copyright {
            metadata.rights = Some(v.clone());
        }

        // Automatic Producer stamping
        metadata.producer = Some("fepdf (https://github.com/akahmys/fepdf)".to_string());

        if options.strip {
            // Strip metadata: we'll clear the fields in the struct
            metadata = MetadataInfo::default();
            metadata.producer = Some("fepdf (optimized)".to_string());
        }

        fepdf_model::metadata::update_document_metadata(&self.inner, &metadata)?;

        let mut stripped = fepdf_model::interpretation::DecisionLog::default();
        if options.strip {
            // Every metadata stream, not only the catalogue's. Runs after the write
            // above, which would otherwise put a fresh packet back.
            fepdf_model::metadata::strip_metadata_streams(&self.inner, &mut stripped);
        }
        Ok(stripped)
    }

    /// Describes the signature field for this save, from what the caller asked for.
    fn signature_field(
        identity: &fepdf_model::cms::SigningIdentity,
        options: &SignOptions,
    ) -> fepdf_model::interactive::SignatureField {
        fepdf_model::interactive::SignatureField {
            page_index: options.page_index,
            field_name: "Signature1".to_string(),
            signed_at: pdf_now(),
            // Table 255 wants /Name only when the signature cannot supply it. The
            // certificate usually can, so this is normally absent.
            signer: identity.common_name().map_or_else(|| options.name.clone(), |_| None),
            reason: options.reason.clone(),
            location: options.location.clone(),
            contact: options.contact_info.clone(),
        }
    }

    /// The one write path, signed or not.
    ///
    /// Signing is not a different way of saving — it is the same file with two fields
    /// filled in afterwards — so the two share this rather than running in parallel. A
    /// signed document produced by a second code path would drift from the unsigned one
    /// silently, and the difference would be invisible until a signature failed to
    /// verify against a file nobody could reproduce.
    fn write_out(
        &self,
        output_path: &Path,
        version: &str,
        options: &SaveOptions,
        signing: Option<(&fepdf_model::cms::SigningIdentity, &SignOptions)>,
    ) -> PdfResult<Vec<Decision>> {
        let stripped = self.settle_metadata(options)?;

        if options.dry_run {
            // Nothing was written, so nothing was lost.
            return Ok(Vec::new());
        }

        let file = std::fs::File::create(output_path).map_err(PdfError::Io)?;
        let (final_arena, root, info) = self.cloned_for_output()?;

        let mut writer = crate::writer::PdfWriter::new(file, &final_arena);
        writer.set_string_encoding(options.string_encoding);
        if options.compress {
            writer.set_compression(options.compression_level);
        }
        writer.set_pack_objects(options.obj_stm);
        let mut encryption = Vec::new();
        Self::apply_encryption(&mut writer, options, &mut encryption)?;
        if let Some((identity, sign_options)) = signing {
            // The field goes into the arena that is about to be written, not into the
            // document: what is signed is this output, and nothing about the document
            // this engine holds changes because a copy of it was signed.
            let field = Self::signature_field(identity, sign_options);
            let signature =
                fepdf_model::interactive::add_signature_field(&final_arena, root, &field)?;
            writer.sign_with(signature, identity)?;
        }
        writer.write_header(version)?;
        writer.finish(root, info)?;
        let mut decisions = self.write_decisions();
        decisions.extend(stripped.into_entries());
        decisions.extend(encryption);
        Ok(decisions)
    }

    /// Encrypts the output, by password or to certificates, or leaves it plain.
    ///
    /// The two are exclusive because a document has one `/Encrypt` and 7.6.4 and 7.6.5
    /// are different handlers. Refusing beats picking one: a caller that asked for both
    /// has a bug, and silently honouring whichever was checked first hides it.
    fn apply_encryption<W: std::io::Write>(
        writer: &mut crate::writer::PdfWriter<'_, W>,
        options: &SaveOptions,
        decisions: &mut Vec<Decision>,
    ) -> PdfResult<()> {
        if options.password.is_some() && !options.recipients.is_empty() {
            return Err(PdfError::Crypto(
                "a document is encrypted with a password or to certificates, not both".into(),
            ));
        }
        if let Some(password) = &options.password {
            let (handler, artifacts) = Self::encryption_for(password, options, decisions)?;
            writer.encrypt_with(handler, artifacts);
        } else if !options.recipients.is_empty() {
            let permissions = match &options.permissions {
                Some(list) => fepdf_model::encryption::permissions_from_keywords(list)?,
                // Every bit granted, as for the standard handler: encrypting a document
                // is not on its own a statement about what may be done with it.
                None => -1,
            };
            let (handler, entries) =
                fepdf_model::security::SecurityHandler::encrypt_to_certificates(
                    &options.recipients,
                    permissions,
                    true,
                )?;
            writer.encrypt_to(handler, entries);
        }
        Ok(())
    }

    /// Builds the handler for an encrypted save, recording what the options left open.
    fn encryption_for(
        password: &str,
        options: &SaveOptions,
        decisions: &mut Vec<Decision>,
    ) -> PdfResult<(
        fepdf_model::security::SecurityHandler,
        fepdf_model::security::EncryptionArtifacts,
    )> {
        let permissions = match &options.permissions {
            Some(list) => fepdf_model::encryption::permissions_from_keywords(list)?,
            // Every bit granted. `/P` is a declaration this engine reports and does not
            // enforce, and encrypting a document is not on its own a statement about
            // what may be done with it once open.
            None => -1,
        };

        let owner = options.owner_password.as_deref().unwrap_or(password);
        if options.owner_password.is_none() {
            // Worth saying out loud rather than defaulting in silence. The owner
            // password lifts `/P` (7.6.4.1), so making it the open password means the
            // permissions restrict nobody who can open the file — which is exactly the
            // party they were meant to restrict.
            decisions.push(Decision::ambiguity(
                "7.6.4.1",
                "no owner password was given",
                "the open password carries owner rights, so /P restricts nobody who can open it",
            ));
        }

        Ok(fepdf_model::security::SecurityHandler::encrypt_new(password, owner, permissions, true)?)
    }

    /// Saves a linearized (Fast Web View) version of the document with custom options.
    pub fn save_linearized(
        &self,
        output_path: &Path,
        version: &str,
        options: &SaveOptions,
    ) -> PdfResult<Vec<Decision>> {
        // Linearization involves object reordering and hint tables.
        // For M67, we implement the object reordering phase.

        // 1. Update Metadata (consistent with save_with_options)
        let mut metadata = self.inner.metadata();
        if let Some(v) = &options.title {
            metadata.title = Some(v.clone());
        }
        if let Some(v) = &options.author {
            metadata.author = Some(v.clone());
        }
        metadata.producer = Some("fepdf (linearized)".to_string());

        fepdf_model::metadata::update_document_metadata(&self.inner, &metadata)?;

        let file = std::fs::File::create(output_path).map_err(PdfError::Io)?;
        let (final_arena, root, info) = self.cloned_for_output()?;

        let mut writer = crate::writer::PdfWriter::new(file, &final_arena);
        writer.set_string_encoding(options.string_encoding);
        writer.set_linearize(true);
        if options.compress {
            writer.set_compression(options.compression_level);
        }

        writer.write_header(version)?;
        writer.finish(root, info)?;
        Ok(self.write_decisions())
    }

    /// Signs the document and saves it.
    ///
    /// What is signed is *this output* — the file this call writes, byte for byte — and
    /// not the document that was read. [ADR-0014] is why: normalising at load means the
    /// engine no longer has the source bytes, so signing a document it did not produce
    /// would sign something the user never saw. Signing its own output is exact.
    ///
    /// This wrote `/SubFilter /adbe.pkcs7.detached` with 8,192 zero bytes for
    /// `/Contents` and a `/ByteRange` of four constants, and refused rather than keep
    /// doing so. The refusal stood until the file could carry a signature that covers
    /// it.
    ///
    /// # Errors
    /// If no certificate and key were given, if they cannot be read, or if the write
    /// fails. Both must be DER: a PEM file converts with `openssl x509 -outform der`
    /// and `openssl pkcs8 -topk8 -nocrypt -outform der`.
    ///
    /// [ADR-0014]: ../../../../docs/adr/0014-the-faithful-copy-path-is-not-built.md
    pub fn save_signed(
        &self,
        output_path: &Path,
        version: &str,
        options: &SaveOptions,
        sign_options: &SignOptions,
    ) -> PdfResult<Vec<Decision>> {
        let certificate = sign_options
            .certificate
            .as_deref()
            .ok_or(PdfError::Crypto("signing needs a certificate".into()))?;
        let key = sign_options
            .private_key
            .as_deref()
            .ok_or(PdfError::Crypto("signing needs a private key".into()))?;
        let identity = fepdf_model::cms::SigningIdentity::from_der(certificate, key)?;
        self.write_out(output_path, version, options, Some((&identity, sign_options)))
    }

    /// Returns the physical viewport of the page (MediaBox).
    pub fn get_page_box(&self, index: usize) -> PdfResult<fepdf_model::graphics::Rect> {
        let page = self.inner.get_page(index)?;
        let box_obj =
            page.resolve_attribute("CropBox").or_else(|| page.resolve_attribute("MediaBox"));

        if let Some(mb) = box_obj
            && let Some(arr_handle) = mb.as_array()
            && let Some(arr) = self.inner.arena().get_array(arr_handle)
            && arr.len() >= 4
        {
            let x1 = arr[0].resolve(self.inner.arena()).as_f64().unwrap_or(0.0);
            let y1 = arr[1].resolve(self.inner.arena()).as_f64().unwrap_or(0.0);
            let x2 = arr[2].resolve(self.inner.arena()).as_f64().unwrap_or(595.0);
            let y2 = arr[3].resolve(self.inner.arena()).as_f64().unwrap_or(842.0);
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

    /// The page's content stream, or `None` where 7.7.3.3 allows it to have none.
    ///
    /// `/Contents` is **optional**: a page without one is empty, and it is a shape that
    /// occurs — a page whose only mark is an annotation's appearance stream. This
    /// returned `Err("Page has no contents")`, so rendering such a page failed and
    /// `extract_text` failed with it. Seven files of the corpus fetched for Phase O-1 are
    /// exactly that, and they were counted as text-extraction failures until the corpus
    /// arrived to disagree — a check firing on conforming input, which ADR-0008 is about.
    fn resolve_page_contents(&self, page: &Page) -> PdfResult<Option<Object>> {
        let Some(contents_obj) = page.resolve_attribute("Contents") else {
            return Ok(None);
        };
        let resolved_contents = match contents_obj {
            Object::Reference(h) => {
                log::debug!("[SDK] Resolving Contents reference {h:?}");
                self.inner.resolve(&h)?
            }
            _ => contents_obj,
        };
        Ok(Some(resolved_contents))
    }

    fn execute_interpreter(
        &self,
        interpreter: &mut Interpreter<'_>,
        resolved_contents: Object,
    ) -> PdfResult<()> {
        let arena = self.inner.arena();
        match resolved_contents {
            Object::Stream(dh, ref data) => {
                if let SublimatedData::Commands { items: cmds, .. } = &**data {
                    interpreter.execute_commands(cmds)?;
                } else {
                    let data = self.inner.decode_stream(&Object::Stream(dh, (*data).clone()))?;
                    interpreter.execute_raw(&data)?;
                }
            }
            Object::Array(ah) => {
                if let Some(arr) = arena.get_array(ah) {
                    for item in arr {
                        let resolved_item = match item {
                            Object::Reference(h) => self.inner.resolve(&h)?,
                            _ => item,
                        };
                        if let Object::Stream(_, ref data) = resolved_item {
                            if let SublimatedData::Commands { items: cmds, .. } = &**data {
                                interpreter.execute_commands(cmds)?;
                            } else {
                                let data = self.inner.decode_stream(&resolved_item)?;
                                interpreter.execute_raw(&data)?;
                            }
                        }
                    }
                }
            }
            _ => {
                let data = self.inner.decode_stream(&resolved_contents)?;
                interpreter.execute_raw(&data)?;
            }
        }
        Ok(())
    }

    /// Renders a page to a provide backend.
    pub fn render_page(
        &self,
        index: usize,
        backend: &mut dyn fepdf_content::RenderBackend,
        initial_transform: kurbo::Affine,
    ) -> PdfResult<()> {
        let page = self.inner.get_page(index)?;
        let res_dh = page.resources_handle();
        // Anything the backend concluded on an earlier page belongs to that page, not
        // this one. Draining first means what is recorded below is what *this* call drew.
        let _ = backend.take_decisions();
        let mut interpreter = Interpreter::new(backend, &self.inner, res_dh, initial_transform);
        // So that an image the engine cannot decode can say how much of *this page* it
        // would have covered, rather than only that one was dropped (ROADMAP Phase L).
        let box_ = page.media_box();
        interpreter.set_page_area((box_.x2 - box_.x1).abs() * (box_.y2 - box_.y1).abs());

        // The contents and the annotations are two halves of what 6.3.2.2 asks a renderer
        // for, and one failing does not cancel the other. `UnknownFilter-PageContentStream.pdf`
        // is the case: its content stream dictionary is malformed, and drawing nothing at
        // all because of that would lose every annotation the page carries. The error is
        // still returned — `extract_text` reports it, and the corpus measurement counts
        // it — but it is returned *after* the page has been drawn as far as it can be.
        let drawn = match self.resolve_page_contents(&page)? {
            Some(resolved_contents) => {
                self.execute_interpreter(&mut interpreter, resolved_contents)
            }
            None => Ok(()),
        };
        // Taken before the drop and handed over after it, because the interpreter holds
        // the backend borrowed until then. Only the page's own content stream is
        // measured: an annotation's appearance is a separate stream with its own
        // interpreter, and 14.7.4.2 numbers marked content per stream — an `/MCID 0` in a
        // widget is not the `/MCID 0` in the page.
        let marks = interpreter.take_mark_bounds();
        drop(interpreter);
        backend.receive_mark_bounds(marks);
        self.render_annotations(index, backend, initial_transform)?;
        // A backend sits below any `Document` and cannot record for itself, so what it
        // concluded about the font programs it was handed is folded in here — after the
        // annotations, because their appearance streams draw glyphs too (ARCHITECTURE
        // §4.3).
        for decision in backend.take_decisions() {
            self.inner.record(decision);
        }
        drawn
    }

    /// Draws every annotation on the page that has an appearance and is meant to be seen
    /// (12.5.5).
    ///
    /// **Not optional for anything that renders.** 6.3.2.2 says a PDF processor that
    /// renders a page shall render the appropriate appearance stream for all annotations
    /// that have one, unless the annotation flags say otherwise — and this engine drew
    /// none at all, so a page whose only mark is an annotation came out blank while every
    /// other reader painted it.
    ///
    /// Each appearance is a form XObject with its own coordinate system, so it gets its
    /// own interpreter: the transform is the page's, with the placement 12.5.5's
    /// algorithm computes applied inside it, and the resources are the appearance's own.
    fn render_annotations(
        &self,
        index: usize,
        backend: &mut dyn fepdf_content::RenderBackend,
        page_transform: kurbo::Affine,
    ) -> PdfResult<()> {
        let arena = self.inner.arena();
        let page = self.inner.get_page(index)?;
        let Some(annots) = page.get_attribute("Annots") else { return Ok(()) };
        let Object::Array(handle) = annots.resolve(arena) else { return Ok(()) };
        let optional_content =
            fepdf_model::optional_content::OptionalContentState::read(&self.inner);
        for entry in arena.get_array(handle).unwrap_or_default() {
            let Some(dict) = entry.resolve(arena).as_dict_handle().and_then(|h| arena.get_dict(h))
            else {
                continue;
            };
            if !self.annotation_is_shown(&dict, &optional_content) {
                continue;
            }
            if let Some((stream, placement, resources)) = self.appearance_of(&dict) {
                let mut inner = Interpreter::new(
                    backend,
                    &self.inner,
                    resources,
                    page_transform * placement.as_affine(),
                );
                // An appearance that will not execute is one annotation, not the page.
                let _ = inner.execute(stream);
            }
        }
        Ok(())
    }

    /// Whether an annotation is meant to be seen on screen (12.5.3, Table 167).
    ///
    /// `Hidden` is bit 2 and means never; `NoView` is bit 6 and means not on a screen,
    /// which is what this renders to. `Invisible` is bit 1 and is **not** applied: it
    /// governs only annotations of no standard type *for which no handler exists*, and
    /// this engine has no handlers at all — it draws appearance streams, which is what
    /// the flag's own "if clear" branch describes.
    fn annotation_is_shown(
        &self,
        dict: &std::collections::BTreeMap<fepdf_model::Handle<PdfName>, Object>,
        optional_content: &fepdf_model::optional_content::OptionalContentState,
    ) -> bool {
        let arena = self.inner.arena();
        let flags =
            dict.get(&arena.name("F")).and_then(|f| f.resolve(arena).as_integer()).unwrap_or(0);
        if flags & 0b10 != 0 || flags & 0b10_0000 != 0 {
            return false;
        }
        match dict.get(&arena.name("OC")) {
            Some(oc) => !matches!(
                optional_content.membership(arena, oc),
                fepdf_model::optional_content::Membership::Hidden
            ),
            None => true,
        }
    }

    /// The annotation's normal appearance, where it goes, and what it draws with.
    fn appearance_of(
        &self,
        dict: &std::collections::BTreeMap<fepdf_model::Handle<PdfName>, Object>,
    ) -> Option<(
        fepdf_model::Handle<Object>,
        fepdf_model::graphics::Matrix,
        fepdf_model::Handle<std::collections::BTreeMap<fepdf_model::Handle<PdfName>, Object>>,
    )> {
        let arena = self.inner.arena();
        let appearances =
            arena.get_dict(dict.get(&arena.name("AP"))?.resolve(arena).as_dict_handle()?)?;
        let normal = appearances.get(&arena.name("N"))?;
        let stream = self.appearance_state(normal, dict)?;
        let stream_dict =
            arena.get_object(stream)?.as_dict_handle().and_then(|h| arena.get_dict(h))?;

        let bbox = read_numbers(arena, stream_dict.get(&arena.name("BBox")), 4)?;
        let matrix = read_numbers(arena, stream_dict.get(&arena.name("Matrix")), 6)
            .map_or_else(fepdf_model::graphics::Matrix::default, |m| {
                fepdf_model::graphics::Matrix::new(m[0], m[1], m[2], m[3], m[4], m[5])
            });
        let rect = read_numbers(arena, dict.get(&arena.name("Rect")), 4)?;
        let placement = fepdf_model::annotation::appearance_placement(
            [bbox[0], bbox[1], bbox[2], bbox[3]],
            matrix,
            &fepdf_model::graphics::Rect::new(rect[0], rect[1], rect[2], rect[3]),
        );
        let resources = stream_dict
            .get(&arena.name("Resources"))
            .and_then(|r| r.resolve(arena).as_dict_handle())
            .unwrap_or_else(|| arena.alloc_dict(std::collections::BTreeMap::new()));
        Some((stream, placement, resources))
    }

    /// `/N` is a stream, or a dictionary of them keyed by the state `/AS` names (12.5.5).
    ///
    /// A checkbox keeps `/Off` and `/Yes` under `/N` and says which is current in `/AS`.
    /// Where `/AS` is missing and the dictionary holds exactly one appearance there is
    /// nothing to choose between, so that one is drawn and the omission recorded.
    fn appearance_state(
        &self,
        normal: &Object,
        annotation: &std::collections::BTreeMap<fepdf_model::Handle<PdfName>, Object>,
    ) -> Option<fepdf_model::Handle<Object>> {
        let arena = self.inner.arena();
        if matches!(normal.resolve(arena), Object::Stream(..)) {
            return normal.as_reference();
        }
        let states = arena.get_dict(normal.resolve(arena).as_dict_handle()?)?;
        if let Some(state) =
            annotation.get(&arena.name("AS")).and_then(|s| s.resolve(arena).as_name())
        {
            return states.get(&state).and_then(Object::as_reference);
        }
        if states.len() == 1 {
            self.inner.record(fepdf_model::interpretation::Decision::repaired(
                "12.5.5",
                "an annotation's /AP /N is a dictionary of states and it names none in /AS",
                "drew the only appearance it holds, since there was nothing to choose between",
            ));
            return states.values().next().and_then(Object::as_reference);
        }
        self.inner.record(fepdf_model::interpretation::Decision::violation(
            "12.5.5",
            format!("an annotation's /AP /N holds {} states and /AS names none", states.len()),
            "drew no appearance, because choosing one would be this engine's choice",
        ));
        None
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
    /// [`fepdf_doc::AuditReport::found_nothing`] and read the scope; do not read
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
    pub fn audit_ua2_report(&self) -> PdfResult<fepdf_doc::AuditReport> {
        MatterhornAuditor::new(&self.inner).audit_report()
    }

    /// The findings alone, for a caller that has read the scope elsewhere.
    ///
    /// # Errors
    /// Fails when the structure tree cannot be read.
    pub fn audit_ua2(&self) -> PdfResult<Vec<AuditFinding>> {
        Ok(self.audit_ua2_report()?.findings)
    }

    /// Returns a comprehensive summary of the document.
    pub fn get_summary(&self) -> PdfResult<DocumentSummary> {
        let pdf_20 = true; // High-level inference
        let findings = self.audit_ua2()?;
        let mut issues = Vec::new();
        for f in findings {
            // **A condition examined and not broken is not an issue.** Since W-21g made
            // it a row of the report, every clean condition arrived here as a
            // `ComplianceIssue` of severity `Warning` — the `_` arm below reads "Pass"
            // that way — so a conforming document listed one warning per check that
            // passed. What a summary's issue list answers is what is wrong.
            if f.outcome == fepdf_doc::Outcome::Sound {
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
            version: if pdf_20 { "2.0".into() } else { "1.7".into() },
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

    /// Scrubs the strings shown inside `rects` on `page_idx`, and answers how many.
    ///
    /// The count is what was removed, not what was asked for: a rectangle over empty
    /// space answers 0, and a caller that treats "it returned" as "something was
    /// redacted" is the shape ADR-0064 was written about.
    pub fn apply_redaction_to_page(&self, page_idx: usize, rects: &[[f32; 4]]) -> PdfResult<usize> {
        apply_physical_redaction_to_page(&self.inner, page_idx, rects)
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

    /// Renders a specific page to an image file, detecting format from extension.
    ///
    /// Requires the `render` feature, which pulls in the Vello + wgpu stack.
    #[cfg(feature = "render")]
    pub fn render_page_to_file(&self, index: usize, output_path: &Path) -> PdfResult<()> {
        self.render_page_to_file_with(index, output_path, Rasteriser::Gpu)
    }

    /// A rectangle of a page, rasterised, as RGBA pixels and the size they came out.
    ///
    /// **The snapshot Acrobat calls スナップショット is a read**, not an `Operation`:
    /// nothing about the document changes, so it belongs here beside `extract_text` and
    /// `render_page` rather than in the vocabulary Rule D governs.
    ///
    /// `keep` is in the page's own space and `scale` is the caller's. A snapshot taken at
    /// whatever the screen happens to be showing is a snapshot nobody can ask for twice,
    /// so the resolution is asked for rather than inferred — `4.0 / 3.0` is the 96 DPI
    /// `render_page_to_file` uses, and twice that is twice the detail.
    ///
    /// **`/UserUnit` is not applied here** and is where `render_page_to_file` does apply
    /// it (Table 31). A caller asking for a region of a page in that page's coordinates
    /// has already said what it wants in those coordinates; multiplying by the unit would
    /// answer a rectangle it did not ask about. A caller that wants the page's own sense
    /// of scale multiplies `scale` by [`Self::get_page_user_unit`].
    ///
    /// # Errors
    /// Fails when the page is not there, when the rectangle has no area, when the scale
    /// is not a positive finite number, or when the rasteriser does.
    #[cfg(feature = "render")]
    pub fn render_region(
        &self,
        index: usize,
        keep: (f64, f64, f64, f64),
        scale: f64,
    ) -> PdfResult<(Vec<u8>, u32, u32)> {
        self.render_region_with(index, keep, scale, Rasteriser::Gpu)
    }

    /// [`Self::render_region`], naming which rasteriser runs.
    ///
    /// # Errors
    /// The same as [`Self::render_region`].
    #[cfg(feature = "render")]
    pub fn render_region_with(
        &self,
        index: usize,
        keep: (f64, f64, f64, f64),
        scale: f64,
        rasteriser: Rasteriser,
    ) -> PdfResult<(Vec<u8>, u32, u32)> {
        let (wide, tall) = (keep.2 - keep.0, keep.3 - keep.1);
        if !scale.is_finite() || scale <= 0.0 {
            return Err(PdfError::Other(format!("a scale of {scale} draws nothing").into()));
        }
        if !(wide.is_finite() && tall.is_finite()) || wide <= 0.0 || tall <= 0.0 {
            return Err(PdfError::Other(
                format!("a region of {wide} by {tall} points has no area").into(),
            ));
        }
        let (width, height) = pixels_across(wide, tall, scale)?;

        let mut backend = VelloBackend::new(Arc::clone(&self.inner.system_fonts));
        // The page is drawn whole and the region is what the frame is put around: the
        // transform puts `keep`'s lower-left corner at the image's lower-left, and the
        // flip is the one every render of a page does — a page counts up from its foot
        // and an image down from its head.
        let transform =
            kurbo::Affine::new([scale, 0.0, 0.0, -scale, -keep.0 * scale, keep.3 * scale]);
        self.render_page(index, &mut backend, transform)?;

        let pixels = pollster::block_on(fepdf_render::headless::render_to_bytes_with(
            backend.scene(),
            width,
            height,
            rasteriser,
        ))
        .map_err(|e: Box<dyn std::error::Error>| PdfError::Other(e.to_string().into()))?;
        Ok((pixels, width, height))
    }

    /// [`PdfDocument::render_page_to_file`], naming which rasteriser runs.
    ///
    /// **A caller wanting the same image twice must ask for `Cpu`.** The engine encodes a
    /// byte-identical scene for a given page every time, and vello's GPU pipeline turns
    /// that one scene into more than one image — three distinct images in eight renders of
    /// `samples/sample.pdf` page 1, one isolated pixel apart at a channel delta of 1. The
    /// CPU shaders give one ([ADR-0043]).
    ///
    /// `scripts/visual_regression.py` is deliberately **not** this caller: it tolerates a
    /// delta of 1, which is the whole size of the difference, and rendering it on the CPU
    /// would stop it exercising the pipeline a user actually gets. What needs this is a
    /// caller for whom "the same" means the same bytes — a hash, a cache key, a signature
    /// over a rendering.
    ///
    /// [ADR-0043]: https://github.com/akahmys/fepdf/blob/main/docs/adr/0043-the-scene-repeats-and-the-rasteriser-does-not.md
    #[cfg(feature = "render")]
    pub fn render_page_to_file_with(
        &self,
        index: usize,
        output_path: &Path,
        rasteriser: Rasteriser,
    ) -> PdfResult<()> {
        let r = self.get_page_box(index)?;
        let w = (r.x2 - r.x1).abs();
        let h = (r.y2 - r.y1).abs();
        let rot = self.get_page_rotation(index)?;
        let (display_w, display_h) = if rot == 90 || rot == 270 { (h, w) } else { (w, h) };

        // 96 DPI, times whatever a user space unit is worth on this page. `/UserUnit` is
        // how a drawing exceeds the 14,400-unit limit a box can express (Table 31): the
        // coordinates stay as written and each one is worth more of an inch, so honouring
        // it is a matter of the scale and nothing else — the content is drawn unchanged.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let scale = (4.0 / 3.0) * self.get_page_user_unit(index)?;
        let width = (display_w * scale).round() as u32;
        let height = (display_h * scale).round() as u32;

        let mut backend = VelloBackend::new(Arc::clone(&self.inner.system_fonts));

        let initial_transform = match rot {
            90 => kurbo::Affine::new([0.0, scale, -scale, 0.0, h * scale, 0.0]),
            180 => kurbo::Affine::new([-scale, 0.0, 0.0, scale, w * scale, 0.0]),
            270 => kurbo::Affine::new([0.0, -scale, scale, 0.0, 0.0, w * scale]),
            _ => kurbo::Affine::new([scale, 0.0, 0.0, -scale, 0.0, h * scale]),
        };

        self.render_page(index, &mut backend, initial_transform)?;

        let format = match output_path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase())
            .as_deref()
        {
            Some("png") => image::ImageFormat::Png,
            Some("jpg" | "jpeg") => image::ImageFormat::Jpeg,
            _ => return Err(PdfError::Other(
                "Unsupported image format. Only PNG and JPEG (.png, .jpg, .jpeg) are supported."
                    .to_string()
                    .into(),
            )),
        };

        // Finalize rendering using the headless bridge
        let scene = backend.scene();
        pollster::block_on(fepdf_render::headless::render_to_image_with(
            scene,
            width,
            height,
            output_path,
            format,
            rasteriser,
        ))
        .map_err(|e: Box<dyn std::error::Error>| PdfError::Other(e.to_string().into()))?;

        Ok(())
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
            String::from_utf8_lossy(&decoded[..PREVIEW])
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
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
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
        _ => Err(PdfError::Other(
            format!("a region of {wide} by {tall} points at {scale} is {across} by {down} pixels, which is no image")
                .into(),
        )),
    }
}

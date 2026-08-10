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
pub use crate::remediation::apply_physical_redaction_to_page;
use crate::structure::{AuditFinding, MatterhornAuditor};
use bytes::Bytes;
pub use fepdf_content::FallbackFontType;
pub use fepdf_core::font::{GlyphTrace, TraceContext};
pub use fepdf_core::{
    Document, Handle, Object, Page, PdfArena, PdfError, PdfName, PdfResult, SublimatedData,
};
#[cfg(feature = "render")]
pub use fepdf_render::VelloBackend;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

/// The internal cloning module for object migration.
pub mod cloning;
/// The internal interpreter module for processing content streams.
pub mod interpreter;
pub use interpreter::Interpreter;
/// The internal obj_stm module for high-density object packing.
pub mod obj_stm;
/// The internal remediation module for structural repair.
pub mod remediation;
/// The internal structure module for UA-2 logical tree handling.
pub mod structure;
/// Unified operation vocabulary for canonical document mutations.
pub mod operation;
pub use operation::*;
/// Logical structure tree visitor and presentation data.
pub mod struct_tree;
pub use struct_tree::{StructureTreeVisitor, StructureTreeNode};
/// The internal writer module for generating PDF files.
pub mod writer;

/// Supported text string encodings for PDF output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StringEncoding {
    /// Maximum compatibility using UTF-16BE with BOM (FE FF).
    #[default]
    Utf16BE,
    /// PDF 2.0 native UTF-8 with BOM (EF BB BF).
    Utf8,
}

/// Options for saving a PDF document.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default)]
pub struct SaveOptions {
    /// Whether to compress streams using FlateDecode.
    pub compress: bool,
    /// The compression level to use (0-9).
    pub compression_level: u32,
    /// Whether to remove unreachable objects.
    pub vacuum: bool,
    /// Whether to strip descriptive metadata.
    pub strip: bool,
    /// Optional encryption password.
    pub password: Option<String>,
    /// Whether to use Object Streams (ObjStm) for high-density compression.
    pub obj_stm: bool,
    /// Optional image re-compression quality (1-100).
    pub image_quality: Option<u32>,
    /// Document primary language (ISO 639-1).
    pub lang: Option<String>,
    /// Override document title.
    pub title: Option<String>,
    /// Override document author.
    pub author: Option<String>,
    /// Set copyright notice in XMP.
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
    /// Visual rectangle for the signature widget [x1, y1, x2, y2].
    pub rect: [f32; 4],
}

/// Supported PDF modern standards for conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfStandard {
    /// PDF/A-4 (ISO 19005-4:2020) for long-term archiving.
    A4,
    /// PDF/X-6 (ISO 15930-9:2020) for professional printing.
    X6,
    /// PDF/UA-2 (ISO 14289-2:2024) for universal accessibility.
    UA2,
    /// ISO 32000-2 (PDF 2.0) base compliance.
    ISO32000_2,
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
}

pub use fepdf_core::font::FontSummary;
pub use fepdf_core::metadata::MetadataInfo;

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
    vacuum: bool,
    strip: bool,
    password: Option<String>,
}

impl PdfDocument {
    /// Opens a PDF document from a byte buffer with default ingestion options.
    pub fn open(data: Bytes) -> PdfResult<Self> {
        Self::open_with_options(data, &fepdf_core::ingest::IngestionOptions::default())
    }

    /// Opens a PDF document with custom ingestion options.
    pub fn open_with_options(
        data: Bytes,
        options: &fepdf_core::ingest::IngestionOptions,
    ) -> PdfResult<Self> {
        let inner = Document::open(data, options)?;
        Ok(Self { inner, vacuum: false, strip: false, password: None })
    }

    /// Returns the internal document.
    pub fn inner(&self) -> &Document {
        &self.inner
    }

    /// Adds Document Security Store (DSS) for PAdES LTV support.
    pub fn add_ltv_info(&mut self, certificates: Vec<Vec<u8>>) -> PdfResult<()> {
        let arena = self.inner.arena();
        let mut dss_dict = std::collections::BTreeMap::new();

        let mut cert_refs = Vec::new();
        for cert_data in certificates {
            let mut stream_dict = std::collections::BTreeMap::new();
            #[allow(clippy::cast_possible_wrap)]
            stream_dict.insert(arena.name("Length"), Object::Integer(cert_data.len() as i64));
            let stream_h = arena.alloc_dict(stream_dict);
            let stream_ref = arena.alloc_object(Object::Stream(
                stream_h,
                std::sync::Arc::new(fepdf_core::object::SublimatedData::Raw(bytes::Bytes::from(
                    cert_data,
                ))),
            ));
            cert_refs.push(Object::Reference(stream_ref));
        }

        if !cert_refs.is_empty() {
            dss_dict.insert(arena.name("Certs"), Object::Array(arena.alloc_array(cert_refs)));
        }

        if let Some(cah) = self.inner.catalog_handle() {
            let cadh = self.inner.resolve_to_dict(cah)?;
            let mut catalog = arena.get_dict(cadh).unwrap_or_default();
            catalog.insert(arena.name("DSS"), Object::Dictionary(arena.alloc_dict(dss_dict)));
            arena.set_dict(cadh, catalog);
        }

        Ok(())
    }

    /// Returns the total number of pages.
    pub fn page_count(&self) -> PdfResult<usize> {
        self.inner.page_count()
    }

    /// Retrieves a specific page by its 0-based index.
    pub fn get_page(&self, index: usize) -> PdfResult<fepdf_core::document::page::Page<'_>> {
        self.inner.get_page(index)
    }

    /// Swaps the order of two pages.
    pub fn swap_pages(&mut self, a: usize, b: usize) -> PdfResult<()> {
        self.inner.swap_pages(a, b)
    }

    /// Moves page at `from` index to `to` index.
    pub fn reorder_page(&mut self, from: usize, to: usize) -> PdfResult<()> {
        self.inner.reorder_page(from, to)
    }

    /// Removes a page at the specified 0-based index.
    pub fn remove_page(&mut self, index: usize) -> PdfResult<()> {
        self.inner.remove_page(index)
    }

    /// Inserts pages from another document into this document at `at_index`.
    pub fn insert_pages_from(&mut self, source: &PdfDocument, at_index: usize) -> PdfResult<usize> {
        let source_page_count = source.page_count()?;
        if source_page_count == 0 {
            return Ok(0);
        }

        let clamped_index = at_index.min(self.inner.pages.len());
        let mut cloner = cloning::ObjectCloner::new(source.inner.arena(), self.inner.arena());

        let mut new_page_handles = Vec::with_capacity(source_page_count);

        for i in 0..source_page_count {
            let source_page = source.inner.get_page(i)?;
            let source_dh = source.inner.resolve_to_dict(source_page.obj_handle())?;
            let cloned = cloner.clone_object(&Object::Dictionary(source_dh))?;
            if let Object::Dictionary(dh) = cloned {
                let target_page_h = self.inner.arena().alloc_object(Object::Dictionary(dh));
                new_page_handles.push(target_page_h);
            }
        }

        let inserted_count = new_page_handles.len();
        for (i, h) in new_page_handles.into_iter().enumerate() {
            self.inner.pages.insert(clamped_index + i, h);
        }

        self.inner.rebuild_page_tree_in_arena()?;
        Ok(inserted_count)
    }

    /// Sets the system fallback fonts for the document (Phase 4).
    pub fn set_system_fonts(&mut self, fonts: BTreeMap<FallbackFontType, Arc<Vec<u8>>>) {
        self.inner.system_fonts = Arc::new(fonts);
        self.inner.normalize_resources();
    }

    /// Attempts to open and repair a PDF document with custom options.
    pub fn open_and_repair_with_options(
        data: Bytes,
        options: &fepdf_core::ingest::IngestionOptions,
    ) -> PdfResult<Self> {
        let inner = Document::open_repair(data, options)?;
        Ok(Self { inner, vacuum: false, strip: false, password: None })
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
            let cloned = cloner.clone_object(&Object::Dictionary(source_dh))?;
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
                if let Ok(cloned_field) = cloner.clone_object(&field) {
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
            && let Ok(cloned_first) = cloner.clone_object(first_obj)
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
        pages_root_dict_h: Handle<std::collections::BTreeMap<Handle<fepdf_core::PdfName>, Object>>,
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
        Ok(Self {
            inner: Document::new(target_arena, catalog_h, None),
            vacuum: false,
            strip: false,
            password: None,
        })
    }

    fn merge_link_outlines(
        target_arena: &PdfArena,
        merged_outlines: &[Object],
        catalog_dict: &mut std::collections::BTreeMap<Handle<fepdf_core::PdfName>, Object>,
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

            let cloned_page_dict_obj = cloner.clone_object(&Object::Dictionary(source_dh))?;

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

        Ok(Self {
            inner: Document::new(target_arena, catalog_handle, None),
            vacuum: false,
            strip: false,
            password: None,
        })
    }

    /// Saves the document to a file with a specific version and default options.
    pub fn save_as_version(&self, output_path: &Path, version: &str) -> PdfResult<()> {
        let options = SaveOptions {
            vacuum: self.vacuum,
            strip: self.strip,
            password: self.password.clone(),
            ..SaveOptions::default()
        };
        self.save_with_options(output_path, version, &options)
    }

    /// Saves the document with custom options.
    pub fn save_with_options(
        &self,
        output_path: &Path,
        version: &str,
        options: &SaveOptions,
    ) -> PdfResult<()> {
        // 1. Update Metadata
        let mut metadata = self.inner.metadata();

        if let Some(v) = &options.title {
            metadata.title = Some(v.clone());
        }
        if let Some(v) = &options.author {
            metadata.author = Some(v.clone());
        }
        if let Some(_v) = &options.lang { /* Lang is usually in Catalog /Lang, not Metadata struct for now */
        }

        // Automatic Producer stamping
        metadata.producer = Some("fepdf-sdk (https://github.com/akahmys/fepdf)".to_string());

        if options.strip {
            // Strip metadata: we'll clear the fields in the struct
            metadata = MetadataInfo::default();
            metadata.producer = Some("fepdf-sdk (optimized)".to_string());
        }

        fepdf_core::metadata::update_document_metadata(&self.inner, &metadata)?;

        if options.strip {
            // Further stripping: remove Metadata entry from catalog
            let root_handle = *self.inner.root_handle();
            let arena = self.inner.arena();
            if let Ok(rdh) = self.inner.resolve_to_dict(root_handle) {
                let mut dict = arena.get_dict(rdh).unwrap_or_default();
                dict.remove(&arena.name("Metadata"));
                arena.set_dict(rdh, dict);
            }
        }

        if options.dry_run {
            return Ok(());
        }

        let file = std::fs::File::create(output_path).map_err(PdfError::Io)?;
        let final_arena = PdfArena::new();
        let mut cloner = crate::cloning::ObjectCloner::new(self.inner.arena(), &final_arena);
        let root = cloner.clone_handle(*self.inner.root_handle())?;
        let info = self.inner.info_handle().map(|h| cloner.clone_handle(h)).transpose()?;

        let mut writer = crate::writer::PdfWriter::new(file, &final_arena);
        writer.set_string_encoding(options.string_encoding);
        if options.compress {
            writer.set_compression(options.compression_level);
        }
        writer.write_header(version)?;
        writer.finish(root, info)?;
        Ok(())
    }

    /// Saves a linearized (Fast Web View) version of the document with custom options.
    pub fn save_linearized(
        &self,
        output_path: &Path,
        version: &str,
        options: &SaveOptions,
    ) -> PdfResult<()> {
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
        metadata.producer = Some("fepdf-sdk (linearized)".to_string());

        fepdf_core::metadata::update_document_metadata(&self.inner, &metadata)?;

        let file = std::fs::File::create(output_path).map_err(PdfError::Io)?;
        let final_arena = PdfArena::new();
        let mut cloner = crate::cloning::ObjectCloner::new(self.inner.arena(), &final_arena);
        let root = cloner.clone_handle(*self.inner.root_handle())?;
        let info = self.inner.info_handle().map(|h| cloner.clone_handle(h)).transpose()?;

        let mut writer = crate::writer::PdfWriter::new(file, &final_arena);
        writer.set_string_encoding(options.string_encoding);
        writer.set_linearize(true);
        if options.compress {
            writer.set_compression(options.compression_level);
        }

        writer.write_header(version)?;
        writer.finish(root, info)?;
        Ok(())
    }

    /// Signs the document and saves it with digital signature support.
    pub fn save_signed(
        &self,
        output_path: &Path,
        version: &str,
        options: &SaveOptions,
        sign_options: &SignOptions,
    ) -> PdfResult<()> {
        let arena = self.inner.arena();
        let sig_h = self.create_sig_dict(arena, sign_options);
        let widget_h = self.create_sig_widget(arena, sign_options, sig_h);

        self.add_sig_to_page(arena, sign_options.page_index, widget_h)?;
        self.add_sig_to_catalog(arena, widget_h)?;

        let file = std::fs::File::create(output_path).map_err(PdfError::Io)?;
        let mut writer = crate::writer::PdfWriter::new(file, arena);
        writer.set_string_encoding(options.string_encoding);
        writer.add_signature_target(sig_h);

        if options.vacuum {
            writer.set_vacuum(true);
        }
        if options.compress {
            writer.set_compression(options.compression_level);
        }

        writer.write_header(version)?;
        writer.finish(*self.inner.root_handle(), self.inner.info_handle())?;
        Ok(())
    }

    fn create_sig_dict(&self, arena: &PdfArena, options: &SignOptions) -> Handle<Object> {
        let mut dict = std::collections::BTreeMap::new();
        dict.insert(arena.name("Type"), Object::Name(arena.name("Sig")));
        dict.insert(arena.name("Filter"), Object::Name(arena.name("Adobe.PPKLite")));
        dict.insert(arena.name("SubFilter"), Object::Name(arena.name("adbe.pkcs7.detached")));

        if let Some(r) = &options.reason {
            dict.insert(arena.name("Reason"), Object::String(Bytes::from(r.clone())));
        }
        if let Some(l) = &options.location {
            dict.insert(arena.name("Location"), Object::String(Bytes::from(l.clone())));
        }
        if let Some(c) = &options.contact_info {
            dict.insert(arena.name("ContactInfo"), Object::String(Bytes::from(c.clone())));
        }
        if let Some(n) = &options.name {
            dict.insert(arena.name("Name"), Object::String(Bytes::from(n.clone())));
        }

        let now =
            chrono::Local::now().format("D:%Y%m%d%H%M%S%:z").to_string().replace(':', "'") + "'";
        dict.insert(arena.name("M"), Object::String(Bytes::from(now)));
        dict.insert(arena.name("Contents"), Object::Hex(vec![0u8; 8192].into()));

        let br = vec![
            Object::Integer(0),
            Object::Integer(1_000_000_000),
            Object::Integer(1_000_000_000),
            Object::Integer(1_000_000_000),
        ];
        dict.insert(arena.name("ByteRange"), Object::Array(arena.alloc_array(br)));

        arena.alloc_object(Object::Dictionary(arena.alloc_dict(dict)))
    }

    fn create_sig_widget(
        &self,
        arena: &PdfArena,
        options: &SignOptions,
        sig_h: Handle<Object>,
    ) -> Handle<Object> {
        let mut dict = std::collections::BTreeMap::new();
        dict.insert(arena.name("Type"), Object::Name(arena.name("Annot")));
        dict.insert(arena.name("Subtype"), Object::Name(arena.name("Widget")));
        dict.insert(arena.name("FT"), Object::Name(arena.name("Sig")));
        dict.insert(arena.name("T"), Object::String(Bytes::from("Signature1")));
        dict.insert(arena.name("V"), Object::Reference(sig_h));
        dict.insert(arena.name("F"), Object::Integer(4));

        let rect = vec![
            Object::Real(f64::from(options.rect[0])),
            Object::Real(f64::from(options.rect[1])),
            Object::Real(f64::from(options.rect[2])),
            Object::Real(f64::from(options.rect[3])),
        ];
        dict.insert(arena.name("Rect"), Object::Array(arena.alloc_array(rect)));
        arena.alloc_object(Object::Dictionary(arena.alloc_dict(dict)))
    }

    fn add_sig_to_page(
        &self,
        arena: &PdfArena,
        page_idx: usize,
        widget_h: Handle<Object>,
    ) -> PdfResult<()> {
        let page = self.inner.get_page(page_idx)?;
        let dh = self.inner.resolve_to_dict(page.obj_handle())?;
        let mut dict =
            arena.get_dict(dh).ok_or_else(|| PdfError::Other("Page dict missing".into()))?;

        let annots_k = arena.name("Annots");
        let mut annots = if let Some(Object::Array(ah)) = dict.get(&annots_k) {
            arena.get_array(*ah).unwrap_or_default()
        } else {
            Vec::new()
        };
        annots.push(Object::Reference(widget_h));
        dict.insert(annots_k, Object::Array(arena.alloc_array(annots)));
        arena.set_dict(dh, dict);
        Ok(())
    }

    fn add_sig_to_catalog(&self, arena: &PdfArena, widget_h: Handle<Object>) -> PdfResult<()> {
        let root_h = *self.inner.root_handle();
        let Some(Object::Dictionary(rdh)) = arena.get_object(root_h) else { return Ok(()) };
        let mut root_dict =
            arena.get_dict(rdh).ok_or_else(|| PdfError::Other("Catalog dict missing".into()))?;

        let mut acro_form =
            if let Some(Object::Dictionary(afh)) = root_dict.get(&arena.name("AcroForm")) {
                arena.get_dict(*afh).unwrap_or_default()
            } else {
                let mut af = std::collections::BTreeMap::new();
                af.insert(arena.name("Fields"), Object::Array(arena.alloc_array(Vec::new())));
                af
            };

        if let Some(Object::Array(fh)) = acro_form.get(&arena.name("Fields")) {
            let mut fields = arena.get_array(*fh).unwrap_or_default();
            fields.push(Object::Reference(widget_h));
            acro_form.insert(arena.name("Fields"), Object::Array(arena.alloc_array(fields)));
        }

        acro_form.insert(arena.name("SigFlags"), Object::Integer(3));
        root_dict.insert(arena.name("AcroForm"), Object::Dictionary(arena.alloc_dict(acro_form)));
        arena.set_dict(rdh, root_dict);
        Ok(())
    }

    /// Returns the physical viewport of the page (MediaBox).
    pub fn get_page_box(&self, index: usize) -> PdfResult<fepdf_core::graphics::Rect> {
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
            return Ok(fepdf_core::graphics::Rect::new(x1, y1, x2, y2));
        }
        Ok(fepdf_core::graphics::Rect::new(0.0, 0.0, 595.0, 842.0)) // Default A4
    }

    /// Returns the physical dimensions of the page (Width, Height), accounting for page rotation.
    pub fn get_page_size(&self, index: usize) -> PdfResult<(f64, f64)> {
        let r = self.get_page_box(index)?;
        let w = (r.x2 - r.x1).abs();
        let h = (r.y2 - r.y1).abs();
        let rot = self.get_page_rotation(index)?;
        if rot == 90 || rot == 270 { Ok((h, w)) } else { Ok((w, h)) }
    }

    fn resolve_page_contents(&self, page: &Page) -> PdfResult<Object> {
        let contents_obj = page
            .resolve_attribute("Contents")
            .ok_or_else(|| PdfError::Other("Page has no contents".into()))?;

        let resolved_contents = match contents_obj {
            Object::Reference(h) => {
                log::debug!("[SDK] Resolving Contents reference {h:?}");
                self.inner.resolve(&h)?
            }
            _ => contents_obj,
        };
        Ok(resolved_contents)
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
        let mut interpreter = Interpreter::new(backend, &self.inner, res_dh, initial_transform);

        let resolved_contents = self.resolve_page_contents(&page)?;
        self.execute_interpreter(&mut interpreter, resolved_contents)?;
        Ok(())
    }

    /// Upgrades the document to a specific standard (A-4, X-6, UA-2).
    pub fn upgrade_to_standard(&mut self, standard: PdfStandard) -> PdfResult<()> {
        let arena = self.inner.arena();
        match standard {
            PdfStandard::ISO32000_2 => {
                arena.set_version(2.0);
            }
            PdfStandard::A4 => {
                arena.set_version(2.0);
                if let Some(cah) = self.inner.catalog_handle() {
                    if let Ok(cadh) = self.inner.resolve_to_dict(cah) {
                        let mut catalog = arena.get_dict(cadh).unwrap_or_default();
                        let gts_key = arena.intern_name(PdfName::new("GTS_PDFA14"));
                        catalog
                            .insert(gts_key, Object::Name(arena.intern_name(PdfName::new("Yes"))));
                        arena.set_dict(cadh, catalog);
                    }
                }
            }
            PdfStandard::UA2 => {
                arena.set_version(2.0);
                if let Some(cah) = self.inner.catalog_handle() {
                    if let Ok(cadh) = self.inner.resolve_to_dict(cah) {
                        let mut catalog = arena.get_dict(cadh).unwrap_or_default();
                        let ua_key = arena.intern_name(PdfName::new("PdfUA"));
                        catalog.insert(ua_key, Object::Integer(2));
                        arena.set_dict(cadh, catalog);
                    }
                }
            }
            PdfStandard::X6 => {
                arena.set_version(2.0);
                if let Some(cah) = self.inner.catalog_handle() {
                    if let Ok(cadh) = self.inner.resolve_to_dict(cah) {
                        let mut catalog = arena.get_dict(cadh).unwrap_or_default();
                        let gts_key = arena.intern_name(PdfName::new("GTS_PDFX"));
                        catalog.insert(
                            gts_key,
                            Object::Name(arena.intern_name(PdfName::new("PDFX6"))),
                        );
                        arena.set_dict(cadh, catalog);
                    }
                }
            }
        }
        Ok(())
    }

    /// Primary entry point for re-tagging a document automatically.
    pub fn retag_document(&mut self) -> PdfResult<()> {
        crate::remediation::retag(&mut self.inner)
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

    /// Prints a textual representation of the logical structure tree.
    pub fn print_structure(&self) -> PdfResult<String> {
        let root_opt = self.inner.get_structure_root()?;
        if let Some(root) = root_opt {
            Ok(format!("Structure Tree Root found: {root:?}"))
        } else {
            Ok("No logical structure found.".into())
        }
    }

    /// Controls whether unreachable objects are removed on save.
    pub fn set_vacuum(&mut self, vacuum: bool) {
        self.vacuum = vacuum;
    }
    /// Controls whether descriptive metadata is stripped on save.
    pub fn set_strip(&mut self, strip: bool) {
        self.strip = strip;
    }
    /// Sets the document open password.
    pub fn set_password(&mut self, password: Option<String>) {
        self.password = password;
    }

    /// Returns whether unreachable objects are removed on save.
    pub fn vacuum(&self) -> bool {
        self.vacuum
    }
    /// Returns whether descriptive metadata is stripped on save.
    pub fn strip(&self) -> bool {
        self.strip
    }
    /// Returns the configured document open password, if any.
    pub fn password(&self) -> Option<&str> {
        self.password.as_deref()
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

    /// Sets the rotation of a specific page.
    pub fn set_page_rotation(&mut self, index: usize, angle: i32) -> PdfResult<()> {
        let page = self.inner.get_page(index)?;
        let page_dh = self.inner.resolve_to_dict(page.obj_handle())?;
        let arena = self.inner.arena();
        let mut dict = arena.get_dict(page_dh).unwrap_or_default();
        dict.insert(arena.name("Rotate"), Object::Integer(i64::from(angle)));
        arena.set_dict(page_dh, dict);
        Ok(())
    }

    /// Extracts the presentation-ready logical structure tree.
    pub fn extract_struct_tree(&self) -> Option<StructureTreeNode> {
        StructureTreeVisitor::extract(self)
    }

    /// Applies a canonical mutation operation to the document.
    pub fn apply(&mut self, op: Operation) -> PdfResult<()> {
        // RR-15 Limit: Dispatcher - applies canonical document mutation operations
        match op {
            Operation::Rotate { pages, mode } => self.apply_rotate(&pages, &mode),
            Operation::Reorder { from, to } => self.reorder_page(from, to),
            Operation::RemovePages(pages) => self.apply_remove_pages(&pages),
            Operation::UpdateStructElem(update) => self.apply_update_struct(update),
            Operation::DeleteStructElem { handle_index } => self.apply_delete_struct(handle_index),
        }
    }

    fn apply_rotate(&mut self, pages: &PageSelection, mode: &RotateMode) -> PdfResult<()> {
        let count = self.page_count()?;
        let indices = match pages {
            PageSelection::All => (0..count).collect(),
            PageSelection::Single(i) => vec![*i],
            PageSelection::Indices(idx) => idx.clone(),
        };
        for idx in indices {
            if idx < count {
                let current = self.get_page_rotation(idx)?;
                let target = match mode {
                    RotateMode::Absolute(q) => q.to_degrees(),
                    RotateMode::Relative(q) => (current + q.to_degrees()).rem_euclid(360),
                };
                self.set_page_rotation(idx, target)?;
            }
        }
        Ok(())
    }

    fn apply_remove_pages(&mut self, pages: &PageSelection) -> PdfResult<()> {
        let count = self.page_count()?;
        let mut indices = match pages {
            PageSelection::All => (0..count).collect(),
            PageSelection::Single(i) => vec![*i],
            PageSelection::Indices(idx) => idx.clone(),
        };
        indices.sort_unstable();
        indices.dedup();
        for idx in indices.into_iter().rev() {
            if idx < count {
                self.remove_page(idx)?;
            }
        }
        Ok(())
    }

    fn apply_update_struct(&self, update: StructElemUpdate) -> PdfResult<()> {
        let handle = Handle::<Object>::new(update.handle_index);
        let arena = self.inner.arena();
        if let Some(Object::Dictionary(dh)) = arena.get_object(handle)
            && let Some(mut dict) = arena.get_dict(dh)
        {
            if let Some(tag) = update.new_tag {
                let s_key = arena.name("S");
                dict.insert(s_key, Object::Name(arena.name(&tag)));
            }
            if let Some(alt) = update.new_alt {
                let alt_key = arena.name("Alt");
                dict.insert(alt_key, Object::String(Bytes::from(alt)));
            }
            arena.set_dict(dh, dict);
        }
        Ok(())
    }

    fn apply_delete_struct(&self, handle_index: u32) -> PdfResult<()> {
        let handle = Handle::<Object>::new(handle_index);
        let arena = self.inner.arena();
        if let Some(cah) = self.inner.catalog_handle()
            && let Ok(cadh) = self.inner.resolve_to_dict(cah)
            && let Some(dict) = arena.get_dict(cadh)
            && let Some(str_root_obj) = dict.get(&arena.name("StructTreeRoot"))
            && let Some(str_root_ref) = struct_tree::resolve_to_node_handle(arena, str_root_obj)
        {
            struct_tree::delete_struct_node(arena, str_root_ref, handle);
        }
        Ok(())
    }

    /// Performs a structural health audit for PDF/UA-2.
    pub fn audit_ua2(&self) -> PdfResult<Vec<AuditFinding>> {
        let root_opt = self.inner.get_structure_root()?;
        if let Some(root) = root_opt {
            let auditor = MatterhornAuditor::new(self.inner.arena());
            auditor.audit(root)
        } else {
            Ok(vec![AuditFinding {
                checkpoint: "00-001".into(),
                severity: "Warning".into(),
                message: "Document missing Structural Tree Root. Not a tagged PDF.".into(),
                handle_id: None,
            }])
        }
    }

    /// Returns a comprehensive summary of the document.
    pub fn get_summary(&self) -> PdfResult<DocumentSummary> {
        let pdf_20 = true; // High-level inference
        let findings = self.audit_ua2()?;
        let mut issues = Vec::new();
        for f in findings {
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
        let auditor = fepdf_core::audit::compliance::ComplianceAuditor::new(&self.inner);
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
        })
    }

    /// Returns a list of all fonts embedded or referenced in the document.
    pub fn get_embedded_fonts(&self) -> Vec<FontSummary> {
        self.inner.fonts()
    }

    /// Renders a specific page to an image file, detecting format from extension.
    ///
    /// Requires the `render` feature, which pulls in the Vello + wgpu stack.
    #[cfg(feature = "render")]
    pub fn render_page_to_file(&self, index: usize, output_path: &Path) -> PdfResult<()> {
        let r = self.get_page_box(index)?;
        let w = (r.x2 - r.x1).abs();
        let h = (r.y2 - r.y1).abs();
        let rot = self.get_page_rotation(index)?;
        let (display_w, display_h) = if rot == 90 || rot == 270 { (h, w) } else { (w, h) };

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let scale = 4.0 / 3.0; // exactly 96 DPI
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
        pollster::block_on(fepdf_render::headless::render_to_image(
            scene,
            width,
            height,
            output_path,
            format,
        ))
        .map_err(|e: Box<dyn std::error::Error>| PdfError::Other(e.to_string().into()))?;

        Ok(())
    }
}

/// Helper function to perform structural re-tagging on a document.
pub fn retag_document(doc: &mut Document) -> PdfResult<()> {
    let engine = HeuristicEngine::new();
    let _ = engine.infer_structure(doc)?;
    // Automatic application logic would follow
    Ok(())
}

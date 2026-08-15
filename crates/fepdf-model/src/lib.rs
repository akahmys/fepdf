//! fepdf Core: PDF 2.0 Refinery Engine.
//!
//! (ISO 32000-2:2020 Compliance Engine v2.1)
//!
//! This crate provides the high-performance Arena-based object model
//! and the Ingestion Gateway for the fepdf toolkit.

extern crate self as fepdf_model;

/// Handle-addressed storage for every object in a document.
pub mod arena;
/// Conformance auditing against the published standards.
pub mod audit;
/// The document catalogue (7.7.2), entry by entry, typed or not.
pub mod catalog;
pub mod color;
/// Content stream rewriting.
pub mod content;
/// Unlocking an encrypted document (ISO 7.6).
pub mod decrypt;
/// The document model: catalogue, page tree, pages.
pub mod document;
/// What protects a document, and how far the engine conforms in handling it (7.6).
pub mod encryption;
/// The error type this engine reports.
pub mod error;
/// A file's layout as clause 7.5 describes it, and the decisions taken reading it.
pub mod file_structure;
pub mod filters;
pub mod font;
pub mod graphics;
pub mod handle;
pub mod ingest;
/// Decisions taken when the input departs from the standard.
/// Interactive features (clause 12): annotations, forms, actions, outlines.
pub mod interactive;
pub mod interpretation;
pub use fepdf_syntax::lexer;
/// Document metadata, from XMP or the `/Info` dictionary.
pub mod metadata;
pub mod object;
pub mod parser;
/// Building objects from the offsets the syntax layer located.
pub mod reader;
pub mod refine;
/// Where documents come from: the bytes-to-`Document` boundary.
pub mod source;
pub use fepdf_syntax::security;
/// PDF physical writer and serialization engine.
pub mod writer;

pub use crate::refine::{ParallelRefinery, commit_to_arena};
pub use arena::PdfArena;
pub use document::Document;
pub use document::extensions::*;
pub use document::page::Page;
pub use writer::{PdfWriter, StringEncoding};

pub use fepdf_macros::FromPdfObject;
pub use graphics::{
    BlendMode, Color, LineCap, LineJoin, Matrix, PixelFormat, StrokeStyle, WindingRule,
};
pub use handle::Handle;
pub use ingest::Ingestor;
pub use object::{FromPdfObject, Object, PdfName, PdfSchema, Reference, SublimatedData};

pub use error::{PdfError, PdfResult};
pub use interpretation::{Decision, DecisionLog, Severity, Strictness};
pub use source::{DocumentSource, PdfSource};

/// Resolves the directory holding bundled resources (fonts, Adobe CMap tables).
///
/// Reads `FEPDF_RESOURCES`, falling back to the pre-rename `FERRUGINOUS_RESOURCES`
/// so that setups configured before the project was renamed keep working, and
/// finally to `default` — which differs per caller, hence the parameter.
pub fn resource_dir(default: &str) -> String {
    std::env::var("FEPDF_RESOURCES")
        .or_else(|_| std::env::var("FERRUGINOUS_RESOURCES"))
        .unwrap_or_else(|_| default.to_string())
}

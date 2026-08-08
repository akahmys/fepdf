//! Ferruginous Core: PDF 2.0 Refinery Engine.
//!
//! (ISO 32000-2:2020 Compliance Engine v2.1)
//!
//! This crate provides the high-performance Arena-based object model
//! and the Ingestion Gateway for the Ferruginous toolkit.

extern crate self as ferruginous_core;

pub mod arena;
pub mod audit;
pub mod color;
pub mod content;
pub mod document;
pub mod error;
pub mod filters;
pub mod font;
pub mod graphics;
pub mod handle;
pub mod ingest;
pub mod lexer;
pub mod metadata;
pub mod object;
pub mod parser;
pub mod refine;
pub mod security;

extern crate chardetng;

pub use crate::refine::{ParallelRefinery, commit_to_arena};
pub use arena::{PdfArena, RemappingTable};
pub use document::Document;
pub use document::page::Page;

pub use ferruginous_macros::FromPdfObject;
pub use graphics::{
    BlendMode, Color, LineCap, LineJoin, Matrix, PixelFormat, StrokeStyle, WindingRule,
};
pub use handle::Handle;
pub use ingest::Ingestor;
pub use object::{FromPdfObject, Object, PdfName, PdfSchema, Reference, SublimatedData};

pub use error::{PdfError, PdfResult};

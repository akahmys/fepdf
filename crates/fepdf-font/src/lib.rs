//! Pure Font Format Engine (CFF, TrueType, OpenType, CMap, AGL, Subsetting & Reconstruction)
//!
//! ISO 32000-2 / ISO 14496-22 compliant font engine. Contains ZERO PDF domain concepts.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Adobe Glyph List (AGL) lookups.
pub mod agl;
/// Standard CFF strings and constants.
pub mod cff_standard;
/// CMap character map parsers and utilities.
pub mod cmap;
/// Surgical font binary reconstructor and SFNT patcher.
pub mod reconstruction;
/// CMap rescue and recovery heuristics.
pub mod rescue;
/// Font subsetting utilities.
pub mod subset;

pub use agl::*;
pub use cff_standard::*;
pub use cmap::*;
pub use reconstruction::*;
pub use rescue::*;
pub use subset::*;

/// Resolves the directory holding bundled resources (fonts, Adobe CMap tables).
pub fn resource_dir(default: &str) -> String {
    std::env::var("FEPDF_RESOURCES")
        .or_else(|_| std::env::var("FERRUGINOUS_RESOURCES"))
        .unwrap_or_else(|_| default.to_string())
}

/// Error type for pure font operations.
#[derive(Debug, thiserror::Error)]
pub enum FontError {
    /// Generic font error with message.
    #[error("{0}")]
    Other(String),
    /// Internal engine error with message.
    #[error("{0}")]
    Internal(String),
}

/// Result type for font operations.
pub type FontResult<T> = Result<T, FontError>;

/// Backwards compatibility alias for PdfResult in font modules.
pub type PdfResult<T> = FontResult<T>;

/// Backwards compatibility alias for PdfError in font modules.
pub type PdfError = FontError;

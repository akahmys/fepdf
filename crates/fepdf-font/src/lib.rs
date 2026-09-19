//! Pure Font Format Engine (CFF, TrueType, OpenType, CMap, AGL, Subsetting & Reconstruction)
//!
//! ISO 32000-2 / ISO 14496-22 compliant font engine. Contains ZERO PDF domain concepts.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod agl;
/// The simple-font base encodings of Annex D.
pub mod annex_d;
/// Standard CFF strings and constants.
pub mod cff_standard;
/// CMap character map parsers and utilities.
pub mod cmap;
/// What a font program permits, from `OS/2.fsType`.
pub mod embedding;
/// Surgical font binary reconstructor and SFNT patcher.
pub mod reconstruction;
/// CMap rescue and recovery heuristics.
pub mod rescue;
/// Adobe Glyph List (AGL) lookups.
/// Where the engine looks for the data it does not carry.
pub mod resources;
/// Font subsetting utilities.
pub mod subset;

pub use agl::*;
pub use cff_standard::*;
pub use cmap::*;
pub use embedding::*;
pub use reconstruction::*;
pub use rescue::*;
pub use subset::*;

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

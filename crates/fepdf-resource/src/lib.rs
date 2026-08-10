//! PDF 2.0 Resource Dictionary Resolution Engine (ISO 32000-2 Clause 7.8 & Clause 9)
//!
//! Converts PDF resource dictionaries (/Font, /XObject, /ColorSpace, /ExtGState, /Pattern, /Shading)
//! into usable high-level resources.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// PDF Color Space resource definitions and resolution.
pub mod color;
/// PDF Font resource definitions, metrics, schema, and loading.
pub mod font;

pub use color::*;
pub use font::*;

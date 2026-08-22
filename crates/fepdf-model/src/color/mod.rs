//! Color Space Management (ISO 32000-2 Clause 8.6)
//!
//! `moxcms` parses ICC profiles here, and that is all it does. **This header used to
//! claim it gave "high-fidelity CMYK -> RGB conversion", and it does not**:
//! `Color::to_rgb` converts `/DeviceCMYK` with the naive `(1 − c)(1 − k)` formula and
//! never consults a profile. Measured on `target/colour/separation.pdf`, where K = 1
//! reaches this engine's raster as `0 0 0` and PDFKit's — which does put a CMYK profile
//! through it — as `26 25 25`. `ROADMAP.md` Phase P carries the entry.
//!
//! Corrected rather than deleted, because the claim is the reason nobody looked: a
//! module that says it is colour managed is not somewhere you go looking for a naive
//! formula. `AGENTS.md`, Hierarchy of Truth — measurement outranks documentation.
//!
//! [`ResolvedColorSpace`] is the other half of this clause: the spaces whose components
//! are not a colour until a function runs (8.6.6).

mod space;

pub use space::ResolvedColorSpace;

use crate::PdfResult;
use crate::graphics::Color;
use moxcms::ColorProfile;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Lightweight representation of a PDF Color Space type for IR and GraphicsState.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorSpaceKind {
    /// DeviceGray.
    DeviceGray,
    /// DeviceRGB.
    DeviceRGB,
    /// DeviceCMYK.
    DeviceCMYK,
    /// CIE-based CalGray.
    CalGray,
    /// CIE-based CalRGB.
    CalRGB,
    /// CIE-based L*a*b*.
    Lab,
    /// ICC-profile based.
    ICCBased,
    /// Pattern space; colour comes from a pattern.
    Pattern,
    /// Indexed palette over a base space.
    Indexed,
    /// Separation (single colorant).
    Separation,
    /// DeviceN (multiple colorants).
    DeviceN,
    /// Unrecognised or absent.
    Unknown,
}

/// Represents a resolved PDF Color Space with associated resources.
#[derive(Debug, Clone)]
pub enum ColorSpace {
    /// DeviceGray.
    DeviceGray,
    /// DeviceRGB.
    DeviceRGB,
    /// DeviceCMYK.
    DeviceCMYK,
    /// CIE-based CalGray.
    CalGray,
    /// CIE-based CalRGB.
    CalRGB,
    /// CIE-based L*a*b*.
    Lab,
    /// ICC-profile based, carrying the parsed profile.
    ICCBased(Arc<ColorProfile>),
    /// Pattern space; colour comes from a pattern.
    Pattern,
    /// Indexed palette over a base space.
    Indexed,
    /// Separation (single colorant).
    Separation,
    /// DeviceN (multiple colorants).
    DeviceN,
    /// Unrecognised or absent.
    Unknown,
}

impl ColorSpace {
    /// Loads an ICCBased color space from raw profile data.
    pub fn from_icc(data: &[u8]) -> PdfResult<Self> {
        let profile =
            ColorProfile::new_from_slice(data).map_err(|e| crate::error::PdfError::Ingestion {
                context: "ICC Profile Loading".into(),
                message: format!("ICC Profile error: {e:?}").into(),
            })?;
        Ok(Self::ICCBased(Arc::new(profile)))
    }

    /// Transforms raw components to their final representation (Normalized RGB/CMYK).
    pub fn transform(&self, components: &[f64]) -> Color {
        match self {
            Self::DeviceGray => Color::Gray(components[0]),
            Self::DeviceRGB => Color::Rgb(components[0], components[1], components[2]),
            Self::DeviceCMYK => {
                Color::Cmyk(components[0], components[1], components[2], components[3])
            }
            Self::ICCBased(_profile) => {
                // In a real implementation: map through ICC profile
                // For now, simple fallback based on component count
                match components.len() {
                    1 => Color::Gray(components[0]),
                    3 => Color::Rgb(components[0], components[1], components[2]),
                    4 => Color::Cmyk(components[0], components[1], components[2], components[3]),
                    _ => Color::Gray(0.0),
                }
            }
            // Colour spaces with no direct component mapping yet. Listed explicitly so
            // that a new `ColorSpaceKind` variant fails to compile here instead of
            // silently resolving to black.
            Self::CalGray
            | Self::CalRGB
            | Self::Lab
            | Self::Pattern
            | Self::Indexed
            | Self::Separation
            | Self::DeviceN
            | Self::Unknown => Color::Gray(0.0),
        }
    }
}

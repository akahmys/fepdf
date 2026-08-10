//! Unified Document Mutation Operation Vocabulary (ISO 32000-2 Protocol).
//!
//! Rule D: Frontends translate input (argv, UI clicks, MCP calls) into an Operation
//! value and pass it to fepdf-sdk. Only fepdf-sdk interprets operations.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
/// Represents a 90-degree quarter rotation (0, 90, 180, 270 degrees).
pub enum Quarter {
    /// 0 degrees (no rotation)
    Q0 = 0,
    /// 90 degrees clockwise
    Q90 = 90,
    /// 180 degrees
    Q180 = 180,
    /// 270 degrees (90 degrees counter-clockwise)
    Q270 = 270,
}

impl Quarter {
    /// Creates a Quarter from an integer angle if it is a multiple of 90.
    pub fn from_degrees(degrees: i32) -> Option<Self> {
        let normalized = degrees.rem_euclid(360);
        match normalized {
            0 => Some(Quarter::Q0),
            90 => Some(Quarter::Q90),
            180 => Some(Quarter::Q180),
            270 => Some(Quarter::Q270),
            _ => None,
        }
    }

    /// Converts Quarter to integer degrees.
    pub const fn to_degrees(self) -> i32 {
        self as i32
    }

    /// Adds another Quarter to this one, wrapping at 360 degrees.
    #[must_use]
    #[allow(clippy::should_implement_trait)]
    pub fn add(self, rhs: Quarter) -> Quarter {
        let sum = (self.to_degrees() + rhs.to_degrees()).rem_euclid(360);
        Self::from_degrees(sum).unwrap_or(Quarter::Q0)
    }
}

impl std::ops::Add for Quarter {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        self.add(rhs)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Rotation mode for page rotation operations.
pub enum RotateMode {
    /// Set absolute rotation angle.
    Absolute(Quarter),
    /// Add relative rotation angle to current rotation.
    Relative(Quarter),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Specifies a set of pages to target for an operation.
pub enum PageSelection {
    /// Target all pages in the document.
    All,
    /// Target a specific single page index (0-based).
    Single(usize),
    /// Target a list of 0-based page indices.
    Indices(Vec<usize>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Parameters for updating a structural element in the document.
pub struct StructElemUpdate {
    /// Target object handle index.
    pub handle_index: u32,
    /// New tag name if updating tag.
    pub new_tag: Option<String>,
    /// New Alt text if updating Alt text.
    pub new_alt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Canonical document mutation operations.
pub enum Operation {
    /// Rotate specified pages according to RotateMode.
    Rotate {
        /// Selection of pages to rotate.
        pages: PageSelection,
        /// Absolute or relative rotation mode.
        mode: RotateMode,
    },
    /// Reorder pages by moving a page from `from` index to `to` index.
    Reorder {
        /// Source 0-based page index.
        from: usize,
        /// Destination 0-based page index.
        to: usize,
    },
    /// Remove specified pages.
    RemovePages(PageSelection),
    /// Update a structural element's tag or Alt text.
    UpdateStructElem(StructElemUpdate),
    /// Delete a structural element by handle index.
    DeleteStructElem {
        /// Target handle index of the structural element object.
        handle_index: u32,
    },
}

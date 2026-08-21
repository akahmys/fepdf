//! Unified Document Mutation Operation Vocabulary (ISO 32000-2 Protocol).
//!
//! Rule D: Frontends translate input (argv, UI clicks, MCP calls) into an Operation
//! value and pass it to fepdf-doc. Only fepdf-doc interprets operations.

pub use fepdf_model::{
    AFRelationship, AnnotationKind, AnnotationSpec, ArticleThread, AssociatedFile,
    CollectionViewMode, FormFieldSpec, FormValue, GeoSpatialAnchor, MeasurementScale,
    MeshShadingSpec, MeshShadingType, OptionalContentProperties, OutlineNode, OutlineTree,
    OutputIntent, PageLabelSpec, PageLabelStyle, PdfAction, PortfolioCollection,
    PublicKeyRecipientSpec, TransitionSpec, TransitionStyle, UnencryptedWrapperSpec, UserProperty,
    UserPropertyValue, VisibilityState,
};
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
/// Position for page decorations (Header/Footer/Bates).
pub enum DecorationPosition {
    /// Top left position.
    TopLeft,
    /// Top center position.
    TopCenter,
    /// Top right position.
    TopRight,
    /// Bottom left position.
    BottomLeft,
    /// Bottom center position.
    BottomCenter,
    /// Bottom right position.
    BottomRight,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

    // --- Phase 2: Metadata & Structure Domain Operations ---
    /// Create or update a PDF Portfolio (/Collection).
    CreatePortfolio(PortfolioCollection),
    /// Update document outlines / bookmarks (/Outlines).
    UpdateOutlines(OutlineTree),
    /// Update optional content properties / layers (/OCProperties).
    UpdateLayers(OptionalContentProperties),
    /// Attach an associated file (/AF) to the document.
    AttachAssociatedFile(AssociatedFile),
    /// Set or update the document output intent (/OutputIntents).
    SetOutputIntent(OutputIntent),
    /// Embed a pronunciation lexicon XML (/PL).
    SetPronunciationLexicon {
        /// Raw XML bytes of the PLS lexicon.
        lexicon_xml_bytes: Vec<u8>,
    },

    // --- Phase 2: Decorations & Annotations Domain Operations ---
    /// Add page header, footer, or watermark text.
    AddPageDecoration {
        /// Target pages.
        pages: PageSelection,
        /// Text string to render.
        text: String,
        /// Position on the page.
        position: DecorationPosition,
        /// The optional content group to put the decoration in, by its
        /// [`crate::LayerGroup`] name (8.11.3.1). `None` draws it unconditionally.
        ///
        /// This is what makes a layer contain something. `UpdateLayers` writes the
        /// groups and, before this existed, nothing was ever marked `/OC` — so every
        /// group the engine created was empty whatever its state, and a document could
        /// not carry a "draft" underlay a reader could turn off. The layer must already
        /// exist: naming one the document does not have is refused rather than ignored,
        /// because a decoration that silently became unconditional is the failure this
        /// entry exists to remove.
        layer: Option<String>,
    },
    /// Apply Bates numbering to pages.
    ApplyBatesNumbering {
        /// Selection of pages.
        pages: PageSelection,
        /// Prefix string (e.g. "CONFIDENTIAL-").
        prefix: String,
        /// Starting number integer.
        start_number: u64,
        /// Total digits count for zero-padding (e.g. 6).
        digits: usize,
        /// Position of the number.
        position: DecorationPosition,
    },
    /// Add an annotation to a page.
    AddAnnotation(AnnotationSpec),
    /// Set a measurement scale for CAD/geospatial drawings (/Measure).
    SetMeasurementScale(MeasurementScale),

    // --- Phase 2: Interactive Forms Domain Operations ---
    /// Set a form field value in AcroForms.
    SetFormFieldValue(FormFieldSpec),

    // --- Phase 5: Navigation, Structure & Action Engine Operations ---
    /// Set page labels (/PageLabels).
    SetPageLabels(Vec<PageLabelSpec>),
    /// Update article threads (/Threads).
    UpdateArticleThreads(Vec<ArticleThread>),
    /// Add user properties to a Tagged PDF element (/UserProperties).
    AddUserProperties {
        /// Target element handle index.
        target_handle: u32,
        /// Properties list.
        properties: Vec<UserProperty>,
    },
    /// Execute an action (GoToR, GoToE, Named, Transition).
    ExecuteAction(PdfAction),

    // --- Phase 6: Advanced Graphics & GIS Operations ---
    /// Set a GIS geographic anchor (/Geo).
    SetGeospatialAnchor(GeoSpatialAnchor),
    /// Add a Type 4-7 mesh shading spec.
    AddMeshShading(MeshShadingSpec),

    // --- Phase 7: Font & Cryptography Operations ---
    /// Set unencrypted wrapper payload (Clause 7.6.7).
    SetUnencryptedWrapper(UnencryptedWrapperSpec),
    /// Add a public key recipient certificate (Clause 7.6.4).
    AddPublicKeyRecipient(PublicKeyRecipientSpec),
}

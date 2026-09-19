//! Unified Document Mutation Operation Vocabulary (ISO 32000-2 Protocol).
//!
//! Rule D: Frontends translate input (argv, UI clicks, MCP calls) into an Operation
//! value and pass it to fepdf-doc. Only fepdf-doc interprets operations.

pub use fepdf_model::{
    AFRelationship, Align, AnnotationKind, AnnotationSpec, ArticleThread, AssociatedFile,
    CollectionViewMode, ContentScale, FormFieldSpec, FormValue, GeoSpatialAnchor, MeasurementScale,
    MeshShadingSpec, MeshShadingType, OptionalContentProperties, OutlineNode, OutlineTree,
    OutputIntent, PageLabelSpec, PageLabelStyle, PageResize, PdfAction, PortfolioCollection,
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

/// Supported PDF modern standards for conversion.
///
/// Lived in the facade until `Operation::Upgrade` needed it. A type an operation carries
/// has to live with the vocabulary: the facade may re-export it, and cannot own it
/// without `fepdf-doc` depending upwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
/// Where a structural element is to be moved to (14.7.4).
pub struct StructElemMove {
    /// Target object handle index of the element to move.
    pub handle_index: u32,
    /// Object handle index of the element it moves relative to.
    pub target_index: u32,
    /// Where it lands relative to that element.
    pub placement: crate::struct_tree::Placement,
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
    /// Move several pages to a new position, as one movement rather than a sequence.
    ///
    /// Distinct from `Reorder`, which moves one page: moving a set one at a time makes
    /// every index after the first depend on the moves before it, and the two frontends
    /// that offered a multi-page drag both got that wrong in their own way.
    ReorderBatch {
        /// The 0-based indices to move, in the document's current numbering.
        sources: Vec<usize>,
        /// Where the moved run is inserted, in that same numbering.
        target: usize,
    },
    /// Duplicate pages, each clone placed immediately after its original.
    DuplicatePages(PageSelection),
    /// Insert every page of another document, given as the bytes of that document.
    ///
    /// The source is bytes rather than a handle to an open document because an operation
    /// is a value: it has to serialise, and `fepdf-mcp` reaches `apply` through JSON. The
    /// GUI already carried the bytes and opened them inside its worker, so this costs it
    /// nothing.
    InsertFrom {
        /// The complete source document.
        source: Vec<u8>,
        /// The 0-based position to insert at, clamped to the page count.
        at: usize,
    },
    /// Put the named pages on a different sheet, and say what happens to what is on them.
    ///
    /// **One operation for both "change the paper size" and "scale".** They are the same
    /// act asked in two directions: a sheet, and a rule for the drawing on it. A4 content
    /// on an A3 sheet is `size` A3 with [`ContentFit::Fit`]; the same content at 90% on
    /// the sheet it is already on is the size it already has with
    /// <code>[ContentFit::Scale](0.9)</code>. Two would have been two places to get the
    /// boxes right.
    ///
    /// `size` is the new `/MediaBox`, in points, placed at the origin — which is where
    /// 14.11.2's boxes are measured from and where every page in this corpus puts its own.
    ResizePages(PageSelection, PageResize),
    /// Add a Document Security Store (`/DSS`, 12.8.4.3) carrying validation certificates.
    ///
    /// **The only piece of `/DSS` that exists.** It was a facade method nothing called
    /// and no test exercised — code that writes a security structure, in the crate that is
    /// supposed to only expose them. It is here rather than deleted because the vocabulary
    /// is where the rest of long-term validation would go, and here rather than left alone
    /// because a mutation outside the vocabulary is how two implementations of one thing
    /// get started (Rule D). `tests/security_store_test.rs` covers what it writes; it
    /// stood untested while becoming an `fepdf-mcp` tool a client can call.
    AddLtvInfo {
        /// DER-encoded certificates, one stream each.
        certificates: Vec<Vec<u8>>,
    },
    /// Rebuild the document's logical structure from heuristics (14.7).
    Retag,
    /// Rewrite the catalogue and version for a target standard.
    Upgrade {
        /// The standard to declare.
        standard: PdfStandard,
    },
    /// Update a structural element's tag or Alt text.
    UpdateStructElem(StructElemUpdate),
    /// Delete a structural element by handle index.
    DeleteStructElem {
        /// Target handle index of the structural element object.
        handle_index: u32,
    },
    /// Move a structural element beside or inside another (14.7.4).
    ///
    /// **Reading order is what a structure tree is for**, and until this existed there
    /// was no way to change it: the vocabulary could retag an element and delete one, so
    /// the only way to put a paragraph in the right place was to delete it and lose its
    /// content. The GUI's tree has had drag-and-drop the whole time and it rearranged the
    /// window's own copy, which is an edit that looks like it worked.
    MoveStructElem(StructElemMove),

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
    /// Replaces every run of text on a page that reads exactly `find`.
    ///
    /// **A run, not a range.** One show-text operator is the unit: a run that reads
    /// `find` is rewritten whole, and one that merely contains it is left alone, because
    /// splitting a run means re-spacing what remains and that is a different piece of work
    /// (W-E4). The replacement is encoded in the font that run is set in and in no other;
    /// a character that font does not draw is refused by name rather than substituted
    /// ([ADR-0090](../../../docs/adr/0090-the-face-a-document-embeds-is-not-a-licence-to-set-new-text.md)).
    EditTextRun {
        /// The page to look on.
        page: usize,
        /// The text a run must read to be replaced.
        find: String,
        /// What it reads afterwards.
        replace: String,
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

impl Operation {
    /// Whether this moves what is drawn on a page.
    ///
    /// **The vocabulary answers for itself.** A frontend keeps things measured against
    /// the pages — the structure tree's rectangles come from where each element's marked
    /// content actually drew — and after an operation that moved the content they point
    /// at where it used to be. The window cannot know which operations those are without
    /// enumerating the vocabulary, which is this crate's business (Rule D).
    ///
    /// Comparing page sizes catches most of it and not all: `ResizePages` with no sheet
    /// named leaves every page exactly the size it was and moves everything on it.
    ///
    /// No wildcard arm, so a new variant does not compile until someone has decided
    /// (Rule 5) — which is the point, since what it prevents fails silently.
    #[must_use]
    pub const fn moves_content(&self) -> bool {
        // RR-15 Limit: Dispatcher - one arm per variant, which is what exhaustive means
        match self {
            Self::Rotate { .. }
            | Self::ResizePages(..)
            | Self::AddPageDecoration { .. }
            | Self::ApplyBatesNumbering { .. } => true,
            // Rebuilt from the marks already on the page, which do not move.
            Self::Retag => false,
            Self::Reorder { .. }
            | Self::RemovePages { .. }
            | Self::ReorderBatch { .. }
            | Self::DuplicatePages { .. }
            | Self::InsertFrom { .. }
            | Self::AddLtvInfo { .. }
            | Self::Upgrade { .. }
            | Self::UpdateStructElem { .. }
            | Self::DeleteStructElem { .. }
            | Self::MoveStructElem { .. }
            | Self::CreatePortfolio { .. }
            | Self::UpdateOutlines { .. }
            | Self::UpdateLayers { .. }
            | Self::AttachAssociatedFile { .. }
            | Self::SetOutputIntent { .. }
            | Self::SetPronunciationLexicon { .. }
            | Self::AddAnnotation { .. }
            | Self::EditTextRun { .. }
            | Self::SetMeasurementScale { .. }
            | Self::SetFormFieldValue { .. }
            | Self::SetPageLabels { .. }
            | Self::UpdateArticleThreads { .. }
            | Self::AddUserProperties { .. }
            | Self::ExecuteAction { .. }
            | Self::SetGeospatialAnchor { .. }
            | Self::AddMeshShading { .. }
            | Self::SetUnencryptedWrapper { .. }
            | Self::AddPublicKeyRecipient { .. } => false,
        }
    }
}

#[cfg(test)]
mod moves_content_tests {
    use super::{ContentScale, Operation, PageResize, PageSelection};

    fn resize(sheet: Option<(f64, f64)>) -> Operation {
        Operation::ResizePages(
            PageSelection::All,
            PageResize { sheet, scale: ContentScale::By(0.6), offset: (0.0, 0.0) },
        )
    }

    /// **A resize naming no sheet still moves what is on the page.**
    ///
    /// This is the case comparing page sizes cannot see: every page comes out the size it
    /// went in, and everything drawn on it has moved. A frontend that watched the sizes
    /// alone left its structure-tree rectangles pointing at where the content used to be.
    #[test]
    fn a_resize_moves_content_whether_or_not_it_names_a_sheet() {
        assert!(resize(None).moves_content());
        assert!(resize(Some((842.0, 1191.0))).moves_content());
    }

    /// Rotation and the two that draw on the page move it; the rest do not.
    #[test]
    fn only_what_touches_the_page_says_it_does() {
        assert!(
            Operation::Rotate {
                pages: PageSelection::All,
                mode: super::RotateMode::Relative(super::Quarter::Q90),
            }
            .moves_content()
        );

        // A reorder moves pages past each other and moves nothing on any of them.
        assert!(!Operation::Reorder { from: 0, to: 1 }.moves_content());
        assert!(!Operation::RemovePages(PageSelection::Single(0)).moves_content());
        // Retag rebuilds the tree from the marks already there, which have not moved.
        assert!(!Operation::Retag.moves_content());
    }
}

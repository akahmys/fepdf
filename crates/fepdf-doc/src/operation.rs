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
    /// Replaces the text of one run — one show-text operator — on a page.
    ///
    /// **A run is what the file declares, and nothing here groups them.** Which runs
    /// belong together is a question about meaning that a content stream does not answer:
    /// characters drawn next to each other may be a word, or a label and its value, or two
    /// columns, and a processor that joined them would be guessing at what it was editing
    /// ([ADR-0091](../../../docs/adr/0091-paragraphs-are-not-inferred-and-overflow-is-shown.md)).
    /// So a caller names a run, by its position among the page's runs, and gets exactly
    /// that run changed.
    ///
    /// The text is encoded in the font that run is set in; a character it does not draw is
    /// refused by name. What follows on the line moves by the difference in advance, which
    /// is what the operators already do, and text that no longer fits is drawn anyway.
    EditRun {
        /// The page the run is on.
        page: usize,
        /// Which run, counting show-text operators from the start of the page's content.
        run: usize,
        /// What it reads afterwards.
        text: String,
    },
    /// Cuts one run in two, after `after` characters of what it reads.
    ///
    /// **A split needs no arithmetic.** Consecutive show-text operators draw from the
    /// current point, so two runs in place of one put the same glyphs in the same places;
    /// what changes is that a caller can then name either half. It is how a reader says
    /// that part of a run is a thing on its own, which is the other half of merging
    /// ([ADR-0091](../../../docs/adr/0091-paragraphs-are-not-inferred-and-overflow-is-shown.md)).
    SplitRun {
        /// The page the run is on.
        page: usize,
        /// Which run, counting show-text operators from the start of the page's content.
        run: usize,
        /// How many of its codes stay in the first half.
        ///
        /// Codes, not characters: reading a run and writing it back is not always the
        /// identity, so a cut made on the reading would lose whatever the reading lost.
        /// `RunInfo::pieces` says what each code reads, which is how a place in the text
        /// becomes a place among the codes.
        after: usize,
    },
    /// Joins one run with the run after it.
    ///
    /// **The other half of `SplitRun`.** A reader who knows two runs are one phrase says
    /// so by joining them; one who knows a run is two things says so by cutting it.
    /// Neither asks this engine to decide which, which is the whole of ADR-0091.
    ///
    /// Nothing may stand between the two. A `Td`, a `Tf`, a `T*` between them moves the
    /// text or changes what it is set in, so joining across one would draw the second
    /// half somewhere it was not; the operator in the way is named rather than stepped
    /// over.
    MergeRuns {
        /// The page the runs are on.
        page: usize,
        /// The first of the two, counting show-text operators from the start of the page.
        run: usize,
    },
    /// Puts one run somewhere else on the page, leaving every other run where it is.
    ///
    /// **A run's position is cumulative, so this is not rewriting an operand.** `Tm` sets
    /// the text matrix and the line matrix together, and showing text advances only the
    /// first, so after a run the two differ and no single `Tm` puts both back. The run is
    /// drawn in a text object of its own and the one it came from is reopened with its
    /// line matrix restored and a `TJ` offset stepping the text matrix on from it.
    ///
    /// The codes are reused rather than re-encoded, so a run this engine reads short can
    /// still be moved, and the run keeps its number.
    MoveRun {
        /// The page the run is on.
        page: usize,
        /// Which run, counting show-text operators from the start of the page's content.
        run: usize,
        /// Where it draws from afterwards, in the page's default user space.
        to: (f64, f64),
    },
    /// Puts several pages onto one sheet, in a grid.
    ///
    /// Each source page becomes a form XObject drawn into a cell, so it keeps the fonts
    /// and images it names without those having to be merged into anything (8.10). What a
    /// page carries beside what it draws — its annotations, above all — belongs to the
    /// page it was on and does not come across.
    CombinePages(PageSelection, PageArrangement),
    /// Cuts one page into several, each carrying one region of it.
    ///
    /// **A split always removes what belongs to the other sheets** (ADR-0088). Half a
    /// drawing, still searchable, on a page showing the other half is a leak dressed as a
    /// feature, so there is no option here to hide rather than cut.
    SplitPage {
        /// The page to cut up.
        page: usize,
        /// How to divide it.
        into: PageDivision,
    },
    /// Cuts pages down to a rectangle (14.11.2).
    ///
    /// **Two things are called cropping and only one of them cuts.** `/CropBox` names the
    /// region a viewer displays and leaves everything else in the file, which is a view:
    /// any reader can move it back and see what it hid. Taking the content out is a
    /// different act with a different consequence, so the caller says which
    /// ([ADR-0088](../../../docs/adr/0088-what-a-crop-puts-outside-the-sheet-is-removed.md)).
    CropPages(PageSelection, CropRegion),
    /// Takes off a page every glyph that falls outside a rectangle.
    ///
    /// **What a crop puts outside the sheet is removed rather than hidden.** `/CropBox`
    /// makes a region the viewer displays (14.11.2) and leaves the rest in the file: half
    /// a drawing, still searchable, on a page showing the other half
    /// ([ADR-0088](../../../docs/adr/0088-what-a-crop-puts-outside-the-sheet-is-removed.md)).
    ///
    /// What stays does not move: a run is not deleted, because that takes its advance
    /// with it, but rewritten as the glyphs that remain and the offsets that stand for
    /// the ones that went.
    RemoveOutside {
        /// The page to cut down.
        page: usize,
        /// What to keep, in the page's default user space: left, bottom, right, top.
        keep: (f64, f64, f64, f64),
    },
    /// Takes one run off the page.
    ///
    /// **Deleting a run is not editing it to nothing.** An emptied run is still a run: it
    /// keeps its number, and a caller can put text back into it. A deleted one is gone
    /// from the listing, and the runs after it move up by one. What the operator did
    /// besides draw — a line movement, a spacing setting — stays, because the rest of the
    /// page is placed by it.
    DeleteRun {
        /// The page the run is on.
        page: usize,
        /// Which run, counting show-text operators from the start of the page's content.
        run: usize,
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
            | Self::EditRun { .. }
            | Self::SplitRun { .. }
            | Self::DeleteRun { .. }
            | Self::MergeRuns { .. }
            | Self::MoveRun { .. }
            | Self::RemoveOutside { .. }
            | Self::CropPages { .. }
            | Self::SplitPage { .. }
            | Self::CombinePages { .. }
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

/// How several pages are laid out on one sheet.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PageArrangement {
    /// The sheet they go onto, or `None` to use the first page's own.
    pub sheet: Option<(f64, f64)>,
    /// How many cells across.
    pub columns: usize,
    /// How many cells down.
    pub rows: usize,
}

/// How one page is cut into several.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PageDivision {
    /// Equal parts, so many across and so many down.
    ///
    /// The pages come out in reading order: across a row first, then down. A drawing cut
    /// two by two is read top-left, top-right, bottom-left, bottom-right, which is the
    /// order somebody laying the sheets out on a table puts them in.
    Grid {
        /// How many parts across.
        columns: usize,
        /// How many parts down.
        rows: usize,
    },
    /// Regions named outright, in the space the page's boxes are written in, in the order
    /// the pages are to come out.
    Regions(Vec<(f64, f64, f64, f64)>),
}

impl PageDivision {
    /// The regions this divides a page of `size` into.
    ///
    /// A grid is worked out here rather than by whoever asks for one, because the sheet's
    /// measurements are the engine's to read: a frontend computing its own would be
    /// reading the page to tell the engine about the page.
    #[must_use]
    pub fn regions(&self, size: (f64, f64)) -> Vec<(f64, f64, f64, f64)> {
        match self {
            Self::Regions(named) => named.clone(),
            Self::Grid { columns, rows } => {
                // Counted as `u32`, which every `f64` holds exactly. A grid finer than
                // four thousand million parts across divides a sheet into pieces no unit
                // of this format can express, and answering no regions to that is what
                // `apply_split_page` refuses.
                let (Ok(across), Ok(down)) = (u32::try_from(*columns), u32::try_from(*rows)) else {
                    return Vec::new();
                };
                if across == 0 || down == 0 {
                    return Vec::new();
                }
                let (wide, tall) = (size.0 / f64::from(across), size.1 / f64::from(down));
                // Down the rows from the top, because that is the order they are read in,
                // and a page's own coordinates count up from its foot.
                (0..down)
                    .flat_map(|row| {
                        (0..across).map(move |column| {
                            let left = f64::from(column) * wide;
                            let top = f64::from(row).mul_add(-tall, size.1);
                            (left, top - tall, left + wide, top)
                        })
                    })
                    .collect()
            }
        }
    }
}

/// What a crop keeps, and what it does with the rest.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CropRegion {
    /// What to keep, in the space the page's boxes are written in: left, bottom, right,
    /// top. It becomes the new sheet, with its lower-left corner at the origin.
    pub keep: (f64, f64, f64, f64),
    /// What happens to the content outside it.
    pub outside: WhatFallsOutside,
}

/// What becomes of the content a crop puts outside the sheet.
///
/// **Named rather than a flag**, because the two are different acts and a reader choosing
/// between them is choosing what leaves the file. A `bool` at the call site says which
/// only to somebody who remembers which way round it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum WhatFallsOutside {
    /// It stays in the file and `/CropBox` hides it, which is a view rather than a cut.
    #[default]
    Stays,
    /// It is taken out of the file (ADR-0088).
    Goes,
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

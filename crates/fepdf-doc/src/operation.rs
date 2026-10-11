//! Unified Document Mutation Operation Vocabulary (ISO 32000-2 Protocol).
//!
//! Rule D: Frontends translate input (argv, UI clicks, MCP calls) into an Operation
//! value and pass it to fepdf-doc. Only fepdf-doc interprets operations.

pub use fepdf_model::{
    AFRelationship, Align, AnnotationKind, AnnotationSpec, ArticleThread, AssociatedFile,
    Authorship, CollectionViewMode, ContentScale, FormFieldSpec, FormValue, GeoSpatialAnchor,
    MeasurementScale, MediaClip, OptionalContentProperties, OutlineNode, OutlineTree, OutputIntent,
    PageLabelSpec, PageLabelStyle, PageResize, PdfAction, PortfolioCollection, PrinterMarkKind,
    ShapeForm, TextSetting, TransitionSpec, TransitionStyle, UnencryptedWrapperSpec, UserProperty,
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
/// Parameters for updating a structural element in the document.
///
/// **Every entry is a text string an element states about its content** (ISO 32000-2
/// Table 355), and `None` leaves it as it is. WTPDF asks for the three after `/Alt`
/// (8.2.5.23, 8.4): the language, the text a figure or ligature stands for, and an
/// abbreviation's expansion.
pub struct StructElemUpdate {
    /// Target object handle index.
    pub handle_index: u32,
    /// New tag name if updating tag.
    pub new_tag: Option<String>,
    /// New Alt text if updating Alt text.
    pub new_alt: Option<String>,
    /// New `/Lang` (14.9.2): the language of the element's content. An empty string
    /// states that it is unknown (14.9.2.2).
    #[serde(default)]
    pub new_lang: Option<String>,
    /// New `/ActualText` (14.9.4): the text the element's content stands for.
    #[serde(default)]
    pub new_actual_text: Option<String>,
    /// New `/E` (14.9.5): the expansion of an abbreviation or acronym.
    #[serde(default)]
    pub new_expansion: Option<String>,
}

/// One attribute of a structure element (ISO 32000-2 14.7.6): the entry `key` of the
/// attribute object whose owner (`/O`) is `owner`.
///
/// **Any owner and any key, written as given.** The standard owners' keys are tables of
/// their own (14.8.5) — `Scope` and `Headers` for `Table`, `ListNumbering` for `List`,
/// `NoteType` for a note, the `Layout` keys, and those of `ARIA-1.1` — and WTPDF names the
/// ones a well-tagged file needs (8.2.6). This writes the entry; which entry is right is
/// the caller's to say.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructAttribute {
    /// Target object handle index of the element.
    pub handle_index: u32,
    /// The attribute owner, as `/O` names it: `Table`, `List`, `Layout`, `ARIA-1.1`, ….
    pub owner: String,
    /// The key within the attribute object: `Scope`, `ListNumbering`, `Placement`, ….
    pub key: String,
    /// What it is set to.
    pub value: AttributeValue,
}

/// An attribute's value, in the PDF types attributes take (14.8.5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AttributeValue {
    /// A name: `Column`, `Decimal`, `Block`.
    Name(String),
    /// A number.
    Number(f64),
    /// A text string.
    Text(String),
    /// A boolean.
    Boolean(bool),
    /// An array of names.
    Names(Vec<String>),
    /// An array of numbers: a `BBox`, a colour.
    Numbers(Vec<f64>),
    /// An array of byte strings: a table cell's `Headers`, which are element IDs.
    Strings(Vec<String>),
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
/// A new structure element round a run of an element's kids (14.7.2).
pub struct StructElemWrap {
    /// Object handle index of the element, or the structure tree root, whose kids are
    /// wrapped.
    pub handle_index: u32,
    /// The first kid wrapped, counted from 0 in its `/K`.
    pub first: usize,
    /// How many kids are wrapped, from `first`.
    pub count: usize,
    /// The new element's structure type: `Caption`, `Lbl`, `LBody`, `RB`, `RT`, `RP`, ….
    pub tag: String,
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
    /// A page for each picture, put in at `at` (ROADMAP AA-5, ADR-0123): a JPEG carried as
    /// it is, a PNG with its alpha as a soft mask, and each page of a TIFF.
    ///
    /// Bytes, as `InsertFrom`'s source is, so the operation serialises.
    InsertImages {
        /// The pictures, each a JPEG, PNG or TIFF file whole.
        images: Vec<Vec<u8>>,
        /// The 0-based position to insert at, clamped to the page count.
        at: usize,
        /// The sheet every page is, in points, the picture fitted inside it; or `None`,
        /// for each page to be its picture's size at the resolution its file states.
        sheet: Option<[f32; 2]>,
    },
    /// Pages of plain text, put in at `at` (ROADMAP AA-6, ADR-0124): lines broken where
    /// UAX #14 allows at the face's advances, pages broken where they fill, and each
    /// paragraph — what a blank line separates — tagged `/P`.
    InsertText {
        /// The text. A line break is kept; a blank line ends a paragraph; a form feed
        /// starts a page.
        text: String,
        /// The 0-based position to insert at, clamped to the page count.
        at: usize,
        /// The sheet, margins, size and spacing.
        #[serde(default)]
        setting: TextSetting,
    },
    /// Put the named pages on a different sheet, and say what happens to what is on them.
    ///
    /// **One operation for both "change the paper size" and "scale".** They are the same
    /// act asked in two directions: a sheet, and a rule for the drawing on it. A4 content
    /// on an A3 sheet is `sheet` A3 with [`ContentScale::Fit`]; the same content at 90% on
    /// the sheet it is already on is `sheet` `None` with
    /// <code>[ContentScale::By](0.9)</code>. Two would have been two places to get the
    /// boxes right.
    ///
    /// `sheet` is the new `/MediaBox`, in points, placed at the origin; the drawing is taken
    /// from where the old box began, which need not be the origin.
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
    /// Declare conformity with a standard in the XMP metadata, as a PDF Declaration's
    /// `pdfd:conformsTo` — WTPDF 6.1's `…/wtpdf/#reuse1.0` or `#accessibility1.0`, or any
    /// other the PDF Association lists. The caller's statement: nothing is checked.
    DeclareConformance {
        /// The URI of what the document conforms to.
        conforms_to: String,
    },
    /// Identify the document with a target standard in its XMP metadata, at version 2.0.
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
    /// Name a pronunciation lexicon (PLS 1.0 XML) in the structure tree root's
    /// `/PronunciationLexicon` (Table 354, 14.9.6), replacing any it named.
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
        /// [`fepdf_model::LayerGroup`] name (8.11.3.1). `None` draws it unconditionally.
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
    /// Creates a form field, with its widget on a page.
    ///
    /// **Through to creation, not only filling** (ADR-0087). A document this engine
    /// declares PDF/UA-2 conforming has to have accessible fields, and a field it did not
    /// create is one it can only complain about.
    AddFormField(NewField),
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
    /// Removes what a page draws inside rectangles, and fills them (12.5.6.23).
    ///
    /// **Removed, not covered.** A glyph whose box meets a region at all goes, and the
    /// glyphs outside keep their places. The regions are then filled as
    /// [`Redaction::fill`] says. What is removed can be read before it is applied, with
    /// `what_redaction_removes` (ROADMAP Y-10).
    Redact(Redaction),
    /// Applies the document's own redaction annotations on the pages selected
    /// (12.5.6.23): what each marks is removed as `Redact` removes it, the annotation goes,
    /// and its place is drawn as Table 195 says — `/RO`, else `/IC` and `/OverlayText`,
    /// else nothing, the region left transparent.
    ApplyRedactAnnotations(PageSelection),
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
    /// Remove an annotation, with its pop-up and every annotation that replies to it
    /// (12.5.6.2), and the structure tree's references to them.
    ///
    /// A widget is refused: it is half of a form field, and a field is removed as a field.
    RemoveAnnotation(AnnotationAt),
    /// Replace the words an annotation carries (`/Contents`), and say when (`/M`).
    ///
    /// A free text annotation is refused, because its appearance draws the words and
    /// would go on drawing the old ones. Remove it and add another.
    EditAnnotation {
        /// Which.
        at: AnnotationAt,
        /// Its words now.
        contents: String,
        /// When, as a date string (7.9.4).
        when: Option<String>,
    },
    /// Answer an annotation: a text annotation whose `/IRT` is the one answered
    /// (12.5.6.2). A reply is not drawn on its own, and has no appearance.
    ReplyToAnnotation {
        /// Which is answered.
        at: AnnotationAt,
        /// What the reply says.
        contents: String,
        /// Who answered, and when.
        by: Authorship,
    },
    /// Put the annotations of an FDF file onto the document (12.7.8.3.4): one whose `/NM`
    /// matches an annotation on its page replaces it in place, and any other is added
    /// (ADR-0117).
    ImportFdf {
        /// The FDF file, whole.
        fdf: Vec<u8>,
    },
    /// Put the annotations of an XFDF file onto the document (ISO 19444-1), as `ImportFdf`
    /// does: matched by name, and drawn from their entries where they carry no appearance
    /// (ADR-0117, ADR-0119).
    ImportXfdf {
        /// The XFDF file, whole.
        xfdf: Vec<u8>,
    },
    /// Set an annotation's state for a person (12.5.6.3).
    ///
    /// Written as the clause says, as a text annotation in reply: to the annotation, the
    /// first time this author sets a state in this model, and to their previous one after.
    /// The author is required, because the clause requires `/T`.
    SetAnnotationState {
        /// Which annotation.
        at: AnnotationAt,
        /// The state, which names its model.
        state: AnnotationState,
        /// Who set it, and when. `author` must be given.
        by: Authorship,
    },
    /// Set a measurement scale for CAD/geospatial drawings (/Measure).
    SetMeasurementScale(MeasurementScale),

    // --- Phase 2: Interactive Forms Domain Operations ---
    /// Set a form field value in AcroForms.
    SetFormFieldValue(FormFieldSpec),
    /// The order a reader's Tab key moves through the annotations of these pages
    /// (`/Tabs`, Table 31).
    SetTabOrder {
        /// Which pages.
        pages: PageSelection,
        /// The order.
        order: TabOrder,
    },
    /// The order the form's calculated fields are recalculated in (`/CO`, Table 224), by
    /// their fully qualified names.
    ///
    /// **Every field that calculates, once each, and nothing else.** `/CO` is required
    /// when any field has a calculation action, so an order that left one out would
    /// write a form that does not conform, and one naming a field that calculates nothing
    /// would recalculate a value nobody computes.
    SetCalculationOrder(Vec<String>),
    /// Lays text an OCR engine read over a page, drawn invisibly (text rendering mode 3)
    /// in an embedded face with `/ToUnicode`, so it is found and copied and never seen
    /// (ADR-0086).
    AddTextLayer {
        /// Which page, counting from zero.
        page: usize,
        /// Each piece of text, and the box on the page it was read from.
        items: Vec<TextLayerItem>,
    },
    /// Moves, scales, turns or replaces one object a page draws with `Do` — an image or a
    /// form XObject — named by its place among the page's `Do` operators, as
    /// `fepdf::xobject::objects_of_page` lists them.
    EditXObject {
        /// The page, counting from zero.
        page: usize,
        /// The object, by its place among the page's `Do` operators.
        object: usize,
        /// What to do to it.
        edit: XObjectEdit,
    },

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
    /// Set one attribute of a structure element, in the attribute object of its owner.
    SetStructAttribute(StructAttribute),
    /// Wrap a run of an element's kids in a new element, which takes their place: the
    /// `Caption` of a `Figure`, the `Lbl` and `LBody` of an `LI`, the `RB`, `RT` and `RP` of
    /// a `Ruby` (WTPDF 8.2.5).
    WrapStructElem(StructElemWrap),
    /// Mark a tagged sequence as an artifact (14.8.2.2, WTPDF 8.3): its `BDC` becomes an
    /// `/Artifact` one, and the structure tree stops claiming its MCID — the parent tree's
    /// entry and the element's `/K`. A running header marked as content, say, becomes the
    /// `Pagination` / `Header` artifact it is.
    MarkArtifact {
        /// The page, counted from 0.
        page: usize,
        /// The MCID of the sequence on that page.
        mcid: i64,
        /// `/Type`: `Pagination`, `Layout`, `Page` or `Background`.
        kind: Option<String>,
        /// `/Subtype`: `Header`, `Footer` or `Watermark`.
        subtype: Option<String>,
    },
    /// Associate a file with a structure element (`/AF`, 14.13): a formula's MathML with
    /// `AFRelationship` `Supplement`, as WTPDF 8.2.5.29 asks, where `AttachAssociatedFile`
    /// associates with the catalogue.
    AttachStructAssociatedFile {
        /// Target object handle index of the element.
        handle_index: u32,
        /// The file, embedded, with its relationship to the element.
        file: AssociatedFile,
    },
    /// Put a structure element in a namespace (`/NS`, 14.7.4): the one the structure tree
    /// root's `/Namespaces` names by `namespace`, added there if it has none. An empty
    /// string removes `/NS`, which puts the element back in the default namespace.
    SetStructNamespace {
        /// Target object handle index of the element.
        handle_index: u32,
        /// The namespace name, a URI: `http://iso.org/pdf2/ssn` for PDF 2.0's standard types.
        namespace: String,
    },
    /// Map a structure type of a namespace to another type (`RoleMapNS`, Table 356): to a
    /// type of the default standard namespace, or of `to_namespace` when given.
    MapStructType {
        /// The namespace whose type is mapped, added to `/Namespaces` if absent.
        namespace: String,
        /// The type being mapped.
        from: String,
        /// The type it is mapped to.
        to: String,
        /// The namespace `to` is in; the default standard namespace when absent.
        to_namespace: Option<String>,
    },
    /// Set the structure elements an element refers to (`/Ref`, ISO 32000-2 Table 355): a
    /// table-of-contents item to its target, a citation to its note and back, a continued
    /// list to its previous part (WTPDF 8.8). An empty list removes the entry.
    SetStructRefs {
        /// Target object handle index of the element that refers.
        handle_index: u32,
        /// Object handle indices of the elements it refers to, in order.
        targets: Vec<u32>,
    },
    /// Sets the catalogue's `/OpenAction` (12.6.2): the action a reader runs when it
    /// opens the document — go to another file (GoToR), to an embedded one (GoToE), a
    /// named action, or a transition. Any `/OpenAction` the document had is replaced.
    ///
    /// **Named for what it writes.** It was `ExecuteAction`, and nothing runs: this
    /// engine writes the action for a reader to run, the way every other operation writes
    /// what it names.
    SetOpenAction(PdfAction),

    // --- Phase 6: Advanced Graphics & GIS Operations ---
    /// Set a GIS geographic anchor (/Geo).
    SetGeospatialAnchor(GeoSpatialAnchor),

    // --- Phase 7: Font & Cryptography Operations ---
    /// Set unencrypted wrapper payload (Clause 7.6.7).
    SetUnencryptedWrapper(UnencryptedWrapperSpec),
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
            | Self::EditXObject { .. }
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
            | Self::InsertImages { .. }
            | Self::InsertText { .. }
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
            | Self::RemoveAnnotation(_)
            | Self::EditAnnotation { .. }
            | Self::ReplyToAnnotation { .. }
            | Self::SetAnnotationState { .. }
            | Self::ImportFdf { .. }
            | Self::ImportXfdf { .. }
            // Laid over the page, which is not moved.
            | Self::AddTextLayer { .. }
            | Self::EditRun { .. }
            | Self::SplitRun { .. }
            | Self::DeleteRun { .. }
            | Self::MergeRuns { .. }
            | Self::MoveRun { .. }
            | Self::RemoveOutside { .. }
            // Taken out where it is, and the rest left where it was.
            | Self::Redact(_)
            | Self::ApplyRedactAnnotations(_)
            | Self::CropPages { .. }
            | Self::SplitPage { .. }
            | Self::CombinePages { .. }
            | Self::AddFormField { .. }
            | Self::SetMeasurementScale { .. }
            | Self::SetFormFieldValue { .. }
            | Self::SetTabOrder { .. }
            | Self::SetCalculationOrder(_)
            | Self::SetPageLabels { .. }
            | Self::UpdateArticleThreads { .. }
            | Self::AddUserProperties { .. }
            | Self::SetStructAttribute(..)
            | Self::SetStructRefs { .. }
            | Self::SetStructNamespace { .. }
            | Self::AttachStructAssociatedFile { .. }
            | Self::MarkArtifact { .. }
            | Self::WrapStructElem(_)
            | Self::DeclareConformance { .. }
            | Self::MapStructType { .. }
            | Self::SetOpenAction { .. }
            | Self::SetGeospatialAnchor { .. }
            | Self::SetUnencryptedWrapper { .. }
            => false,
        }
    }
}

/// An annotation already on a page: which page, and its place in that page's `/Annots`
/// ([ADR-0115]).
///
/// [ADR-0115]: ../../../docs/adr/0115-an-operation-names-an-annotation-by-its-place-on-the-page.md
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct AnnotationAt {
    /// The page, from zero.
    pub page: usize,
    /// Its place in the page's `/Annots`, from zero.
    pub index: usize,
}

/// A state an annotation can be given (Table 174). **Each names its model**, so a state
/// cannot be paired with the wrong one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnnotationState {
    /// `Marked`, in the `Marked` model.
    Marked,
    /// `Unmarked`, in the `Marked` model.
    Unmarked,
    /// `Accepted`, in the `Review` model: the reviewer agrees with the change.
    Accepted,
    /// `Rejected`, in the `Review` model.
    Rejected,
    /// `Cancelled`, in the `Review` model.
    Cancelled,
    /// `Completed`, in the `Review` model.
    Completed,
    /// `None`, in the `Review` model: nothing said about the change.
    None,
}

impl AnnotationState {
    /// `/State`, as Table 174 spells it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Marked => "Marked",
            Self::Unmarked => "Unmarked",
            Self::Accepted => "Accepted",
            Self::Rejected => "Rejected",
            Self::Cancelled => "Cancelled",
            Self::Completed => "Completed",
            Self::None => "None",
        }
    }

    /// `/StateModel`: the model this state belongs to.
    #[must_use]
    pub const fn model(self) -> &'static str {
        match self {
            Self::Marked | Self::Unmarked => "Marked",
            Self::Accepted | Self::Rejected | Self::Cancelled | Self::Completed | Self::None => {
                "Review"
            }
        }
    }

    /// The state `/State` and `/StateModel` name, if they name one of Table 174's.
    #[must_use]
    pub fn named(state: &str, model: &str) -> Option<Self> {
        [
            Self::Marked,
            Self::Unmarked,
            Self::Accepted,
            Self::Rejected,
            Self::Cancelled,
            Self::Completed,
            Self::None,
        ]
        .into_iter()
        .find(|s| s.name() == state && s.model() == model)
    }
}

/// What an `EditXObject` does to the object it names. Each is said on the page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum XObjectEdit {
    /// Moves it so its lower left corner is at `to`, in points from the page's.
    Move {
        /// Where.
        to: (f64, f64),
    },
    /// Scales it about its centre.
    Scale {
        /// By how much: 2 is twice the size.
        by: f64,
    },
    /// Turns it about its centre, anticlockwise.
    Rotate {
        /// By how many degrees.
        degrees: f64,
    },
    /// Draws a JPEG in its place, stretched over the same square. An image only.
    Replace {
        /// The picture.
        jpeg: Vec<u8>,
    },
}

/// The order a reader's Tab key moves through a page's annotations (Table 31, `/Tabs`).
///
/// **Absent means unspecified**, which 12.5.1 leaves to the reader — and 3,719 of the
/// 3,731 pages carrying annotations in the samples and the external corpus have no
/// `/Tabs` at all (`cargo run --release --example tab_and_calc`, 2026-09-26).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TabOrder {
    /// `R`: across each row, then down.
    Row,
    /// `C`: down each column, then across.
    Column,
    /// `S`: the order of the structure tree, which PDF/UA asks of a page with annotations.
    Structure,
    /// `A` (PDF 2.0): the order `/Annots` lists them in.
    Annotations,
    /// `W` (PDF 2.0): `/Annots` order, the widgets first and everything else after.
    Widgets,
}

impl TabOrder {
    /// The name `/Tabs` holds.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Row => "R",
            Self::Column => "C",
            Self::Structure => "S",
            Self::Annotations => "A",
            Self::Widgets => "W",
        }
    }
}

/// A form field to create, and where its widget sits.
///
/// **`/TU` is not optional here.** A field without one is a Matterhorn failure this engine
/// already reports, and [ADR-0087](../../../docs/adr/0087-a-form-field-is-created-here-not-only-filled.md)
/// was taken so that it could repair what it names rather than only name it. A creator
/// that let the defect in would be the auditor writing its own findings.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NewField {
    /// The page the widget goes on, counting from zero.
    pub page: usize,
    /// Where on it, in the page's own space: left, bottom, right, top.
    pub rect: (f64, f64, f64, f64),
    /// `/T`, the field's name, which is what an edit to its value names.
    pub name: String,
    /// `/TU`, what a reader is told the field is for — announced by a screen reader and
    /// shown as a tooltip.
    pub tooltip: String,
    /// What kind of field it is, and what that kind needs.
    pub kind: FieldKind,
}

/// The nine kinds of widget a form is made of (12.7.5).
///
/// **The kind decides `/FT` and `/Ff` together**, which is why they are one choice here
/// rather than a type and a bag of flags: bit 13 is `Multiline` on a text field and
/// nothing on any other, and a caller assembling those by hand assembles them wrong.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FieldKind {
    /// One line of text.
    Text {
        /// What it holds to begin with.
        value: String,
    },
    /// Several lines of text (Table 228, bit 13).
    TextArea {
        /// What it holds to begin with.
        value: String,
    },
    /// A text field that shows what is typed as dots (Table 228, bit 14).
    ///
    /// The value is not given here: a password written into a document is a password in
    /// the document, and this engine will not put one there.
    Password,
    /// A box that is ticked or not.
    CheckBox {
        /// Whether it starts ticked.
        on: bool,
    },
    /// One of a group, of which one at a time is chosen (Table 230, bit 16).
    ///
    /// The group is the field; this button is a widget of it, and the field's name is the
    /// name of the state that choosing it writes into the group's `/V` (12.7.5.2.4).
    RadioButton {
        /// The name of the group it belongs to, which is the field name they share.
        group: String,
        /// Whether it starts chosen.
        on: bool,
    },
    /// A button that does something rather than holding a value (Table 230, bit 17).
    PushButton {
        /// What it says on it.
        caption: String,
    },
    /// A list that drops down, and shows the chosen one when it is closed (Table 231,
    /// bit 18).
    ComboBox {
        /// What it offers, as `/Opt`.
        options: Vec<String>,
        /// Which of them it starts on.
        value: String,
    },
    /// A list that stands open.
    ListBox {
        /// What it offers, as `/Opt`.
        options: Vec<String>,
        /// Which of them it starts on.
        value: String,
    },
    /// A place for a signature, which is signed rather than filled (12.7.5.5).
    Signature,
}

impl FieldKind {
    /// `/FT`, the field type this kind is (Table 226).
    #[must_use]
    pub const fn field_type(&self) -> &'static str {
        match self {
            Self::Text { .. } | Self::TextArea { .. } | Self::Password => "Tx",
            Self::CheckBox { .. } | Self::RadioButton { .. } | Self::PushButton { .. } => "Btn",
            Self::ComboBox { .. } | Self::ListBox { .. } => "Ch",
            Self::Signature => "Sig",
        }
    }

    /// `/Ff`, the flags this kind sets. Counted from bit 1, so bit 13 is `1 << 12`.
    #[must_use]
    pub const fn flags(&self) -> i64 {
        match self {
            Self::Text { .. } | Self::CheckBox { .. } | Self::ListBox { .. } | Self::Signature => 0,
            // Table 228: Multiline, then Password.
            Self::TextArea { .. } => 1 << 12,
            Self::Password => 1 << 13,
            // Table 230: Radio, then Pushbutton.
            Self::RadioButton { .. } => 1 << 15,
            Self::PushButton { .. } => 1 << 16,
            // Table 231: Combo.
            Self::ComboBox { .. } => 1 << 17,
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

/// A piece of text an OCR engine read, and where.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TextLayerItem {
    /// What it says, on one line.
    pub text: String,
    /// The box it was read from, in default user space: left, bottom, right, top. The text
    /// is set to its height and stretched to its width.
    pub rect: [f64; 4],
}

/// What a redaction removes: rectangles on one page.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Redaction {
    /// The page, from 0.
    pub page: usize,
    /// The regions, in the page's default user space: left, bottom, right, top.
    pub regions: Vec<(f64, f64, f64, f64)>,
    /// What the regions are filled with, as a `/Redact` annotation's `/IC` says it
    /// (Table 195): no components for no fill, one for gray, three for RGB, four for
    /// CMYK, each from 0 to 1.
    ///
    /// **Absent, they are filled black**, and that is recorded as a `Decision`: nothing
    /// the caller said chose it, so this engine did.
    #[serde(default)]
    pub fill: Option<Vec<f64>>,
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

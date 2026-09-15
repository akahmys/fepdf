//! ISO 32000-2 Extended Domain Models
//!
//! Strongly-typed domain value objects for Portfolio (/Collection),
//! Outlines (/Outlines), Optional Content / Layers (/OCProperties),
//! Associated Files (/AF), Output Intents (/OutputIntents),
//! Measurement Scale (/Measure), Annotations (/Annots), AcroForms,
//! Page Labels (/PageLabels), Article Threads (/Threads), User Properties (/UserProperties),
//! Actions & Transitions, GIS Anchors (/Geo), Mesh Shading, and Encryption Wrappers.

use serde::{Deserialize, Serialize};

/// Layout view mode for PDF Portfolios (ISO 32000-2 Section 12.3.5)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CollectionViewMode {
    /// Show detailed list.
    #[default]
    Details,
    /// Show icon tiles.
    Tile,
    /// Hide UI controls.
    Hidden,
}

/// PDF Portfolio / Collection definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PortfolioCollection {
    /// View layout for the collection.
    pub view_mode: CollectionViewMode,
    /// Initial document to show when opened.
    pub initial_document: Option<String>,
    /// Items inside the portfolio.
    pub items: Vec<PortfolioItem>,
}

/// A single item contained within a PDF Portfolio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortfolioItem {
    /// Filename of the embedded document.
    pub filename: String,
    /// MIME type of the file.
    pub mime_type: Option<String>,
    /// Human-readable description.
    pub description: Option<String>,
    /// Total file size in bytes.
    pub size_bytes: u64,
    /// Binary content of the embedded file.
    pub data: Vec<u8>,
}

/// How what is drawn on a page is resized when the sheet under it changes (14.11.2).
///
/// **Separate from where it is placed.** These were one enum, and the four values it had
/// were four pairs: "keep the size, at the origin", "keep the size, centred", "scale to
/// fit, centred", "scale by a factor, centred". Two of the nine combinations were
/// reachable and the useful ones were not — a page scaled to fit but held against the
/// binding edge, or shrunk to leave a margin on one side only.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ContentScale {
    /// Left at the size it was drawn.
    Keep,
    /// Scaled so the whole of it lands on the sheet, leaving a margin on one axis.
    ///
    /// The smaller of the two ratios, which is what "all of it is on the sheet" comes to.
    Fit,
    /// Scaled so the sheet is covered, with what does not fit hanging over the edges.
    ///
    /// The larger of the two ratios. What hangs over is not displayed — `/CropBox`
    /// becomes the sheet — and stays in the file, which is the same bargain any offset
    /// past the edge makes.
    ///
    /// **Both of these keep the aspect, and there is no third that does not.** Scaling
    /// each axis by its own ratio fills the sheet exactly and leaves nothing on it the
    /// shape it was drawn as: a page of text with the letters stretched is a fault
    /// someone has to notice, and no one asks a document for it on purpose. If that ever
    /// turns out to be wanted it is a fourth value here, not a flag on these two.
    Fill,
    /// Scaled by a factor of the caller's choosing.
    ///
    /// With the sheet left alone this is the other thing "scale" means: the drawing
    /// shrinks and the margins grow, which is what printing inside a bound edge asks for.
    By(f64),
}

/// Where the content sits on the sheet along one axis.
///
/// **Not a field of [`PageResize`] — a way of working out one.** It was a field, beside
/// the offset, and the two said the same thing twice: an offset already spans every
/// position, and "centred" is `(0, 0)`. What alignment is good for is *naming* the
/// position a person wants, so [`PageResize::offset_to`] turns a name into the number the
/// offset field then shows. Press "left" and the number appears; change it afterwards and
/// it is yours.
///
/// The cost of a name that becomes a number: change the scale afterwards and the number
/// no longer touches the edge. That is the trade for one axis instead of two, and for a
/// binding margin reading as `x + 42` rather than as "centred, and also shifted".
///
/// `Start` is the side the origin is on — the left, and the bottom — because that is
/// where a PDF measures from (8.3.2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Align {
    /// Against the origin: left, or bottom.
    Start,
    /// In the middle.
    #[default]
    Middle,
    /// Against the far edge: right, or top.
    End,
}

impl Align {
    /// How far from the middle a run of `content` sits within `sheet`.
    ///
    /// Measured from the middle because that is where an offset of zero puts it.
    #[must_use]
    pub fn from_middle(self, content: f64, sheet: f64) -> f64 {
        let room = (sheet - content) / 2.0;
        match self {
            Self::Start => -room,
            Self::Middle => 0.0,
            Self::End => room,
        }
    }
}

/// A sheet to put pages on, how what is drawn there is resized, and where it lands
/// (14.11.2).
///
/// One struct rather than four fields on the operation, which is the shape the vocabulary
/// already uses where an operation takes more than a couple of things —
/// `UpdateStructElem` and `MoveStructElem` each carry one.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PageResize {
    /// The sheet to put the pages on, or `None` to keep the one they are already on.
    ///
    /// **`None` is what makes "scale the content" a thing this can say.** Scaling a
    /// drawing inside the sheet it is already on had to be asked for by naming that same
    /// sheet again, which meant reading it off the page first and getting it wrong for a
    /// document whose pages are not all one size.
    pub sheet: Option<(f64, f64)>,
    /// How what is drawn there is resized.
    pub scale: ContentScale,
    /// Where it sits, as a distance from the middle of the sheet in points: right, then
    /// up.
    ///
    /// **Zero is centred, which is why there is no separate placement.** A binding margin
    /// is `(42.0, 0.0)` and reads as one; an edge is a number [`Self::offset_to`] works
    /// out for a form to put here.
    pub offset: (f64, f64),
}

impl PageResize {
    /// The sheets a person names, in points, from the two standards that name them.
    ///
    /// **Here rather than in a frontend.** A frontend that typed these numbers out would
    /// be the second place they were written, and the third would disagree with it:
    /// `fepdf-cli` and `fepdf-mcp` want the same list, by the same names. The names are
    /// identifiers — `A4` is `A4` in every language — so they carry no locale key.
    ///
    /// ISO 216's A and B series are defined in millimetres and rounded to whole points
    /// here, which is what a PDF `/MediaBox` is written in and what every producer in
    /// this corpus writes.
    pub const SHEETS: [(&'static str, (f64, f64)); 8] = [
        ("A3", (842.0, 1191.0)),
        ("A4", (595.0, 842.0)),
        ("A5", (420.0, 595.0)),
        ("B4", (709.0, 1001.0)),
        ("B5", (499.0, 709.0)),
        ("Letter", (612.0, 792.0)),
        ("Legal", (612.0, 1008.0)),
        ("Tabloid", (792.0, 1224.0)),
    ];

    /// The sheet `name` stands for, if it is one of [`Self::SHEETS`].
    #[must_use]
    pub fn sheet(name: &str) -> Option<(f64, f64)> {
        Self::SHEETS.iter().find(|(known, _)| *known == name).map(|(_, size)| *size)
    }

    /// The same sheet turned on its side.
    #[must_use]
    pub const fn landscape(size: (f64, f64)) -> (f64, f64) {
        (size.1, size.0)
    }

    /// How far `from`, scaled and offset, hangs over each edge of `to`: left, bottom,
    /// right, top, in points. Zero where it does not.
    ///
    /// **Arithmetic, not interpretation.** Whether a particular mark falls outside needs
    /// the page read; whether the *page* does is the box against the sheet, which a form
    /// can ask on every frame while a number is being dragged.
    ///
    /// What hangs over is not shown — `/CropBox` becomes the sheet, and 14.11.2 makes
    /// that the region a viewer displays — and it is still in the file. Those are
    /// different things and only the first is visible, which is why this exists to be
    /// said out loud.
    #[must_use]
    pub fn overhang(&self, from: (f64, f64), to: (f64, f64), scale: f64) -> [f64; 4] {
        let content = (from.0 * scale, from.1 * scale);
        let centred = ((to.0 - content.0) / 2.0, (to.1 - content.1) / 2.0);
        let at = (centred.0 + self.offset.0, centred.1 + self.offset.1);
        [
            (-at.0).max(0.0),
            (-at.1).max(0.0),
            (at.0 + content.0 - to.0).max(0.0),
            (at.1 + content.1 - to.1).max(0.0),
        ]
    }

    /// The offset that puts `from`, scaled by `scale`, where `place` names on `to`.
    ///
    /// **So that a form offers the named positions without doing geometry.** A panel that
    /// worked out "flush left" itself would be the second place this arithmetic lives, and
    /// the third would disagree.
    #[must_use]
    pub fn offset_to(
        place: (Align, Align),
        from: (f64, f64),
        to: (f64, f64),
        scale: f64,
    ) -> (f64, f64) {
        (place.0.from_middle(from.0 * scale, to.0), place.1.from_middle(from.1 * scale, to.1))
    }
}

/// Bookmark / Outline tree item (ISO 32000-2 Section 12.3.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutlineNode {
    /// Display title for the bookmark item.
    pub title: String,
    /// Destination page index (0-indexed).
    pub destination_page: usize,
    /// Child bookmarks.
    pub children: Vec<OutlineNode>,
}

/// Releases a nested outline with a worklist, not with the stack.
///
/// **The derived `Drop` aborted the process past about 5,000 levels.** Releasing a node
/// released its `children`, which released theirs, one stack frame per level all the way
/// down, and [ADR-0061](../../../../docs/adr/0061-four-walks-bounded-and-two-that-were-not-what-the-sweep-said.md)
/// measured the abort between 5,000 and 10,000 while the walk that *builds* one is
/// bounded well above it. No bound on any walk moves this: it is the type's own
/// destructor, and every caller that builds an outline in Rust has it.
///
/// Each node's children are moved out before that node dies, so the node the compiler
/// drops always has an empty `Vec` and recurses nowhere. This runs on every `OutlineNode`,
/// including the shallow ones, and costs one `Vec` per level of actual nesting.
impl Drop for OutlineNode {
    fn drop(&mut self) {
        let mut pending = std::mem::take(&mut self.children);
        while let Some(mut node) = pending.pop() {
            pending.append(&mut node.children);
        }
    }
}

/// Full Outline Tree for document navigation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OutlineTree {
    /// Root bookmark nodes.
    pub items: Vec<OutlineNode>,
}

/// Visibility state of an Optional Content Group (Layer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VisibilityState {
    /// Layer is visible by default.
    #[default]
    On,
    /// Layer is hidden by default.
    Off,
}

/// A single Layer / Optional Content Group (ISO 32000-2 Section 8.11).
///
/// **`name` is the identity.** This carried an `id` beside it, for as long as nothing
/// wrote either one anywhere a reader could find: 8.11's Table 96 gives a group `/Name`
/// and nothing else that identifies it, so a second identifier had no slot in the file to
/// go to and no operation could have referred to a layer by it. `Operation::AddPageDecoration`
/// names the layer it puts content in by `name`, which is also what a viewer's layer
/// panel shows. Two layers sharing a name are the caller's ambiguity to avoid; the first
/// one wins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerGroup {
    /// User-facing display name, and what an operation names this layer by.
    pub name: String,
    /// Default visibility state.
    pub default_state: VisibilityState,
    /// Whether the layer should be printed. Reaches the file as a `/Usage` `/Print`
    /// state with a `/AS` entry that applies it (8.11.4.5) — without the second the
    /// first is a description nothing acts on.
    pub printable: bool,
}

/// Optional Content Properties container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OptionalContentProperties {
    /// Defined layers in the document.
    pub layers: Vec<LayerGroup>,
}

/// Semantic relationship of Associated Files (ISO 32000-2 Section 14.13).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AFRelationship {
    /// Original source document (e.g. CAD or Word source).
    Source,
    /// Structured data (e.g. CSV or XML data).
    Data,
    /// Supplemental information.
    Supplement,
    /// Alternative representation.
    Alternative,
    /// Unspecified relationship.
    #[default]
    Unspecified,
}

/// Associated File specification (`/AF`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssociatedFile {
    /// Filename of the associated file.
    pub filename: String,
    /// Semantic relationship to parent object.
    pub relationship: AFRelationship,
    /// MIME type.
    pub mime_type: String,
    /// Binary content of the associated file.
    pub data: Vec<u8>,
}

/// Output Intent definition for color management (ISO 32000-2 Section 14.11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputIntent {
    /// Output intent subtype.
    pub subtype: String,
    /// Target output profile identifier.
    pub identifier: String,
    /// Additional info string.
    pub info: Option<String>,
    /// ICC profile bytes if embedded.
    pub icc_profile_bytes: Option<Vec<u8>>,
}

/// Unit scale measurement for CAD / Geospatial drawings (ISO 32000-2 Section 13.5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasurementScale {
    /// Target page index (0-indexed).
    pub page: usize,
    /// Scale ratio factor.
    pub scale_ratio: f32,
    /// Label for unit (e.g. "mm", "m", "in").
    pub unit_label: String,
}

/// Annotations specification (ISO 32000-2 Section 12.5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AnnotationKind {
    /// Hyperlink annotation.
    Link {
        /// Target page index.
        destination_page: usize,
        /// Optional external URL.
        url: Option<String>,
    },
    /// Text highlight annotation.
    Highlight {
        /// RGB color floats.
        color_rgb: [f32; 3],
    },
    /// Sticky note text comment.
    TextComment {
        /// Text content of comment.
        contents: String,
    },
    /// Rubber stamp annotation.
    Stamp {
        /// Image bytes of the stamp.
        stamp_image_bytes: Vec<u8>,
    },
}

/// Annotation instance on a page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnnotationSpec {
    /// Target page index (0-indexed).
    pub page: usize,
    /// Bounding rectangle `[x1, y1, x2, y2]`.
    pub rect: [f32; 4],
    /// Type and payload of annotation.
    pub kind: AnnotationKind,
}

/// Form Field Value representation (AcroForms).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormValue {
    /// Text string value.
    Text(String),
    /// Single choice selection value.
    Choice(String),
    /// Boolean checkbox value.
    Boolean(bool),
}

/// Form Field specification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormFieldSpec {
    /// Fully qualified field name.
    pub name: String,
    /// Field value.
    pub value: FormValue,
}

// --- Phase 5: Navigation, Structure & Action Engine Domain Models ---

/// Numbering style for Page Labels (ISO 32000-2 Section 12.4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PageLabelStyle {
    /// Standard decimal Arabic numerals (1, 2, 3...).
    #[default]
    Decimal,
    /// Uppercase Roman numerals (I, II, III...).
    UpperRoman,
    /// Lowercase Roman numerals (i, ii, iii...).
    LowerRoman,
    /// Uppercase Alphabetic (A, B, C...).
    UpperAlpha,
    /// Lowercase Alphabetic (a, b, c...).
    LowerAlpha,
}

/// Page Label specification (`/PageLabels`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageLabelSpec {
    /// 0-indexed page range start.
    pub start_page: usize,
    /// Numbering style.
    pub style: PageLabelStyle,
    /// Optional prefix string (e.g. "Appendix-").
    pub prefix: Option<String>,
    /// Starting number (defaults to 1).
    pub start_number: u32,
}

/// A bead in an Article Thread (ISO 32000-2 Section 12.4.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArticleBead {
    /// Target page index.
    pub page: usize,
    /// Bounding rectangle `[x1, y1, x2, y2]` of the article bead.
    pub rect: [f32; 4],
}

/// An Article Thread (`/Threads`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArticleThread {
    /// Title of the article.
    pub title: String,
    /// Ordered list of article beads across pages.
    pub beads: Vec<ArticleBead>,
}

/// Value of a User Property (ISO 32000-2 Section 14.6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UserPropertyValue {
    /// Text value.
    Text(String),
    /// Numeric value.
    Number(f64),
    /// Boolean value.
    Boolean(bool),
}

/// User Property attribute on a Tagged PDF node (`/UserProperties`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserProperty {
    /// Property key name.
    pub name: String,
    /// Property value.
    pub value: UserPropertyValue,
    /// Optional formatted value string.
    pub formatted: Option<String>,
}

/// Visual Transition Style between pages (ISO 32000-2 Section 12.4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TransitionStyle {
    /// Split transition.
    #[default]
    Split,
    /// Blinds transition.
    Blinds,
    /// Box transition.
    Box,
    /// Wipe transition.
    Wipe,
    /// Dissolve transition.
    Dissolve,
    /// Glitter transition.
    Glitter,
    /// Fly transition.
    Fly,
}

/// Page Transition specification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransitionSpec {
    /// Transition visual style.
    pub style: TransitionStyle,
    /// Duration in seconds.
    pub duration_seconds: f32,
}

/// Action Types (ISO 32000-2 Section 12.6.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PdfAction {
    /// Remote PDF jump (`GoToR`).
    GoToRemote {
        /// Target file path.
        file_path: String,
        /// Target page index in remote document.
        page: usize,
    },
    /// Embedded file jump (`GoToE`).
    GoToEmbedded {
        /// Target embedded file name in portfolio.
        embedded_name: String,
        /// Target page index.
        page: usize,
    },
    /// Named standard action (`Named`).
    Named(String),
    /// Trigger transition.
    Transition(TransitionSpec),
}

// --- Phase 6: Advanced Graphics & GIS Domain Models ---

/// GIS Geographic Anchor definition (`/Geo` ISO 32000-2 Section 13.10).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeoSpatialAnchor {
    /// Target page index.
    pub page: usize,
    /// Latitude degrees.
    pub latitude: f64,
    /// Longitude degrees.
    pub longitude: f64,
    /// Altitude meters if specified.
    pub altitude_meters: Option<f64>,
    /// Well-Known Text (WKT) Coordinate Reference System.
    pub crs_wkt: String,
}

/// Mesh Shading Type (Type 4 to 7 Shading ISO 32000-2 Section 8.7.4.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MeshShadingType {
    /// Free-form triangle mesh.
    #[default]
    FreeFormTriangleMesh = 4,
    /// Lattice-form triangle mesh.
    LatticeFormTriangleMesh = 5,
    /// Coons patch mesh.
    CoonsPatchMesh = 6,
    /// Tensor-product patch mesh.
    TensorProductPatchMesh = 7,
}

/// Mesh Shading specification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeshShadingSpec {
    /// Shading type (4 to 7).
    pub shading_type: MeshShadingType,
    /// Color space name.
    pub color_space: String,
    /// Raw shading stream data bytes.
    pub data_bytes: Vec<u8>,
}

// --- Phase 7: Font Engine & Cryptography Domain Models ---

/// Unencrypted Wrapper Payload specification (ISO 32000-2 Section 7.6.7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnencryptedWrapperSpec {
    /// Visible guide message for legacy readers.
    pub notice_message: String,
    /// Encrypted payload stream bytes.
    pub encrypted_payload_bytes: Vec<u8>,
}

/// Public Key Recipient Certificate specification (ISO 32000-2 Section 7.6.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicKeyRecipientSpec {
    /// DER-encoded X.509 certificate of the recipient.
    pub certificate_der_bytes: Vec<u8>,
    /// Encrypted file key bytes for this recipient.
    pub encrypted_key_bytes: Vec<u8>,
}

#[cfg(test)]
mod sheet_tests {
    use super::PageResize;

    /// The A series halves along its long edge, which is what defines it (ISO 216).
    ///
    /// Rounding to whole points costs at most a point, so the check is that each sheet is
    /// the next one's long edge within one — not that the arithmetic is exact.
    #[test]
    fn each_a_sheet_is_half_the_one_before_it() {
        let a3 = PageResize::sheet("A3").expect("A3");
        let a4 = PageResize::sheet("A4").expect("A4");
        let a5 = PageResize::sheet("A5").expect("A5");
        assert!((a3.1 / 2.0 - a4.0).abs() <= 1.0, "A3 halved is {} and A4 is {}", a3.1 / 2.0, a4.0);
        assert!((a4.1 / 2.0 - a5.0).abs() <= 1.0, "A4 halved is {} and A5 is {}", a4.1 / 2.0, a5.0);
    }

    /// Every sheet is taller than it is wide, so `landscape` means something.
    #[test]
    fn the_sheets_are_named_portrait() {
        for (name, size) in PageResize::SHEETS {
            assert!(size.1 > size.0, "{name} is listed on its side: {size:?}");
            assert_eq!(PageResize::landscape(size), (size.1, size.0));
        }
    }

    /// A name nobody declared answers nothing, rather than a default sheet.
    #[test]
    fn an_unknown_name_is_not_guessed_at() {
        assert!(PageResize::sheet("A9").is_none());
        assert!(PageResize::sheet("a4").is_none(), "the names are identifiers, not prose");
    }
}

#[cfg(test)]
mod overhang_tests {
    use super::{ContentScale, PageResize};

    const A4: (f64, f64) = (595.0, 842.0);

    fn at(offset: (f64, f64)) -> PageResize {
        PageResize { sheet: Some(A4), scale: ContentScale::Keep, offset }
    }

    /// Centred on the sheet it was drawn for, nothing hangs over.
    #[test]
    fn a_centred_page_on_its_own_sheet_hangs_over_nothing() {
        assert_eq!(at((0.0, 0.0)).overhang(A4, A4, 1.0), [0.0; 4]);
    }

    /// Moved right by 30, thirty points hang over the right edge and none over the left.
    #[test]
    fn an_offset_hangs_over_the_edge_it_is_moved_towards() {
        let over = at((30.0, 0.0)).overhang(A4, A4, 1.0);
        assert!((over[2] - 30.0).abs() < 1e-9, "right: {over:?}");
        assert_eq!((over[0], over[1], over[3]), (0.0, 0.0, 0.0), "{over:?}");
    }

    /// Room on the sheet absorbs an offset until there is none left.
    #[test]
    fn room_on_the_sheet_is_used_before_anything_hangs_over() {
        let bigger = (A4.0 + 100.0, A4.1);
        // Fifty points of room on each side, so a shift of forty stays on the sheet.
        assert_eq!(at((40.0, 0.0)).overhang(A4, bigger, 1.0), [0.0; 4]);
        let over = at((60.0, 0.0)).overhang(A4, bigger, 1.0);
        assert!((over[2] - 10.0).abs() < 1e-9, "ten points should hang over: {over:?}");
    }

    /// Scaling the content down makes room, which is what a scale is for.
    #[test]
    fn a_smaller_drawing_hangs_over_less() {
        let over =
            PageResize { sheet: Some(A4), scale: ContentScale::By(0.5), offset: (30.0, 0.0) }
                .overhang(A4, A4, 0.5);
        assert_eq!(over, [0.0; 4], "half an A4 shifted by 30 is still on the sheet: {over:?}");
    }
}

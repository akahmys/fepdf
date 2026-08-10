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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerGroup {
    /// Unique identifier for the layer.
    pub id: String,
    /// User-facing display name.
    pub name: String,
    /// Default visibility state.
    pub default_state: VisibilityState,
    /// Whether layer is printable.
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

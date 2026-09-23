/// ISO 32000-2 Extended domain models.
/// What the catalogue's entries hold (Table 29), read.
pub mod entries;
pub mod extensions;
/// Pages and the page tree.
pub mod page;
pub mod structure;

use self::page::Page;
use crate::color::ResolvedColorSpace;
use crate::error::PdfError;
use crate::font::{FallbackFontType, FontResource};
use crate::handle::DictHandle;
use crate::{FromPdfObject, Handle, Object, PdfArena, PdfName, PdfResult};
use parking_lot::RwLock;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// What the source document was, kept so the output can say what it derives from.
///
/// Saving produces a new document (ADR-0012): normalisation at load means the arena
/// already differs from the file, the revision chain is merged to a single newest
/// state, and no code path writes a faithful copy. The output is therefore a derived
/// work, and this is what it derives *from*.
#[derive(Debug, Clone, Default)]
pub struct Provenance {
    /// The source's `xmpMM:DocumentID`, or its trailer `/ID[0]` when it carried no XMP.
    /// This is the immediate parent, and becomes `xmpMM:DerivedFrom`.
    pub source_id: Option<String>,
    /// The root of the derivation chain: the source's own `xmpMM:OriginalDocumentID`
    /// if it had one, and otherwise its `DocumentID`, because then *it* is the root.
    ///
    /// Kept separately from `source_id` because the two diverge the moment a document
    /// is saved twice. Writing the parent into both makes the second save forget where
    /// the chain began, which is the one thing `OriginalDocumentID` is for.
    pub original_id: Option<String>,
    /// Signature dictionaries the source carried (12.8). They cannot survive: a
    /// signature covers a byte range, and these are not those bytes.
    pub signatures: usize,
}

/// Refined PDF Catalog (Root) Dictionary (ISO 32000-2:2020 Clause 7.7.2)
///
/// `Serialize` so that what the engine *read* can be compared, not just which keys
/// survived. `crosscheck_selfread.sh` compared the catalogue key for key and by the
/// shape of each value, which is all it could do while 26 of the 32 entries were
/// `Option<Object>`; now that they are read, a save that preserved `/MarkInfo` as a
/// dictionary while losing `/Marked` inside it is a difference something can see.
#[derive(Debug, Clone, FromPdfObject, serde::Serialize)]
#[pdf_dict(clause = "7.7.2")]
pub struct PdfCatalog {
    #[pdf_key("Pages")]
    /// `/Pages`: root of the page tree (7.7.3.2).
    ///
    /// What the root *declares* — the page count it claims, and the attributes its pages
    /// inherit. The pages themselves are `Document::pages`, with inheritance already
    /// resolved into each one (ADR-0013).
    pub pages: Option<entries::Located<entries::PageTreeRoot>>,
    #[pdf_key("StructTreeRoot")]
    /// `/StructTreeRoot`: root of the logical structure tree (14.7.4.2).
    pub struct_tree_root: Option<entries::Located<entries::StructTreeRoot>>,
    #[pdf_key("MarkInfo")]
    /// `/MarkInfo`: whether the document is tagged (14.7.1).
    pub mark_info: Option<entries::MarkInfo>,
    #[pdf_key("Metadata")]
    /// `/Metadata`: the XMP metadata stream (14.3.2), decoded and read.
    pub metadata: Option<entries::XmpMetadata>,
    #[pdf_key("Version")]
    /// `/Version`: a version overriding the file header (7.7.2). The later of the two
    /// wins, so a reader that ignores it reads a 2.0 file as whatever the header says.
    pub version: Option<entries::DeclaredVersion>,
    #[pdf_key("AcroForm")]
    /// `/AcroForm`: the interactive form's own settings (12.7.2). The fields are walked
    /// by [`crate::interactive::FormFields`], which needs the whole document.
    pub acro_form: Option<entries::AcroForm>,
    #[pdf_key("Names")]
    /// `/Names`: which of Table 31's name trees the document declares, and how many
    /// names each holds (7.7.4).
    pub names: Option<entries::NameDictionary>,
    #[pdf_key("Outlines")]
    /// `/Outlines`: what the root of the bookmark tree declares (12.3.3). The tree is
    /// walked by [`crate::interactive::Outline`], which compares `/Count` with it.
    pub outlines: Option<entries::OutlineRoot>,
    #[pdf_key("OpenAction")]
    /// `/OpenAction`: what happens when the document opens (12.6) — a destination
    /// written in place, or an action. Both forms occur in the corpus.
    pub open_action: Option<entries::TriggeredAction>,
    #[pdf_key("AA")]
    /// `/AA`: actions triggered by document events (12.6.3, Table 197).
    pub additional_actions: Option<entries::AdditionalActions>,
    #[pdf_key("PageMode")]
    /// `/PageMode`: what a viewer shows beside the page when the document opens.
    pub page_mode: Option<PageMode>,
    #[pdf_key("PageLayout")]
    /// `/PageLayout`: how a viewer arranges the pages.
    pub page_layout: Option<PageLayout>,
    #[pdf_key("Dests")]
    /// `/Dests`: named destinations, keyed by name (12.3.2.3, PDF 1.1).
    ///
    /// The 1.2 form lives under `/Names` instead, and `NamedDestinations` reads both —
    /// see [`crate::destination`] for why typing only this one would have covered 651
    /// destinations out of 279,501.
    pub dests: Option<crate::destination::DestsDictionary>,
    #[pdf_key("ViewerPreferences")]
    /// `/ViewerPreferences`: how the document asks to be presented (12.2).
    pub viewer_preferences: Option<ViewerPreferences>,
    #[pdf_key("Lang")]
    /// `/Lang`: the natural language of the document's text (14.9.2.1), as a BCP 47 tag.
    pub lang: Option<String>,
    #[pdf_key("Type")]
    /// `/Type`: the type of PDF object this dictionary describes; must be `Catalog` (7.7.2).
    pub catalog_type: Option<Handle<PdfName>>,
    #[pdf_key("PageLabels")]
    /// `/PageLabels`: what a viewer shows instead of a page index (12.4.2), as the
    /// ranges the number tree holds.
    pub page_labels: Option<entries::PageLabels>,
    #[pdf_key("Threads")]
    /// `/Threads`: the articles the document defines (12.4.3).
    pub threads: Option<entries::ArticleThreads>,
    #[pdf_key("OutputIntents")]
    /// `/OutputIntents`: what the file was prepared to be printed on (14.11.5), and
    /// which standard each intent claims.
    pub output_intents: Option<entries::OutputIntents>,
    #[pdf_key("OCProperties")]
    /// `/OCProperties`: the optional content the document defines (8.11.4.3).
    pub oc_properties: Option<entries::OptionalContent>,
    #[pdf_key("Collection")]
    /// `/Collection`: collection dictionary for document portfolios (12.3.5).
    pub collection: Option<Object>,
    #[pdf_key("AF")]
    /// `/AF`: the files this document carries (14.13), read since a corpus presented
    /// seventeen of them.
    pub associated_files: Option<entries::AssociatedFiles>,
    #[pdf_key("Extensions")]
    /// `/Extensions`: developer extensions dictionary (7.12).
    pub extensions: Option<Object>,
    #[pdf_key("URI")]
    /// `/URI`: document-level URI dictionary (12.6.4.7).
    pub uri: Option<Object>,
    #[pdf_key("SpiderInfo")]
    /// `/SpiderInfo`: Web Capture information dictionary (14.10.2).
    pub spider_info: Option<Object>,
    #[pdf_key("PieceInfo")]
    /// `/PieceInfo`: private data left by the applications that touched the document
    /// (14.5), keyed by the name each calls itself.
    pub piece_info: Option<entries::PieceInfo>,
    #[pdf_key("Perms")]
    /// `/Perms`: permissions dictionary (12.8.4).
    pub perms: Option<Object>,
    #[pdf_key("Legal")]
    /// `/Legal`: legal attestation dictionary (12.8.5).
    pub legal: Option<Object>,
    #[pdf_key("Requirements")]
    /// `/Requirements`: what the document says a processor must support to handle it
    /// (12.11). Read because this engine declines a subset the standard lets it decline,
    /// and this is how a document asks for that subset.
    pub requirements: Option<entries::DocumentRequirements>,
    #[pdf_key("NeedsRendering")]
    /// `/NeedsRendering`: flag indicating whether appearance streams must be generated (12.7.2).
    pub needs_rendering: Option<bool>,
    #[pdf_key("DSS")]
    /// `/DSS`: Document Security Store dictionary (12.8.4.3).
    pub dss: Option<Object>,
    #[pdf_key("DPartRoot")]
    /// `/DPartRoot`: Document Part hierarchy root dictionary (14.12).
    pub dpart_root: Option<Object>,
}

/// `/ViewerPreferences` (12.2, Table 147): how the document asks to be presented.
///
/// The most common untyped catalogue entry — six of the nine samples carry one — and
/// until now the only thing reading it was `PdfDocument::viewer_direction`, which walked
/// the raw dictionary looking for one key. That is the "any handling is ad hoc" the
/// roadmap means by untyped.
///
/// **Every field is an `Option`, including the booleans that Table 147 gives defaults
/// for.** A document that says nothing must not come back claiming `false`: "unstated"
/// and "explicitly off" are different facts, the first belongs to the viewer's policy
/// and the second to the document, and a report that conflates them cannot say what the
/// file declares. The corpus makes the point — `fy05.pdf` carries an *empty*
/// `/ViewerPreferences`, which under defaulting would read identically to four
/// deliberate `false`s.
///
/// Only two of these keys occur in the corpus at all: `DisplayDocTitle` in four files
/// and `Direction` in one. The rest are typed because Table 147 is one dictionary of
/// scalars rather than a subsystem — unlike `DSS`, `AF` and `DPartRoot`, which are
/// absent from the corpus *and* would each need machinery, which is why Phase D leaves
/// them for later.
#[derive(Debug, Clone, FromPdfObject, serde::Serialize)]
#[pdf_dict(clause = "12.2")]
pub struct ViewerPreferences {
    #[pdf_key("HideToolbar")]
    /// Hide the viewer's toolbars.
    pub hide_toolbar: Option<bool>,
    #[pdf_key("HideMenubar")]
    /// Hide the viewer's menu bar.
    pub hide_menubar: Option<bool>,
    #[pdf_key("HideWindowUI")]
    /// Hide scroll bars and other window furniture.
    pub hide_window_ui: Option<bool>,
    #[pdf_key("FitWindow")]
    /// Resize the window to the first page.
    pub fit_window: Option<bool>,
    #[pdf_key("CenterWindow")]
    /// Centre the window on the screen.
    pub center_window: Option<bool>,
    #[pdf_key("DisplayDocTitle")]
    /// Show `dc:title` in the title bar instead of the file name (1.4).
    ///
    /// The one entry the corpus actually exercises: four files set it, three true and
    /// one false. PDF/UA requires it true, which is why an accessibility-minded
    /// producer sets it.
    pub display_doc_title: Option<bool>,
    #[pdf_key("NonFullScreenPageMode")]
    /// What to show on leaving full-screen. Table 147 allows four of [`PageMode`]'s
    /// values — not `FullScreen`, which would be circular, and not `UseAttachments`.
    pub non_full_screen_page_mode: Option<PageMode>,
    #[pdf_key("Direction")]
    /// Reading order for spreads (1.3).
    pub direction: Option<Direction>,
    #[pdf_key("ViewArea")]
    /// Which page boundary to display. **Deprecated in PDF 2.0**, and typed anyway
    /// because files written before it exist and this engine reads 1.7.
    pub view_area: Option<PageBoundary>,
    #[pdf_key("ViewClip")]
    /// Which page boundary to clip to when displaying. Deprecated in PDF 2.0.
    pub view_clip: Option<PageBoundary>,
    #[pdf_key("PrintArea")]
    /// Which page boundary to print. Deprecated in PDF 2.0.
    pub print_area: Option<PageBoundary>,
    #[pdf_key("PrintClip")]
    /// Which page boundary to clip to when printing. Deprecated in PDF 2.0.
    pub print_clip: Option<PageBoundary>,
    #[pdf_key("PrintScaling")]
    /// The print dialogue's default scaling (1.6).
    pub print_scaling: Option<PrintScaling>,
    #[pdf_key("Duplex")]
    /// The print dialogue's default duplex handling (1.7).
    pub duplex: Option<Duplex>,
    #[pdf_key("PickTrayByPDFSize")]
    /// Choose the paper tray by page size (1.7).
    pub pick_tray_by_pdf_size: Option<bool>,
    #[pdf_key("PrintPageRange")]
    /// Page ranges for the print dialogue, as pairs of first and last (1.7).
    ///
    /// The array is reached and its elements are resolvable; the pairs are not turned
    /// into a range type, because no corpus file carries this and a domain type nothing
    /// exercises is a container before its contents.
    pub print_page_range: Option<Handle<Vec<Object>>>,
    #[pdf_key("NumCopies")]
    /// The print dialogue's default copy count (1.7).
    pub num_copies: Option<i64>,
    #[pdf_key("Enforce")]
    /// Which preferences a viewer should enforce rather than offer (2.0).
    ///
    /// Reached as an array, for the same reason as `print_page_range`.
    pub enforce: Option<Handle<Vec<Object>>>,
}

/// `/Direction` (Table 147): reading order for two-page spreads.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Direction {
    /// Left to right.
    L2R,
    /// Right to left, which includes vertical Japanese written right to left.
    R2L,
    /// A name the standard does not define here, kept verbatim.
    Other(String),
}

impl Direction {
    /// The name this value was read from, so a report can print what the file said
    /// rather than a Rust identifier — including for `Other`, where the two differ.
    #[must_use]
    pub fn as_name(&self) -> &str {
        match self {
            Self::L2R => "L2R",
            Self::R2L => "R2L",
            Self::Other(name) => name,
        }
    }
}

/// A page boundary, as the deprecated `/ViewArea` family names one (14.11.2).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PageBoundary {
    /// `/MediaBox`.
    MediaBox,
    /// `/CropBox`.
    CropBox,
    /// `/BleedBox`.
    BleedBox,
    /// `/TrimBox`.
    TrimBox,
    /// `/ArtBox`.
    ArtBox,
    /// A name the standard does not define here, kept verbatim.
    Other(String),
}

/// `/PrintScaling` (Table 147).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PrintScaling {
    /// No scaling: one PDF unit to one device unit.
    None,
    /// Whatever the viewer would do anyway.
    AppDefault,
    /// A name the standard does not define here, kept verbatim.
    Other(String),
}

/// `/Duplex` (Table 147).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Duplex {
    /// One side only.
    Simplex,
    /// Two-sided, flipping about the short edge.
    DuplexFlipShortEdge,
    /// Two-sided, flipping about the long edge.
    DuplexFlipLongEdge,
    /// A name the standard does not define here, kept verbatim.
    Other(String),
}

impl crate::object::FromPdfObject for Direction {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        Ok(match named(&obj, arena)?.as_str() {
            "L2R" => Self::L2R,
            "R2L" => Self::R2L,
            other => Self::Other(other.to_string()),
        })
    }
}

impl crate::object::FromPdfObject for PageBoundary {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        Ok(match named(&obj, arena)?.as_str() {
            "MediaBox" => Self::MediaBox,
            "CropBox" => Self::CropBox,
            "BleedBox" => Self::BleedBox,
            "TrimBox" => Self::TrimBox,
            "ArtBox" => Self::ArtBox,
            other => Self::Other(other.to_string()),
        })
    }
}

impl crate::object::FromPdfObject for PrintScaling {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        Ok(match named(&obj, arena)?.as_str() {
            "None" => Self::None,
            "AppDefault" => Self::AppDefault,
            other => Self::Other(other.to_string()),
        })
    }
}

impl crate::object::FromPdfObject for Duplex {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        Ok(match named(&obj, arena)?.as_str() {
            "Simplex" => Self::Simplex,
            "DuplexFlipShortEdge" => Self::DuplexFlipShortEdge,
            "DuplexFlipLongEdge" => Self::DuplexFlipLongEdge,
            other => Self::Other(other.to_string()),
        })
    }
}

/// `/PageMode` (Table 29): what a viewer shows alongside the page.
///
/// `Other` keeps a name this list does not have rather than folding it to a default.
/// The set has grown twice — `UseOC` in 1.5, `UseAttachments` in 1.6 — so a file may
/// legitimately carry a value newer than this code, and mapping that to `UseNone` would
/// be inventing an answer. Nothing is lost, and a caller can see it was unrecognised.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PageMode {
    /// Neither outlines nor thumbnails.
    UseNone,
    /// The outline pane.
    UseOutlines,
    /// The thumbnail pane.
    UseThumbs,
    /// Full-screen, with no viewer chrome.
    FullScreen,
    /// The optional content group pane (1.5).
    UseOC,
    /// The attachments pane (1.6).
    UseAttachments,
    /// A name the standard does not define here, kept verbatim.
    Other(String),
}

/// `/PageLayout` (Table 29): how a viewer arranges the pages.
///
/// `Other` for the same reason as [`PageMode::Other`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PageLayout {
    /// One page at a time.
    SinglePage,
    /// One column, scrolling.
    OneColumn,
    /// Two columns, odd-numbered pages to the left.
    TwoColumnLeft,
    /// Two columns, odd-numbered pages to the right.
    TwoColumnRight,
    /// Two pages at a time, odd-numbered to the left (1.5).
    TwoPageLeft,
    /// Two pages at a time, odd-numbered to the right (1.5).
    TwoPageRight,
    /// A name the standard does not define here, kept verbatim.
    Other(String),
}

impl crate::object::FromPdfObject for PageMode {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        Ok(match named(&obj, arena)?.as_str() {
            "UseNone" => Self::UseNone,
            "UseOutlines" => Self::UseOutlines,
            "UseThumbs" => Self::UseThumbs,
            "FullScreen" => Self::FullScreen,
            "UseOC" => Self::UseOC,
            "UseAttachments" => Self::UseAttachments,
            other => Self::Other(other.to_string()),
        })
    }
}

impl crate::object::FromPdfObject for PageLayout {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        Ok(match named(&obj, arena)?.as_str() {
            "SinglePage" => Self::SinglePage,
            "OneColumn" => Self::OneColumn,
            "TwoColumnLeft" => Self::TwoColumnLeft,
            "TwoColumnRight" => Self::TwoColumnRight,
            "TwoPageLeft" => Self::TwoPageLeft,
            "TwoPageRight" => Self::TwoPageRight,
            other => Self::Other(other.to_string()),
        })
    }
}

/// The name an object is, for the two catalogue entries whose values are names.
fn named(obj: &Object, arena: &PdfArena) -> PdfResult<String> {
    obj.resolve(arena)
        .as_name()
        .and_then(|h| arena.get_name_str(h))
        .ok_or_else(|| crate::PdfError::Parse { pos: 0, message: "expected a name".into() })
}

type FontGroupMap = BTreeMap<(String, String), Vec<DictHandle>>;
type BestToUnicodeMap = BTreeMap<(String, String), Object>;

/// A refined PDF document.
pub struct Document {
    arena: PdfArena,
    root: Handle<Object>,
    info: Option<Handle<Object>>,
    /// Page handles in reading order.
    pub pages: Vec<Handle<Object>>,
    /// Non-fatal problems recorded during ingestion.
    /// What the engine decided where the input departed from the standard.
    pub decisions: crate::interpretation::DecisionLog,
    /// System font cache (shared across pages).
    pub system_fonts: Arc<BTreeMap<FallbackFontType, Arc<Vec<u8>>>>,
    /// Parsed FontResource cache to prevent redundant parsing across pages.
    pub font_cache: Arc<RwLock<BTreeMap<Handle<Object>, Arc<FontResource>>>>,
    /// Resolved `/ColorSpace` cache, for the same reason as `font_cache` and measured the
    /// same way.
    ///
    /// **Measured 2026-09-10**: `pattern_color_test` resolved 1,514 colour spaces for
    /// 2,488 colours and built 1,514 ICC transforms out of them — the same few resources,
    /// once per `cs` operator, on every page. A transform costs about 2.6ms to build, so
    /// 3.9s of a 15.3s test was work already done. Per document because the key is an
    /// arena handle and an arena belongs to one document; across pages because that is
    /// where the repetition is.
    pub space_cache: Arc<RwLock<BTreeMap<crate::color::SpaceKey, Option<Arc<ResolvedColorSpace>>>>>,
    /// Whether bundled fonts stand in for unparseable embedded programs.
    pub force_fallback: bool,
    /// Description of the encryption that was in force, if any.
    pub security_method: String,
    /// Permission flags recovered from the encryption dictionary.
    pub permissions: Option<i32>,
    /// Which password authenticated. `/P` restricts only [`Access::User`] (7.6.4.1).
    pub access: Option<fepdf_syntax::security::Access>,
    /// Whether a script processor above this engine runs this document's ECMAScript.
    ///
    /// **Session state, not a property of the file.** It says what this run does and is
    /// never written to the output. A frontend that will run the document's scripts
    /// declares it with [`Self::declare_script_processor`], and the one site that would
    /// otherwise record having skipped them reads it (12.6.3). `apply` cannot know: the
    /// script processor sits above the facade, so an `Operation` reaches this crate
    /// before anything can say whether the run will follow
    /// ([ADR-0032](../../../docs/adr/0032-running-scripts-is-a-frontend-verb-not-an-operation.md)).
    runs_scripts: std::sync::atomic::AtomicBool,
    /// What the source document was, for the output to record as its origin.
    pub provenance: Provenance,
    /// Optional-content groups a *viewer* has turned on or off, over what the
    /// configuration says (8.11.4.3).
    ///
    /// **Not part of the document.** `save` never writes it and no `Operation` produces
    /// it: 6.3.2.3 requires an interactive processor to let a person toggle a layer, and
    /// a person doing that is not editing the file. Behind a lock and reached through
    /// `&self` for the same reason [`Document::record`] is — the render path holds a
    /// shared reference and the panel sits above it.
    layer_overrides: parking_lot::Mutex<BTreeMap<Handle<Object>, bool>>,
}

impl Document {
    /// Says that this run executes the document's ECMAScript, so nothing reports skipping it.
    ///
    /// A frontend calls this before applying operations it will follow with a script run.
    /// Declaring it and then not running is worse than not declaring it: the engine stops
    /// warning about the staleness it would otherwise name.
    pub fn declare_script_processor(&self) {
        self.runs_scripts.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Whether [`Self::declare_script_processor`] was called on this document.
    #[must_use]
    pub fn runs_scripts(&self) -> bool {
        self.runs_scripts.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Records a decision taken about this document, through a shared reference.
    ///
    /// The interpreter holds `&Document` and departs from the standard while it runs —
    /// an image whose filter this engine cannot decode is skipped, and the page's text
    /// survives because of it. That departure belongs in the same log as the ones the
    /// reader takes, so that one question — "what did the engine decide about this
    /// file" — has one answer (`ARCHITECTURE.md` §4.3, ADR-0018).
    pub fn record(&self, decision: crate::interpretation::Decision) {
        self.decisions.push(decision);
    }

    /// Turns an optional-content group on or off for viewing, over the configuration.
    ///
    /// See [`Document::layer_overrides`]'s field note: this is view state, not a document
    /// change, so it takes `&self` and leaves the saved file alone.
    pub fn set_layer_visible(&self, group: Handle<Object>, on: bool) {
        self.layer_overrides.lock().insert(group, on);
    }

    /// Forgets every viewer override, returning to what the configuration says.
    pub fn reset_layer_visibility(&self) {
        self.layer_overrides.lock().clear();
    }

    /// The viewer overrides in force.
    #[must_use]
    pub fn layer_overrides(&self) -> BTreeMap<Handle<Object>, bool> {
        self.layer_overrides.lock().clone()
    }

    /// Creates a new document wrapper.
    pub fn new(arena: PdfArena, root: Handle<Object>, info: Option<Handle<Object>>) -> Self {
        Self {
            arena,
            root,
            info,
            pages: Vec::new(),
            decisions: crate::interpretation::DecisionLog::default(),
            system_fonts: Arc::new(BTreeMap::new()),
            font_cache: Arc::new(RwLock::new(BTreeMap::new())),
            space_cache: Arc::new(RwLock::new(BTreeMap::new())),
            force_fallback: false,
            security_method: "No Security".to_string(),
            permissions: None,
            access: None,
            provenance: Provenance::default(),
            runs_scripts: std::sync::atomic::AtomicBool::new(false),
            layer_overrides: parking_lot::Mutex::new(BTreeMap::new()),
        }
    }

    /// Walks `/Pages` and records what it finds, which is what makes this document
    /// answer questions about its pages.
    ///
    /// **A document built by hand has an empty page index.** [`Self::new`] leaves it
    /// empty and nothing fills it, so `page_count` answers 0, `get_page` fails for every
    /// index and `get_page_handle` answers `None` — while the writer, which walks the
    /// catalogue itself, writes every page correctly. `PdfDocument::extract_pages` and
    /// `PdfDocument::merge` both built documents that way: the file each produced was
    /// right and the object each returned said it held nothing.
    ///
    /// Ingestion calls this too, after the page tree has been normalised. It is separate
    /// from [`Self::new`] because at that point in ingestion the tree is not yet in the
    /// shape this walk expects.
    pub fn index_pages(&mut self) {
        self.pages = self.find_all_pages();
    }

    /// Creates a new document wrapper with issues.
    pub fn with_issues(
        arena: PdfArena,
        root: Handle<Object>,
        info: Option<Handle<Object>>,
        issues: Vec<crate::interpretation::Decision>,
    ) -> Self {
        Self {
            arena,
            root,
            info,
            pages: Vec::new(),
            decisions: crate::interpretation::DecisionLog::from(issues),
            system_fonts: Arc::new(BTreeMap::new()),
            font_cache: Arc::new(RwLock::new(BTreeMap::new())),
            space_cache: Arc::new(RwLock::new(BTreeMap::new())),
            force_fallback: false,
            security_method: "No Security".to_string(),
            permissions: None,
            access: None,
            provenance: Provenance::default(),
            runs_scripts: std::sync::atomic::AtomicBool::new(false),
            layer_overrides: parking_lot::Mutex::new(BTreeMap::new()),
        }
    }

    /// What is lost by writing this document out, when its `/P` said not to.
    ///
    /// `/P` is a declaration, not a lock: it is readable without a password, it is not
    /// cryptographically bound to any operation, and 7.6.4.1 puts obeying it at
    /// `should` rather than `shall`. So this refuses nothing.
    ///
    /// What it does refuse to do is stay quiet, about two losses rather than one.
    ///
    /// The content changes because this engine normalises at load (`ARCHITECTURE.md`
    /// §4.4): by the time a `Document` exists it already differs from the file, and no
    /// code path writes a faithful copy — `samples/fy05.pdf` differs in 378 of 4,574
    /// objects even with refinement turned off. So bit 4 is not something the engine
    /// declines to honour; it is something the architecture cannot honour. The wording
    /// says that rather than "wrote it anyway", which would imply a choice.
    ///
    /// The declaration goes too. A trailer still claiming `/Encrypt` over plain objects
    /// makes Acrobat report error 135, so `/Encrypt` is dropped and `/P` with it. Until
    /// this, the engine took a document reading "do not modify, do not reassemble",
    /// rewrote it, and produced one declaring nothing at all — in silence.
    ///
    /// Only under [`Access::User`]. An owner password carries full access (7.6.4.1),
    /// including the right to change the permissions, so there is nothing to report.
    #[must_use]
    pub fn permissions_lost_on_write(&self) -> Option<crate::interpretation::Decision> {
        use fepdf_syntax::security::Access;
        if self.access != Some(Access::User) {
            return None;
        }
        let bits = self.permissions?;
        let denied: Vec<&str> = [(4, "modification"), (11, "assembly"), (6, "annotation")]
            .iter()
            .filter(|(bit, _)| bits & (1 << (bit - 1)) == 0)
            .map(|(_, name)| *name)
            .collect();
        if denied.is_empty() {
            return None;
        }
        Some(crate::interpretation::Decision::violation(
            "7.6.4.2",
            format!(
                "the document was opened with user access and its /P ({bits}) permits no {}",
                denied.join(" and no ")
            ),
            "this engine normalises at load and has no path that writes a faithful copy, \
             so the output is modified; /Encrypt cannot survive decryption either, so it \
             declares no permissions at all",
        ))
    }

    /// What the source carried that this output cannot: signatures.
    ///
    /// A signature covers a byte range, and the output is not those bytes — this
    /// engine normalises at load and writes no incremental update, so there is no path
    /// by which a signature could remain valid. Carrying an invalid one forward would
    /// be worse than dropping it, and dropping it silently is what this prevents.
    ///
    /// Not a refusal. Saving produces a new document (ADR-0012), and a new document
    /// does not bear someone else's signature; `xmpMM:DerivedFrom` in the output says
    /// what it came from.
    #[must_use]
    pub fn signatures_lost_on_write(&self) -> Option<crate::interpretation::Decision> {
        if self.provenance.signatures == 0 {
            return None;
        }
        Some(crate::interpretation::Decision::violation(
            "12.8",
            format!("the source carried {} digital signature(s)", self.provenance.signatures),
            "the output is a new document derived from it, so they are not carried; \
             a signature covers bytes this output does not reproduce",
        ))
    }

    /// Opens a PDF document from bytes with specific options.
    pub fn open(data: bytes::Bytes, options: &crate::ingest::IngestionOptions) -> PdfResult<Self> {
        let raw = crate::reader::load_document(&data)?;
        let ingested = crate::ingest::Ingestor::ingest(raw, options)?;
        let mut doc =
            Self::with_issues(ingested.arena, ingested.root, ingested.info, ingested.issues);
        doc.force_fallback = options.force_fallback;
        doc.security_method = ingested.security_method;
        doc.permissions = ingested.permissions;
        doc.access = ingested.access;
        doc.provenance = ingested.provenance;

        // Populate font cache from ingestion
        {
            let mut cache = doc.font_cache.write();
            for (idx, res) in ingested.font_cache {
                cache.insert(Handle::new(idx), res);
            }
        }

        doc.load_system_fonts();
        doc.normalize_resources();
        doc.normalize_page_tree();
        doc.index_pages();
        doc.rebuild_page_tree_in_arena()?;
        Ok(doc)
    }

    /// Attempts to open and repair a PDF document with specific options.
    pub fn open_repair(
        data: bytes::Bytes,
        options: &crate::ingest::IngestionOptions,
    ) -> PdfResult<Self> {
        // Recovery is not a separate mode: the reader scans for `N G obj` whenever the
        // cross-reference is unusable, and records the substitution as a `Decision`.
        // There is therefore nothing for a repair path to do that `open` does not
        // already do (ADR-0003). Kept as a distinct entry point because callers name
        // their intent with it.
        Self::open(data, options)
    }

    /// Loads a PDF document from a file path using default options.
    pub fn load(path: &std::path::Path) -> PdfResult<Self> {
        let data = std::fs::read(path).map_err(|e| PdfError::Other(e.to_string().into()))?;
        Self::open(bytes::Bytes::from(data), &crate::ingest::IngestionOptions::default())
    }

    #[cfg(target_os = "macos")]
    fn load_mac_fallbacks(
        fonts: &mut BTreeMap<FallbackFontType, Arc<Vec<u8>>>,
        missing_types: &[FallbackFontType],
    ) {
        let mac_paths = [
            (
                crate::font::FallbackFontType::JapaneseSerif,
                "/System/Library/Fonts/ヒラギノ明朝 ProN.ttc",
            ),
            (
                crate::font::FallbackFontType::JapaneseSans,
                "/System/Library/Fonts/ヒラギノ角ゴ Interface.ttc",
            ),
            (crate::font::FallbackFontType::Serif, "/System/Library/Fonts/Times.ttc"),
            (crate::font::FallbackFontType::SansSerif, "/System/Library/Fonts/Helvetica.ttc"),
            (crate::font::FallbackFontType::Monospace, "/System/Library/Fonts/Courier.dfont"),
        ];
        for (ftype, path) in mac_paths {
            if missing_types.contains(&ftype)
                && let Ok(data) = std::fs::read(path)
            {
                fonts.insert(ftype, Arc::new(data));
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn load_windows_fallbacks(
        fonts: &mut BTreeMap<FallbackFontType, Arc<Vec<u8>>>,
        missing_types: &[FallbackFontType],
    ) {
        let win_paths = [
            (crate::font::FallbackFontType::JapaneseSerif, "C:\\Windows\\Fonts\\msmincho.ttc"),
            (crate::font::FallbackFontType::JapaneseSans, "C:\\Windows\\Fonts\\msgothic.ttc"),
            (crate::font::FallbackFontType::Serif, "C:\\Windows\\Fonts\\times.ttf"),
            (crate::font::FallbackFontType::SansSerif, "C:\\Windows\\Fonts\\arial.ttf"),
            (crate::font::FallbackFontType::Monospace, "C:\\Windows\\Fonts\\cour.ttf"),
        ];
        for (ftype, path) in win_paths {
            if missing_types.contains(&ftype)
                && let Ok(data) = std::fs::read(path)
            {
                fonts.insert(ftype, Arc::new(data));
            }
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    fn load_linux_fallbacks(
        fonts: &mut BTreeMap<FallbackFontType, Arc<Vec<u8>>>,
        missing_types: &[FallbackFontType],
    ) {
        let linux_paths = [
            (
                crate::font::FallbackFontType::JapaneseSerif,
                "/usr/share/fonts/truetype/fonts-japanese-mincho.ttf",
            ),
            (
                crate::font::FallbackFontType::JapaneseSans,
                "/usr/share/fonts/truetype/fonts-japanese-gothic.ttf",
            ),
            (
                crate::font::FallbackFontType::Serif,
                "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
            ),
            (
                crate::font::FallbackFontType::SansSerif,
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            ),
            (
                crate::font::FallbackFontType::Monospace,
                "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            ),
        ];
        for (ftype, path) in linux_paths {
            if missing_types.contains(&ftype)
                && let Ok(data) = std::fs::read(path)
            {
                fonts.insert(ftype, Arc::new(data));
            }
        }
    }

    fn load_platform_fallback_fonts(
        fonts: &mut BTreeMap<FallbackFontType, Arc<Vec<u8>>>,
        missing_types: &[FallbackFontType],
    ) {
        #[cfg(target_os = "macos")]
        Self::load_mac_fallbacks(fonts, missing_types);

        #[cfg(target_os = "windows")]
        Self::load_windows_fallbacks(fonts, missing_types);

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        Self::load_linux_fallbacks(fonts, missing_types);
    }

    /// Loads the fallback faces into this document.
    ///
    /// The assembly itself is [`crate::document::fallback_fonts`], because three callers
    /// wanted it and each had written its own: this one, `VelloBackend::load_system_fonts`
    /// with no platform fallback under it, and `fepdf-cli`'s `host_cjk_fallbacks` with a
    /// hand-written path list that named macOS and Debian and no Windows at all.
    pub fn load_system_fonts(&mut self) {
        self.system_fonts = Arc::new(fallback_fonts());
    }
    /// Returns a reference to the internal arena.
    pub fn arena(&self) -> &PdfArena {
        &self.arena
    }

    /// Drops what [`Self::resolved_color_space`] remembered.
    ///
    /// The cache is keyed by arena handle, and `PdfArena::set_object` writes a handle in
    /// place — so an edit that rewrote a `/ColorSpace` array would leave the old space
    /// answering for the new one. No `Operation` rewrites one today; this is called where
    /// a document is edited so that none has to remember to, and it costs one `clear` per
    /// edit.
    pub fn forget_color_spaces(&self) {
        self.space_cache.write().clear();
    }

    /// The colour space a `/ColorSpace` resource entry resolves to, parsed once.
    ///
    /// Nothing here decides what to *do* with the space — `/Indexed` is held off the
    /// operand path by the interpreter, not by this — so the cache holds what parsing
    /// says and callers keep their own policy. It holds the misses too: an entry that
    /// does not parse does not parse on the next page either.
    #[must_use]
    pub fn resolved_color_space(&self, entry: &Object) -> Option<Arc<ResolvedColorSpace>> {
        let Some(key) = crate::color::space_key(entry) else {
            return ResolvedColorSpace::parse(entry, &self.arena).map(Arc::new);
        };
        if let Some(hit) = self.space_cache.read().get(&key) {
            return hit.clone();
        }
        let resolved = ResolvedColorSpace::parse(entry, &self.arena).map(Arc::new);
        self.space_cache.write().insert(key, resolved.clone());
        resolved
    }

    /// Returns the handle to the document root (Catalog).
    pub fn root_handle(&self) -> &Handle<Object> {
        &self.root
    }

    /// Returns the catalog dictionary handle.
    pub fn catalog_handle(&self) -> Option<Handle<Object>> {
        Some(self.root)
    }

    /// The catalogue (7.7.2) as a typed value.
    ///
    /// Reads the dictionary afresh on each call rather than caching: the arena is the
    /// document's one normalised state (ADR-0013) and anything that edits the catalogue
    /// edits it there, so a cached struct would be a second copy free to disagree.
    ///
    /// # Errors
    /// Fails when `/Root` does not resolve, or resolves to something that is not a
    /// conforming catalogue.
    pub fn catalog(&self) -> PdfResult<PdfCatalog> {
        let object = self
            .arena
            .get_object(self.root)
            .ok_or_else(|| PdfError::Arena("/Root does not resolve".into()))?;
        PdfCatalog::from_pdf_object(object, &self.arena)
    }

    /// Returns the handle to the document info dictionary, if it exists.
    pub fn info_handle(&self) -> Option<Handle<Object>> {
        self.info
    }

    /// Resolves an indirect handle into an object.
    pub fn resolve(&self, handle: &Handle<Object>) -> PdfResult<Object> {
        self.arena
            .get_object(*handle)
            .ok_or_else(|| PdfError::Arena("Failed to resolve handle".into()))
    }

    /// Retrieves a font resource, loading it if not already cached.
    pub fn get_font(&self, handle: Handle<Object>) -> PdfResult<Arc<FontResource>> {
        {
            let cache = self.font_cache.read();
            if let Some(res) = cache.get(&handle) {
                return Ok(Arc::clone(res));
            }
        }

        let obj = self.resolve(&handle)?;
        let dict_h =
            obj.as_dict_handle().ok_or_else(|| PdfError::Other("Not a dictionary".into()))?;
        let dict = self
            .arena
            .get_dict(dict_h)
            .ok_or_else(|| PdfError::Other("Missing dictionary".into()))?;

        let font_res = FontResource::load(&dict, self)?;
        let arc_res = Arc::new(font_res);

        self.font_cache.write().insert(handle, Arc::clone(&arc_res));
        Ok(arc_res)
    }

    /// Decodes a stream object.
    pub fn decode_stream(&self, obj: &Object) -> PdfResult<bytes::Bytes> {
        match obj {
            Object::Stream(dict_handle, data) => {
                let dict = self.arena.get_dict(*dict_handle).ok_or_else(|| PdfError::Filter {
                    filter: "None".into(),
                    message: "Missing stream dictionary".into(),
                })?;
                let raw_bytes = self.arena.get_stream_bytes(data)?;
                self.arena.process_filters(&raw_bytes, &dict)
            }
            // Naming what arrived, because what arrives is the diagnosis. A page whose
            // `/Contents` did not parse leaves a `Null` in the slot, and this reported
            // "Object is not a stream" — true, uninformative, and pointing at the wrong
            // stage. `UnknownFilter-PageContentStream.pdf` closes its content stream
            // dictionary with a single `>`; the reader already records that as a
            // violation naming the object and the offset, and this message sent the
            // reader looking at filters instead.
            other => Err(PdfError::Filter {
                filter: "None".into(),
                message: format!(
                    "expected a stream, found {}",
                    match other {
                        Object::Null => "null — the object did not parse; see the decisions",
                        Object::Dictionary(_) => "a dictionary with no stream data",
                        Object::Reference(_) => "an unresolved reference",
                        _ => "another kind of object",
                    }
                )
                .into(),
            }),
        }
    }

    /// Resolves an indirect object handle to its current dictionary pool handle.
    pub fn resolve_to_dict(&self, handle: Handle<Object>) -> PdfResult<DictHandle> {
        self.arena
            .get_object(handle)
            .and_then(|obj| obj.as_dict_handle())
            .ok_or_else(|| PdfError::Other(format!("Object {handle:?} is not a dictionary").into()))
    }

    /// Returns the total number of pages in the document.
    pub fn page_count(&self) -> PdfResult<usize> {
        Ok(self.pages.len())
    }

    /// Retrieves a specific page by its 0-based index.
    pub fn get_page(&self, index: usize) -> PdfResult<Page<'_>> {
        let page_handle = self
            .pages
            .get(index)
            .ok_or_else(|| PdfError::Other("Page index out of bounds".into()))?;
        let parent_chain = self.get_parent_chain(*page_handle);
        Ok(Page::new(&self.arena, *page_handle, parent_chain))
    }

    /// Returns the handle of a specific page by its 0-based index.
    pub fn get_page_handle(&self, index: usize) -> Option<Handle<Object>> {
        self.pages.get(index).copied()
    }

    // `swap_pages` stood here, and went when Rule D removed the facade method that was its
    // only route out of this crate. Nothing called either one, and no test touched them:
    // a public swap in the model, a public swap in the facade, and not one caller in four
    // frontends or in any test. It was never given an `Operation` because nothing had ever
    // asked for one.
    //
    // Deleted rather than given a variant, on the criterion ADR-0026 states: a capability
    // is required when work already undertaken depends on it, and nothing here does. Two
    // `Operation::Reorder`s express a swap if a caller ever needs one, and then it will be
    // built against a caller instead of before one — which is the failure this codebase
    // keeps paying for (ADR-0007).

    /// Page reorder operation (moves page from `from` index to `to` index with immediate page tree reconstruction)
    pub fn reorder_page(&mut self, from: usize, to: usize) -> PdfResult<()> {
        if from >= self.pages.len() || to >= self.pages.len() {
            return Err(PdfError::Other("Index out of bounds".into()));
        }
        let page = self.pages.remove(from);
        self.pages.insert(to, page);
        self.rebuild_page_tree_in_arena()?;
        Ok(())
    }

    /// Batch page reorder operation (moves multiple pages specified by `source_indices` to `target_insert_pos`).
    pub fn reorder_pages_batch(
        &mut self,
        source_indices: &[usize],
        target_insert_pos: usize,
    ) -> PdfResult<std::ops::Range<usize>> {
        if source_indices.is_empty() {
            return Ok(0..0);
        }
        let total = self.pages.len();
        if target_insert_pos > total {
            return Err(PdfError::Other("Target index out of bounds".into()));
        }
        for &idx in source_indices {
            if idx >= total {
                return Err(PdfError::Other("Source index out of bounds".into()));
            }
        }

        let selected_set: BTreeSet<usize> = source_indices.iter().copied().collect();
        let selected_before_target =
            source_indices.iter().filter(|&&idx| idx < target_insert_pos).count();
        let insert_idx_in_remaining = target_insert_pos.saturating_sub(selected_before_target);

        let mut remaining_pages = Vec::with_capacity(total - selected_set.len());
        let mut moving_pages = Vec::with_capacity(selected_set.len());

        for (i, page) in self.pages.drain(..).enumerate() {
            if selected_set.contains(&i) {
                moving_pages.push((i, page));
            } else {
                remaining_pages.push(page);
            }
        }

        moving_pages.sort_by_key(|(orig_idx, _)| *orig_idx);
        let count = moving_pages.len();
        let clamped_insert_idx = insert_idx_in_remaining.min(remaining_pages.len());

        let mut new_pages = Vec::with_capacity(total);
        new_pages.extend(remaining_pages.drain(..clamped_insert_idx));
        for (_, page) in moving_pages {
            new_pages.push(page);
        }
        new_pages.extend(remaining_pages);

        self.pages = new_pages;
        self.rebuild_page_tree_in_arena()?;

        Ok(clamped_insert_idx..(clamped_insert_idx + count))
    }

    /// Page removal operation (O(1) logical removal with immediate B-tree arena synchronization)
    pub fn remove_page(&mut self, index: usize) -> PdfResult<()> {
        if index >= self.pages.len() {
            return Err(PdfError::Other("Index out of bounds".into()));
        }
        self.pages.remove(index);
        self.rebuild_page_tree_in_arena()?;
        Ok(())
    }

    fn create_empty_page_tree(&self) -> PdfResult<()> {
        let pages_root_key = self.arena.name("Pages");
        let type_key = self.arena.name("Type");
        let count_key = self.arena.name("Count");
        let kids_key = self.arena.name("Kids");

        let mut root_dict = BTreeMap::new();
        root_dict.insert(type_key, Object::Name(pages_root_key));
        root_dict.insert(count_key, Object::Integer(0));
        root_dict.insert(kids_key, Object::Array(self.arena.alloc_array(Vec::new())));

        let root_dh = self.arena.alloc_dict(root_dict);
        let root_h = self.arena.alloc_object(Object::Dictionary(root_dh));

        // Update Catalog
        let catalog_dh = self.resolve_to_dict(self.root)?;
        let mut catalog_dict = self.arena.get_dict(catalog_dh).unwrap_or_default();
        catalog_dict.insert(pages_root_key, Object::Reference(root_h));
        self.arena.set_dict(catalog_dh, catalog_dict);
        Ok(())
    }

    fn build_page_tree_layer(&self, layer: &[Object], max_kids: usize) -> PdfResult<Vec<Object>> {
        let mut next_layer = Vec::new();
        for chunk in layer.chunks(max_kids) {
            let mut total_count = 0;
            let mut kids_refs = Vec::new();

            for kid_obj in chunk {
                kids_refs.push(kid_obj.clone());
                if let Some(kh) = kid_obj.as_reference() {
                    let kid_dh = self.resolve_to_dict(kh)?;
                    let kid_dict = self.arena.get_dict(kid_dh).unwrap_or_default();
                    total_count += self.get_node_count(&kid_dict);
                }
            }

            let pages_root_key = self.arena.name("Pages");
            let type_key = self.arena.name("Type");
            let count_key = self.arena.name("Count");
            let kids_key = self.arena.name("Kids");

            let mut pages_dict = BTreeMap::new();
            pages_dict.insert(type_key, Object::Name(pages_root_key));
            pages_dict.insert(count_key, Object::Integer(total_count as i64));
            pages_dict.insert(kids_key, Object::Array(self.arena.alloc_array(kids_refs)));

            let pages_dh = self.arena.alloc_dict(pages_dict);
            let pages_h = self.arena.alloc_object(Object::Dictionary(pages_dh));

            for kid_obj in chunk {
                if let Some(kh) = kid_obj.as_reference() {
                    let kid_dh = self.resolve_to_dict(kh)?;
                    let mut kid_dict = self.arena.get_dict(kid_dh).unwrap_or_default();
                    kid_dict.insert(self.arena.name("Parent"), Object::Reference(pages_h));
                    self.arena.set_dict(kid_dh, kid_dict);
                }
            }

            next_layer.push(Object::Reference(pages_h));
        }
        Ok(next_layer)
    }

    /// Dynamically rebuilds a clean, balanced B-Tree (max_kids = 50) in the arena.
    pub fn rebuild_page_tree_in_arena(&mut self) -> PdfResult<()> {
        let max_kids = 50;
        let mut current_layer: Vec<Object> =
            self.pages.iter().map(|&h| Object::Reference(h)).collect();

        if current_layer.is_empty() {
            return self.create_empty_page_tree();
        }

        // Build the first layer of Pages nodes
        current_layer = self.build_page_tree_layer(&current_layer, max_kids)?;

        // Loop until we have a single root node in the subsequent layers
        while current_layer.len() > 1 {
            current_layer = self.build_page_tree_layer(&current_layer, max_kids)?;
        }

        // Now current_layer has exactly one node (the root)
        if let Some(root_obj) = current_layer.first()
            && let Some(new_root_h) = root_obj.as_reference()
        {
            // Update Catalog /Pages reference
            let catalog_dh = self.resolve_to_dict(self.root)?;
            let mut catalog_dict = self.arena.get_dict(catalog_dh).unwrap_or_default();
            catalog_dict.insert(self.arena.name("Pages"), Object::Reference(new_root_h));
            self.arena.set_dict(catalog_dh, catalog_dict);

            // Root node in the page tree MUST NOT have a Parent key
            let root_dh = self.resolve_to_dict(new_root_h)?;
            let mut root_dict = self.arena.get_dict(root_dh).unwrap_or_default();
            root_dict.remove(&self.arena.name("Parent"));
            self.arena.set_dict(root_dh, root_dict);
        }

        Ok(())
    }

    /// Retrieves the parent Pages node chain from a leaf Page node up to the root.
    pub fn get_parent_chain(&self, page_h: Handle<Object>) -> Vec<Handle<Object>> {
        let mut chain = Vec::new();
        let mut current = page_h;
        while let Ok(dict_h) = self.resolve_to_dict(current) {
            let Some(dict) = self.arena.get_dict(dict_h) else { break };
            let parent_key = self.arena.name("Parent");
            if let Some(parent_obj) = dict.get(&parent_key)
                && let Some(parent_h) = parent_obj.resolve(&self.arena).as_reference()
            {
                chain.push(parent_h);
                current = parent_h;
            } else {
                break;
            }
        }
        chain.reverse();
        chain
    }

    /// Returns a list of all page object handles in the document.
    ///
    /// **A page tree that will not walk is recorded, not swallowed.** Both failures here
    /// were `if let Ok(..)` and `let _ =`, which is the shape that cost this engine a
    /// catalogue and eleven objects once already (Phase G). It cost pages too:
    /// `UnknownFilter-xrefstm.pdf` names `/Pages 5 0 R`, object 5 was indexed only by a
    /// cross-reference stream written with `/XXXDecode`, and the recovery scan does not
    /// find it — so the walk failed, the failure was dropped, and `inspect info` reported
    /// **"Pages: 0"** about a file that has one. Reported as a `Violation`: something was
    /// lost, and 7.7.3.2 requires a page tree with at least one leaf, so this cannot fire
    /// on a conforming document.
    pub fn find_all_pages(&self) -> Vec<Handle<Object>> {
        let mut pages = Vec::new();
        match self.get_pages_root() {
            Ok(root) => {
                let mut seen = std::collections::BTreeSet::new();
                if let Err(why) = self.walk_pages_recursive(root, &mut pages, 0, &mut seen) {
                    self.record(crate::interpretation::Decision::violation(
                        "7.7.3.2",
                        format!("the page tree could not be walked: {why}"),
                        format!(
                            "kept the {} pages reached before it stopped; the rest of the \
                             tree is not in this document",
                            pages.len()
                        ),
                    ));
                }
            }
            Err(why) => self.record(crate::interpretation::Decision::violation(
                "7.7.3.2",
                format!("the catalogue's page tree could not be reached: {why}"),
                "reported the document as having no pages, because none can be found",
            )),
        }
        pages
    }

    /// Collects the leaves under `node_h`, refusing to walk the same node twice.
    ///
    /// **Two guards, because they answer different questions.** `seen` is what makes the
    /// count right: 7.7.3.2 requires a page tree, every node of which has one `/Parent`,
    /// so a node reached twice is not conforming and expanding it invents pages. `depth`
    /// is what keeps the stack safe on a tree that is deep and legitimate — those are not
    /// bounded by the object count, because each level is a separate object and the
    /// parser's nesting limit does not apply across them.
    ///
    /// **Before 2026-09-05 there was only `depth`, and it produced a wrong answer
    /// quietly.** A two-node loop around a single page read as **sixteen pages** — one
    /// pushed every two levels until the limit — and was written back out as
    /// `/Kids [6 0 R x16]` with `DECISIONS TAKEN READING` saying "none".
    fn walk_pages_recursive(
        &self,
        node_h: Handle<Object>,
        out: &mut Vec<Handle<Object>>,
        depth: usize,
        seen: &mut std::collections::BTreeSet<Handle<Object>>,
    ) -> PdfResult<()> {
        if depth > 32 {
            return Err(PdfError::Other("Page tree depth limit exceeded".into()));
        }
        if !seen.insert(node_h) {
            self.record_second_visit(node_h);
            return Ok(());
        }

        let dict_h = self.resolve_to_dict(node_h)?;
        let dict = self
            .arena
            .get_dict(dict_h)
            .ok_or_else(|| PdfError::Other("Invalid node in page tree".into()))?;

        let type_key = self.arena.name("Type");
        let node_type = dict
            .get(&type_key)
            .and_then(|o| o.resolve(&self.arena).as_name())
            .and_then(|h| self.arena.get_name(h));

        if let Some(name) = node_type
            && name.as_str() == "Page"
        {
            out.push(node_h);
            return Ok(());
        }

        self.walk_kids(&dict, out, depth, seen)
    }

    /// Records that the page tree came back to a node it had already walked.
    ///
    /// Not an error, which is why the caller records and returns `Ok`: reaching a node
    /// twice is not a failure of the subtree below it, it is a node already counted.
    /// Routed through [`Self::walk_kids`]'s error path it read "the page tree does not
    /// walk below object 2", which says the wrong thing about it.
    fn record_second_visit(&self, node_h: Handle<Object>) {
        self.record(crate::interpretation::Decision::violation(
            "7.7.3.2",
            format!(
                "the page tree reaches object {} a second time, so it is not a tree",
                node_h.index()
            ),
            "did not walk it again, so the pages under it are counted once",
        ));
    }

    /// Walks each `/Kids` entry of one node, recording the branches that will not walk.
    ///
    /// **Recorded and not swallowed.** This loop read
    /// `let _ = self.walk_pages_recursive(..)` until 2026-09-05, one scope deeper than
    /// the two that [`Self::find_all_pages`]'s own doc comment describes removing — so
    /// the depth limit's error went nowhere and a looping tree was expanded in silence.
    /// A failing branch still does not take the rest of the document with it, which is
    /// why this records rather than propagates.
    fn walk_kids(
        &self,
        dict: &BTreeMap<Handle<PdfName>, Object>,
        out: &mut Vec<Handle<Object>>,
        depth: usize,
        seen: &mut std::collections::BTreeSet<Handle<Object>>,
    ) -> PdfResult<()> {
        let Some(kids_obj) = dict.get(&self.arena.name("Kids")) else {
            return Ok(());
        };
        let ah = kids_obj
            .resolve(&self.arena)
            .as_array()
            .ok_or_else(|| PdfError::Other("Invalid Kids array".into()))?;
        let Some(kids) = self.arena.get_array(ah) else {
            return Ok(());
        };
        for kid in kids {
            if let Some(h) = kid.as_reference()
                && let Err(why) = self.walk_pages_recursive(h, out, depth + 1, seen)
            {
                self.record(crate::interpretation::Decision::violation(
                    "7.7.3.2",
                    format!("the page tree does not walk below object {}: {why}", h.index()),
                    "stopped there and kept the pages reached by the other branches",
                ));
            }
        }
        Ok(())
    }

    fn get_pages_root(&self) -> PdfResult<Handle<Object>> {
        let catalog_obj = self
            .arena
            .get_object(self.root)
            .ok_or_else(|| PdfError::Other("Missing document catalog".into()))?;
        // The one entry, not the whole catalogue: a document's pages must not become
        // unreachable because some other entry of Table 29 will not parse.
        entries::entry::<entries::Located<entries::PageTreeRoot>>(
            &self.arena,
            &catalog_obj,
            "Pages",
        )?
        .and_then(|p| p.reference)
        .ok_or_else(|| PdfError::Other("The catalogue names no page tree (7.7.2)".into()))
    }

    fn get_node_count(&self, dict: &BTreeMap<Handle<PdfName>, Object>) -> usize {
        let count_key = self.arena.name("Count");
        if let Some(count) = dict.get(&count_key).and_then(|o| o.resolve(&self.arena).as_integer())
        {
            return usize::try_from(count).unwrap_or(0);
        }
        // Leaf Page nodes usually lack /Count, they count as 1
        let type_key = self.arena.name("Type");
        if let Some(t) = dict.get(&type_key).and_then(|o| o.resolve(&self.arena).as_name())
            && let Some(name) = self.arena.get_name(t)
            && name.as_str() == "Page"
        {
            return 1;
        }
        0
    }

    /// Returns the handle to the Structure Tree Root dictionary, if it exists.
    ///
    /// The one entry, for the reason `get_pages_root` gives: auditing a document's
    /// structure must not depend on the legibility of `/MarkInfo`.
    pub fn get_structure_root(&self) -> PdfResult<Option<Handle<Object>>> {
        let catalog_obj = self
            .arena
            .get_object(self.root)
            .ok_or_else(|| PdfError::Other("Missing document catalog".into()))?;
        Ok(entries::entry::<entries::Located<entries::StructTreeRoot>>(
            &self.arena,
            &catalog_obj,
            "StructTreeRoot",
        )?
        .and_then(|s| s.reference))
    }

    /// Returns the document metadata.
    pub fn metadata(&self) -> crate::metadata::MetadataInfo {
        crate::metadata::extract_metadata(self)
    }

    /// Returns a list of fonts used in the document.
    pub fn fonts(&self) -> Vec<crate::font::FontSummary> {
        crate::font::list_fonts(self)
    }

    /// Normalizes document resources at load-time (Phase 3).
    /// Group fonts by BaseFont and CIDSystemInfo to share ToUnicode mappings.
    pub fn normalize_resources(&mut self) {
        let (font_groups, best_to_unicode) = self.discover_font_groups();
        self.propagate_tounicode_mappings(font_groups, best_to_unicode);
    }

    /// Normalizes the page tree by pushing down inherited attributes (Phase 4).
    pub fn normalize_page_tree(&mut self) {
        let root_h = match self.get_pages_root() {
            Ok(h) => h,
            Err(_) => return,
        };

        let mut inherited = BTreeMap::new();
        let _ = self.push_down_attributes_recursive(root_h, &mut inherited, 0);
    }

    fn process_leaf_page(
        &self,
        dict_h: Handle<BTreeMap<Handle<PdfName>, Object>>,
        dict: &BTreeMap<Handle<PdfName>, Object>,
        local_inherited: BTreeMap<Handle<PdfName>, Object>,
    ) -> PdfResult<()> {
        let mut leaf_dict = dict.clone();
        for (key, val) in local_inherited {
            leaf_dict.entry(key).or_insert(val);
        }

        // Ensure CropBox and Rotate are explicitly set for Acrobat standardization
        let mb_key = self.arena.name("MediaBox");
        let cb_key = self.arena.name("CropBox");
        let rot_key = self.arena.name("Rotate");

        if !leaf_dict.contains_key(&cb_key)
            && let Some(mb_val) = leaf_dict.get(&mb_key)
        {
            leaf_dict.insert(cb_key, mb_val.clone());
        }
        leaf_dict.entry(rot_key).or_insert(Object::Integer(0));

        self.arena.set_dict(dict_h, leaf_dict);
        Ok(())
    }

    fn process_pages_node(
        &self,
        dict_h: Handle<BTreeMap<Handle<PdfName>, Object>>,
        dict: &BTreeMap<Handle<PdfName>, Object>,
        local_inherited: &mut BTreeMap<Handle<PdfName>, Object>,
        depth: usize,
    ) -> PdfResult<()> {
        let kids_key = self.arena.name("Kids");
        let kids_obj = dict
            .get(&kids_key)
            .ok_or_else(|| PdfError::Other("Missing Kids in Pages node".into()))?;
        let ah = kids_obj
            .resolve(&self.arena)
            .as_array()
            .ok_or_else(|| PdfError::Other("Invalid Kids array".into()))?;
        let kids = self
            .arena
            .get_array(ah)
            .ok_or_else(|| PdfError::Other("Invalid kids array handle".into()))?;
        for kid in kids {
            if let Some(kh) = kid.as_reference() {
                self.push_down_attributes_recursive(kh, local_inherited, depth + 1)?;
            }
        }

        let mut pages_dict = dict.clone();
        for attr in ["Resources", "MediaBox", "CropBox", "Rotate"] {
            pages_dict.remove(&self.arena.name(attr));
        }
        self.arena.set_dict(dict_h, pages_dict);
        Ok(())
    }

    #[allow(clippy::needless_pass_by_ref_mut)]
    fn push_down_attributes_recursive(
        &self,
        node_h: Handle<Object>,
        inherited: &mut BTreeMap<Handle<PdfName>, Object>,
        depth: usize,
    ) -> PdfResult<()> {
        if depth > 32 {
            return Err(PdfError::Other("Page tree depth limit exceeded".into()));
        }

        let dict_h = self.resolve_to_dict(node_h)?;
        let dict =
            self.arena.get_dict(dict_h).ok_or_else(|| PdfError::Other("Invalid node".into()))?;

        let type_key = self.arena.name("Type");
        let node_type = dict
            .get(&type_key)
            .and_then(|o| o.resolve(&self.arena).as_name())
            .and_then(|h| self.arena.get_name(h));

        // Update inherited attributes for this level
        let attrs = ["Resources", "MediaBox", "CropBox", "Rotate"];
        let mut local_inherited = inherited.clone();
        for attr in attrs {
            let key = self.arena.name(attr);
            if let Some(val) = dict.get(&key) {
                local_inherited.insert(key, val.clone());
            }
        }

        if let Some(name) = &node_type
            && name.as_str() == "Page"
        {
            return self.process_leaf_page(dict_h, &dict, local_inherited);
        }

        if let Some(name) = &node_type
            && name.as_str() == "Pages"
        {
            return self.process_pages_node(dict_h, &dict, &mut local_inherited, depth);
        }

        Err(PdfError::Other("Invalid node type in page tree".into()))
    }

    fn discover_font_groups(&self) -> (FontGroupMap, BestToUnicodeMap) {
        let arena = &self.arena;
        let mut font_groups = BTreeMap::new();
        let mut best_to_unicode = BTreeMap::new();
        let mut best_to_unicode_count = BTreeMap::new();

        let type_key = arena.name("Type");
        let font_val = arena.name("Font");

        for h in arena.all_dict_handles() {
            // **Ask before copying.** This copied every dictionary in the arena to read
            // one entry of it — 720,603 of the 1,882,351 `get_dict` calls opening
            // `samples/intel_sdm.pdf`, and all but the font dictionaries discarded on the
            // next line (ROADMAP W-A4).
            let Some(t_h) = arena.dict_entry(h, type_key).and_then(|o| o.resolve(arena).as_name())
            else {
                continue;
            };
            if t_h != font_val {
                continue;
            }
            let Some(dict) = arena.get_dict(h) else { continue };
            self.group_one_font(
                h,
                &dict,
                &mut font_groups,
                &mut best_to_unicode,
                &mut best_to_unicode_count,
            );
        }
        (font_groups, best_to_unicode)
    }

    /// One font dictionary, into its group and that group's best `/ToUnicode`.
    ///
    /// Split out of [`Self::discover_font_groups`] when the loop above it grew past
    /// RR-15 Rule 1's fifty lines. **Only a CID font is grouped**: the group is keyed by
    /// base name and `CIDSystemInfo`, which a simple font has none of.
    fn group_one_font(
        &self,
        handle: DictHandle,
        dict: &BTreeMap<Handle<PdfName>, Object>,
        font_groups: &mut FontGroupMap,
        best_to_unicode: &mut BestToUnicodeMap,
        best_count: &mut BTreeMap<(String, String), usize>,
    ) {
        let arena = &self.arena;
        if !dict.contains_key(&arena.name("DescendantFonts")) {
            return;
        }
        let base_font = dict
            .get(&arena.name("BaseFont"))
            .and_then(|o| o.resolve(arena).as_name())
            .and_then(|h| arena.get_name_str(h))
            .unwrap_or_else(|| "Untitled".to_string());
        let key = (base_font, self.extract_csi_string(dict));
        font_groups.entry(key.clone()).or_default().push(handle);

        let Some(tu) = dict.get(&arena.name("ToUnicode")) else { return };
        let Ok(data) = self.decode_stream(&tu.resolve(arena)) else { return };
        let Ok(map) = crate::font::cmap::CMap::parse(&data) else { return };

        // The group keeps the richest `/ToUnicode` any of its members carries.
        let count = map.mappings.len();
        if best_count.get(&key).is_none_or(|best| count > *best) {
            best_count.insert(key.clone(), count);
            best_to_unicode.insert(key, tu.clone());
        }
    }

    fn extract_csi_string(&self, dict: &BTreeMap<Handle<PdfName>, Object>) -> String {
        let arena = &self.arena;
        if let Some(df_obj) = dict.get(&arena.name("DescendantFonts"))
            && let Some(ah) = df_obj.resolve(arena).as_array()
            && let Some(arr) = arena.get_array(ah)
            && let Some(df_h) = arr.first().and_then(|o| o.resolve(arena).as_dict_handle())
            && let Some(df_dict) = arena.get_dict(df_h)
            && let Some(csi_obj) = df_dict.get(&arena.name("CIDSystemInfo"))
            && let Some(csi_h) = csi_obj.resolve(arena).as_dict_handle()
            && let Some(csi_dict) = arena.get_dict(csi_h)
        {
            let r = csi_dict
                .get(&arena.name("Registry"))
                .map(|o| o.resolve(arena))
                .as_ref()
                .and_then(|o| o.as_string())
                .map(|s| String::from_utf8_lossy(s).to_string())
                .unwrap_or_default();
            let o = csi_dict
                .get(&arena.name("Ordering"))
                .map(|o| o.resolve(arena))
                .as_ref()
                .and_then(|o| o.as_string())
                .map(|s| String::from_utf8_lossy(s).to_string())
                .unwrap_or_default();
            return format!("{r}-{o}");
        }
        String::new()
    }

    fn propagate_tounicode_mappings(
        &self,
        font_groups: FontGroupMap,
        best_to_unicode: BestToUnicodeMap,
    ) {
        let arena = &self.arena;
        let to_unicode_key = arena.name("ToUnicode");
        for (key, fonts) in font_groups {
            if let Some(best_tu) = best_to_unicode.get(&key) {
                for font_h in fonts {
                    if let Some(mut dict) = arena.get_dict(font_h)
                        && !dict.contains_key(&to_unicode_key)
                    {
                        dict.insert(to_unicode_key, best_tu.clone());
                        arena.set_dict(font_h, dict);
                    }
                }
            }
        }
    }

    /// Returns the sublimated data for a stream object.
    pub fn get_sublimated_data(
        &self,
        handle: Handle<Object>,
    ) -> Option<std::sync::Arc<crate::object::SublimatedData>> {
        self.arena.get_sublimated_data(handle)
    }
}

/// Gives `FallbackFontType::Default` a face, copying whichever general-purpose one was
/// found.
///
/// A font resource that says nothing about its shape infers `Default`, and no loader
/// populated that key, so the lookup missed and the caller got no data at all. Sans
/// first because a face chosen for "no preference" should be the plainest available.
fn seed_default_face(fonts: &mut BTreeMap<crate::font::FallbackFontType, Arc<Vec<u8>>>) {
    use crate::font::FallbackFontType;
    if fonts.contains_key(&FallbackFontType::Default) {
        return;
    }
    for source in [
        FallbackFontType::SansSerif,
        FallbackFontType::Serif,
        FallbackFontType::Monospace,
        FallbackFontType::JapaneseSans,
    ] {
        if let Some(data) = fonts.get(&source).cloned() {
            fonts.insert(FallbackFontType::Default, data);
            return;
        }
    }
}

/// The fallback faces, from the resource directory if there is one and from the platform's
/// own fonts otherwise.
///
/// **There were three of these and they did not agree.** The model's had the platform
/// fallback below it; `VelloBackend::load_system_fonts` had none and returned an empty map
/// when the resource directory was absent, which is every released archive; and
/// `fepdf-cli`'s `host_cjk_fallbacks` had a hand-written list of macOS and Debian paths and
/// no Windows at all, so a Windows user of the CLI had no fallback face for any script.
///
/// One assembly, so that what a caller gets does not depend on which crate it asked.
#[must_use]
pub fn fallback_fonts() -> BTreeMap<crate::font::FallbackFontType, Arc<Vec<u8>>> {
    use crate::font::FallbackFontType;

    let mut fonts = BTreeMap::new();

    // The resource directory first: a document that ships its own faces means them.
    if let Some(base) = fepdf_font::resources::locate(fepdf_font::resources::Resource::Fonts) {
        for (kind, filename) in [
            (FallbackFontType::Serif, "serif.ttf"),
            (FallbackFontType::SansSerif, "sans.ttf"),
            (FallbackFontType::Monospace, "mono.ttf"),
            (FallbackFontType::JapaneseSerif, "mincho.ttf"),
            (FallbackFontType::JapaneseSans, "gothic.ttf"),
        ] {
            if let Ok(data) = std::fs::read(base.join(filename)) {
                fonts.insert(kind, Arc::new(data));
            }
        }
    }

    let missing: Vec<_> = [
        FallbackFontType::Serif,
        FallbackFontType::SansSerif,
        FallbackFontType::Monospace,
        FallbackFontType::JapaneseSerif,
        FallbackFontType::JapaneseSans,
    ]
    .into_iter()
    .filter(|kind| !fonts.contains_key(kind))
    .collect();

    if !missing.is_empty() {
        Document::load_platform_fallback_fonts(&mut fonts, &missing);
    }

    // `Default` is what a font resource gets when nothing about it suggests a face, and it
    // is in no `missing` list above — so nothing ever put it in the map and every lookup
    // for it missed.
    seed_default_face(&mut fonts);
    fonts
}

#[cfg(test)]
mod permission_notice {
    //! `/P` is reported, never enforced — and reported to exactly one party.

    use super::*;
    use fepdf_syntax::security::Access;

    /// A document with the given access level and permission bits, and nothing else.
    fn with(access: Option<Access>, permissions: Option<i32>) -> Document {
        let arena = PdfArena::new();
        let root = arena.alloc_object(Object::Null);
        let mut doc = Document::new(arena, root, None);
        doc.access = access;
        doc.permissions = permissions;
        doc
    }

    #[test]
    fn a_source_signature_is_reported_as_not_carried() {
        // A signature covers a byte range; the output is not those bytes. Carrying an
        // invalid one forward would be worse than dropping it, and dropping it in
        // silence is what this prevents.
        let mut doc = with(None, None);
        doc.provenance.signatures = 2;
        let decision = doc.signatures_lost_on_write().expect("a notice is owed");
        assert!(decision.found.contains('2'), "{decision}");
        assert!(decision.action.contains("new document derived"), "{decision}");
        assert!(!decision.action.contains("refus"), "nothing is refused: {decision}");
    }

    #[test]
    fn an_unsigned_source_says_nothing() {
        assert!(with(None, None).signatures_lost_on_write().is_none());
    }

    #[test]
    fn user_access_against_a_restrictive_p_is_reported() {
        // samples/unicode_16.pdf: bits 4 and 11 clear, opened with the default
        // password, which 7.6.4.1 makes user access.
        let doc = with(Some(Access::User), Some(-1036));
        let decision = doc.permissions_lost_on_write().expect("a notice is owed");
        assert!(decision.found.contains("modification"), "{decision}");
        assert!(decision.found.contains("assembly"), "{decision}");
        // The action describes what happened, never a refusal: /P is a declaration
        // and 7.6.4.1 puts obeying it at `should`. It also must not imply a choice —
        // normalisation at load means no faithful copy exists to write.
        assert!(decision.action.contains("the output is modified"), "{decision}");
        assert!(decision.action.contains("declares no permissions"), "{decision}");
        assert!(
            !decision.action.contains("refus") && !decision.action.contains("declined"),
            "nothing is refused: {decision}"
        );
    }

    #[test]
    fn owner_access_is_not_nagged() {
        // 7.6.4.1: the owner password carries full access, "including the ability to
        // change the document's passwords and access permissions". Reporting a loss to
        // the party entitled to cause it would make the notice noise.
        assert!(with(Some(Access::Owner), Some(-1036)).permissions_lost_on_write().is_none());
    }

    #[test]
    fn a_permissive_p_says_nothing() {
        // -4 clears only the two reserved low bits: everything is permitted, so no
        // declaration is lost by writing. A notice here would fire on every encrypted
        // document and stop being a signal (ADR-0008).
        assert!(with(Some(Access::User), Some(-4)).permissions_lost_on_write().is_none());
    }

    #[test]
    fn an_unencrypted_document_says_nothing() {
        assert!(with(None, None).permissions_lost_on_write().is_none());
        assert!(with(None, Some(-1036)).permissions_lost_on_write().is_none());
    }

    #[test]
    fn each_denied_bit_is_named() {
        // Bit 6 is annotations; naming which bits were set is what makes the notice
        // actionable rather than a warning that something was lost.
        let doc = with(Some(Access::User), Some(!0b0010_0000));
        let decision = doc.permissions_lost_on_write().expect("bit 6 is clear");
        assert!(decision.found.contains("annotation"), "{decision}");
    }
}

#[cfg(test)]
mod page_tree_is_a_tree {
    //! 7.7.3.2 requires a page tree. A file that presents a graph is reported as one.

    use super::*;
    use crate::ingest::IngestionOptions;

    fn assemble(objects: &[&str]) -> bytes::Bytes {
        let out = fepdf_fixtures::assemble(objects);
        bytes::Bytes::from(out)
    }

    fn open(objects: &[&str]) -> Document {
        Document::open(assemble(objects), &IngestionOptions::default()).expect("the fixture reads")
    }

    /// A `/Kids` that names an ancestor yields the pages that are there, and says so.
    ///
    /// **This is the defect it was written for.** The walk had a depth limit and no
    /// memory, so it followed the loop until the limit: one page pushed every two levels
    /// until depth 32 gave **sixteen**. The document then *was* sixteen pages — written
    /// back out as `/Kids [6 0 R x16]` — and `DECISIONS TAKEN READING` said "none".
    ///
    /// Verified by removing the visited set: the count returns to 16 and the violation
    /// disappears. A depth limit cannot catch this; only remembering can.
    #[test]
    fn a_kids_that_names_an_ancestor_counts_its_page_once() {
        let doc = open(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
            "<< /Type /Pages /Parent 2 0 R /Kids [2 0 R] /Count 1 >>",
        ]);

        assert_eq!(doc.find_all_pages().len(), 1, "the file has one page object");

        let decisions = doc.decisions.entries();
        let found = decisions.iter().find(|d| d.clause == "7.7.3.2").expect(
            "a page tree that is not a tree is a departure from 7.7.3.2 and must be recorded",
        );
        assert_eq!(found.severity, crate::interpretation::Severity::Violation);
        assert!(found.found.contains("a second time"), "{}", found.found);
    }

    /// A nested page tree that is a tree still yields every page.
    ///
    /// Without this, a `seen` that pruned too eagerly — or one keyed on the wrong thing —
    /// passes the test above by finding nothing at all.
    #[test]
    fn a_nested_tree_still_yields_every_page() {
        let doc = open(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 3 >>",
            "<< /Type /Pages /Parent 2 0 R /Kids [5 0 R 6 0 R] /Count 2 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
            "<< /Type /Page /Parent 3 0 R /MediaBox [0 0 612 792] >>",
            "<< /Type /Page /Parent 3 0 R /MediaBox [0 0 612 792] >>",
        ]);

        assert_eq!(doc.find_all_pages().len(), 3, "two levels, three leaves");
        assert!(
            doc.decisions.entries().iter().all(|d| d.clause != "7.7.3.2"),
            "a conforming tree records nothing"
        );
    }

    /// One page named twice under one parent is counted once, and reported.
    ///
    /// The looping case above needs two levels to close; this is the one-level form of
    /// the same non-conformance, and it is the shape a producer actually emits.
    #[test]
    fn a_page_named_twice_is_counted_once() {
        let doc = open(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R 3 0 R] /Count 2 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
        ]);

        assert_eq!(doc.find_all_pages().len(), 1, "one page object, named twice");
        assert!(
            doc.decisions.entries().iter().any(|d| d.clause == "7.7.3.2"),
            "naming the same page twice is a departure and is recorded"
        );
    }
}

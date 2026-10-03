/// ISO 32000-2 Extended domain models.
/// What the catalogue's entries hold (Table 29), read.
pub mod entries;
pub mod extensions;
/// Pages and the page tree.
pub mod page;
pub mod structure;

/// What loading settles once: resources and page attributes pushed down, and fonts
/// grouped so a `/ToUnicode` one has is shared.
mod normalisation;
/// The page tree: moving, removing and rebuilding pages, and walking the tree to find
/// them.
mod page_tree;
/// The fonts this machine has, found where each platform keeps them, for a page whose
/// fonts are not embedded.
mod system_fonts;

use self::page::Page;
use crate::color::ResolvedColorSpace;
use crate::error::PdfError;
use crate::font::{FallbackFontType, FontResource};
use crate::handle::DictHandle;
use crate::{FromPdfObject, Handle, Object, PdfArena, PdfName, PdfResult};
use parking_lot::RwLock;
use std::collections::BTreeMap;
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
    /// The version the file's `%PDF-M.m` header declared, as read; `None` for a document
    /// built in memory. The arena's version is what a save writes, which is another thing.
    pub header_version: Option<String>,
    /// Optional-content groups a *viewer* has turned on or off, over what the
    /// configuration says (8.11.4.3).
    ///
    /// **Not part of the document.** `save` never writes it and no `Operation` produces
    /// it: 6.3.2.3 requires an interactive processor to let a person toggle a layer, and
    /// a person doing that is not editing the file. Behind a lock and reached through
    /// `&self` for the same reason [`Document::record`] is — the render path holds a
    /// shared reference and the panel sits above it.
    layer_overrides: parking_lot::Mutex<BTreeMap<Handle<Object>, bool>>,
    /// An empty resource dictionary, allocated with the document, for a reader to draw
    /// with where a stream names none. **Allocated before the arena is sealed**: a reader
    /// that allocated one each time wrote into the document it read (ROADMAP Y-11).
    /// Nothing references it, so nothing writes it out.
    no_resources: Handle<BTreeMap<Handle<PdfName>, Object>>,
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
        Self::with_issues(arena, root, info, Vec::new())
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
        let no_resources = arena.alloc_dict(BTreeMap::new());
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
            security_method: crate::decrypt::NO_SECURITY.to_string(),
            permissions: None,
            access: None,
            provenance: Provenance::default(),
            header_version: None,
            runs_scripts: std::sync::atomic::AtomicBool::new(false),
            layer_overrides: parking_lot::Mutex::new(BTreeMap::new()),
            no_resources,
        }
    }

    /// An empty resource dictionary to draw with where a stream names none, the same one
    /// every time; see the field.
    #[must_use]
    pub fn no_resources(&self) -> Handle<BTreeMap<Handle<PdfName>, Object>> {
        self.no_resources
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
        let header_version = Some(raw.version.clone());
        let ingested = crate::ingest::Ingestor::ingest(raw, options)?;
        let mut doc =
            Self::with_issues(ingested.arena, ingested.root, ingested.info, ingested.issues);
        doc.force_fallback = options.force_fallback;
        doc.security_method = ingested.security_method;
        doc.permissions = ingested.permissions;
        doc.access = ingested.access;
        doc.provenance = ingested.provenance;
        doc.header_version = header_version;

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
        crate::ingest::require_page_resources(&doc);
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
        let data = std::fs::read(path)?;
        Self::open(bytes::Bytes::from(data), &crate::ingest::IngestionOptions::default())
    }

    /// Returns a reference to the internal arena.
    pub fn arena(&self) -> &PdfArena {
        &self.arena
    }

    /// Runs `change` as the one way this document changes once loaded: its arena
    /// unsealed for it alone (ROADMAP Y-11), and **everything put back if it fails**
    /// (Y-F12) — the arena's pools, the page list, and the decisions it recorded — with
    /// the caches keyed by arena handles dropped, since a handle it allocated is gone.
    ///
    /// # Errors
    /// What `change` returns, after the document is as it was.
    pub fn change<R>(&mut self, change: impl FnOnce(&mut Self) -> PdfResult<R>) -> PdfResult<R> {
        let arena = self.arena.clone();
        let pages = self.pages.clone();
        let decided = self.decisions.len();
        let result = arena.transaction(|| change(self));
        if result.is_err() {
            self.pages = pages;
            self.decisions.truncate(decided);
            self.font_cache.write().clear();
            self.forget_color_spaces();
        }
        result
    }

    /// Runs `write` with the arena unsealed **outside `apply`**, and puts back what it
    /// wrote if it fails.
    ///
    /// **For the physical redaction route alone**, which three frontends call through
    /// `&Document` and which Y-10 replaces with an `Operation`; this goes with it. Named
    /// for that, so that a second caller reads as what it is (ROADMAP Y-11).
    ///
    /// # Errors
    /// What `write` returns, after the arena is as it was.
    pub fn redaction_until_y10<R>(&self, write: impl FnOnce() -> PdfResult<R>) -> PdfResult<R> {
        self.arena.transaction(write)
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
        let dict_h = obj
            .as_dict_handle()
            .ok_or_else(|| PdfError::refused("get_font", "Not a dictionary"))?;
        let dict =
            self.arena.get_dict(dict_h).ok_or_else(|| PdfError::internal("Missing dictionary"))?;

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
        self.arena.get_object(handle).and_then(|obj| obj.as_dict_handle()).ok_or_else(|| {
            PdfError::violation("7.3.7", format!("Object {handle:?} is not a dictionary"))
        })
    }

    /// Whether the file carried an `/Encrypt` dictionary — opened or not. Ingestion takes
    /// the dictionary away once it has decrypted with it, so this is where it is known.
    #[must_use]
    pub fn is_encrypted(&self) -> bool {
        self.security_method != crate::decrypt::NO_SECURITY
    }

    /// Returns the total number of pages in the document.
    pub fn page_count(&self) -> PdfResult<usize> {
        Ok(self.pages.len())
    }

    /// Retrieves a specific page by its 0-based index.
    pub fn get_page(&self, index: usize) -> PdfResult<Page<'_>> {
        let page_handle = self.page_handle(index)?;
        let parent_chain = self.get_parent_chain(page_handle);
        Ok(Page::new(&self.arena, page_handle, parent_chain))
    }

    /// The page at `index`, or [`Missing::Page`](crate::Missing::Page) saying how many
    /// there are.
    ///
    /// # Errors
    ///
    /// `index` is past the last page.
    pub fn page_handle(&self, index: usize) -> PdfResult<Handle<Object>> {
        self.pages.get(index).copied().ok_or_else(|| PdfError::no_page(index, self.pages.len()))
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

    /// Returns the handle to the Structure Tree Root dictionary, if it exists.
    ///
    /// The one entry, for the reason `get_pages_root` gives: auditing a document's
    /// structure must not depend on the legibility of `/MarkInfo`.
    pub fn get_structure_root(&self) -> PdfResult<Option<Handle<Object>>> {
        let catalog_obj = self
            .arena
            .get_object(self.root)
            .ok_or_else(|| PdfError::violation("7.7.2", "Missing document catalog"))?;
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

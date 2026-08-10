pub mod conformance;
/// ISO 32000-2 Extended domain models.
pub mod extensions;
/// Pages and the page tree.
pub mod page;
/// Page-tree traversal strategies.
pub mod strategy;
pub mod structure;

use self::page::Page;
pub use self::strategy::{PageTreeStrategy, PageTreeView};
use crate::error::PdfError;
use crate::font::{FallbackFontType, FontResource};
use crate::{FromPdfObject, Handle, Object, PdfArena, PdfName, PdfResult};
use parking_lot::RwLock;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Refined PDF Catalog (Root) Dictionary (ISO 32000-2:2020 Clause 7.7.2)
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "7.7.2")]
pub struct PdfCatalog {
    #[pdf_key("Pages")]
    /// `/Pages`: root of the page tree.
    pub pages: Handle<Object>,
    #[pdf_key("StructTreeRoot")]
    /// `/StructTreeRoot`: root of the logical structure tree.
    pub struct_tree_root: Option<Handle<Object>>,
    #[pdf_key("MarkInfo")]
    /// `/MarkInfo`: whether the document is tagged.
    pub mark_info: Option<Object>,
    #[pdf_key("Metadata")]
    /// `/Metadata`: the XMP metadata stream.
    pub metadata: Option<Object>,
    #[pdf_key("Version")]
    /// `/Version`: a version overriding the file header.
    pub version: Option<Handle<PdfName>>,
    #[pdf_key("AcroForm")]
    /// `/AcroForm`: interactive form definition.
    pub acro_form: Option<Object>,
    #[pdf_key("Names")]
    /// `/Names`: the document's name dictionaries.
    pub names: Option<Object>,
    #[pdf_key("Outlines")]
    /// `/Outlines`: the bookmark tree.
    pub outlines: Option<Object>,
    #[pdf_key("OpenAction")]
    /// `/OpenAction`: action performed when the document opens.
    pub open_action: Option<Object>,
    #[pdf_key("AA")]
    /// `/AA`: additional actions triggered by document events.
    pub additional_actions: Option<Object>,
}

/// Type alias for a dictionary handle to satisfy clippy complexity rules.
pub type DictHandle = Handle<BTreeMap<Handle<PdfName>, Object>>;
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
    pub ingestion_issues: Vec<String>,
    /// System font cache (shared across pages).
    pub system_fonts: Arc<BTreeMap<FallbackFontType, Arc<Vec<u8>>>>,
    /// Parsed FontResource cache to prevent redundant parsing across pages.
    pub font_cache: Arc<RwLock<BTreeMap<Handle<Object>, Arc<FontResource>>>>,
    /// Whether bundled fonts stand in for unparseable embedded programs.
    pub force_fallback: bool,
    /// Description of the encryption that was in force, if any.
    pub security_method: String,
    /// Permission flags recovered from the encryption dictionary.
    pub permissions: Option<i32>,
}

impl Document {
    /// Creates a new document wrapper.
    pub fn new(arena: PdfArena, root: Handle<Object>, info: Option<Handle<Object>>) -> Self {
        Self {
            arena,
            root,
            info,
            pages: Vec::new(),
            ingestion_issues: Vec::new(),
            system_fonts: Arc::new(BTreeMap::new()),
            font_cache: Arc::new(RwLock::new(BTreeMap::new())),
            force_fallback: false,
            security_method: "No Security".to_string(),
            permissions: None,
        }
    }

    /// Creates a new document wrapper with issues.
    pub fn with_issues(
        arena: PdfArena,
        root: Handle<Object>,
        info: Option<Handle<Object>>,
        issues: Vec<String>,
    ) -> Self {
        Self {
            arena,
            root,
            info,
            pages: Vec::new(),
            ingestion_issues: issues,
            system_fonts: Arc::new(BTreeMap::new()),
            font_cache: Arc::new(RwLock::new(BTreeMap::new())),
            force_fallback: false,
            security_method: "No Security".to_string(),
            permissions: None,
        }
    }

    /// Opens a PDF document from bytes with specific options.
    pub fn open(data: bytes::Bytes, options: &crate::ingest::IngestionOptions) -> PdfResult<Self> {
        let mut lopdf_doc = lopdf::Document::load_mem(&data)
            .map_err(|e| PdfError::Parse { pos: 0, message: e.to_string().into() })?;

        // Attempt to decrypt with empty password if encrypted
        if lopdf_doc.is_encrypted() {
            match lopdf_doc.decrypt("") {
                Ok(()) => {}
                Err(_e) => {
                    // We will try manual Pass 0 decryption in Ingestor
                }
            }
        }

        let ingested = crate::ingest::Ingestor::ingest(&mut lopdf_doc, options)?;
        let mut doc =
            Self::with_issues(ingested.arena, ingested.root, ingested.info, ingested.issues);
        doc.force_fallback = options.force_fallback;
        doc.security_method = ingested.security_method;
        doc.permissions = ingested.permissions;

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
        doc.pages = doc.find_all_pages();
        doc.rebuild_page_tree_in_arena()?;
        Ok(doc)
    }

    /// Attempts to open and repair a PDF document with specific options.
    pub fn open_repair(
        data: bytes::Bytes,
        options: &crate::ingest::IngestionOptions,
    ) -> PdfResult<Self> {
        // lopdf's load_mem is already quite robust, but we could add more repair logic here
        Self::open(data, options)
    }

    /// Loads a PDF document from a file path using default options.
    pub fn load(path: &std::path::Path) -> PdfResult<Self> {
        let data = std::fs::read(path).map_err(|e| PdfError::Other(e.to_string().into()))?;
        Self::open(bytes::Bytes::from(data), &crate::ingest::IngestionOptions::default())
    }

    #[cfg(target_os = "macos")]
    fn load_mac_fallbacks(
        &self,
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
            if missing_types.contains(&ftype) {
                let _ = std::fs::read(path).map(|data| {
                    fonts.insert(ftype, Arc::new(data));
                });
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn load_windows_fallbacks(
        &self,
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
            if missing_types.contains(&ftype) {
                let _ = std::fs::read(path).map(|data| {
                    fonts.insert(ftype, Arc::new(data));
                });
            }
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    fn load_linux_fallbacks(
        &self,
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
            if missing_types.contains(&ftype) {
                let _ = std::fs::read(path).map(|data| {
                    fonts.insert(ftype, Arc::new(data));
                });
            }
        }
    }

    fn load_platform_fallback_fonts(
        &self,
        fonts: &mut BTreeMap<FallbackFontType, Arc<Vec<u8>>>,
        missing_types: &[FallbackFontType],
    ) {
        #[cfg(target_os = "macos")]
        self.load_mac_fallbacks(fonts, missing_types);

        #[cfg(target_os = "windows")]
        self.load_windows_fallbacks(fonts, missing_types);

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        self.load_linux_fallbacks(fonts, missing_types);
    }

    /// Loads system fonts from well-known paths.
    pub fn load_system_fonts(&mut self) {
        let mut fonts = BTreeMap::new();

        // 1. First check the configured resource directory
        let resource_dir = crate::resource_dir("resources");
        let base_path = std::path::Path::new(&resource_dir).join("fonts");
        let mappings = [
            (crate::font::FallbackFontType::Serif, "serif.ttf"),
            (crate::font::FallbackFontType::SansSerif, "sans.ttf"),
            (crate::font::FallbackFontType::Monospace, "mono.ttf"),
            (crate::font::FallbackFontType::JapaneseSerif, "mincho.ttf"),
            (crate::font::FallbackFontType::JapaneseSans, "gothic.ttf"),
        ];

        for (ftype, filename) in mappings {
            if let Ok(data) = std::fs::read(base_path.join(filename)) {
                fonts.insert(ftype, Arc::new(data));
            }
        }

        // 2. Fallback to platform-specific well-known paths for missing fonts
        let missing_types: Vec<_> = [
            crate::font::FallbackFontType::Serif,
            crate::font::FallbackFontType::SansSerif,
            crate::font::FallbackFontType::Monospace,
            crate::font::FallbackFontType::JapaneseSerif,
            crate::font::FallbackFontType::JapaneseSans,
        ]
        .into_iter()
        .filter(|ft| !fonts.contains_key(ft))
        .collect();

        if !missing_types.is_empty() {
            self.load_platform_fallback_fonts(&mut fonts, &missing_types);
        }

        self.system_fonts = Arc::new(fonts);
    }
    /// Returns a reference to the internal arena.
    pub fn arena(&self) -> &PdfArena {
        &self.arena
    }

    /// Returns the handle to the document root (Catalog).
    pub fn root_handle(&self) -> &Handle<Object> {
        &self.root
    }

    /// Returns the catalog dictionary handle.
    pub fn catalog_handle(&self) -> Option<Handle<Object>> {
        Some(self.root)
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
            _ => Err(PdfError::Filter {
                filter: "None".into(),
                message: "Object is not a stream".into(),
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

    /// Page order swap operation (O(1) logical swap with immediate B-tree arena synchronization)
    pub fn swap_pages(&mut self, a: usize, b: usize) -> PdfResult<()> {
        if a >= self.pages.len() || b >= self.pages.len() {
            return Err(PdfError::Other("Index out of bounds".into()));
        }
        self.pages.swap(a, b);
        self.rebuild_page_tree_in_arena()?;
        Ok(())
    }

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

    fn build_page_tree_layer(
        &self,
        layer: &[Object],
        max_kids: usize,
    ) -> PdfResult<Vec<Object>> {
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

    /// Returns an on-demand, read-only virtual structured view of the pages tree.
    pub fn get_page_tree_view(&self, strategy: PageTreeStrategy) -> PageTreeView<'_> {
        match strategy {
            PageTreeStrategy::Flat => PageTreeView::Flat(&self.pages),
            PageTreeStrategy::Balanced { max_kids } => {
                Self::build_virtual_balanced_view(&self.pages, max_kids)
            }
        }
    }

    fn build_virtual_balanced_view(pages: &[Handle<Object>], max_kids: usize) -> PageTreeView<'_> {
        if pages.len() <= max_kids {
            PageTreeView::Flat(pages)
        } else {
            let mut nodes = Vec::new();
            for chunk in pages.chunks(max_kids) {
                nodes.push(Self::build_virtual_balanced_view(chunk, max_kids));
            }
            PageTreeView::Balanced { max_kids, nodes }
        }
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
    pub fn find_all_pages(&self) -> Vec<Handle<Object>> {
        let mut pages = Vec::new();
        if let Ok(root) = self.get_pages_root() {
            let _ = self.walk_pages_recursive(root, &mut pages, 0);
        }
        pages
    }

    fn walk_pages_recursive(
        &self,
        node_h: Handle<Object>,
        out: &mut Vec<Handle<Object>>,
        depth: usize,
    ) -> PdfResult<()> {
        if depth > 32 {
            return Err(PdfError::Other("Page tree depth limit exceeded".into()));
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

        let kids_key = self.arena.name("Kids");
        if let Some(kids_obj) = dict.get(&kids_key) {
            let ah = kids_obj
                .resolve(&self.arena)
                .as_array()
                .ok_or_else(|| PdfError::Other("Invalid Kids array".into()))?;
            if let Some(kids) = self.arena.get_array(ah) {
                for kid in kids {
                    if let Some(h) = kid.as_reference() {
                        let _ = self.walk_pages_recursive(h, out, depth + 1);
                    }
                }
            }
        }
        Ok(())
    }

    fn get_pages_root(&self) -> PdfResult<Handle<Object>> {
        let catalog_obj = self
            .arena
            .get_object(self.root)
            .ok_or_else(|| PdfError::Other("Missing document catalog".into()))?;
        let catalog = PdfCatalog::from_pdf_object(catalog_obj, &self.arena)?;
        Ok(catalog.pages)
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

    /// Returns high-level compliance information about the document.
    pub fn compliance_info(&self) -> PdfResult<conformance::ComplianceInfo> {
        let mut info = conformance::ComplianceInfo::default();

        let catalog_obj = self
            .arena
            .get_object(self.root)
            .ok_or_else(|| PdfError::Other("Missing document catalog".into()))?;
        let catalog = PdfCatalog::from_pdf_object(catalog_obj, &self.arena)?;

        // 1. Check for /StructTreeRoot
        info.has_struct_tree = catalog.struct_tree_root.is_some();

        // 2. Check for /MarkInfo -> /Marked true
        if let Some(mark_info_obj) = catalog.mark_info {
            let marked_key = self.arena.name("Marked");
            if let Some(mark_dict) = mark_info_obj
                .resolve(&self.arena)
                .as_dict_handle()
                .and_then(|h| self.arena.get_dict(h))
                && let Some(marked) =
                    mark_dict.get(&marked_key).and_then(|o| o.resolve(&self.arena).as_bool())
            {
                info.is_marked = marked;
            }
        }

        // 3. Extract Metadata Conformance
        let pdf_20 = catalog
            .version
            .and_then(|n| self.arena.get_name(n))
            .map(|n| n.as_str() == "2.0")
            .unwrap_or(false);

        if info.has_struct_tree && pdf_20 {
            info.metadata.pdf_ua_part = Some(2);
        }

        Ok(info)
    }

    /// Returns the handle to the Structure Tree Root dictionary, if it exists.
    pub fn get_structure_root(&self) -> PdfResult<Option<Handle<Object>>> {
        let catalog_obj = self
            .arena
            .get_object(self.root)
            .ok_or_else(|| PdfError::Other("Missing document catalog".into()))?;
        let catalog = PdfCatalog::from_pdf_object(catalog_obj, &self.arena)?;
        Ok(catalog.struct_tree_root)
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
        // Clear font cache to force re-parsing with potential new system fonts
        self.font_cache.write().clear();

        let (font_groups, best_to_unicode) = self.discover_font_groups();
        self.propagate_tounicode_mappings(font_groups, best_to_unicode);
        self.resolve_missing_font_data();
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

    fn resolve_missing_font_data(&self) {
        let system_fonts = self.system_fonts.clone();
        let cache = self.font_cache.clone();

        let mut cache_write = cache.write();
        for res in cache_write.values_mut() {
            let res_mut = Arc::make_mut(res);
            if res_mut.data.is_none()
                && let Some(ftype) = res_mut.fallback_type
                && let Some(sys_data) = system_fonts.get(&ftype)
            {
                res_mut.data = Some(Arc::clone(sys_data));
                let _ = res_mut.perform_reconstruction();
            }
        }
    }

    fn discover_font_groups(&self) -> (FontGroupMap, BestToUnicodeMap) {
        let arena = &self.arena;
        let mut font_groups = BTreeMap::new();
        let mut best_to_unicode = BTreeMap::new();
        let mut best_to_unicode_count = BTreeMap::new();

        let type_key = arena.name("Type");
        let font_val = arena.name("Font");
        let base_font_key = arena.name("BaseFont");
        let to_unicode_key = arena.name("ToUnicode");
        let _descendant_fonts_key = arena.name("DescendantFonts");

        for h in arena.all_dict_handles() {
            let Some(dict) = arena.get_dict(h) else { continue };
            if let Some(t_h) = dict.get(&type_key).and_then(|o| o.resolve(arena).as_name())
                && t_h == font_val
            {
                let base_font = dict
                    .get(&base_font_key)
                    .and_then(|o| o.resolve(arena).as_name())
                    .and_then(|h| arena.get_name_str(h))
                    .unwrap_or_else(|| "Untitled".to_string());
                let is_cid = dict.contains_key(&arena.name("DescendantFonts"));
                if is_cid {
                    let csi_str = self.extract_csi_string(&dict);
                    let key = (base_font, csi_str);
                    font_groups.entry(key.clone()).or_insert_with(Vec::new).push(h);

                    if let Some(tu) = dict.get(&to_unicode_key)
                        && let Ok(data) = self.decode_stream(&tu.resolve(arena))
                        && let Ok(m) = crate::font::cmap::CMap::parse(&data)
                    {
                        let count = m.mappings.len();
                        use std::collections::btree_map::Entry;
                        match best_to_unicode_count.entry(key.clone()) {
                            Entry::Vacant(e) => {
                                e.insert(count);
                                best_to_unicode.insert(key, tu.clone());
                            }
                            Entry::Occupied(mut e) => {
                                if count > *e.get() {
                                    e.insert(count);
                                    best_to_unicode.insert(key, tu.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
        (font_groups, best_to_unicode)
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

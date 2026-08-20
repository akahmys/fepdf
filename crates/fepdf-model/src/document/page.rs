use crate::handle::Handle;
use crate::{FromPdfObject, Object, PdfArena, PdfName};
use std::collections::BTreeMap;

/// A high-level representation of a PDF page.
///
/// PDF Page Dictionary (ISO 32000-2:2020 Clause 7.7.3.3)
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "7.7.3.3")]
pub struct PdfPageDict {
    #[pdf_key("Type")]
    /// `/Type`, always `Page`.
    pub kind: PdfName,
    #[pdf_key("Parent")]
    /// `/Parent`: the enclosing page-tree node.
    pub parent: Handle<crate::Object>,
    #[pdf_key("Contents")]
    /// `/Contents`: one content stream, or an array of them.
    pub contents: Option<crate::Object>, // Single stream or array
    #[pdf_key("Resources")]
    /// `/Resources`: fonts, images and other resources this page names.
    pub resources: Option<crate::Object>,
    #[pdf_key("MediaBox")]
    /// `/MediaBox`: the page's physical extent.
    pub media_box: Option<crate::graphics::Rect>,
    #[pdf_key("Annots")]
    /// `/Annots`: annotations attached to this page.
    pub annots: Option<Handle<Vec<crate::Object>>>,
}

/// A page together with the arena and parent chain needed to resolve it.
pub struct Page<'a> {
    arena: &'a PdfArena,
    obj_handle: Handle<Object>,
    parent_chain: Vec<Handle<Object>>,
}

impl<'a> Page<'a> {
    /// Binds a page handle to its arena and inherited parent chain.
    pub fn new(
        arena: &'a PdfArena,
        obj_handle: Handle<Object>,
        parent_chain: Vec<Handle<Object>>,
    ) -> Self {
        Self { arena, obj_handle, parent_chain }
    }

    /// Reads an attribute from the page, then up the inheritance chain.
    pub fn get_attribute(&self, name: &str) -> Option<Object> {
        let name_handle = self.arena.get_name_by_str(name)?;

        // 1. Check local page dictionary
        if let Some(Object::Dictionary(dh)) = self.arena.get_object(self.obj_handle)
            && let Some(dict) = self.arena.get_dict(dh)
            && let Some(val) = dict.get(&name_handle)
        {
            return Some(val.clone());
        }

        // 2. Check parent chain (Page Tree nodes)
        for &parent_obj_handle in self.parent_chain.iter().rev() {
            if let Some(Object::Dictionary(dh)) = self.arena.get_object(parent_obj_handle)
                && let Some(parent_dict) = self.arena.get_dict(dh)
                && let Some(val) = parent_dict.get(&name_handle)
            {
                return Some(val.clone());
            }
        }

        None
    }

    /// Resolves a page attribute, following the inheritance chain and resolving references.
    pub fn resolve_attribute(&self, name: &str) -> Option<Object> {
        self.get_attribute(name).map(|o| o.resolve(self.arena))
    }

    /// Returns the handle to the page dictionary.
    pub fn obj_handle(&self) -> Handle<Object> {
        self.obj_handle
    }

    /// Returns the current pool handle to the page resources.
    pub fn resources_handle(&self) -> Handle<BTreeMap<Handle<PdfName>, Object>> {
        self.resolve_attribute("Resources")
            .and_then(|o| o.as_dict_handle())
            .unwrap_or_else(|| self.arena.alloc_dict(BTreeMap::new()))
    }

    /// Returns the handle(s) to the page contents.
    pub fn contents_handles(&self) -> Vec<Handle<Object>> {
        match self.get_attribute("Contents") {
            Some(Object::Array(h)) => self
                .arena
                .get_array(h)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|o| o.as_reference())
                .collect(),
            Some(Object::Reference(h)) => vec![h],
            Some(stream_obj @ Object::Stream(_, _)) => {
                // If it's a direct stream, we need to find its handle in the arena.
                self.arena.find_object(&stream_obj).into_iter().collect()
            }
            _ => Vec::new(),
        }
    }

    /// Returns the page media box.
    pub fn media_box(&self) -> crate::graphics::Rect {
        self.resolve_attribute("MediaBox")
            .and_then(|o| crate::graphics::Rect::from_pdf_object(o, self.arena).ok())
            .unwrap_or_else(|| crate::graphics::Rect::new(0.0, 0.0, 612.0, 792.0)) // Default Letter
    }
}

/// PDF Annotation Dictionary (12.5).
///
/// Defined in [`crate::annotation`] and re-exported here, where it used to live. It held
/// seven of Table 166's nineteen entries and knew nothing of any subtype; the entries it
/// now reads, and the survey that decided which subtypes get readers, are in that module.
pub use crate::annotation::PdfAnnotation;

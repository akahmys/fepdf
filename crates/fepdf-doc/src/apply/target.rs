//! What a content-rewriting edit works on: a page's content, or a form XObject's
//! (8.10), each with the resources its content names things in.
//!
//! **A redaction reaches into forms** (ROADMAP Y-10): a form a page draws holds text,
//! images and paths like the page does, and leaving them is leaving what the region
//! covers. The steps that rewrite a page's content take one of these instead of a page
//! number, so the same steps rewrite a form.

use fepdf_model::object::SublimatedData;
use fepdf_model::{DictHandle, Document, Handle, Object, PdfError, PdfResult};
use std::sync::Arc;

/// A content stream and the resources it names things in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// A page's content: all its `/Contents`, as one.
    Page(usize),
    /// A form XObject's stream, and the resources of what draws it, which a form with
    /// none of its own reads by (7.8.3).
    Form(Handle<Object>, DictHandle),
}

impl Target {
    /// The content, decoded; nothing when there is none.
    ///
    /// # Errors
    /// Fails when the page is not there or a stream cannot be decoded.
    pub fn content(self, doc: &Document) -> PdfResult<Option<bytes::Bytes>> {
        match self {
            Self::Page(page) => super::text::page_content(doc, page),
            Self::Form(form, _) => {
                let Some(stream) = doc.arena().get_object(form) else { return Ok(None) };
                doc.decode_stream(&stream).map(Some)
            }
        }
    }

    /// Puts `content` in place of what [`Self::content`] read.
    ///
    /// # Errors
    /// Fails when the page or the form is not there.
    pub fn write(self, doc: &Document, content: Vec<u8>) -> PdfResult<()> {
        match self {
            Self::Page(page) => super::text::write_page_content(doc, page, content),
            Self::Form(form, _) => {
                let arena = doc.arena();
                let Some(Object::Stream(dict_h, _)) = arena.get_object(form) else {
                    return Err(PdfError::refused("rewrite a form", "it is not a stream"));
                };
                let mut dict = arena.get_dict(dict_h).unwrap_or_default();
                // Written back decoded, so what described the encoding goes with it.
                for key in ["Filter", "DecodeParms", "Length"] {
                    dict.remove(&arena.name(key));
                }
                let data = Arc::new(SublimatedData::Raw(bytes::Bytes::from(content)));
                arena.set_object(form, Object::Stream(arena.alloc_dict(dict), data));
                Ok(())
            }
        }
    }

    /// The resources the content names things in: the page's, settled on the page where
    /// it inherits them, or the form's own.
    ///
    /// # Errors
    /// Fails when the page is not there, or the form has no resources of its own — which a
    /// redaction gives every form it copies, so that what it adds stays with the copy.
    pub fn resources(self, doc: &Document) -> PdfResult<DictHandle> {
        let arena = doc.arena();
        match self {
            Self::Page(page) => {
                let page_h = doc.page_handle(page)?;
                let page_dh = doc.resolve_to_dict(page_h)?;
                let mut page_dict = arena.get_dict(page_dh).unwrap_or_default();
                let resources =
                    super::annotations::ensure_page_resources(doc, page_h, &mut page_dict);
                arena.set_dict(page_dh, page_dict);
                Ok(resources)
            }
            Self::Form(form, _) => arena
                .get_object(form)
                .and_then(|o| o.as_dict_handle())
                .and_then(|d| arena.dict_entry(d, arena.name("Resources")))
                .and_then(|r| r.resolve(arena).as_dict_handle())
                .ok_or_else(|| {
                    PdfError::refused("rewrite a form", "it has no resources of its own")
                }),
        }
    }

    /// The resources the content names things in, read without settling anything: for a
    /// reader that writes nothing. A form with none of its own reads by what draws it.
    ///
    /// # Errors
    /// As [`Self::resources`].
    pub fn resources_read(self, doc: &Document) -> PdfResult<DictHandle> {
        match self {
            Self::Page(page) => {
                let arena = doc.arena();
                let page_h = doc.page_handle(page)?;
                Ok(fepdf_model::Page::new(arena, page_h, doc.get_parent_chain(page_h))
                    .resources_handle())
            }
            Self::Form(_, inherited) => Ok(self.resources(doc).unwrap_or(inherited)),
        }
    }
}

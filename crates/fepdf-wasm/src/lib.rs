//! fepdf WASM: WebAssembly bridge for the fepdf PDF engine.
//!
//! Provides a JavaScript-friendly interface for document loading and rendering.

use bytes::Bytes;
use fepdf::PdfDocument as SdkDocument;
use wasm_bindgen::prelude::*;

/// A JavaScript-friendly wrapper for a PDF document.
#[wasm_bindgen]
pub struct PdfDocument {
    inner: SdkDocument,
}

#[wasm_bindgen]
impl PdfDocument {
    /// Opens a PDF document from a byte array.
    #[wasm_bindgen(constructor)]
    pub fn new(data: &[u8]) -> Result<PdfDocument, JsValue> {
        console_error_panic_hook::set_once();
        let bytes = Bytes::copy_from_slice(data);
        let inner = SdkDocument::open(bytes).map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(PdfDocument { inner })
    }

    /// Returns the total number of pages in the document.
    #[wasm_bindgen(getter)]
    pub fn page_count(&self) -> Result<usize, JsValue> {
        self.inner.page_count().map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Renders a specific page to a canvas — or rather, says that it does not.
    ///
    /// # Errors
    /// Always. See [`render_page_refusal`].
    pub fn render_page(&self, index: usize, canvas_id: &str) -> Result<(), JsValue> {
        Err(JsValue::from_str(&render_page_refusal(index, canvas_id)))
    }
}

/// Why `render_page` refuses, in terms a caller can act on.
///
/// **It used to return `Ok(())` having drawn nothing**, which is worse than being
/// unimplemented: a caller was told the page had been rendered and got a blank canvas,
/// with nothing anywhere to say otherwise. Not being able to do something is a fact about
/// this crate; reporting success for it is a fact about the caller's next hour.
///
/// Rendering here needs a WebGPU surface through `web-sys` and the facade's `render`
/// feature, and neither is present. It is not written as a stub that might one day fill
/// in, because a stub is what this was: the comment saying implementation "will involve"
/// a WebGPU surface had been there long enough for the roadmap to find it by measurement
/// rather than by memory.
///
/// Separate from the `wasm_bindgen` method so it can be tested on the host, which
/// `JsValue` cannot be — the same shape `fepdf-mcp` uses for its tools.
#[must_use]
pub fn render_page_refusal(index: usize, canvas_id: &str) -> String {
    format!(
        "fepdf-wasm cannot render: page {index} was not drawn to {canvas_id:?}. \
         This build has no WebGPU surface and does not enable the facade's `render` \
         feature. Use page_count and the text APIs, or render outside the browser."
    )
}

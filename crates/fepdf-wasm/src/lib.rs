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

    /// Renders a specific page to a Canvas 2D context.
    ///
    /// Note: This is a placeholder for the WebGPU/WebGL rendering pipeline.
    pub fn render_page(&self, _index: usize, _canvas_id: &str) -> Result<(), JsValue> {
        // Implementation will involve setting up a WebGPU surface via web-sys
        // and calling the vello renderer.
        Ok(())
    }
}

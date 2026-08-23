//! `render_page` must refuse, and say so.
//!
//! It returned `Ok(())` having drawn nothing, which is worse than being unimplemented: a
//! caller was told the page had been rendered and got a blank canvas, with nothing
//! anywhere to say otherwise. This is the guard against that coming back — a future
//! `Ok(())` here fails, and an actual renderer is expected to delete this file rather
//! than to make it pass.
//!
//! Tests the message rather than the `wasm_bindgen` method, because `JsValue` cannot be
//! constructed off a WebAssembly target and this suite runs on the host.

use fepdf_wasm::render_page_refusal;

#[test]
fn the_refusal_names_what_was_not_drawn() {
    let message = render_page_refusal(3, "viewer-canvas");
    assert!(message.contains('3'), "the page it did not draw: {message}");
    assert!(message.contains("viewer-canvas"), "the canvas it did not draw to: {message}");
}

#[test]
fn the_refusal_says_why_and_what_to_do_instead() {
    let message = render_page_refusal(0, "c");
    assert!(message.contains("cannot render"), "unambiguous: {message}");
    assert!(
        message.contains("WebGPU") || message.contains("render"),
        "names the missing piece: {message}"
    );
}

//! Small Rust-generated images must own JS pixels before another Wasm allocation.
//! Worker/canvas ImageData already owns its pixels and bypasses this helper.
use wasm_bindgen::JsValue;

/// RGBA8 rows, top first. A borrowed Wasm view can be detached by memory.grow
/// before the deferred upload, or reused when the source Vec is dropped.
pub fn image_data(width: u32, height: u32, rgba: &[u8]) -> Result<web_sys::ImageData, JsValue> {
    let pixels = js_sys::Uint8ClampedArray::from(rgba);
    web_sys::ImageData::new_with_js_u8_clamped_array_and_sh(&pixels, width, height)
}

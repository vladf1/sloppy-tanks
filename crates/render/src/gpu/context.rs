//! What both backends share about the browser's graphics API: the first GPU failure,
//! surfaced by the next `render` call (the page stops and shows it instead of drawing
//! on), and the API's name.

use std::sync::{Arc, Mutex};

/// The first GPU failure, shared with the backend's callbacks.
#[derive(Clone, Default)]
pub struct ErrorSlot(Arc<Mutex<Option<String>>>);

impl ErrorSlot {
    pub fn set(&self, message: String) {
        let mut slot = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if slot.is_none() {
            #[cfg(target_arch = "wasm32")]
            web_sys::console::error_1(&message.clone().into());
            #[cfg(not(target_arch = "wasm32"))]
            eprintln!("{message}");
            *slot = Some(message);
        }
    }

    pub fn get(&self) -> Option<String> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

/// The graphics API this build draws with; startup errors and Stats for nerds name it,
/// and the page tells an unavailable one from other failures by the prefix
/// (`src/engine.ts`). Natively wgpu offers each system one backend (`BACKENDS`).
pub const GRAPHICS_API: &str = if cfg!(feature = "webgl") {
    "WebGL"
} else if cfg!(target_arch = "wasm32") {
    "WebGPU"
} else if cfg!(target_os = "macos") {
    "Metal"
} else if cfg!(target_os = "windows") {
    "Direct3D 12"
} else {
    "Graphics"
};

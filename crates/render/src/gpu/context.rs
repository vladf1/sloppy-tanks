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
            web_sys::console::error_1(&message.clone().into());
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

/// The browser API this build draws with; startup errors name it, and the page
/// tells an unavailable one from other failures by the prefix (`src/engine.ts`).
pub const GRAPHICS_API: &str = if cfg!(feature = "webgl") {
    "WebGL"
} else {
    "WebGPU"
};

//! What the page around the shell (`index.html`) hears from it. The shell draws
//! everything on one canvas, so the page's loading view and the measurement scripts
//! learn its progress here:
//!
//! - the root element's `data-shell` attribute: `ready` once the shell has presented
//!   its first frame (the loading view then goes), `failed` once it stopped;
//! - a `sloppy:failed` event on `window` whose `detail` says why, in words, which the
//!   loading view shows with a retry;
//! - performance marks: `sloppy:first-frame` (once), `sloppy:arena-ready` (each time
//!   Battle Setup's arena has finished preparing) and `sloppy:round-start` (each GO).
//!
//! The page's own marks (`index.html`) come before these. Natively there is no page and
//! these do nothing.

#[cfg(target_arch = "wasm32")]
use std::cell::Cell;

#[cfg(target_arch = "wasm32")]
thread_local! {
    /// The first frame has been presented and the page told.
    static PRESENTED: Cell<bool> = const { Cell::new(false) };
}

/// A frame has been presented: the first one hands the page to the shell.
pub fn frame_presented() {
    #[cfg(target_arch = "wasm32")]
    if !PRESENTED.replace(true) {
        set_state("ready");
        mark("sloppy:first-frame");
    }
}

/// Battle Setup's arena has finished preparing.
pub fn arena_ready() {
    #[cfg(target_arch = "wasm32")]
    mark("sloppy:arena-ready");
}

/// A single-player round has started.
pub fn round_started() {
    #[cfg(target_arch = "wasm32")]
    mark("sloppy:round-start");
}

/// The shell has stopped, for `reason`: the page says so in place of the game.
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
pub fn failed(reason: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        set_state("failed");
        let init = web_sys::CustomEventInit::new();
        init.set_detail(&reason.into());
        if let Some(window) = web_sys::window()
            && let Ok(event) =
                web_sys::CustomEvent::new_with_event_init_dict("sloppy:failed", &init)
        {
            let _ = window.dispatch_event(&event);
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn set_state(state: &str) {
    if let Some(root) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.document_element())
    {
        let _ = root.set_attribute("data-shell", state);
    }
}

#[cfg(target_arch = "wasm32")]
fn mark(name: &str) {
    if let Some(performance) = web_sys::window().and_then(|window| window.performance()) {
        let _ = performance.mark(name);
    }
}

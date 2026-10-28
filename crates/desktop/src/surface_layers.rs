//! A window's surface handed from one engine to the next. On macOS wgpu draws into a
//! `CAMetalLayer` it adds as a sublayer of the view's layer (`raw-window-metal`), and
//! dropping the surface leaves that layer attached (the view's layer retains it, and it
//! keeps observing the view's bounds), so every room joined or left would stack another
//! dead layer under the live one. Remove them once the old engine is gone, before the
//! next one makes its own.

use winit::window::Window;

/// Detach the Metal layers a dropped surface left on `window`'s view. Call only while
/// no renderer owns the window.
#[cfg(target_os = "macos")]
pub fn release(window: &Window) {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return;
    };
    let Some(metal_layer) = AnyClass::get(c"CAMetalLayer") else {
        return;
    };
    let view = handle.ns_view.as_ptr().cast::<AnyObject>();
    // SAFETY: `ns_view` is the window's live NSView (we are on the main thread, where
    // winit runs); `layer`, `sublayers`, `count`, `objectAtIndex:`, `isKindOfClass:` and
    // `removeFromSuperlayer` are the documented AppKit/Core Animation messages, and the
    // sublayers are retained while we walk a copy of the list.
    unsafe {
        let root: Option<Retained<AnyObject>> = msg_send![view, layer];
        let Some(root) = root else { return };
        let sublayers: Option<Retained<AnyObject>> = msg_send![&*root, sublayers];
        let Some(sublayers) = sublayers else { return };
        let count: usize = msg_send![&*sublayers, count];
        let layers: Vec<Retained<AnyObject>> = (0..count)
            .map(|index| msg_send![&*sublayers, objectAtIndex: index])
            .collect();
        for layer in layers {
            let stale: bool = msg_send![&*layer, isKindOfClass: metal_layer];
            if stale {
                let _: () = msg_send![&*layer, removeFromSuperlayer];
            }
        }
    }
}

/// Other platforms create and destroy their swap chains with the surface.
#[cfg(not(target_os = "macos"))]
pub fn release(_window: &Window) {}

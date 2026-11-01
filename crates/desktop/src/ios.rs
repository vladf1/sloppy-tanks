//! What the iPhone and iPad need beyond winit 0.30. Apps built with the iOS 27 SDK
//! launch only with the scene life cycle (the bundle's `UIApplicationSceneManifest`),
//! and UIKit shows a window only once it belongs to a scene; winit creates its
//! `UIWindow` from the screen's bounds with none, so the window joins the app's scene
//! here. winit also hears a hardware keyboard only as text, so its view controller
//! takes the keyboard's presses (`hardware_keys.rs` turns them into game keys).

use winit::window::Window;

/// Whether the app runs on an iPhone, which plays the phone edition: UIKit's phone
/// idiom, as the one bundle for both (`UIDeviceFamily` 1 and 2) reports it on each.
pub fn is_phone() -> bool {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};

    /// `UIUserInterfaceIdiomPhone`.
    const PHONE_IDIOM: isize = 0;
    let Some(device) = AnyClass::get(c"UIDevice") else {
        return false;
    };
    // SAFETY: `currentDevice` and `userInterfaceIdiom` are documented UIKit messages,
    // sent from the main thread, where `main` runs.
    unsafe {
        let current: Retained<AnyObject> = msg_send![device, currentDevice];
        let idiom: isize = msg_send![&*current, userInterfaceIdiom];
        idiom == PHONE_IDIOM
    }
}

/// Put the window winit made into the app's window scene and show it. Call once,
/// right after creating the window (winit's `resumed` comes after the scene is active).
pub fn attach_to_scene(window: &Window) {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::UiKit(handle) = handle.as_raw() else {
        return;
    };
    let (Some(application), Some(window_scene)) = (
        AnyClass::get(c"UIApplication"),
        AnyClass::get(c"UIWindowScene"),
    ) else {
        return;
    };
    let view = handle.ui_view.as_ptr().cast::<AnyObject>();
    // SAFETY: `ui_view` is winit's live view and we are on the main thread, where winit
    // runs; `window`, `sharedApplication`, `connectedScenes`, `allObjects`, `count`,
    // `objectAtIndex:`, `isKindOfClass:`, `setWindowScene:`, `coordinateSpace`,
    // `bounds`, `setFrame:` and `makeKeyAndVisible` are documented UIKit/Foundation
    // messages, and every object is retained while it is used.
    unsafe {
        let ui_window: Option<Retained<AnyObject>> = msg_send![view, window];
        let Some(ui_window) = ui_window else { return };
        let app: Retained<AnyObject> = msg_send![application, sharedApplication];
        let scenes: Retained<AnyObject> = msg_send![&*app, connectedScenes];
        let scenes: Retained<AnyObject> = msg_send![&*scenes, allObjects];
        let count: usize = msg_send![&*scenes, count];
        for index in 0..count {
            let scene: Retained<AnyObject> = msg_send![&*scenes, objectAtIndex: index];
            let is_window_scene: bool = msg_send![&*scene, isKindOfClass: window_scene];
            if !is_window_scene {
                continue;
            }
            let _: () = msg_send![&*ui_window, setWindowScene: &*scene];
            // The window was sized from the screen before it had a scene; the scene's
            // bounds are the interface's, turned the way the device is held.
            let space: Retained<AnyObject> = msg_send![&*scene, coordinateSpace];
            let bounds: objc2_core_foundation::CGRect = msg_send![&*space, bounds];
            let _: () = msg_send![&*ui_window, setFrame: bounds];
            let _: () = msg_send![&*ui_window, makeKeyAndVisible];
            return;
        }
    }
}

thread_local! {
    /// Keys the view controller heard go down (true) or up since the shell last read
    /// them, by USB HID usage (`UIKeyboardHIDUsage`).
    static PRESSES: std::cell::RefCell<Vec<(isize, bool)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The view controller class `listen_for_keys` taught to take presses.
static LISTENING_CLASS: std::sync::OnceLock<usize> = std::sync::OnceLock::new();

/// The keyboard's presses and releases since the last call.
pub fn take_presses() -> Vec<(isize, bool)> {
    PRESSES.with_borrow_mut(std::mem::take)
}

/// Let winit's view controller hear a hardware keyboard. winit 0.30 hears one only as
/// text while a field is typed in, never as keys going down and up, so its view
/// controller gets UIKit's press handlers (which carry both, with the key's HID usage)
/// and may become first responder: no text field to show the on-screen keyboard for.
pub fn listen_for_keys(window: &Window) {
    use objc2::ffi;
    use objc2::runtime::{AnyObject, Bool, Imp, Sel};
    use objc2::sel;

    type Presses = extern "C-unwind" fn(&AnyObject, Sel, &AnyObject, *mut AnyObject);
    extern "C-unwind" fn began(
        this: &AnyObject,
        _: Sel,
        presses: &AnyObject,
        event: *mut AnyObject,
    ) {
        record(this, presses, event, Phase::Began);
    }
    extern "C-unwind" fn ended(
        this: &AnyObject,
        _: Sel,
        presses: &AnyObject,
        event: *mut AnyObject,
    ) {
        record(this, presses, event, Phase::Ended);
    }
    extern "C-unwind" fn cancelled(
        this: &AnyObject,
        _: Sel,
        presses: &AnyObject,
        event: *mut AnyObject,
    ) {
        record(this, presses, event, Phase::Cancelled);
    }
    extern "C-unwind" fn can_become_first_responder(_: &AnyObject, _: Sel) -> Bool {
        Bool::YES
    }

    let Some(controller) = view_controller(window) else {
        return;
    };
    // SAFETY: the controller is winit's live `WinitUIViewController` on the main thread.
    // Each method added has the signature and type encoding UIKit declares for its
    // selector (`v@:@@`, `B@:`); `class_addMethod` adds them to winit's subclass only,
    // and only once, since a second call finds them there and does nothing.
    unsafe {
        let class = ffi::object_getClass(controller).cast_mut();
        LISTENING_CLASS.get_or_init(|| class as usize);
        let press = |handler: Presses| std::mem::transmute::<Presses, Imp>(handler);
        ffi::class_addMethod(
            class,
            sel!(pressesBegan:withEvent:),
            press(began),
            c"v@:@@".as_ptr(),
        );
        ffi::class_addMethod(
            class,
            sel!(pressesEnded:withEvent:),
            press(ended),
            c"v@:@@".as_ptr(),
        );
        ffi::class_addMethod(
            class,
            sel!(pressesCancelled:withEvent:),
            press(cancelled),
            c"v@:@@".as_ptr(),
        );
        let answer = std::mem::transmute::<extern "C-unwind" fn(&AnyObject, Sel) -> Bool, Imp>(
            can_become_first_responder,
        );
        ffi::class_addMethod(
            class,
            sel!(canBecomeFirstResponder),
            answer,
            c"B@:".as_ptr(),
        );
    }
}

/// Which of UIKit's press handlers ran.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Began,
    Ended,
    Cancelled,
}

/// Note each press's key, then hand the presses to UIKit's own handler, which passes
/// them along the responder chain as before.
fn record(
    this: &objc2::runtime::AnyObject,
    presses: &objc2::runtime::AnyObject,
    event: *mut objc2::runtime::AnyObject,
    phase: Phase,
) {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};

    // SAFETY: `presses` is the `NSSet<UIPress *>` UIKit passes; `allObjects`, `count`,
    // `objectAtIndex:`, `key` (nil for a press without one) and `keyCode` are documented
    // messages. The superclass is UIKit's `UIViewController`, whose handler takes the
    // same arguments.
    unsafe {
        let all: Retained<AnyObject> = msg_send![presses, allObjects];
        let count: usize = msg_send![&*all, count];
        for index in 0..count {
            let press: Retained<AnyObject> = msg_send![&*all, objectAtIndex: index];
            let key: Option<Retained<AnyObject>> = msg_send![&*press, key];
            if let Some(key) = key {
                let usage: isize = msg_send![&*key, keyCode];
                PRESSES.with_borrow_mut(|presses| presses.push((usage, phase == Phase::Began)));
            }
        }
        let Some(&class) = LISTENING_CLASS.get() else {
            return;
        };
        let superclass = objc2::ffi::class_getSuperclass(class as *const AnyClass);
        let Some(superclass) = superclass.as_ref() else {
            return;
        };
        let event = event.cast_const();
        let _: () = match phase {
            Phase::Began => {
                msg_send![super(this, superclass), pressesBegan: presses, withEvent: event]
            }
            Phase::Ended => {
                msg_send![super(this, superclass), pressesEnded: presses, withEvent: event]
            }
            Phase::Cancelled => {
                msg_send![super(this, superclass), pressesCancelled: presses, withEvent: event]
            }
        };
    }
}

/// Keep the view controller first responder, so the keyboard's presses reach it,
/// whenever winit's view is not (it is while a text field is typed in).
pub fn hold_key_focus(window: &Window) {
    use objc2::msg_send;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::UiKit(handle) = handle.as_raw() else {
        return;
    };
    let Some(controller) = view_controller(window) else {
        return;
    };
    let view = handle.ui_view.as_ptr().cast::<objc2::runtime::AnyObject>();
    // SAFETY: winit's live view and view controller, on the main thread;
    // `isFirstResponder` and `becomeFirstResponder` are documented UIResponder messages.
    unsafe {
        let typing: bool = msg_send![view, isFirstResponder];
        let listening: bool = msg_send![controller, isFirstResponder];
        if !typing && !listening {
            let _: bool = msg_send![controller, becomeFirstResponder];
        }
    }
}

/// winit's view controller for `window`.
fn view_controller(window: &Window) -> Option<&objc2::runtime::AnyObject> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let handle = window.window_handle().ok()?;
    let RawWindowHandle::UiKit(handle) = handle.as_raw() else {
        return None;
    };
    let controller = handle.ui_view_controller?;
    // SAFETY: winit keeps the controller alive as long as its window, which outlives
    // the returned reference's borrow of `window`.
    Some(unsafe { controller.cast::<objc2::runtime::AnyObject>().as_ref() })
}

//! The cursor lock that lets the mouse turn the first-person view (`controls.ts`'s
//! pointer capture). While first person steers the cursor is locked. Esc with the cursor
//! locked only frees it, as a browser spends Esc on releasing a lock, so the first Esc
//! frees the cursor and the second pauses; the view then holds still, the cursor reaches
//! the HUD, and the next click on the arena takes the lock back without firing. A death
//! keeps the lock, so the respawn needs no click (a browser re-locks only after one, and
//! shows its notice again), and Esc frees it to pick the next tank.

#[cfg(not(target_arch = "wasm32"))]
use winit::window::CursorGrabMode;
use winit::window::Window;

/// A browser may release the lock on Esc just before it delivers the key; a key this
/// soon after the release belongs to the same press.
const ESC_RELEASE_WINDOW_MS: f64 = 250.0;
/// A lock the browser has granted before but not this time within this long was
/// refused (it wants a click first): aiming waits for one.
const LOCK_ANSWER_MS: f64 = 1000.0;

/// What the window's cursor should do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grab {
    Lock,
    Release,
}

#[derive(Debug)]
pub struct PointerLock {
    /// The cursor is locked, or its lock was asked for and not refused.
    held: bool,
    /// A page's browser has reported this lock in place.
    confirmed: bool,
    /// A page's browser has granted a lock before: one it does not grant was refused.
    works: bool,
    /// When the lock was last asked for, ms.
    requested_at: f64,
    /// First person wanted the lock when the player or the browser freed it: aiming
    /// waits for a click on the arena.
    freed: bool,
    /// When the browser let the lock go by itself (Esc, a window switch), ms.
    released_at: f64,
    /// The browser let the last lock go by itself: it locks again only for a click.
    browser_let_go: bool,
    /// The last frame wanted the lock: first person steers a running battle.
    wanted: bool,
}

impl Default for PointerLock {
    fn default() -> Self {
        Self {
            held: false,
            confirmed: false,
            works: false,
            requested_at: f64::NEG_INFINITY,
            freed: false,
            released_at: f64::NEG_INFINITY,
            browser_let_go: false,
            wanted: false,
        }
    }
}

impl PointerLock {
    /// The cursor is locked: the mouse turns the view and egui sees no pointer.
    pub fn held(&self) -> bool {
        self.held
    }

    /// First person steers with a free cursor: the view holds still and the next click
    /// on the arena takes the lock back (the HUD says so).
    pub fn waits_for_click(&self) -> bool {
        self.wanted && self.freed
    }

    /// This frame's wish: `wanted` while first person steers a running battle with no
    /// menu open, `steering` while the player can act (a death keeps a held lock but
    /// asks for none). `browser` is whether a page's browser holds the lock now (natively
    /// `None`: the window's grab is the lock). `now` in ms.
    pub fn frame(
        &mut self,
        wanted: bool,
        steering: bool,
        browser: Option<bool>,
        now: f64,
    ) -> Option<Grab> {
        self.wanted = wanted;
        if let Some(locked) = browser
            && self.held
        {
            if locked {
                self.confirmed = true;
                self.works = true;
            } else if self.confirmed || (self.works && now - self.requested_at > LOCK_ANSWER_MS) {
                // The browser let it go (Esc, a window switch) or refused it: the round
                // plays on and aiming waits for a click.
                if self.confirmed {
                    self.released_at = now;
                }
                self.browser_let_go = true;
                self.held = false;
                self.confirmed = false;
                self.freed = wanted;
                return Some(Grab::Release);
            }
        }
        if !wanted {
            // Leaving first person, a menu or a hidden window gives the cursor back, to
            // lock again as soon as first person steers; a browser that let go of it by
            // itself locks again only for a click.
            self.freed &= self.browser_let_go;
            return self.held.then(|| self.release());
        }
        (steering && !self.held && !self.freed).then(|| self.lock(now))
    }

    /// Esc went down: the release to make when it only frees the cursor (it is locked,
    /// or the browser just let it go for this same press), so it must not pause; `None`
    /// when it is the shell's Esc.
    pub fn escape(&mut self, now: f64) -> Option<Grab> {
        if !self.held && now - self.released_at >= ESC_RELEASE_WINDOW_MS {
            return None;
        }
        // This press is spent; the next Esc opens the menu.
        self.released_at = f64::NEG_INFINITY;
        self.freed = self.wanted;
        Some(if self.held {
            self.release()
        } else {
            Grab::Release
        })
    }

    /// A mouse button went down on the arena (not on the HUD); a lock when it takes the
    /// freed cursor back, and then the press only aims: it does not fire.
    pub fn press(&mut self, steering: bool, now: f64) -> Option<Grab> {
        (self.waits_for_click() && steering).then(|| {
            self.freed = false;
            self.lock(now)
        })
    }

    /// The window could not lock the cursor (natively: no pointer to lock).
    pub fn refused(&mut self) {
        self.held = false;
    }

    fn lock(&mut self, now: f64) -> Grab {
        self.held = true;
        self.confirmed = false;
        self.browser_let_go = false;
        self.requested_at = now;
        Grab::Lock
    }

    fn release(&mut self) -> Grab {
        self.held = false;
        self.confirmed = false;
        Grab::Release
    }
}

/// Lock the window's cursor or give it back. Natively a lock the window refuses (no
/// pointer to lock) leaves the cursor free: `false`.
pub fn apply(window: &Window, grab: Grab) -> bool {
    match grab {
        Grab::Lock => {
            let captured = request_lock(window);
            if captured {
                window.set_cursor_visible(false);
            }
            captured
        }
        Grab::Release => {
            release(window);
            window.set_cursor_visible(true);
            true
        }
    }
}

/// Give the cursor back. iPadOS Safari has no Pointer Lock at all, and winit's release
/// calls `document.exitPointerLock()` regardless: the error it throws there unwinds
/// through the shell mid-event and stops it for good, so a page calls it only where the
/// browser has it.
pub fn release(window: &Window) {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = window;
        if let Some(document) = web_sys::window().and_then(|window| window.document())
            && let Some(exit) = method(&document, "exitPointerLock")
        {
            let _ = exit.call0(&document);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = window.set_cursor_grab(CursorGrabMode::None);
    }
}

/// `target`'s method `name`, where the browser has it.
#[cfg(target_arch = "wasm32")]
fn method(target: &wasm_bindgen::JsValue, name: &str) -> Option<js_sys::Function> {
    use wasm_bindgen::JsCast;
    js_sys::Reflect::get(target, &name.into())
        .ok()
        .and_then(|method| method.dyn_into().ok())
}

/// Ask for the lock. A page's browser answers later and may refuse (it wants a click,
/// or the window's focus): the refusal is expected, so it is caught rather than left on
/// the console as an unhandled rejection, as the old page caught it.
fn request_lock(window: &Window) -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        use js_sys::Function;
        use wasm_bindgen::{JsCast, JsValue, prelude::Closure};
        use winit::platform::web::WindowExtWebSys;
        thread_local! {
            /// One handler that drops every refusal.
            static IGNORE: Function = Closure::<dyn FnMut(JsValue)>::new(|_| {})
                .into_js_value()
                .unchecked_into();
        }
        let Some(canvas) = window.canvas() else {
            return false;
        };
        let Some(answer) =
            method(&canvas, "requestPointerLock").and_then(|request| request.call0(&canvas).ok())
        else {
            return false;
        };
        // Older browsers answer with events only.
        if let Some(catch) = method(&answer, "catch") {
            IGNORE.with(|ignore| {
                let _ = catch.call1(&answer, ignore);
            });
        }
        true
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        window
            .set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
            .is_ok()
    }
}

/// Whether a page's browser holds the lock on the window's canvas now; natively `None`.
pub fn browser_locked(window: &Window) -> Option<bool> {
    #[cfg(target_arch = "wasm32")]
    {
        use winit::platform::web::WindowExtWebSys;
        let canvas = window.canvas()?;
        let document = web_sys::window()?.document()?;
        let locked = document.pointer_lock_element();
        let canvas: &web_sys::Element = &canvas;
        Some(locked.is_some_and(|element| element == *canvas))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = window;
        None
    }
}

/// Raw mouse travel as winit reports it, in points (the page's `movementX`, which the
/// old page turned the view by). winit takes Chromium's `movementX` for device pixels and
/// other browsers' for CSS pixels, which it scales up by the pixel ratio; Chromium
/// reports CSS pixels too, so elsewhere the travel is scaled back. Natively macOS
/// reports points and Windows raw counts, as before.
pub fn look_travel(window: &Window, delta: f64) -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        if !chromium() {
            return delta / window.scale_factor();
        }
    }
    let _ = window;
    delta
}

/// Whether the page runs in a Chromium browser, as winit tells (`web_sys::engine`).
#[cfg(target_arch = "wasm32")]
fn chromium() -> bool {
    use js_sys::{Array, Reflect};
    use wasm_bindgen::JsCast;
    let Some(navigator) = web_sys::window().map(|window| window.navigator()) else {
        return false;
    };
    let data = Reflect::get(&navigator, &"userAgentData".into()).unwrap_or_default();
    if data.is_undefined() || data.is_null() {
        return navigator
            .user_agent()
            .is_ok_and(|agent| agent.contains("Chrome/"));
    }
    Reflect::get(&data, &"brands".into())
        .ok()
        .and_then(|brands| brands.dyn_into::<Array>().ok())
        .is_some_and(|brands| {
            brands.iter().any(|brand| {
                Reflect::get(&brand, &"brand".into())
                    .ok()
                    .and_then(|name| name.as_string())
                    .is_some_and(|name| name == "Chromium")
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page whose browser grants the lock a frame after it is asked for.
    fn locked_page() -> PointerLock {
        let mut lock = PointerLock::default();
        assert_eq!(lock.frame(true, true, Some(false), 0.0), Some(Grab::Lock));
        assert_eq!(lock.frame(true, true, Some(true), 16.0), None);
        assert!(lock.held());
        lock
    }

    #[test]
    fn the_first_esc_frees_the_cursor_and_the_second_pauses() {
        let mut lock = locked_page();
        assert_eq!(lock.escape(100.0), Some(Grab::Release));
        assert!(!lock.held() && lock.waits_for_click());
        // The browser reports the release it was asked for; nothing more to do.
        assert_eq!(lock.frame(true, true, Some(false), 116.0), None);
        assert_eq!(lock.escape(500.0), None, "the second Esc pauses");
    }

    #[test]
    fn an_esc_the_browser_spent_on_its_own_release_does_not_pause() {
        let mut lock = locked_page();
        // The browser freed the cursor before it delivered the key.
        assert_eq!(
            lock.frame(true, true, Some(false), 100.0),
            Some(Grab::Release)
        );
        assert!(lock.waits_for_click());
        assert_eq!(lock.escape(150.0), Some(Grab::Release), "the same press");
        assert_eq!(lock.escape(400.0), None, "a later Esc pauses");
        // A release with no Esc after it (a window switch) still waits for a click.
        let mut lock = locked_page();
        lock.frame(true, true, Some(false), 100.0);
        assert_eq!(lock.escape(600.0), None);
        assert!(lock.waits_for_click());
    }

    #[test]
    fn a_click_on_the_arena_takes_the_cursor_back() {
        let mut lock = locked_page();
        lock.escape(100.0);
        // The freed view does not ask for the lock by itself.
        assert_eq!(lock.frame(true, true, Some(false), 116.0), None);
        assert_eq!(lock.press(true, 200.0), Some(Grab::Lock));
        assert!(lock.held() && !lock.waits_for_click());
        assert_eq!(lock.press(true, 300.0), None, "a held lock's click fires");
    }

    #[test]
    fn a_death_keeps_the_lock_and_esc_frees_it_to_pick_a_tank() {
        let mut lock = locked_page();
        assert_eq!(lock.frame(true, false, Some(true), 100.0), None);
        assert!(lock.held(), "the wreck keeps the cursor locked");
        assert_eq!(lock.escape(200.0), Some(Grab::Release));
        // Dead, the free cursor picks a card: no lock, and a click is no retake.
        assert_eq!(lock.frame(true, false, Some(false), 216.0), None);
        assert_eq!(lock.press(false, 300.0), None);
        // The new tank waits for a click on the arena to look again.
        assert_eq!(lock.frame(true, true, Some(false), 400.0), None);
        assert!(lock.waits_for_click());
        assert_eq!(lock.press(true, 500.0), Some(Grab::Lock));
    }

    #[test]
    fn a_menu_gives_the_cursor_back_and_closing_it_locks_again() {
        let mut lock = locked_page();
        assert_eq!(
            lock.frame(false, true, Some(true), 100.0),
            Some(Grab::Release)
        );
        assert!(!lock.waits_for_click());
        assert_eq!(lock.frame(true, true, Some(false), 200.0), Some(Grab::Lock));
        // Esc's freed cursor opens the menu; RESUME locks at once again.
        let mut lock = locked_page();
        lock.escape(100.0);
        lock.frame(false, true, Some(false), 200.0);
        assert_eq!(lock.frame(true, true, Some(false), 300.0), Some(Grab::Lock));
        // A lock the browser let go of by itself takes a click to come back, menu or not.
        let mut lock = locked_page();
        lock.frame(true, true, Some(false), 100.0);
        lock.frame(false, true, Some(false), 500.0);
        assert_eq!(lock.frame(true, true, Some(false), 600.0), None);
        assert!(lock.waits_for_click());
    }

    #[test]
    fn a_refused_lock_waits_for_a_click_once_locks_have_worked() {
        let mut lock = locked_page();
        lock.frame(false, true, Some(true), 100.0);
        assert_eq!(lock.frame(true, true, Some(false), 200.0), Some(Grab::Lock));
        assert_eq!(
            lock.frame(true, true, Some(false), 900.0),
            None,
            "still asking"
        );
        assert_eq!(
            lock.frame(true, true, Some(false), 1300.0),
            Some(Grab::Release)
        );
        assert!(lock.waits_for_click());
        // A browser that never locks keeps looking with the free mouse.
        let mut lock = PointerLock::default();
        assert_eq!(lock.frame(true, true, Some(false), 0.0), Some(Grab::Lock));
        assert_eq!(lock.frame(true, true, Some(false), 5000.0), None);
        assert!(lock.held());
    }

    #[test]
    fn natively_the_grab_is_the_lock() {
        let mut lock = PointerLock::default();
        assert_eq!(lock.frame(true, true, None, 0.0), Some(Grab::Lock));
        assert_eq!(lock.frame(true, true, None, 5000.0), None);
        assert_eq!(lock.escape(6000.0), Some(Grab::Release));
        assert_eq!(lock.escape(6100.0), None);
    }
}

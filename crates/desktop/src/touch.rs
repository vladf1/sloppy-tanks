//! Touch play in the iOS app and on a page's touch screen (`touch-controls.ts`): the drive
//! stick in the bottom-left corner, the mine button with the zoom pair stacked over it in
//! the bottom-right (a phone's edition has no mines, and its zoom pair joins the top
//! row), and a finger anywhere else on the arena aims there and fires while it is down.
//! Each finger belongs to whatever it first touched: one that lands on a control or the
//! arena never reaches egui, so a second finger still presses the HUD's buttons while
//! a thumb drives, and one that lands on a menu or a HUD button stays egui's.
//!
//! The device's class is decided once, when the app starts (`player-preferences.ts`,
//! `index.html`'s head script): it picks the phone edition, the graphics the engine
//! starts with and whether the overhead reticle shows.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use egui::{Pos2, Rect, Vec2, pos2, vec2};
use winit::event::{Touch, TouchPhase};

use sloppy_render::presentation::input::{STICK_ANCHOR_SHIFT, ZOOM_STEP};
use sloppy_render::quality::DeviceClass;
use sloppy_web::hud::Hud;

use crate::input::Controls;
use crate::preferences::TouchSetting;

/// Whether this device plays by touch: the iOS app, the phone edition, and a page on a
/// touch screen as Settings' Touch controls say (Auto: the browser reported one, or a
/// finger touched the page). Touch play has the touch controls and no pointer lock;
/// Macs and PCs play with keyboard and mouse.
pub fn screen() -> bool {
    plays_by_touch(
        cfg!(target_os = "ios") || phone(),
        setting(),
        TOUCH_DETECTED.load(Ordering::Relaxed),
    )
}

/// The rule [`screen`] applies: a device that has no other controls always plays by
/// touch; elsewhere the setting decides, and Auto follows what was `detected`. A touch
/// never overrides Off.
fn plays_by_touch(touch_only: bool, setting: TouchSetting, detected: bool) -> bool {
    touch_only
        || match setting {
            TouchSetting::On => true,
            TouchSetting::Off => false,
            TouchSetting::Auto => detected,
        }
}

/// Whether Settings offers Touch controls: in a page, but not in the phone edition,
/// which has no other controls. The iOS app always plays by touch, and the desktop app
/// with keyboard and mouse (macOS has no touch screens; Windows' are left to the page).
pub fn offers_setting() -> bool {
    cfg!(target_arch = "wasm32") && !phone()
}

/// The kind of device the app runs on, as the app started.
pub fn device() -> DeviceClass {
    match DEVICE.load(Ordering::Relaxed) {
        TABLET => DeviceClass::Tablet,
        PHONE => DeviceClass::Phone,
        _ => DeviceClass::Desktop,
    }
}

/// The phone edition: an iPhone, a coarse pointer on a small screen, or `?phone`
/// (`--phone`).
pub fn phone() -> bool {
    device() == DeviceClass::Phone
}

const DESKTOP: u8 = 0;
const TABLET: u8 = 1;
const PHONE: u8 = 2;
/// A coarse pointer on a screen whose shorter side is under this many CSS pixels is a
/// phone. The screen's, not the window's: turning the phone keeps the same edition.
#[cfg(target_arch = "wasm32")]
const PHONE_SCREEN_SIDE: i32 = 600;

static DEVICE: AtomicU8 = AtomicU8::new(if cfg!(target_os = "ios") {
    TABLET
} else {
    DESKTOP
});
static TOUCH_DETECTED: AtomicBool = AtomicBool::new(cfg!(target_os = "ios"));
static SETTING: AtomicU8 = AtomicU8::new(0);

fn setting() -> TouchSetting {
    match SETTING.load(Ordering::Relaxed) {
        1 => TouchSetting::On,
        2 => TouchSetting::Off,
        _ => TouchSetting::Auto,
    }
}

/// The saved Touch controls choice (`Preferences::load` and `save` keep it current).
pub fn set_setting(touch: TouchSetting) {
    let value = match touch {
        TouchSetting::Auto => 0,
        TouchSetting::On => 1,
        TouchSetting::Off => 2,
    };
    SETTING.store(value, Ordering::Relaxed);
}

/// The device class from what the browser says, as the page's shell decided it: the
/// phone edition for a coarse pointer on a small screen; a tablet for another coarse
/// primary pointer (a tablet, a car's screen) or iPadOS Safari, which presents itself as
/// a Mac with touch points; otherwise a desktop. A touch laptop's primary pointer is
/// fine, so on Auto it plays with keyboard and mouse until a finger touches the page
/// ([`touched`]). The iOS app is a phone on an iPhone and a tablet on an iPad. `phone`
/// (`?phone`) asks for the phone edition anywhere.
pub fn detect(phone: bool) {
    #[cfg(target_arch = "wasm32")]
    let (phone, touch) = {
        let Some(window) = web_sys::window() else {
            return;
        };
        let navigator = window.navigator();
        let agent = navigator.user_agent().unwrap_or_default();
        let ipad = agent.contains("Macintosh") && navigator.max_touch_points() > 1;
        let matches = |query: &str| {
            window
                .match_media(query)
                .ok()
                .flatten()
                .is_some_and(|query| query.matches())
        };
        let coarse = matches("(pointer: coarse)");
        let small = window.screen().is_ok_and(|screen| {
            let side = |size: Result<i32, _>| size.unwrap_or(i32::MAX);
            side(screen.width()).min(side(screen.height())) < PHONE_SCREEN_SIDE
        });
        let phone = phone || (coarse && small);
        // Opened from the Home Screen (`navigator.standalone` on iOS) the game has no
        // browser bars to hide.
        let home_screen_app = matches("(display-mode: standalone)")
            || js_sys::Reflect::get(&navigator, &"standalone".into())
                .is_ok_and(|standalone| standalone.as_bool() == Some(true));
        let iphone = agent.contains("iPhone") || agent.contains("iPod");
        HOME_SCREEN_TIP.store(phone && iphone && !home_screen_app, Ordering::Relaxed);
        (phone, ipad || coarse)
    };
    // The iOS app is one bundle for both: an iPhone plays the phone edition.
    #[cfg(target_os = "ios")]
    let phone = phone || crate::ios::is_phone();
    #[cfg(not(target_arch = "wasm32"))]
    let touch = cfg!(target_os = "ios");
    let class = if phone {
        PHONE
    } else if touch {
        TABLET
    } else {
        DESKTOP
    };
    DEVICE.store(class, Ordering::Relaxed);
    TOUCH_DETECTED.store(touch, Ordering::Relaxed);
}

/// An iPhone's browser cannot hide its bars for a page, so the phone edition's Battle
/// Setup suggests the Home Screen there (`offer-home-screen`).
pub fn home_screen_tip() -> bool {
    HOME_SCREEN_TIP.load(Ordering::Relaxed)
}

static HOME_SCREEN_TIP: AtomicBool = AtomicBool::new(false);

/// A finger touched the screen: on Auto a page plays by touch from now on, as on a touch
/// screen its browser did not report. The desktop app keeps its keyboard and mouse.
pub fn touched() {
    if cfg!(target_arch = "wasm32") {
        TOUCH_DETECTED.store(true, Ordering::Relaxed);
    }
}

/// The drive stick's square (its radius and dead zone are the engine's), and the mine
/// button's and zoom keys' radii.
pub const STICK_SIZE: f32 = 176.0;
pub const MINE_RADIUS: f32 = 38.0;
pub const ZOOM_RADIUS: f32 = 26.0;
/// The controls' distance from the screen's sides and bottom, or from the safe area's
/// edge where that is further in.
const SIDE: f32 = 24.0;
const BOTTOM: f32 = 28.0;
/// A phone's smaller stick, nearer its corner.
pub const PHONE_STICK_SIZE: f32 = 150.0;
const PHONE_STICK_MARGIN: f32 = 12.0;

/// Where the touch controls sit, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchLayout {
    pub stick: Rect,
    /// The mine button's centre; a phone's edition has no mines.
    pub mine: Option<Pos2>,
    /// The zoom keys: round keys over the mine button, or a phone's pair in its top row.
    pub zoom_in: Rect,
    pub zoom_out: Rect,
}

impl TouchLayout {
    /// The layout on a screen of `viewport` whose safe area is `content`; `phone`: the
    /// phone edition's.
    pub fn new(viewport: Rect, content: Rect, phone: bool) -> Self {
        if phone {
            let left = (viewport.left() + PHONE_STICK_MARGIN).max(content.left());
            let bottom = (viewport.bottom() - PHONE_STICK_MARGIN).min(content.bottom());
            let bar = PhoneBar::new(viewport, content);
            return Self {
                stick: Rect::from_min_size(
                    pos2(left, bottom - PHONE_STICK_SIZE),
                    Vec2::splat(PHONE_STICK_SIZE),
                ),
                mine: None,
                zoom_in: bar.zoom_in,
                zoom_out: bar.zoom_out,
            };
        }
        let left = (viewport.left() + SIDE).max(content.left());
        let right = (viewport.right() - SIDE).min(content.right());
        let bottom = (viewport.bottom() - BOTTOM).min(content.bottom());
        let x = right - MINE_RADIUS;
        let key = |y: f32| Rect::from_center_size(pos2(x, y), Vec2::splat(2.0 * ZOOM_RADIUS));
        Self {
            stick: Rect::from_min_size(pos2(left, bottom - STICK_SIZE), Vec2::splat(STICK_SIZE)),
            // The mine sits level with the stick's centre; the zoom pair over it, one
            // family of thumb buttons.
            mine: Some(pos2(x, bottom - 50.0 - MINE_RADIUS)),
            zoom_out: key(bottom - 136.0 - ZOOM_RADIUS),
            zoom_in: key(bottom - 196.0 - ZOOM_RADIUS),
        }
    }

    pub fn of(context: &egui::Context) -> Self {
        Self::new(context.viewport_rect(), context.content_rect(), phone())
    }

    /// The room between the stick and the mine button (or the screen's edge) that the
    /// tank panel takes.
    pub fn panel_span(&self, right: f32) -> (f32, f32) {
        let end = self
            .mine
            .map_or(right, |mine| mine.x - MINE_RADIUS - PANEL_MINE_GAP);
        (self.stick.right() + PANEL_STICK_GAP, end)
    }
}

/// The tank panel's distance from the drive stick and from the mine button
/// (`.touch-enabled .bottom`).
const PANEL_STICK_GAP: f32 = 12.0;
const PANEL_MINE_GAP: f32 = 16.0;

/// The phone's row of keys over the arena: one key's side, a thumb's gap between keys
/// that stand apart, and their distance from the top and the sides.
pub const PHONE_KEY: f32 = 44.0;
const PHONE_KEY_GAP: f32 = 10.0;
const PHONE_BAR_TOP: f32 = 6.0;
const PHONE_BAR_SIDE: f32 = 20.0;
const PHONE_BAR_SIDE_UPRIGHT: f32 = 8.0;

/// The phone's top row (`.phone .hud-actions`): held sideways, the zoom pair, first
/// person and pause stand together in the right corner, a thumb's gap apart; held
/// upright the row splits, the zoom pair in the left corner and first person with pause
/// in the right, so the score fits between them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhoneBar {
    pub zoom_out: Rect,
    pub zoom_in: Rect,
    pub view: Rect,
    pub pause: Rect,
    /// Held upright: the pairs stand in opposite corners.
    pub upright: bool,
}

impl PhoneBar {
    pub fn new(viewport: Rect, content: Rect) -> Self {
        let upright = viewport.height() > viewport.width();
        let top = (viewport.top() + PHONE_BAR_TOP).max(content.top());
        let key = |left: f32| Rect::from_min_size(pos2(left, top), Vec2::splat(PHONE_KEY));
        if upright {
            let left = (viewport.left() + PHONE_BAR_SIDE_UPRIGHT).max(content.left());
            let right = (viewport.right() - PHONE_BAR_SIDE_UPRIGHT).min(content.right());
            return Self {
                zoom_out: key(left),
                zoom_in: key(left + PHONE_KEY),
                view: key(right - 2.0 * PHONE_KEY),
                pause: key(right - PHONE_KEY),
                upright,
            };
        }
        // Clear of a landscape phone's rounded corner and notch.
        let right = (viewport.right() - PHONE_BAR_SIDE).min(content.right() - 8.0);
        let pause = key(right - PHONE_KEY);
        let view = pause.translate(vec2(-PHONE_KEY - PHONE_KEY_GAP, 0.0));
        let zoom_in = view.translate(vec2(-PHONE_KEY - PHONE_KEY_GAP, 0.0));
        Self {
            zoom_out: zoom_in.translate(vec2(-PHONE_KEY, 0.0)),
            zoom_in,
            view,
            pause,
            upright,
        }
    }

    pub fn of(context: &egui::Context) -> Self {
        Self::new(context.viewport_rect(), context.content_rect())
    }
}

/// Which touch controls a battle shows (`touch-controls.css`, `#hud.dead`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Showing {
    /// None: no battle is running, or a dialog is over it.
    Nothing,
    /// The zoom pair alone, while the player's tank is a wreck waiting to respawn.
    Zoom,
    /// All of them: the player can act.
    All,
}

/// The touch controls in a running battle with no menu: all of them while the player
/// can act, the zoom pair alone while the tank is a wreck.
pub struct Play {
    pub layout: TouchLayout,
    pub first_person: bool,
    pub mine_ready: bool,
    /// The player can act: the stick, the mine button and the arena take fingers.
    pub acting: bool,
}

impl Play {
    /// The controls on `context`'s screen, or `None` while they are hidden.
    pub fn live(
        context: &egui::Context,
        showing: Showing,
        first_person: bool,
        hud: Option<&Hud<'_>>,
    ) -> Option<Self> {
        (screen() && showing != Showing::Nothing).then(|| Self {
            layout: TouchLayout::of(context),
            first_person,
            mine_ready: hud.is_none_or(|hud| hud.human.mine_cooldown <= 0.0),
            acting: showing == Showing::All,
        })
    }
}

/// Whether `point` is on one of egui's areas: a menu, the HUD's toolbar or tank panel.
/// egui spans the safe area with an interactable background layer of its own (so
/// panels can catch clicks), and the arena lies under it, so that layer does not count.
pub fn over_ui(context: &egui::Context, point: Pos2) -> bool {
    context
        .layer_id_at(point)
        .is_some_and(|layer| layer.order != egui::Order::Background)
}

/// Which fingers egui owns; the controls track their own in [`Controls::touch`].
#[derive(Default)]
pub struct Fingers {
    menus: HashSet<u64>,
    /// The player has touched the arena: the fire hint has done its job.
    pub learned: bool,
}

impl Fingers {
    /// Route a touch to the controls or to egui; true when egui should see it.
    /// `over_ui` says whether a point is on one of egui's areas (a menu, the HUD's
    /// toolbar or tank panel).
    pub fn route(
        &mut self,
        touch: &Touch,
        scale: f64,
        play: Option<&Play>,
        over_ui: impl Fn(Pos2) -> bool,
        screen: Vec2,
        controls: &mut Controls,
    ) -> bool {
        let logical = touch.location.to_logical::<f32>(scale);
        let point = pos2(logical.x, logical.y);
        let at = (f64::from(point.x), f64::from(point.y));
        let size = (f64::from(screen.x), f64::from(screen.y));
        match touch.phase {
            TouchPhase::Started => {
                let Some(play) = play else {
                    self.menus.insert(touch.id);
                    return true;
                };
                let layout = &play.layout;
                let on = |center: Pos2, radius: f32| center.distance(point) <= radius;
                // First person has no zoom keys.
                let zoom = if play.first_person {
                    None
                } else if layout.zoom_in.contains(point) {
                    Some(-ZOOM_STEP)
                } else if layout.zoom_out.contains(point) {
                    Some(ZOOM_STEP)
                } else {
                    None
                };
                if !play.acting {
                    // A wreck's player keeps only the zoom pair, which lies under the
                    // respawn card where the two meet (an upright tablet): the card
                    // keeps its fingers.
                    match zoom.filter(|_| !over_ui(point)) {
                        Some(amount) => controls.zoom_by(amount),
                        None => {
                            self.menus.insert(touch.id);
                            return true;
                        }
                    }
                } else if layout.stick.contains(point) {
                    let center = layout.stick.center();
                    let shift = (point - center).clamp(
                        Vec2::splat(-STICK_ANCHOR_SHIFT),
                        Vec2::splat(STICK_ANCHOR_SHIFT),
                    );
                    let origin = center + shift;
                    controls.begin_drive(touch.id, (origin.x, origin.y), (point.x, point.y));
                } else if layout.mine.is_some_and(|mine| on(mine, MINE_RADIUS)) {
                    if play.mine_ready {
                        controls.drop_mine();
                    }
                } else if let Some(amount) = zoom {
                    controls.zoom_by(amount);
                } else if over_ui(point) {
                    self.menus.insert(touch.id);
                    return true;
                } else {
                    controls.begin_arena(touch.id, at, size);
                    self.learned = true;
                }
                false
            }
            TouchPhase::Moved => {
                if self.menus.contains(&touch.id) {
                    return true;
                }
                // A thumb still on the stick when the tank is wrecked lets go of it.
                if play.is_some_and(|play| play.acting) {
                    controls.move_drive(touch.id, (point.x, point.y));
                    controls.move_arena(touch.id, at, size);
                } else {
                    controls.end_touch(touch.id);
                }
                false
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                if self.menus.remove(&touch.id) {
                    return true;
                }
                controls.end_touch(touch.id);
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::dpi::PhysicalPosition;
    use winit::event::DeviceId;

    fn touch(id: u64, phase: TouchPhase, x: f64, y: f64) -> Touch {
        Touch {
            device_id: DeviceId::dummy(),
            phase,
            location: PhysicalPosition::new(x, y),
            force: None,
            id,
        }
    }

    const SCREEN: Vec2 = Vec2::new(1180.0, 820.0);

    fn play() -> Play {
        let screen = Rect::from_min_size(Pos2::ZERO, SCREEN);
        Play {
            layout: TouchLayout::new(screen, screen, false),
            first_person: false,
            mine_ready: true,
            acting: true,
        }
    }

    #[test]
    fn the_layout_keeps_the_controls_in_their_corners() {
        let screen = Rect::from_min_size(Pos2::ZERO, SCREEN);
        let layout = TouchLayout::new(screen, screen, false);
        assert_eq!(layout.stick.left_bottom(), pos2(24.0, 792.0));
        let mine = layout.mine.expect("a tablet's mine button");
        assert_eq!(mine, pos2(1118.0, 704.0));
        assert!(layout.zoom_in.center().y < layout.zoom_out.center().y);
        assert!(layout.zoom_out.bottom() < mine.y - MINE_RADIUS);
        // A safe area further in than the margins moves them in.
        let content = Rect::from_min_max(pos2(0.0, 24.0), pos2(1180.0, 800.0));
        assert_eq!(
            TouchLayout::new(screen, content, false).stick.bottom(),
            792.0
        );
        let content = Rect::from_min_max(pos2(30.0, 0.0), pos2(1180.0, 780.0));
        assert_eq!(
            TouchLayout::new(screen, content, false).stick.left_bottom(),
            pos2(30.0, 780.0)
        );
    }

    /// The phone edition at its smallest screen either way up: the smaller stick in its
    /// corner, no mine, and the top row's keys thumb-sized, on screen and apart.
    #[test]
    fn a_phone_keeps_the_stick_and_its_top_row_apart_both_ways_up() {
        for size in [vec2(852.0, 393.0), vec2(393.0, 852.0)] {
            let screen = Rect::from_min_size(Pos2::ZERO, size);
            let layout = TouchLayout::new(screen, screen, true);
            assert_eq!(layout.mine, None);
            assert_eq!(layout.stick.left_bottom(), pos2(12.0, size.y - 12.0));
            assert_eq!(layout.stick.width(), PHONE_STICK_SIZE);
            let bar = PhoneBar::new(screen, screen);
            assert_eq!(
                (layout.zoom_in, layout.zoom_out),
                (bar.zoom_in, bar.zoom_out)
            );
            let keys = [bar.zoom_out, bar.zoom_in, bar.view, bar.pause];
            for (index, key) in keys.iter().enumerate() {
                assert!(
                    screen.contains_rect(*key),
                    "{size:?}: key {index} on screen"
                );
                assert!(key.width() >= 44.0 && key.height() >= 44.0);
                assert!(!key.intersects(layout.stick));
                for other in &keys[index + 1..] {
                    let overlap = key.intersect(*other);
                    assert!(overlap.width() <= 0.0 || overlap.height() <= 0.0);
                }
            }
            assert!(bar.zoom_out.left() < bar.zoom_in.left());
            if bar.upright {
                assert!(bar.zoom_in.right() < size.x / 2.0 && bar.view.left() > size.x / 2.0);
            } else {
                // A thumb's gap between the zoom pair, first person and pause.
                assert!(bar.view.left() - bar.zoom_in.right() >= 10.0);
                assert!(bar.pause.left() - bar.view.right() >= 10.0);
                assert!(bar.zoom_out.left() > size.x / 2.0);
            }
        }
    }

    #[test]
    fn fingers_belong_to_what_they_first_touched() {
        let mut fingers = Fingers::default();
        let mut controls = Controls::default();
        let play = play();
        let stick = play.layout.stick.center();
        let route = |fingers: &mut Fingers, controls: &mut Controls, touch: Touch| {
            fingers.route(
                &touch,
                1.0,
                Some(&play),
                |point| point.y < 60.0,
                SCREEN,
                controls,
            )
        };
        // The stick, then a second finger on the HUD's toolbar: egui gets only that one.
        let thumb = touch(
            1,
            TouchPhase::Started,
            f64::from(stick.x),
            f64::from(stick.y),
        );
        assert!(!route(&mut fingers, &mut controls, thumb));
        let tap = touch(2, TouchPhase::Started, 1100.0, 30.0);
        assert!(route(&mut fingers, &mut controls, tap));
        let lift = touch(2, TouchPhase::Ended, 600.0, 400.0);
        assert!(route(&mut fingers, &mut controls, lift));
        let push = touch(1, TouchPhase::Moved, f64::from(stick.x), 0.0);
        assert!(!route(&mut fingers, &mut controls, push));
        assert!(controls.take_input().touch_move.1 < -0.9);
        // A third finger on the arena fires.
        let shot = touch(3, TouchPhase::Started, 600.0, 400.0);
        assert!(!route(&mut fingers, &mut controls, shot));
        assert!(fingers.learned);
        assert!(controls.take_input().fire);
    }

    #[test]
    fn the_buttons_drop_a_mine_and_zoom() {
        let mut fingers = Fingers::default();
        let mut controls = Controls::default();
        let mut play = play();
        let press = |fingers: &mut Fingers, controls: &mut Controls, play: &Play, at: Pos2| {
            let touch = touch(9, TouchPhase::Started, f64::from(at.x), f64::from(at.y));
            fingers.route(&touch, 1.0, Some(play), |_| false, SCREEN, controls);
            let lift = Touch {
                phase: TouchPhase::Ended,
                ..touch
            };
            fingers.route(&lift, 1.0, Some(play), |_| false, SCREEN, controls);
        };
        let mine = play.layout.mine.expect("a tablet's mine button");
        press(&mut fingers, &mut controls, &play, mine);
        press(
            &mut fingers,
            &mut controls,
            &play,
            play.layout.zoom_in.center(),
        );
        let input = controls.take_input();
        assert!(input.mine && !input.fire);
        assert_eq!(input.zoom, -ZOOM_STEP);
        // A mine still reloading and the zoom of first person do nothing.
        play.mine_ready = false;
        play.first_person = true;
        press(&mut fingers, &mut controls, &play, mine);
        assert!(!controls.take_input().mine);
        let at = play.layout.zoom_out.center();
        let held = touch(5, TouchPhase::Started, f64::from(at.x), f64::from(at.y));
        fingers.route(&held, 1.0, Some(&play), |_| false, SCREEN, &mut controls);
        let input = controls.take_input();
        assert!(
            input.fire,
            "first person has no zoom keys: the finger aims there"
        );
        assert_eq!(input.zoom, 0.0);
    }

    /// While the tank is a wreck the zoom pair still zooms, except where the respawn
    /// card lies over it; every other finger, the stick's corner included, is egui's,
    /// and a thumb left on the stick lets go of it.
    #[test]
    fn a_wrecks_player_keeps_only_the_zoom_pair() {
        let mut fingers = Fingers::default();
        let mut controls = Controls::default();
        let stick = play().layout.stick.center();
        let thumb = touch(
            1,
            TouchPhase::Started,
            f64::from(stick.x),
            f64::from(stick.y),
        );
        assert!(!fingers.route(&thumb, 1.0, Some(&play()), |_| false, SCREEN, &mut controls));
        let push = touch(1, TouchPhase::Moved, f64::from(stick.x), 0.0);
        assert!(!fingers.route(&push, 1.0, Some(&play()), |_| false, SCREEN, &mut controls));
        let wreck = Play {
            acting: false,
            ..play()
        };
        let zoom_in = wreck.layout.zoom_in.center();
        // The card's edge covers the zoom pair's left half.
        let card = |point: Pos2| point.x < zoom_in.x;
        let press = |fingers: &mut Fingers, controls: &mut Controls, id, at: Pos2| {
            let touch = touch(id, TouchPhase::Started, f64::from(at.x), f64::from(at.y));
            fingers.route(&touch, 1.0, Some(&wreck), card, SCREEN, controls)
        };
        assert!(!press(
            &mut fingers,
            &mut controls,
            2,
            zoom_in + vec2(10.0, 0.0)
        ));
        assert!(press(
            &mut fingers,
            &mut controls,
            3,
            zoom_in - vec2(10.0, 0.0)
        ));
        assert!(press(&mut fingers, &mut controls, 4, stick));
        assert!(press(&mut fingers, &mut controls, 5, pos2(600.0, 400.0)));
        let held = touch(1, TouchPhase::Moved, f64::from(stick.x), 10.0);
        assert!(!fingers.route(&held, 1.0, Some(&wreck), card, SCREEN, &mut controls));
        let input = controls.take_input();
        assert_eq!(input.zoom, -ZOOM_STEP);
        assert!(!input.fire);
        assert_eq!(input.touch_move, (0.0, 0.0));
    }

    /// Two passes of a screen with one HUD button at its top right, as the toolbar is:
    /// egui hit-tests against the areas the last pass showed.
    fn hud_context() -> egui::Context {
        let context = egui::Context::default();
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SCREEN)),
                ..Default::default()
            };
            let mut output = context.run_pass(input, |context| {
                egui::Area::new(egui::Id::unique("toolbar"))
                    .fixed_pos(pos2(1000.0, 10.0))
                    .order(egui::Order::Middle)
                    .show(context, |ui| {
                        ui.allocate_exact_size(Vec2::new(150.0, 46.0), egui::Sense::click());
                    });
            });
            // No painter takes the font atlas here.
            output.textures_delta.clear();
        }
        context
    }

    #[test]
    fn the_arena_under_egui_background_is_not_ui() {
        let context = hud_context();
        assert!(
            context.layer_id_at(pos2(600.0, 400.0)).is_some(),
            "egui's background layer covers the arena"
        );
        assert!(!over_ui(&context, pos2(600.0, 400.0)));
        assert!(over_ui(&context, pos2(1050.0, 30.0)));
    }

    #[test]
    fn an_arena_finger_fires_past_egui_and_a_button_finger_does_not() {
        let context = hud_context();
        let mut fingers = Fingers::default();
        let mut controls = Controls::default();
        let play = play();
        let over = |point| over_ui(&context, point);
        let shot = touch(1, TouchPhase::Started, 600.0, 400.0);
        assert!(!fingers.route(&shot, 1.0, Some(&play), over, SCREEN, &mut controls));
        assert!(controls.take_input().fire);
        let tap = touch(2, TouchPhase::Started, 1050.0, 30.0);
        assert!(fingers.route(&tap, 1.0, Some(&play), over, SCREEN, &mut controls));
    }

    #[test]
    fn the_setting_decides_touch_play_where_the_device_has_other_controls() {
        use TouchSetting::{Auto, Off, On};
        for detected in [false, true] {
            // The iOS app and the phone edition have no other controls.
            for setting in [Auto, On, Off] {
                assert!(plays_by_touch(true, setting, detected));
            }
            assert!(plays_by_touch(false, On, detected));
            assert!(
                !plays_by_touch(false, Off, detected),
                "a touch never overrides Off"
            );
            assert_eq!(plays_by_touch(false, Auto, detected), detected);
        }
    }

    /// On a phone a thumb drives while another finger aims and fires, the top row's zoom
    /// keys step the camera, and lifting each finger lets go of only what it held.
    #[test]
    fn a_phone_drives_and_fires_at_once_and_zooms_from_its_top_row() {
        let size = vec2(852.0, 393.0);
        let screen = Rect::from_min_size(Pos2::ZERO, size);
        let play = Play {
            layout: TouchLayout::new(screen, screen, true),
            first_person: false,
            mine_ready: true,
            acting: true,
        };
        let mut fingers = Fingers::default();
        let mut controls = Controls::default();
        // Physical pixels at three device pixels a point.
        let mut route = |controls: &mut Controls, id, phase, point: Pos2| {
            let touch = touch(
                id,
                phase,
                f64::from(point.x) * 3.0,
                f64::from(point.y) * 3.0,
            );
            fingers.route(&touch, 3.0, Some(&play), |_| false, size, controls)
        };
        let stick = play.layout.stick.center();
        let shot = pos2(700.0, 170.0);
        assert!(!route(&mut controls, 1, TouchPhase::Started, stick));
        assert!(!route(
            &mut controls,
            1,
            TouchPhase::Moved,
            stick + vec2(58.0, 0.0)
        ));
        assert!(!route(&mut controls, 2, TouchPhase::Started, shot));
        // A second arena finger cannot take the aim from the firing one.
        assert!(!route(
            &mut controls,
            3,
            TouchPhase::Started,
            pos2(100.0, 170.0)
        ));
        assert!(!route(
            &mut controls,
            3,
            TouchPhase::Moved,
            pos2(120.0, 170.0)
        ));
        let zoom_out = play.layout.zoom_out.center();
        assert!(!route(&mut controls, 4, TouchPhase::Started, zoom_out));
        assert!(!route(&mut controls, 4, TouchPhase::Ended, zoom_out));
        let input = controls.take_input();
        assert!(input.fire && input.touch_move.0 > 0.9);
        assert!(
            input.pointer.0 > 0.6,
            "the first arena finger aims: {:?}",
            input.pointer
        );
        assert_eq!(input.zoom, ZOOM_STEP);
        // A cancelled finger stops firing; the thumb still drives until it lifts.
        assert!(!route(&mut controls, 2, TouchPhase::Cancelled, shot));
        let input = controls.take_input();
        assert!(!input.fire && input.touch_move.0 > 0.9);
        assert!(!route(&mut controls, 1, TouchPhase::Ended, stick));
        assert_eq!(controls.take_input().touch_move, (0.0, 0.0));
    }

    #[test]
    fn without_play_every_finger_is_the_menus() {
        let mut fingers = Fingers::default();
        let mut controls = Controls::default();
        let start = touch(4, TouchPhase::Started, 600.0, 400.0);
        assert!(fingers.route(&start, 2.0, None, |_| false, SCREEN, &mut controls));
        let end = touch(4, TouchPhase::Ended, 600.0, 400.0);
        assert!(fingers.route(&end, 2.0, None, |_| false, SCREEN, &mut controls));
        assert!(!controls.take_input().fire);
    }
}

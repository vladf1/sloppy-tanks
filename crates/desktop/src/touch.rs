//! Touch play on the iPad (`touch-controls.ts`): the drive stick in the bottom-left
//! corner, the mine button with the zoom pair stacked over it in the bottom-right, and
//! a finger anywhere else on the arena aims there and fires while it is down. Each
//! finger belongs to whatever it first touched: one that lands on a control or the
//! arena never reaches egui, so a second finger still presses the HUD's buttons while
//! a thumb drives, and one that lands on a menu or a HUD button stays egui's.

use std::collections::HashSet;

use egui::{Pos2, Rect, Vec2, pos2};
use winit::event::{Touch, TouchPhase};

use sloppy_render::presentation::input::{STICK_ANCHOR_SHIFT, ZOOM_STEP};
use sloppy_web::hud::Hud;

use crate::input::Controls;

/// The drive stick's square (its radius and dead zone are the engine's), and the mine
/// button's and zoom keys' radii.
pub const STICK_SIZE: f32 = 176.0;
pub const MINE_RADIUS: f32 = 38.0;
pub const ZOOM_RADIUS: f32 = 26.0;
/// The controls' distance from the screen's sides and bottom, or from the safe area's
/// edge where that is further in.
const SIDE: f32 = 24.0;
const BOTTOM: f32 = 28.0;

/// Where the touch controls sit, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchLayout {
    pub stick: Rect,
    pub mine: Pos2,
    pub zoom_in: Pos2,
    pub zoom_out: Pos2,
}

impl TouchLayout {
    /// The layout on a screen of `viewport` whose safe area is `content`.
    pub fn new(viewport: Rect, content: Rect) -> Self {
        let left = (viewport.left() + SIDE).max(content.left());
        let right = (viewport.right() - SIDE).min(content.right());
        let bottom = (viewport.bottom() - BOTTOM).min(content.bottom());
        let x = right - MINE_RADIUS;
        Self {
            stick: Rect::from_min_size(pos2(left, bottom - STICK_SIZE), Vec2::splat(STICK_SIZE)),
            // The mine sits level with the stick's centre; the zoom pair over it, one
            // family of thumb buttons.
            mine: pos2(x, bottom - 50.0 - MINE_RADIUS),
            zoom_out: pos2(x, bottom - 136.0 - ZOOM_RADIUS),
            zoom_in: pos2(x, bottom - 196.0 - ZOOM_RADIUS),
        }
    }

    pub fn of(context: &egui::Context) -> Self {
        Self::new(context.viewport_rect(), context.content_rect())
    }

    /// The room between the stick and the mine button that the tank panel takes.
    pub fn panel_span(&self) -> (f32, f32) {
        (self.stick.right() + 12.0, self.mine.x - MINE_RADIUS - 12.0)
    }
}

/// The touch controls while the player can act: alive, in a running battle, no menu.
pub struct Play {
    pub layout: TouchLayout,
    pub first_person: bool,
    pub mine_ready: bool,
}

impl Play {
    /// The controls on `context`'s screen, or `None` while they are hidden.
    pub fn live(
        context: &egui::Context,
        acting: bool,
        first_person: bool,
        hud: Option<&Hud<'_>>,
    ) -> Option<Self> {
        (crate::TOUCH && acting).then(|| Self {
            layout: TouchLayout::of(context),
            first_person,
            mine_ready: hud.is_none_or(|hud| hud.human.mine_cooldown <= 0.0),
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
                if layout.stick.contains(point) {
                    let center = layout.stick.center();
                    let shift = (point - center).clamp(
                        Vec2::splat(-STICK_ANCHOR_SHIFT),
                        Vec2::splat(STICK_ANCHOR_SHIFT),
                    );
                    let origin = center + shift;
                    controls.begin_drive(touch.id, (origin.x, origin.y), (point.x, point.y));
                } else if on(layout.mine, MINE_RADIUS) {
                    if play.mine_ready {
                        controls.drop_mine();
                    }
                } else if !play.first_person && on(layout.zoom_in, ZOOM_RADIUS) {
                    controls.zoom_by(-ZOOM_STEP);
                } else if !play.first_person && on(layout.zoom_out, ZOOM_RADIUS) {
                    controls.zoom_by(ZOOM_STEP);
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
                if play.is_some() {
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
            layout: TouchLayout::new(screen, screen),
            first_person: false,
            mine_ready: true,
        }
    }

    #[test]
    fn the_layout_keeps_the_controls_in_their_corners() {
        let screen = Rect::from_min_size(Pos2::ZERO, SCREEN);
        let layout = TouchLayout::new(screen, screen);
        assert_eq!(layout.stick.left_bottom(), pos2(24.0, 792.0));
        assert_eq!(layout.mine, pos2(1118.0, 704.0));
        assert!(layout.zoom_in.y < layout.zoom_out.y && layout.zoom_out.y < layout.mine.y);
        // A safe area further in than the margins moves them in.
        let content = Rect::from_min_max(pos2(0.0, 24.0), pos2(1180.0, 800.0));
        assert_eq!(TouchLayout::new(screen, content).stick.bottom(), 792.0);
        let content = Rect::from_min_max(pos2(30.0, 0.0), pos2(1180.0, 780.0));
        assert_eq!(
            TouchLayout::new(screen, content).stick.left_bottom(),
            pos2(30.0, 780.0)
        );
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
        press(&mut fingers, &mut controls, &play, play.layout.mine);
        press(&mut fingers, &mut controls, &play, play.layout.zoom_in);
        let input = controls.take_input();
        assert!(input.mine && !input.fire);
        assert_eq!(input.zoom, -ZOOM_STEP);
        // A mine still reloading and the zoom of first person do nothing.
        play.mine_ready = false;
        play.first_person = true;
        press(&mut fingers, &mut controls, &play, play.layout.mine);
        assert!(!controls.take_input().mine);
        let at = play.layout.zoom_out;
        let held = touch(5, TouchPhase::Started, f64::from(at.x), f64::from(at.y));
        fingers.route(&held, 1.0, Some(&play), |_| false, SCREEN, &mut controls);
        let input = controls.take_input();
        assert!(
            input.fire,
            "first person has no zoom keys: the finger aims there"
        );
        assert_eq!(input.zoom, 0.0);
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

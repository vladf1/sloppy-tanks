//! Keyboard, mouse and touch for the engine (`src/game/controls.ts`,
//! `touch-input.ts`): continuous input stays held, one-shot presses (a mine, an ammo
//! choice, a wheel step) queue until [`Controls::take_input`] hands them to the engine,
//! which applies them on its next simulation tick.

use std::time::Duration;

use sloppy_render::presentation::aim_assist::AimMode;
use sloppy_render::presentation::input::{InputFrame, STICK_RADIUS, ZOOM_STEP, stick_push};
use web_time::Instant;
use winit::event::{ElementState, MouseButton, MouseScrollDelta};
use winit::keyboard::KeyCode;

/// Ammunition slots, keys 1-5 (`AMMO_ORDER`).
const AMMO_SLOTS: u8 = 5;

/// The fingers that drive and fire (`TouchInput`): the drive stick's finger and its
/// push, the fire stick's finger, and the finger on the arena, which aims there and
/// fires while it is down.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TouchInput {
    /// The stick's finger and the point its push is measured from, in points.
    pub drive: Option<(u64, (f32, f32))>,
    /// The arena's finger and its last x in points, for first person's look.
    pub arena: Option<(u64, f64)>,
    /// The finger's offset from the stick's origin in stick radii (x right, y down);
    /// the knob follows it to the rim.
    pub knob: (f32, f32),
    /// The push past the dead zone, length ≤ 1 (x right, z down).
    pub push: (f64, f64),
    /// The fire stick's finger, while one holds it.
    pub fire: Option<FireFinger>,
    /// A shot the fire stick let go of: the turret keeps that aim and the gun fires
    /// until this time, so a tap or a release during the last moment of a reload
    /// still shoots.
    pub shot: Option<(Instant, AimMode, (f32, f32))>,
}

/// The fire stick (`crate::touch`): a tap fires at the tracked target, holding it still
/// keeps firing at that target, and a drag aims along its direction (pulled toward an
/// enemy near it) and fires when it lets go, unless it came back to the middle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FireFinger {
    pub id: u64,
    /// The point its drag is measured from, in points.
    pub origin: (f32, f32),
    pub since: Instant,
    /// The finger's offset from the origin in stick radii (x right, y down).
    pub knob: (f32, f32),
    pub stage: FireStage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FireStage {
    /// Just down: a tap if it lifts soon without dragging.
    Pressed,
    /// Held still: the gun fires at the tracked target until it lifts.
    Holding,
    /// Dragged out: it aims along the drag and fires on release.
    Aiming,
    /// Dragged back to the middle: lifting it fires nothing.
    Cancelled,
}

/// The fire stick: a drag this far out (stick radii) aims by hand, one back inside
/// `FIRE_CANCEL` drops the shot, and a press held this long without a drag fires.
const FIRE_AIM: f32 = 0.35;
const FIRE_CANCEL: f32 = 0.25;
const FIRE_HOLD: Duration = Duration::from_millis(220);
/// How long a released shot holds the trigger: several simulation ticks, short of the
/// quickest reload, so it fires once.
const FIRE_WINDOW: Duration = Duration::from_millis(200);

/// What a key press asks of the shell rather than the tank.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shortcut {
    /// Esc: pause, or leave a menu.
    Escape,
    /// V: first person.
    ToggleView,
    /// N: Stats for nerds, when Settings allows them.
    NerdStats,
    /// F11 or Ctrl/Cmd+F: fullscreen.
    Fullscreen,
}

#[derive(Default)]
pub struct Controls {
    up: [bool; 2],
    down: [bool; 2],
    left: [bool; 2],
    right: [bool; 2],
    fire_key: bool,
    fire_button: bool,
    mine: bool,
    ammo_slot: u8,
    ammo_step: i8,
    wheel_ammo: i8,
    zoom: f64,
    /// Pointer in NDC (y up).
    pointer: (f32, f32),
    /// Horizontal raw mouse travel since the last frame, while the cursor is grabbed.
    look: f64,
    pub toggle_view: bool,
    /// A modifier is held: keys are shortcuts, not controls.
    command: bool,
    pub touch: TouchInput,
    /// First person on touch: the stick's sideways push turns the view and only its
    /// forward push drives (no strafing).
    pub stick_turns: bool,
    /// Overhead touch play: the fire stick aims the turret (with the engine's assist)
    /// whenever no finger on the arena does.
    pub touch_aims: bool,
}

impl Controls {
    /// Clear held and queued input (a menu opened, the window lost focus, death). A
    /// finger still down then drives and fires nothing until it lifts.
    pub fn clear(&mut self) {
        let pointer = self.pointer;
        let command = self.command;
        let stick_turns = self.stick_turns;
        let touch_aims = self.touch_aims;
        *self = Self {
            pointer,
            command,
            stick_turns,
            touch_aims,
            ..Self::default()
        };
    }

    /// A finger landed on the drive stick: its push is measured from `origin`.
    pub fn begin_drive(&mut self, finger: u64, origin: (f32, f32), at: (f32, f32)) {
        if self.touch.drive.is_none() {
            self.touch.drive = Some((finger, origin));
            self.move_drive(finger, at);
        }
    }

    /// The stick's finger moved to `at` (points).
    pub fn move_drive(&mut self, finger: u64, at: (f32, f32)) {
        let Some((owner, origin)) = self.touch.drive else {
            return;
        };
        if owner != finger {
            return;
        }
        let x = (at.0 - origin.0) / STICK_RADIUS;
        let y = (at.1 - origin.1) / STICK_RADIUS;
        self.touch.knob = (x, y);
        self.touch.push = stick_push(f64::from(x), f64::from(y));
    }

    /// A finger landed on the fire stick at `at`, its drag measured from `origin`.
    pub fn begin_fire(&mut self, finger: u64, origin: (f32, f32), at: (f32, f32), now: Instant) {
        if self.touch.fire.is_none() {
            self.touch.fire = Some(FireFinger {
                id: finger,
                origin,
                since: now,
                knob: (0.0, 0.0),
                stage: FireStage::Pressed,
            });
            self.move_fire(finger, at);
        }
    }

    /// The fire stick's finger moved to `at` (points).
    pub fn move_fire(&mut self, finger: u64, at: (f32, f32)) {
        let Some(fire) = self.touch.fire.as_mut().filter(|fire| fire.id == finger) else {
            return;
        };
        fire.knob = (
            (at.0 - fire.origin.0) / STICK_RADIUS,
            (at.1 - fire.origin.1) / STICK_RADIUS,
        );
        let out = fire.knob.0.hypot(fire.knob.1);
        fire.stage = match fire.stage {
            _ if out >= FIRE_AIM => FireStage::Aiming,
            FireStage::Aiming if out < FIRE_CANCEL => FireStage::Cancelled,
            stage => stage,
        };
    }

    /// A finger landed on the open arena at `at` on a screen of `size` (points): the
    /// turret turns there and fires until it lifts.
    pub fn begin_arena(&mut self, finger: u64, at: (f64, f64), size: (f64, f64)) {
        if self.touch.arena.is_none() {
            self.touch.arena = Some((finger, at.0));
            self.aim_at(at, size);
        }
    }

    /// The arena's finger moved: it aims where it is, and its sideways travel turns
    /// the first-person view (the engine reads each in its own view only).
    pub fn move_arena(&mut self, finger: u64, at: (f64, f64), size: (f64, f64)) {
        let Some((owner, last_x)) = self.touch.arena else {
            return;
        };
        if owner != finger {
            return;
        }
        self.look += at.0 - last_x;
        self.touch.arena = Some((owner, at.0));
        self.aim_at(at, size);
    }

    /// A finger let go without lifting (a wreck took the controls, the system cancelled
    /// it): whatever it held springs back, and the fire stick fires nothing.
    pub fn clear_touch(&mut self, finger: u64) {
        if self.touch.fire.is_some_and(|fire| fire.id == finger) {
            self.touch.fire = None;
        }
        self.end_touch(finger, Instant::now());
    }

    /// A finger lifted: the stick it held springs back, or the shot it held stops, or
    /// the fire stick's tap or drag fires.
    pub fn end_touch(&mut self, finger: u64, now: Instant) {
        if let Some(fire) = self.touch.fire.filter(|fire| fire.id == finger) {
            self.touch.fire = None;
            let aim = match fire.stage {
                FireStage::Pressed => Some((AimMode::Track, (0.0, 0.0))),
                FireStage::Aiming => Some((AimMode::Manual, fire.knob)),
                FireStage::Holding | FireStage::Cancelled => None,
            };
            self.touch.shot = aim.map(|(mode, stick)| (now + FIRE_WINDOW, mode, stick));
        }
        if self.touch.drive.is_some_and(|(owner, _)| owner == finger) {
            self.touch.drive = None;
            self.touch.knob = (0.0, 0.0);
            self.touch.push = (0.0, 0.0);
        }
        if self.touch.arena.is_some_and(|(owner, _)| owner == finger) {
            self.touch.arena = None;
        }
    }

    /// The touch mine button.
    pub fn drop_mine(&mut self) {
        self.mine = true;
    }

    /// The touch zoom keys: metres out (positive) or in.
    pub fn zoom_by(&mut self, metres: f64) {
        self.zoom += metres;
    }

    /// Cmd on macOS, Ctrl on Windows (or Alt): game keys are not shortcuts then.
    pub fn set_command(&mut self, held: bool) {
        self.command = held;
    }

    /// A key went down or up; returns the shortcut a press asks for. `playing` gates
    /// the ammo keys, which only matter while the tank can fire.
    pub fn key(
        &mut self,
        code: KeyCode,
        state: ElementState,
        repeat: bool,
        playing: bool,
    ) -> Option<Shortcut> {
        let pressed = state == ElementState::Pressed;
        if pressed && (code == KeyCode::F11 || (self.command && code == KeyCode::KeyF)) {
            return (!repeat).then_some(Shortcut::Fullscreen);
        }
        if pressed && code == KeyCode::Escape {
            return (!repeat).then_some(Shortcut::Escape);
        }
        if pressed && self.command {
            return None;
        }
        match code {
            KeyCode::KeyW => self.up[0] = pressed,
            KeyCode::ArrowUp => self.up[1] = pressed,
            KeyCode::KeyS => self.down[0] = pressed,
            KeyCode::ArrowDown => self.down[1] = pressed,
            KeyCode::KeyA => self.left[0] = pressed,
            KeyCode::ArrowLeft => self.left[1] = pressed,
            KeyCode::KeyD => self.right[0] = pressed,
            KeyCode::ArrowRight => self.right[1] = pressed,
            KeyCode::Space => self.fire_key = pressed,
            KeyCode::KeyV if pressed && !repeat => return Some(Shortcut::ToggleView),
            KeyCode::KeyN if pressed && !repeat => return Some(Shortcut::NerdStats),
            KeyCode::KeyQ if pressed && !repeat && playing => self.ammo_step = -1,
            KeyCode::KeyE if pressed && !repeat && playing => self.ammo_step = 1,
            _ if pressed && !repeat && playing => {
                if let Some(slot) = ammo_slot(code) {
                    self.ammo_slot = slot;
                }
            }
            _ => {}
        }
        None
    }

    pub fn button(&mut self, button: MouseButton, state: ElementState) {
        let pressed = state == ElementState::Pressed;
        match button {
            MouseButton::Left => self.fire_button = pressed,
            MouseButton::Right if pressed => self.mine = true,
            _ => {}
        }
    }

    /// The wheel: Shift zooms, a plain notch steps the ammunition.
    pub fn wheel(&mut self, delta: MouseScrollDelta, shift: bool, playing: bool) {
        let (x, y) = match delta {
            MouseScrollDelta::LineDelta(x, y) => (f64::from(x), f64::from(y)),
            MouseScrollDelta::PixelDelta(position) => (position.x, position.y),
        };
        // Wheel down (towards the player) is positive in the page's `deltaY`.
        let down = -y;
        if shift {
            let amount = if down != 0.0 { down } else { -x };
            if amount != 0.0 {
                self.zoom += amount.signum() * ZOOM_STEP;
            }
        } else if down != 0.0 && playing {
            self.wheel_ammo = if down > 0.0 { 1 } else { -1 };
        }
    }

    /// The cursor or a finger at `(x, y)` in a window of `size` (both in pixels, or
    /// both in points).
    pub fn aim_at(&mut self, (x, y): (f64, f64), (width, height): (f64, f64)) {
        if width > 0.0 && height > 0.0 {
            self.pointer = (
                (x / width * 2.0 - 1.0) as f32,
                (1.0 - y / height * 2.0) as f32,
            );
        }
    }

    /// Raw horizontal mouse travel in logical pixels (first person look).
    pub fn look(&mut self, dx: f64) {
        self.look += dx;
    }

    /// Queue an ammo slot (1-5), as the HUD's rack does.
    pub fn select_ammo(&mut self, slot: u8) {
        self.ammo_slot = slot.min(AMMO_SLOTS);
    }

    /// This frame's raw control state; the one-shot presses are consumed.
    pub fn take_input(&mut self) -> InputFrame {
        self.take_input_at(Instant::now())
    }

    /// The fire stick's part of the frame at `now`: whether it fires, and what aims the
    /// turret. Touch play aims with the stick unless a finger on the arena does; first
    /// person aims with the view, and its fire stick only fires.
    fn fire_stick(&mut self, now: Instant) -> (bool, AimMode, (f32, f32)) {
        if let Some(fire) = self.touch.fire.as_mut()
            && fire.stage == FireStage::Pressed
            && now.duration_since(fire.since) >= FIRE_HOLD
        {
            fire.stage = FireStage::Holding;
        }
        self.touch.shot = self.touch.shot.filter(|(until, ..)| now < *until);
        let held = self.touch.fire.is_some();
        if !self.touch_aims || self.touch.arena.is_some() {
            return (
                held || self.touch.shot.is_some(),
                AimMode::Pointer,
                (0.0, 0.0),
            );
        }
        if let Some(fire) = self.touch.fire {
            return match fire.stage {
                FireStage::Pressed | FireStage::Holding => {
                    (fire.stage == FireStage::Holding, AimMode::Track, (0.0, 0.0))
                }
                FireStage::Aiming => (false, AimMode::Manual, fire.knob),
                FireStage::Cancelled => (false, AimMode::Track, (0.0, 0.0)),
            };
        }
        match self.touch.shot {
            Some((_, mode, stick)) => (true, mode, stick),
            None => (false, AimMode::Track, (0.0, 0.0)),
        }
    }

    /// [`Self::take_input`] at `now`.
    pub fn take_input_at(&mut self, now: Instant) -> InputFrame {
        let (stick_fire, aim_mode, aim_stick) = self.fire_stick(now);
        let (push_x, push_z) = self.touch.push;
        let frame = InputFrame {
            up: self.up.contains(&true),
            down: self.down.contains(&true),
            left: self.left.contains(&true),
            right: self.right.contains(&true),
            touch_move: (if self.stick_turns { 0.0 } else { push_x }, push_z),
            fire: self.fire_key || self.fire_button || self.touch.arena.is_some() || stick_fire,
            mine: self.mine,
            ammo_slot: self.ammo_slot,
            ammo_step: self.ammo_step,
            pointer: self.pointer,
            stick_turn: if self.stick_turns { push_x as f32 } else { 0.0 },
            look_pixels: self.look,
            zoom: self.zoom,
            toggle_view: self.toggle_view,
            wheel_ammo: self.wheel_ammo,
            aim_stick,
            aim_mode,
        };
        self.mine = false;
        self.ammo_slot = 0;
        self.ammo_step = 0;
        self.wheel_ammo = 0;
        self.zoom = 0.0;
        self.look = 0.0;
        self.toggle_view = false;
        frame
    }
}

/// Keys 1-5 on the main row or the number pad.
fn ammo_slot(code: KeyCode) -> Option<u8> {
    use KeyCode::*;
    match code {
        Digit1 | Numpad1 => Some(1),
        Digit2 | Numpad2 => Some(2),
        Digit3 | Numpad3 => Some(3),
        Digit4 | Numpad4 => Some(4),
        Digit5 | Numpad5 => Some(5),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_keys_drive_and_one_shots_last_one_frame() {
        let mut controls = Controls::default();
        controls.key(KeyCode::KeyW, ElementState::Pressed, false, true);
        controls.key(KeyCode::Digit3, ElementState::Pressed, false, true);
        controls.button(MouseButton::Right, ElementState::Pressed);
        let first = controls.take_input();
        assert!(first.up && first.mine);
        assert_eq!(first.ammo_slot, 3);
        let second = controls.take_input();
        assert!(second.up && !second.mine);
        assert_eq!(second.ammo_slot, 0);
    }

    #[test]
    fn either_key_of_a_pair_holds_until_both_are_up() {
        let mut controls = Controls::default();
        controls.key(KeyCode::KeyA, ElementState::Pressed, false, true);
        controls.key(KeyCode::ArrowLeft, ElementState::Pressed, false, true);
        controls.key(KeyCode::KeyA, ElementState::Released, false, true);
        assert!(controls.take_input().left);
        controls.key(KeyCode::ArrowLeft, ElementState::Released, false, true);
        assert!(!controls.take_input().left);
    }

    #[test]
    fn shift_wheel_zooms_and_plain_wheel_steps_ammo() {
        let mut controls = Controls::default();
        controls.wheel(MouseScrollDelta::LineDelta(0.0, -1.0), true, true);
        assert_eq!(controls.take_input().zoom, ZOOM_STEP);
        controls.wheel(MouseScrollDelta::LineDelta(0.0, 1.0), false, true);
        assert_eq!(controls.take_input().wheel_ammo, -1);
    }

    #[test]
    fn the_cursor_aims_in_device_coordinates() {
        let mut controls = Controls::default();
        controls.aim_at((0.0, 0.0), (200.0, 100.0));
        assert_eq!(controls.take_input().pointer, (-1.0, 1.0));
        controls.aim_at((150.0, 75.0), (200.0, 100.0));
        assert_eq!(controls.take_input().pointer, (0.5, -0.5));
    }

    #[test]
    fn the_stick_drives_past_its_dead_zone_and_springs_back() {
        let mut controls = Controls::default();
        controls.begin_drive(7, (100.0, 100.0), (100.0, 100.0 + STICK_RADIUS * 0.1));
        assert_eq!(controls.take_input().touch_move, (0.0, 0.0));
        // Another finger cannot take the stick over.
        controls.begin_drive(8, (0.0, 0.0), (0.0, 0.0));
        controls.move_drive(7, (100.0, 100.0 - STICK_RADIUS * 2.0));
        let (x, z) = controls.take_input().touch_move;
        assert_eq!(x, 0.0);
        assert!(
            (z + 1.0).abs() < 1e-9,
            "a push past the rim is full speed: {z}"
        );
        assert_eq!(controls.touch.knob, (0.0, -2.0));
        controls.end_touch(8, Instant::now());
        assert_ne!(controls.take_input().touch_move, (0.0, 0.0));
        controls.end_touch(7, Instant::now());
        assert_eq!(controls.take_input().touch_move, (0.0, 0.0));
    }

    #[test]
    fn first_person_turns_with_the_stick_instead_of_strafing() {
        let mut controls = Controls {
            stick_turns: true,
            ..Controls::default()
        };
        controls.begin_drive(1, (0.0, 0.0), (STICK_RADIUS, 0.0));
        let input = controls.take_input();
        assert_eq!(input.touch_move, (0.0, 0.0));
        assert_eq!(input.stick_turn, 1.0);
        controls.clear();
        assert!(
            controls.stick_turns,
            "a cleared frame keeps the view's mode"
        );
    }

    #[test]
    fn an_arena_finger_aims_fires_and_looks_until_it_lifts() {
        let mut controls = Controls::default();
        controls.begin_arena(3, (150.0, 75.0), (200.0, 100.0));
        controls.move_arena(3, (160.0, 75.0), (200.0, 100.0));
        let input = controls.take_input();
        assert!(input.fire);
        assert_eq!(input.pointer, (0.6, -0.5));
        assert_eq!(input.look_pixels, 10.0);
        controls.end_touch(3, Instant::now());
        assert!(!controls.take_input().fire);
    }

    #[test]
    fn a_cleared_finger_drives_nothing_until_it_lifts() {
        let mut controls = Controls::default();
        controls.begin_drive(1, (0.0, 0.0), (STICK_RADIUS, 0.0));
        controls.begin_arena(2, (10.0, 10.0), (100.0, 100.0));
        controls.clear();
        controls.move_drive(1, (0.0, STICK_RADIUS));
        controls.move_arena(2, (20.0, 10.0), (100.0, 100.0));
        let input = controls.take_input();
        assert_eq!(input.touch_move, (0.0, 0.0));
        assert!(!input.fire);
        assert_eq!(input.look_pixels, 0.0);
    }

    fn fire_stick() -> (Controls, Instant) {
        let controls = Controls {
            touch_aims: true,
            ..Controls::default()
        };
        (controls, Instant::now())
    }

    #[test]
    fn a_fire_stick_tap_fires_once_at_the_tracked_target() {
        let (mut controls, start) = fire_stick();
        assert_eq!(controls.take_input_at(start).aim_mode, AimMode::Track);
        controls.begin_fire(4, (0.0, 0.0), (3.0, 2.0), start);
        let pressed = controls.take_input_at(start + Duration::from_millis(50));
        assert!(!pressed.fire);
        let lift = start + Duration::from_millis(100);
        controls.end_touch(4, lift);
        let shot = controls.take_input_at(lift);
        assert!(shot.fire);
        assert_eq!(shot.aim_mode, AimMode::Track);
        let after = controls.take_input_at(lift + FIRE_WINDOW);
        assert!(!after.fire, "the trigger lets go after the window");
    }

    #[test]
    fn a_held_fire_stick_keeps_firing_until_it_lifts() {
        let (mut controls, start) = fire_stick();
        controls.begin_fire(4, (0.0, 0.0), (0.0, 0.0), start);
        let held = start + FIRE_HOLD;
        assert!(controls.take_input_at(held).fire);
        assert!(controls.take_input_at(held + Duration::from_secs(2)).fire);
        controls.end_touch(4, held + Duration::from_secs(2));
        assert!(
            !controls.take_input_at(held + Duration::from_secs(2)).fire,
            "lifting a hold fires no extra shot"
        );
    }

    #[test]
    fn a_fire_stick_drag_aims_fires_on_release_and_cancels_in_the_middle() {
        let (mut controls, start) = fire_stick();
        controls.begin_fire(4, (0.0, 0.0), (0.0, 0.0), start);
        controls.move_fire(4, (STICK_RADIUS, 0.0));
        // Held long enough to count as a hold, a drag still only aims.
        let input = controls.take_input_at(start + FIRE_HOLD * 2);
        assert_eq!(input.aim_mode, AimMode::Manual);
        assert_eq!(input.aim_stick, (1.0, 0.0));
        assert!(!input.fire);
        controls.end_touch(4, start + FIRE_HOLD * 2);
        let shot = controls.take_input_at(start + FIRE_HOLD * 2);
        assert!(shot.fire);
        assert_eq!(
            (shot.aim_mode, shot.aim_stick),
            (AimMode::Manual, (1.0, 0.0)),
            "the shot keeps the drag's aim"
        );
        // Dragged out and back to the middle: nothing fires.
        let later = start + Duration::from_secs(5);
        controls.begin_fire(5, (0.0, 0.0), (0.0, 0.0), later);
        controls.move_fire(5, (0.0, STICK_RADIUS));
        controls.move_fire(5, (0.0, STICK_RADIUS * 0.1));
        assert_eq!(controls.take_input_at(later).aim_mode, AimMode::Track);
        controls.end_touch(5, later);
        assert!(!controls.take_input_at(later).fire);
    }

    #[test]
    fn the_fire_stick_leaves_the_aim_to_a_mouse_or_an_arena_finger() {
        let (mut controls, start) = fire_stick();
        controls.begin_arena(6, (10.0, 10.0), (100.0, 100.0));
        assert_eq!(controls.take_input_at(start).aim_mode, AimMode::Pointer);
        controls.end_touch(6, start);
        controls.touch_aims = false;
        controls.begin_fire(4, (0.0, 0.0), (0.0, 0.0), start);
        let input = controls.take_input_at(start);
        assert_eq!(input.aim_mode, AimMode::Pointer);
        assert!(input.fire, "first person's fire stick fires at once");
    }

    #[test]
    fn modified_keys_are_shortcuts_not_controls() {
        let mut controls = Controls::default();
        controls.set_command(true);
        assert_eq!(
            controls.key(KeyCode::KeyF, ElementState::Pressed, false, true),
            Some(Shortcut::Fullscreen)
        );
        controls.key(KeyCode::KeyW, ElementState::Pressed, false, true);
        assert!(!controls.take_input().up);
    }
}

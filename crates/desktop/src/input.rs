//! Keyboard, mouse and touch for the engine (`src/game/controls.ts`,
//! `touch-input.ts`): continuous input stays held, one-shot presses (a mine, an ammo
//! choice, a wheel step) queue until [`Controls::take_input`] hands them to the engine,
//! which applies them on its next simulation tick.

use sloppy_render::presentation::input::{InputFrame, slot};
use winit::event::{ElementState, MouseButton, MouseScrollDelta};
use winit::keyboard::KeyCode;

/// Metres of zoom per Shift + wheel notch, and per touch zoom key.
pub const ZOOM_STEP: f64 = 2.0;
/// Ammunition slots, keys 1-5 (`AMMO_ORDER`).
const AMMO_SLOTS: u8 = 5;
/// Points from the drive stick's origin to a full push (`STICK_RADIUS`).
pub const STICK_RADIUS: f32 = 58.0;
/// The share of a full push that drives nothing, so a resting thumb stays put.
const STICK_DEADZONE: f64 = 0.12;

/// The fingers that drive and fire (`TouchInput`): the drive stick's finger and its
/// push, and the finger on the arena, which aims there and fires while it is down.
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
}

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
}

impl Controls {
    /// Clear held and queued input (a menu opened, the window lost focus, death). A
    /// finger still down then drives and fires nothing until it lifts.
    pub fn clear(&mut self) {
        let pointer = self.pointer;
        let command = self.command;
        let stick_turns = self.stick_turns;
        *self = Self {
            pointer,
            command,
            stick_turns,
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
        let distance = f64::from(x.hypot(y));
        let speed = ((distance.min(1.0) - STICK_DEADZONE) / (1.0 - STICK_DEADZONE)).max(0.0);
        self.touch.push = if distance > 0.0 {
            (
                f64::from(x) / distance * speed,
                f64::from(y) / distance * speed,
            )
        } else {
            (0.0, 0.0)
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

    /// A finger lifted: the stick it held springs back, or the shot it held stops.
    pub fn end_touch(&mut self, finger: u64) {
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
        let (push_x, push_z) = self.touch.push;
        let frame = InputFrame {
            up: self.up.contains(&true),
            down: self.down.contains(&true),
            left: self.left.contains(&true),
            right: self.right.contains(&true),
            touch_move: (if self.stick_turns { 0.0 } else { push_x }, push_z),
            fire: self.fire_key || self.fire_button || self.touch.arena.is_some(),
            mine: self.mine,
            ammo_slot: self.ammo_slot,
            ammo_step: self.ammo_step,
            pointer: self.pointer,
            stick_turn: if self.stick_turns { push_x as f32 } else { 0.0 },
            look_pixels: self.look,
            zoom: self.zoom,
            toggle_view: self.toggle_view,
            wheel_ammo: self.wheel_ammo,
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
        controls.end_touch(8);
        assert_ne!(controls.take_input().touch_move, (0.0, 0.0));
        controls.end_touch(7);
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
        controls.end_touch(3);
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

/// `frame`'s packed input (`sloppy_render::presentation::input::slot`).
pub fn pack(input: &InputFrame) -> [f32; slot::LENGTH] {
    let flag = |on: bool| if on { 1.0 } else { 0.0 };
    let mut packed = [0.0; slot::LENGTH];
    packed[slot::UP] = flag(input.up);
    packed[slot::DOWN] = flag(input.down);
    packed[slot::LEFT] = flag(input.left);
    packed[slot::RIGHT] = flag(input.right);
    packed[slot::TOUCH_MOVE_X] = input.touch_move.0 as f32;
    packed[slot::TOUCH_MOVE_Z] = input.touch_move.1 as f32;
    packed[slot::FIRE] = flag(input.fire);
    packed[slot::MINE] = flag(input.mine);
    packed[slot::AMMO_SLOT] = f32::from(input.ammo_slot);
    packed[slot::AMMO_STEP] = f32::from(input.ammo_step);
    packed[slot::POINTER_X] = input.pointer.0;
    packed[slot::POINTER_Y] = input.pointer.1;
    packed[slot::STICK_TURN] = input.stick_turn;
    packed[slot::LOOK_PIXELS] = input.look_pixels as f32;
    packed[slot::ZOOM] = input.zoom as f32;
    packed[slot::TOGGLE_VIEW] = flag(input.toggle_view);
    packed[slot::WHEEL_AMMO] = f32::from(input.wheel_ammo);
    packed
}

#[cfg(test)]
mod pack_tests {
    use super::*;

    #[test]
    fn packing_round_trips_through_the_engine_reader() {
        let mut controls = Controls::default();
        controls.key(KeyCode::KeyD, ElementState::Pressed, false, true);
        controls.key(KeyCode::KeyE, ElementState::Pressed, false, true);
        controls.aim_at((50.0, 25.0), (100.0, 100.0));
        controls.wheel(MouseScrollDelta::LineDelta(0.0, 1.0), true, true);
        let input = controls.take_input();
        assert_eq!(InputFrame::from_slice(&pack(&input)), input);
    }
}

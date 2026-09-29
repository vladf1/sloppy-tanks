//! Command building from `controls.ts` and `touch-input.ts`: the page forwards
//! raw control state once per frame as a packed `f32` array, and this turns it
//! into the tick's `VehicleCommand`. Continuous input (movement, fire) stays
//! held; one-shot actions (a mine, an ammo choice) queue until a simulation
//! tick consumes them, never once per rendered frame.

use sloppy_core::sim::ammunition::{AMMO_ORDER, AMMO_SCROLL_INTERVAL_MS};
use sloppy_core::sim::{AmmoSelection, VehicleCommand};

/// Slots of the packed input frame. Booleans are 0 or 1.
pub mod slot {
    /// W/↑, S/↓, A/←, D/→ held.
    pub const UP: usize = 0;
    pub const DOWN: usize = 1;
    pub const LEFT: usize = 2;
    pub const RIGHT: usize = 3;
    /// Touch drive stick, screen-relative (x right, z down), length ≤ 1.
    pub const TOUCH_MOVE_X: usize = 4;
    pub const TOUCH_MOVE_Z: usize = 5;
    /// Primary button or the touch aim stick past its fire ring.
    pub const FIRE: usize = 6;
    /// Mine presses since the last frame (right button, touch ✹).
    pub const MINE: usize = 7;
    /// Ammo slot chosen since the last frame: 1–5 (keys 1–5, HUD slots), 0 none.
    pub const AMMO_SLOT: usize = 8;
    /// Q/E since the last frame: -1 previous, +1 next, 0 none.
    pub const AMMO_STEP: usize = 9;
    /// Pointer in NDC over the canvas (x right, y up, -1..1).
    pub const POINTER_X: usize = 10;
    pub const POINTER_Y: usize = 11;
    /// 1 while the touch aim stick decides the aim (the mouse moved last: 0).
    pub const TOUCH_AIMING: usize = 12;
    /// Touch aim stick direction (unit, screen-relative, y down).
    pub const TOUCH_AIM_X: usize = 13;
    pub const TOUCH_AIM_Y: usize = 14;
    /// 1 while a finger holds the aim stick (first person turns with it).
    pub const AIM_STICK_HELD: usize = 15;
    /// Horizontal mouse travel in CSS pixels since the last frame, for first
    /// person; the page leaves it 0 while a freed cursor waits for a click.
    pub const LOOK_PIXELS: usize = 16;
    /// Zoom change in metres (shift-wheel ±2, pinch).
    pub const ZOOM: usize = 17;
    /// 1 when V or the view button toggled first person.
    pub const TOGGLE_VIEW: usize = 18;
    /// Plain wheel since the last frame: -1 previous ammo, +1 next, 0 none.
    /// Throttled here like the TypeScript scroll interval.
    pub const WHEEL_AMMO: usize = 19;
    pub const LENGTH: usize = 20;
}

/// One frame of raw control state.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InputFrame {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub touch_move: (f64, f64),
    pub fire: bool,
    pub mine: bool,
    pub ammo_slot: u8,
    pub ammo_step: i8,
    pub pointer: (f32, f32),
    pub touch_aiming: bool,
    pub touch_aim: (f32, f32),
    pub aim_stick_held: bool,
    pub look_pixels: f64,
    pub zoom: f64,
    pub toggle_view: bool,
    pub wheel_ammo: i8,
}

fn sign(value: f32) -> i8 {
    if value > 0.0 {
        1
    } else if value < 0.0 {
        -1
    } else {
        0
    }
}

impl InputFrame {
    /// Read a packed frame; missing slots read as 0 and non-finite values as 0.
    pub fn from_slice(data: &[f32]) -> Self {
        let get = |index: usize| {
            data.get(index)
                .copied()
                .filter(|value| value.is_finite())
                .unwrap_or(0.0)
        };
        let on = |index: usize| get(index) != 0.0;
        Self {
            up: on(slot::UP),
            down: on(slot::DOWN),
            left: on(slot::LEFT),
            right: on(slot::RIGHT),
            touch_move: (get(slot::TOUCH_MOVE_X) as f64, get(slot::TOUCH_MOVE_Z) as f64),
            fire: on(slot::FIRE),
            mine: get(slot::MINE) > 0.0,
            ammo_slot: get(slot::AMMO_SLOT).clamp(0.0, AMMO_ORDER.len() as f32) as u8,
            ammo_step: sign(get(slot::AMMO_STEP)),
            pointer: (
                get(slot::POINTER_X).clamp(-1.0, 1.0),
                get(slot::POINTER_Y).clamp(-1.0, 1.0),
            ),
            touch_aiming: on(slot::TOUCH_AIMING),
            touch_aim: (get(slot::TOUCH_AIM_X), get(slot::TOUCH_AIM_Y)),
            aim_stick_held: on(slot::AIM_STICK_HELD),
            look_pixels: get(slot::LOOK_PIXELS) as f64,
            zoom: get(slot::ZOOM) as f64,
            toggle_view: on(slot::TOGGLE_VIEW),
            wheel_ammo: sign(get(slot::WHEEL_AMMO)),
        }
    }
}

/// Queued one-shot actions between frames and ticks (`Controls` state).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CommandBuilder {
    mine: bool,
    ammo_selection: Option<AmmoSelection>,
    last_ammo_scroll_ms: Option<f64>,
}

impl CommandBuilder {
    /// Queue this frame's one-shot actions. `active` is whether the player may
    /// act (playing and alive); inactive presses are dropped, like the page's
    /// input handlers did.
    pub fn queue(&mut self, input: &InputFrame, active: bool, now_ms: f64) {
        if !active {
            return;
        }
        if input.mine {
            self.mine = true;
        }
        if input.ammo_slot > 0 {
            self.ammo_selection = Some(AmmoSelection::Weapon(
                AMMO_ORDER[input.ammo_slot as usize - 1],
            ));
        } else if input.ammo_step != 0 {
            self.ammo_selection = Some(AmmoSelection::Step(input.ammo_step));
        } else if input.wheel_ammo != 0
            && self
                .last_ammo_scroll_ms
                .is_none_or(|last| now_ms - last >= AMMO_SCROLL_INTERVAL_MS)
        {
            self.ammo_selection = Some(AmmoSelection::Step(input.wheel_ammo));
            self.last_ammo_scroll_ms = Some(now_ms);
        }
    }

    /// Drop queued actions (pause, death, results, a new round).
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// The tick's command, consuming queued one-shot actions. Keys win over the
    /// touch stick on each axis.
    pub fn command(&mut self, input: &InputFrame, aim: f64, active: bool) -> VehicleCommand {
        let axis = |positive: bool, negative: bool, touch: f64| {
            let keys = f64::from(positive as u8) - f64::from(negative as u8);
            if keys != 0.0 { keys } else { touch }
        };
        let mine = std::mem::take(&mut self.mine);
        let ammo_selection = self.ammo_selection.take().filter(|_| active);
        VehicleCommand {
            move_x: axis(input.right, input.left, input.touch_move.0),
            move_z: axis(input.down, input.up, input.touch_move.1),
            aim,
            fire: input.fire,
            mine,
            ammo_selection,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_core::sim::Weapon;

    fn packed(values: &[(usize, f32)]) -> Vec<f32> {
        let mut data = vec![0.0; slot::LENGTH];
        for &(index, value) in values {
            data[index] = value;
        }
        data
    }

    #[test]
    fn keys_win_over_the_touch_stick_and_one_shots_fire_once() {
        let input = InputFrame::from_slice(&packed(&[
            (slot::UP, 1.0),
            (slot::TOUCH_MOVE_X, 0.5),
            (slot::TOUCH_MOVE_Z, 0.25),
            (slot::MINE, 1.0),
            (slot::AMMO_SLOT, 3.0),
            (slot::FIRE, 1.0),
        ]));
        let mut builder = CommandBuilder::default();
        builder.queue(&input, true, 0.0);
        let command = builder.command(&input, 1.5, true);
        assert_eq!(command.move_x, 0.5);
        assert_eq!(command.move_z, -1.0);
        assert!(command.fire && command.mine);
        assert_eq!(command.aim, 1.5);
        assert_eq!(
            command.ammo_selection,
            Some(AmmoSelection::Weapon(Weapon::Rocket))
        );
        // A second tick in the same frame keeps held input but not the one-shots.
        let again = builder.command(&input, 1.5, true);
        assert!(!again.mine && again.ammo_selection.is_none() && again.fire);
    }

    #[test]
    fn inactive_players_queue_nothing_and_the_wheel_is_throttled() {
        let wheel = InputFrame::from_slice(&packed(&[(slot::WHEEL_AMMO, 3.0), (slot::MINE, 1.0)]));
        let mut builder = CommandBuilder::default();
        builder.queue(&wheel, false, 0.0);
        assert_eq!(builder, CommandBuilder::default());
        builder.queue(&wheel, true, 1000.0);
        assert_eq!(
            builder.command(&wheel, 0.0, true).ammo_selection,
            Some(AmmoSelection::Step(1))
        );
        builder.queue(&wheel, true, 1050.0);
        assert!(builder.command(&wheel, 0.0, true).ammo_selection.is_none());
        builder.queue(&wheel, true, 1130.0);
        assert!(builder.command(&wheel, 0.0, true).ammo_selection.is_some());
        assert!(InputFrame::from_slice(&[f32::NAN]) == InputFrame::default());
    }
}

//! When the client sends input (`src/net/input-cadence.ts`): 20 Hz while anything is held
//! or changing, a 1 Hz refresh while idle so the seat is kept without renewing held
//! controls at a slower rate. A key press or release goes out without waiting for the
//! next 20 Hz slot, since that wait adds directly to how late the tank responds.

use super::player_controls::{Aim, ControlInput};

const ACTIVE_INTERVAL_MS: f64 = 50.0;
/// The shortest gap before a control change goes out. It bounds the send rate at 40 a
/// second, clear of the server's 60 input messages a second (`MAX_INPUTS_PER_SECOND`).
const CONTROL_CHANGE_INTERVAL_MS: f64 = 25.0;
const IDLE_INTERVAL_MS: f64 = 1000.0;
const AIM_POSITION_EPSILON: f64 = 0.01;
const AIM_ANGLE_EPSILON: f64 = 0.001;

#[derive(Clone, Debug)]
pub struct InputCadence {
    previous: Option<ControlInput>,
    sent_at: f64,
}

impl Default for InputCadence {
    fn default() -> Self {
        Self {
            previous: None,
            sent_at: f64::NEG_INFINITY,
        }
    }
}

impl InputCadence {
    /// Whether `input` should be sent at `now_ms`.
    pub fn due(&self, input: &ControlInput, now_ms: f64) -> bool {
        let elapsed = now_ms - self.sent_at;
        if elapsed < CONTROL_CHANGE_INTERVAL_MS {
            return false;
        }
        let Some(previous) = &self.previous else {
            return true;
        };
        if control_changed(input, previous) {
            return true;
        }
        if elapsed < ACTIVE_INTERVAL_MS {
            return false;
        }
        if input.control_epoch != previous.control_epoch
            || input.move_x != 0.0
            || input.move_z != 0.0
            || input.fire
            || !input.actions.is_empty()
            || input.move_x != previous.move_x
            || input.move_z != previous.move_z
            || input.fire != previous.fire
            || elapsed >= IDLE_INTERVAL_MS
        {
            return true;
        }
        match (input.aim, previous.aim) {
            (Aim::Angle(angle), Aim::Angle(old)) => {
                let difference = angle - old;
                difference.sin().atan2(difference.cos()).abs() >= AIM_ANGLE_EPSILON
            }
            (Aim::Point { x, z }, Aim::Point { x: old_x, z: old_z }) => {
                (x - old_x).hypot(z - old_z) >= AIM_POSITION_EPSILON
            }
            _ => true,
        }
    }

    /// Records that `input` went out at `now_ms`.
    pub fn sent(&mut self, input: &ControlInput, now_ms: f64) {
        self.previous = Some(ControlInput {
            actions: Vec::new(),
            ..input.clone()
        });
        self.sent_at = now_ms;
    }
}

/// A press or release: a one-shot action, fire toggling, or a movement axis starting,
/// stopping or reversing. Aim and a stick's steady drift keep the 20 Hz cadence.
fn control_changed(input: &ControlInput, previous: &ControlInput) -> bool {
    let direction = |value: f64| (value > 0.0, value < 0.0);
    !input.actions.is_empty()
        || input.fire != previous.fire
        || direction(input.move_x) != direction(previous.move_x)
        || direction(input.move_z) != direction(previous.move_z)
}

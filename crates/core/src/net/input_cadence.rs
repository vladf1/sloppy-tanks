//! When the client sends input (`src/net/input-cadence.ts`): 20 Hz while anything is held
//! or changing, a 1 Hz refresh while idle so the seat is kept without renewing held
//! controls at a slower rate.

use super::player_controls::{Action, Aim};

const ACTIVE_INTERVAL_MS: f64 = 50.0;
const IDLE_INTERVAL_MS: f64 = 1000.0;
const AIM_POSITION_EPSILON: f64 = 0.01;
const AIM_ANGLE_EPSILON: f64 = 0.001;

/// One input sample before it is numbered and stamped with the observed tick.
#[derive(Clone, Debug, PartialEq)]
pub struct InputSample {
    pub control_epoch: u64,
    pub move_x: f64,
    pub move_z: f64,
    pub aim: Aim,
    pub fire: bool,
    pub actions: Vec<Action>,
}

#[derive(Clone, Debug)]
pub struct InputCadence {
    previous: Option<InputSample>,
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
    pub fn due(&self, input: &InputSample, now_ms: f64) -> bool {
        let elapsed = now_ms - self.sent_at;
        if elapsed < ACTIVE_INTERVAL_MS {
            return false;
        }
        let Some(previous) = &self.previous else {
            return true;
        };
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
    pub fn sent(&mut self, input: &InputSample, now_ms: f64) {
        self.previous = Some(InputSample {
            actions: Vec::new(),
            ..input.clone()
        });
        self.sent_at = now_ms;
    }
}

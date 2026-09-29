//! Port of `tank-suspension.ts`: render-only hull pitch and roll from the body's
//! acceleration. Sampling simulation time avoids pulses between physics ticks.

/// 2 degrees, including sudden stops and knockback.
const MAX_PITCH: f64 = 0.035;
/// 1.4 degrees.
const MAX_ROLL: f64 = 0.025;
const RESPONSE: f64 = 18.0;
/// Radians of pitch and roll per m/s² of forward and sideways acceleration.
const PITCH_PER_ACCELERATION: f64 = 0.001125;
const ROLL_PER_ACCELERATION: f64 = 0.00175;
/// A longer sampling gap (a pause, a respawn) restarts from level.
const MAX_SAMPLE_GAP: f64 = 0.2;
const MAX_FRAME_TIME: f64 = 0.1;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SuspensionAxis {
    pub angle: f64,
    velocity: f64,
}

impl SuspensionAxis {
    /// Exact critically damped spring: soft settling without frame-rate-dependent wobble.
    fn step(&mut self, target: f64, dt: f64) {
        let offset = self.angle - target;
        let impulse = self.velocity + RESPONSE * offset;
        let decay = (-RESPONSE * dt).exp();
        self.angle = target + (offset + impulse * dt) * decay;
        self.velocity = (self.velocity - RESPONSE * impulse * dt) * decay;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TankSuspension {
    pub pitch: SuspensionAxis,
    pub roll: SuspensionAxis,
    sample_time: Option<f64>,
    vx: f64,
    vz: f64,
    pitch_target: f64,
    roll_target: f64,
}

impl TankSuspension {
    /// `time` is simulation time; `dt` the frame time (0 while not playing).
    pub fn update(&mut self, vx: f64, vz: f64, heading: f64, time: f64, dt: f64) {
        let elapsed = self.sample_time.map_or(0.0, |sample| time - sample);
        if self.sample_time.is_none() || !(0.0..=MAX_SAMPLE_GAP).contains(&elapsed) {
            self.pitch_target = 0.0;
            self.roll_target = 0.0;
            self.vx = vx;
            self.vz = vz;
            self.sample_time = Some(time);
        } else if elapsed > 0.0 {
            let ax = (vx - self.vx) / elapsed;
            let az = (vz - self.vz) / elapsed;
            let (sin, cos) = heading.sin_cos();
            self.pitch_target =
                (-(ax * sin + az * cos) * PITCH_PER_ACCELERATION).clamp(-MAX_PITCH, MAX_PITCH);
            self.roll_target =
                ((ax * cos - az * sin) * ROLL_PER_ACCELERATION).clamp(-MAX_ROLL, MAX_ROLL);
            self.vx = vx;
            self.vz = vz;
            self.sample_time = Some(time);
        }
        let frame_time = dt.clamp(0.0, MAX_FRAME_TIME);
        self.pitch.step(self.pitch_target, frame_time);
        self.roll.step(self.roll_target, frame_time);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn braking_pitches_forward_and_settles() {
        let mut suspension = TankSuspension::default();
        suspension.update(0.0, 10.0, 0.0, 0.0, 1.0 / 60.0);
        assert_eq!(suspension.pitch.angle, 0.0);
        // Decelerating along +Z (heading 0) pitches the nose down (positive angle).
        suspension.update(0.0, 0.0, 0.0, 0.1, 1.0 / 60.0);
        assert!(suspension.pitch.angle > 0.0);
        for i in 0..200 {
            suspension.update(0.0, 0.0, 0.0, 0.1 + i as f64 / 60.0, 1.0 / 60.0);
        }
        assert!(suspension.pitch.angle.abs() < 1e-4);
        assert!(suspension.pitch.angle <= MAX_PITCH);
    }

    #[test]
    fn a_long_gap_restarts_level() {
        let mut suspension = TankSuspension::default();
        suspension.update(0.0, 0.0, 0.0, 0.0, 0.016);
        suspension.update(30.0, 0.0, 0.0, 5.0, 0.016);
        assert_eq!(suspension.roll.angle, 0.0);
    }
}

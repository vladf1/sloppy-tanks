//! Render-only hull pitch and roll: the body leans against its acceleration on
//! sprung road wheels, rocks back when its gun fires and away from a hit. An
//! underdamped spring gives the hull weight: it overshoots once and settles,
//! like a heavy toy on soft springs. Sampling simulation time avoids pulses
//! between physics ticks.

use super::view_settings::TANK_FEEL;

/// About 6 degrees of steady lean from driving, including sudden stops and knockback.
const MAX_PITCH: f64 = 0.1;
/// About 5 degrees of body roll in a hard turn.
const MAX_ROLL: f64 = 0.085;
/// Kicks may throw the hull further than steady driving leans it.
const MAX_KICK_ANGLE: f64 = 0.2;
/// Spring angular frequency (rad/s) and damping ratio: one soft overshoot.
const STIFFNESS: f64 = 11.0;
const DAMPING_RATIO: f64 = 0.42;
/// Radians of pitch and roll per m/s² of forward and sideways acceleration.
const PITCH_PER_ACCELERATION: f64 = 0.0034;
const ROLL_PER_ACCELERATION: f64 = 0.0034;
/// The spring integrates in substeps no longer than this, so any frame rate
/// gives the same motion.
const SUBSTEP: f64 = 1.0 / 240.0;
/// Below this offset (rad) and speed (rad/s) the hull comes to rest.
const SETTLE_ANGLE: f64 = 2e-4;
const SETTLE_VELOCITY: f64 = 4e-3;
/// A longer sampling gap (a pause, a respawn) restarts from level.
const MAX_SAMPLE_GAP: f64 = 0.2;
const MAX_FRAME_TIME: f64 = 0.1;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SuspensionAxis {
    pub angle: f64,
    velocity: f64,
}

impl SuspensionAxis {
    /// Semi-implicit spring step toward `target`.
    fn step(&mut self, target: f64, dt: f64) {
        let mut remaining = dt;
        while remaining > 0.0 {
            let h = remaining.min(SUBSTEP);
            let acceleration = -STIFFNESS * STIFFNESS * (self.angle - target)
                - 2.0 * DAMPING_RATIO * STIFFNESS * self.velocity;
            self.velocity += acceleration * h;
            self.angle = (self.angle + self.velocity * h).clamp(-MAX_KICK_ANGLE, MAX_KICK_ANGLE);
            remaining -= h;
        }
        // An imperceptible remainder rests exactly on the target instead of
        // trembling on for seconds.
        if (self.angle - target).abs() < SETTLE_ANGLE && self.velocity.abs() < SETTLE_VELOCITY {
            self.angle = target;
            self.velocity = 0.0;
        }
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

    /// Throw the top of the hull toward world direction `(dx, dz)` with angular
    /// speed `strength` (rad/s): a gun's recoil pushes it away from the muzzle, a
    /// hit away from the shot.
    pub fn kick(&mut self, dx: f64, dz: f64, heading: f64, strength: f64) {
        let (sin, cos) = heading.sin_cos();
        // Positive pitch dips the nose (toward +forward); positive roll leans the
        // top toward the hull's -X side.
        self.pitch.velocity += (dx * sin + dz * cos) * strength;
        self.roll.velocity -= (dx * cos - dz * sin) * strength;
    }
}

/// A planar offset on an underdamped spring: the whole tank shoved by a hit.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Shove {
    pub x: f64,
    pub z: f64,
    vx: f64,
    vz: f64,
}

impl Shove {
    pub fn kick(&mut self, vx: f64, vz: f64) {
        self.vx += vx;
        self.vz += vz;
    }

    pub fn step(&mut self, dt: f64) {
        let mut remaining = dt.clamp(0.0, MAX_FRAME_TIME);
        let (frequency, damping) = (TANK_FEEL.shove_frequency, TANK_FEEL.shove_damping);
        while remaining > 0.0 {
            let h = remaining.min(SUBSTEP);
            self.vx += (-frequency * frequency * self.x - 2.0 * damping * frequency * self.vx) * h;
            self.vz += (-frequency * frequency * self.z - 2.0 * damping * frequency * self.vz) * h;
            self.x += self.vx * h;
            self.z += self.vz * h;
            remaining -= h;
        }
    }
}

/// Everything a tank's drawn pose adds to its simulated one: suspension, the
/// shove and flash of a hit, a shot waiting to rock the hull, and the distance its
/// treads have rolled.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TankMotion {
    pub suspension: TankSuspension,
    pub shove: Shove,
    /// Hit flash brightness (0..`max_flash`).
    pub flash: f64,
    /// A shot fired since the last pose: its rock strength (rad/s).
    pub pending_fire: f64,
    /// A hit since the last pose: the direction it pushes and its rock strength.
    pub pending_hit: Option<(f64, f64, f64)>,
    /// Signed metres driven, for the tread scroll.
    pub travel: f64,
    /// Seconds since the drawn gun last fired (`None` before its first shot).
    pub gun_age: Option<f64>,
}

impl TankMotion {
    /// The tank took `damage` from a shot or blast at `origin`, while at `at`.
    pub fn hit(&mut self, at: (f64, f64), origin: Option<(f64, f64)>, damage: f64) {
        self.flash = (self.flash + damage * TANK_FEEL.flash_per_damage).min(TANK_FEEL.max_flash);
        let Some(origin) = origin else { return };
        let (dx, dz) = (at.0 - origin.0, at.1 - origin.1);
        let length = dx.hypot(dz);
        if length < 1e-3 {
            return;
        }
        let (dx, dz) = (dx / length, dz / length);
        let shove = (damage * TANK_FEEL.hit_shove_per_damage).min(TANK_FEEL.max_hit_shove);
        self.shove.kick(dx * shove, dz * shove);
        let rock = (damage * TANK_FEEL.hit_rock_per_damage).min(TANK_FEEL.max_hit_rock);
        self.pending_hit = Some((dx, dz, rock));
    }

    /// Advance a live tank's pose by `dt` of play: apply the shot and hit waiting
    /// since the last frame, settle the springs and roll the treads.
    pub fn update(&mut self, tank: &MotionInput, dt: f64) {
        let (sin, cos) = tank.heading.sin_cos();
        if self.pending_fire > 0.0 {
            // The recoil throws the turret's top back from the muzzle.
            let (aim_sin, aim_cos) = tank.aim.sin_cos();
            self.suspension
                .kick(-aim_sin, -aim_cos, tank.heading, self.pending_fire);
            // And shoves the whole tank back a hand's width on its springs.
            let shove = TANK_FEEL.fire_shove * self.pending_fire / TANK_FEEL.fire_rock;
            self.shove.kick(-aim_sin * shove, -aim_cos * shove);
            self.pending_fire = 0.0;
            self.gun_age = Some(0.0);
        }
        if let Some(age) = self.gun_age.as_mut() {
            *age += dt.max(0.0);
        }
        if let Some((dx, dz, rock)) = self.pending_hit.take() {
            self.suspension.kick(dx, dz, tank.heading, rock);
        }
        self.suspension
            .update(tank.vx, tank.vz, tank.heading, tank.time, dt);
        self.shove.step(dt);
        self.flash *= (-TANK_FEEL.flash_decay * dt.max(0.0)).exp();
        self.travel += (tank.vx * sin + tank.vz * cos) * dt.max(0.0);
    }

    /// The drawn gun's recoil (0..1): a slam back, a beat held, and an eased run
    /// out, at least as far back as the simulation's own `recoil`.
    pub fn gun_recoil(&self, recoil: f64) -> f64 {
        let simulated = recoil.clamp(0.0, 1.0);
        let simulated = simulated * simulated * (3.0 - 2.0 * simulated);
        let Some(age) = self.gun_age else {
            return simulated;
        };
        let (slam, hold, out) = (
            TANK_FEEL.recoil_slam,
            TANK_FEEL.recoil_hold,
            TANK_FEEL.recoil_return,
        );
        let drawn = if age < slam {
            age / slam
        } else if age < slam + hold {
            1.0
        } else {
            let t = ((age - slam - hold) / out).min(1.0);
            // Ease out: quick at first, settling softly into battery.
            (1.0 - t).powi(3)
        };
        drawn.max(simulated)
    }

    /// The top-run tread offset in model units.
    pub fn tread_scroll(&self) -> f64 {
        (self.travel * TANK_FEEL.track_scroll).rem_euclid(TANK_FEEL.track_period)
    }

    /// The instance tint: white-hot at the hit, cooling through a warm glow.
    pub fn tint(&self) -> [f32; 3] {
        let [r, g, b] = TANK_FEEL.flash_color;
        let flash = self.flash;
        // The glow warms as it fades: blue and green cool first.
        [
            (1.0 + r * flash) as f32,
            (1.0 + g * flash * flash.sqrt()) as f32,
            (1.0 + b * flash * flash) as f32,
        ]
    }
}

/// What a tank's motion reads from the frame's state.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MotionInput {
    pub vx: f64,
    pub vz: f64,
    pub heading: f64,
    pub aim: f64,
    /// Simulation time.
    pub time: f64,
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
        assert!(suspension.pitch.angle <= MAX_KICK_ANGLE);
    }

    #[test]
    fn firing_forward_lifts_the_nose_and_a_side_shot_rolls_away() {
        let mut forward = TankSuspension::default();
        forward.kick(0.0, -1.0, 0.0, 1.0);
        forward.update(0.0, 0.0, 0.0, 0.0, 0.05);
        assert!(forward.pitch.angle < 0.0 && forward.roll.angle.abs() < 1e-9);
        let mut side = TankSuspension::default();
        // The top thrown toward -X rolls positive.
        side.kick(-1.0, 0.0, 0.0, 1.0);
        side.update(0.0, 0.0, 0.0, 0.0, 0.05);
        assert!(side.roll.angle > 0.0 && side.pitch.angle.abs() < 1e-9);
    }

    #[test]
    fn a_hit_shoves_away_from_the_shot_and_settles() {
        let mut motion = TankMotion::default();
        motion.hit((0.0, 0.0), Some((-5.0, 0.0)), 40.0);
        assert!(motion.flash > 0.0);
        let input = MotionInput::default();
        motion.update(&input, 0.05);
        assert!(motion.shove.x > 0.0 && motion.shove.z.abs() < 1e-9);
        for _ in 0..120 {
            motion.update(&input, 1.0 / 60.0);
        }
        assert!(motion.shove.x.abs() < 1e-3 && motion.flash < 1e-3);
    }

    #[test]
    fn a_long_gap_restarts_level() {
        let mut suspension = TankSuspension::default();
        suspension.update(0.0, 0.0, 0.0, 0.0, 0.016);
        suspension.update(30.0, 0.0, 0.0, 5.0, 0.016);
        assert_eq!(suspension.roll.angle, 0.0);
    }
}

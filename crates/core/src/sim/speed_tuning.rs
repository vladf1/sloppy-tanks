//! Local playtest speed controls. They belong to this world; multiplayer uses the defaults.

use super::simulation::Simulation;
use super::tank_lifecycle::soft_ccd_prediction;

const MIN_SCALE: f64 = 0.5;
const MAX_SCALE: f64 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeedSetting {
    TankSpeed,
    BulletSpeed,
}

/// Apply a speed scale (clamped to 0.5–2, or 1 online) and return the scale in effect.
pub fn tune_speed(simulation: &mut Simulation, key: SpeedSetting, value: f64) -> f64 {
    let scale = if simulation.multiplayer() {
        1.0
    } else if value.is_finite() {
        MIN_SCALE.max(MAX_SCALE.min(value))
    } else {
        1.0
    };
    match key {
        SpeedSetting::TankSpeed => {
            simulation.speed_tuning.tank_speed = scale;
            for tank in simulation.tanks.iter().filter(|tank| tank.alive) {
                simulation.world.bodies[tank.body]
                    .set_soft_ccd_prediction(soft_ccd_prediction(tank.kind, scale) as f32);
            }
        }
        SpeedSetting::BulletSpeed => {
            let previous = simulation.speed_tuning.bullet_speed;
            simulation.speed_tuning.bullet_speed = scale;
            for shot in &mut simulation.shots {
                shot.vx *= scale / previous;
                shot.vz *= scale / previous;
            }
        }
    }
    scale
}

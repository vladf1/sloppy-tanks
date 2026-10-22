//! Shell tracers: a glowing streak in the firing team's color behind every shell,
//! pellet and disc in flight, with a hot glow on its head, so shots read from the
//! top-down camera as bright lines across the arena. A tracer grows out of the
//! muzzle rather than reaching back into the tank that fired it. Written into
//! the additive streak pool after the sparks each frame.

use glam::Vec3;
use sloppy_core::sim::Weapon;
use sloppy_core::sim::data::TEAM_COLORS;
use sloppy_core::sim::render_state::RenderShot;

use super::pool::PoolBuffer;
use super::projectiles::{PROJECTILE_CAPACITY, shot_position};
use super::sparks::{glow, streak};
use crate::color::hex_to_linear;

/// How hot a tracer burns over its team's color, and the white it adds.
const TRACER_GAIN: f32 = 3.2;
const TRACER_WHITE: f32 = 0.9;
const TRACER_OPACITY: f32 = 0.85;

/// A munition's tracer: streak length and width, and head glow diameter (m).
fn tracer(kind: Weapon) -> Option<(f32, f32, f32)> {
    match kind {
        Weapon::Standard => Some((3.0, 0.2, 0.75)),
        Weapon::Spread => Some((1.5, 0.15, 0.5)),
        Weapon::Piercing => Some((4.0, 0.15, 0.6)),
        Weapon::Ricochet => Some((1.8, 0.2, 0.75)),
        // Missiles carry their own flame and smoke trail.
        Weapon::Rocket | Weapon::Tow => None,
    }
}

#[derive(Clone, Debug)]
pub struct Tracers {
    /// Where each shell in flight was first drawn, by shot id, so its tracer
    /// grows from there; reused every frame.
    starts: Vec<(u32, Vec3)>,
    colors: [[f32; 3]; 2],
}

impl Default for Tracers {
    fn default() -> Self {
        Self {
            starts: Vec::with_capacity(PROJECTILE_CAPACITY),
            colors: TEAM_COLORS
                .map(|hex| hex_to_linear(hex).map(|c| c * TRACER_GAIN + TRACER_WHITE)),
        }
    }
}

impl Tracers {
    pub fn reset(&mut self) {
        self.starts.clear();
    }

    pub fn update(&mut self, shots: &[RenderShot], alpha: f64, streaks: &mut PoolBuffer) {
        // Forget shells no longer in flight; a ricochet keeps its id and is
        // re-anchored where it bounced (a sharp turn in its velocity).
        self.starts
            .retain(|(id, _)| shots.iter().any(|shot| shot.id == *id));
        for shot in shots.iter().take(PROJECTILE_CAPACITY) {
            let Some((length, width, head)) = tracer(shot.weapon) else {
                continue;
            };
            let speed = shot.vx.hypot(shot.vz);
            if speed <= 0.0 {
                continue;
            }
            let direction = Vec3::new((shot.vx / speed) as f32, 0.0, (shot.vz / speed) as f32);
            let position = shot_position(shot, alpha);
            let start = match self.starts.iter_mut().find(|(id, _)| *id == shot.id) {
                Some((_, start)) => {
                    // A bounce leaves the start off the shell's line: start over.
                    let back = position - *start;
                    if back.normalize_or_zero().dot(direction) < 0.9 {
                        *start = position;
                    }
                    *start
                }
                None => {
                    if self.starts.len() < PROJECTILE_CAPACITY {
                        self.starts.push((shot.id, position));
                    }
                    position
                }
            };
            let length = length.min(position.distance(start)).max(width);
            let [r, g, b] = self.colors[shot.team.index()];
            streak(
                streaks,
                position - direction * (length * 0.5),
                direction,
                width,
                length,
                [r, g, b, TRACER_OPACITY],
            );
            glow(streaks, position, head, [r, g, b, TRACER_OPACITY]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_core::sim::Team;

    fn shell(id: u32, weapon: Weapon, x: f64) -> RenderShot {
        RenderShot {
            id,
            weapon,
            team: Team::Red,
            x,
            z: 0.0,
            y: Some(1.0),
            visual_y: None,
            vx: 40.0,
            vz: 0.0,
        }
    }

    #[test]
    fn tracers_grow_from_the_muzzle_and_skip_missiles() {
        let mut tracers = Tracers::default();
        let mut streaks = PoolBuffer::new(64);
        let shots = [
            shell(1, Weapon::Standard, 0.0),
            shell(2, Weapon::Rocket, 0.0),
        ];
        tracers.update(&shots, 1.0, &mut streaks);
        assert_eq!(streaks.len(), 2, "one shell's streak and head");
        let first = streaks.records()[0].world().y_axis.length();
        streaks.clear();
        let shots = [shell(1, Weapon::Standard, 2.0)];
        tracers.update(&shots, 1.0, &mut streaks);
        let grown = streaks.records()[0].world().y_axis.length();
        assert!(
            grown > first && (grown - 2.0).abs() < 1e-4,
            "{first} {grown}"
        );
        tracers.update(&[], 1.0, &mut streaks);
        assert!(tracers.starts.is_empty());
    }
}

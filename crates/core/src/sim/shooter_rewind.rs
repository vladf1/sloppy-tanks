//! "Favor the shooter" for rooms: a player's shell meets the other tanks where its
//! shooter's screen showed them, which is a view delay behind the present, rewound by at
//! most [`MAX_REWIND_TICKS`] so a slow connection cannot shoot far into the past. Cover,
//! debris, mines and other shells stay where they are now; only hulls are rewound.
//!
//! The host records every hull's sweep each tick before projectiles move, and fixes each
//! shell's rewind from its shooter's view delay the first time it is swept. Bots' shells
//! and local play rewind nothing, and a simulation without this record (single player)
//! runs exactly as before.

use std::collections::{HashMap, VecDeque};

use rapier3d::prelude::Rotation;

use super::math::{Point3, Vec2};
use super::simulation::Simulation;

/// The furthest a shell's targets are rewound: 200 ms. A view delay beyond it (a round
/// trip over about 110 ms) leaves the rest for the shooter to lead.
pub const MAX_REWIND_TICKS: u32 = 12;
/// A room's client draws another tank from its predicted world, in the present like its
/// own hull, once this near the hull (metres), and from the delayed display beyond the far
/// distance, so a tank about to be rammed is drawn where the hull will meet it. The host
/// rewinds a tank for a shell by the share of the view delay it is drawn behind.
pub const PRESENT_TANKS_NEAR: f64 = 5.0;
pub const PRESENT_TANKS_FAR: f64 = 10.0;

/// The share of the present a tank `gap` metres from a player's hull is drawn at.
pub fn present_share(gap: f64) -> f64 {
    let share =
        ((PRESENT_TANKS_FAR - gap) / (PRESENT_TANKS_FAR - PRESENT_TANKS_NEAR)).clamp(0.0, 1.0);
    share * share * (3.0 - 2.0 * share)
}

/// The ticks a shell rewinding `ticks` rewinds a tank `gap` metres from its shooter.
pub fn target_ticks(ticks: u32, gap: f64) -> u32 {
    (f64::from(ticks) * (1.0 - present_share(gap))).round() as u32
}

/// Where a hull was during one tick: from `previous` to `end`, turned by `rotation`.
#[derive(Clone, Copy, Debug)]
pub struct PastHull {
    pub life: u32,
    pub previous: Vec2,
    pub end: Point3,
    pub rotation: Rotation,
}

#[derive(Clone, Debug, Default)]
pub struct ShooterRewind {
    /// Ticks each player tank's view of the others runs behind its present, from its
    /// newest input.
    pub view_ticks: HashMap<u32, u32>,
    /// Each live shell's rewind, fixed when it is first swept.
    shells: HashMap<u32, u32>,
    /// The hulls of the last ticks, newest last, by tank id.
    ticks: VecDeque<HashMap<u32, PastHull>>,
}

impl ShooterRewind {
    /// Records this tick's hulls and fixes the rewind of shells fired since the last tick.
    pub fn record(simulation: &mut Simulation) {
        let Some(mut rewind) = simulation.shooter_rewind.take() else {
            return;
        };
        let mut hulls = if rewind.ticks.len() > MAX_REWIND_TICKS as usize {
            rewind.ticks.pop_front().expect("checked above")
        } else {
            HashMap::new()
        };
        hulls.clear();
        hulls.extend(
            simulation
                .tanks
                .iter()
                .filter(|tank| tank.alive)
                .map(|tank| {
                    let body = &simulation.world.bodies[tank.body];
                    (
                        tank.id,
                        PastHull {
                            life: tank.life,
                            previous: tank.previous,
                            end: simulation.body_translation(tank.body),
                            rotation: *body.rotation(),
                        },
                    )
                }),
        );
        rewind.ticks.push_back(hulls);
        let ShooterRewind {
            view_ticks, shells, ..
        } = &mut rewind;
        shells.retain(|id, _| simulation.shots.iter().any(|shot| shot.id == *id));
        for shot in &simulation.shots {
            shells.entry(shot.id).or_insert_with(|| {
                view_ticks
                    .get(&shot.owner)
                    .map_or(0, |ticks| (*ticks).min(MAX_REWIND_TICKS))
            });
        }
        simulation.shooter_rewind = Some(rewind);
    }

    /// The ticks shell `shot` rewinds its targets by.
    pub fn shell_ticks(&self, shot: u32) -> u32 {
        self.shells.get(&shot).copied().unwrap_or(0)
    }

    /// Where tank `id` in life `life` was `ticks` ticks ago, if it was alive then.
    pub fn hull(&self, id: u32, life: u32, ticks: u32) -> Option<&PastHull> {
        let index = self.ticks.len().checked_sub(1 + ticks as usize)?;
        self.ticks[index].get(&id).filter(|hull| hull.life == life)
    }

    /// Forgets the recorded hulls and shells (a new round), keeping the view delays.
    pub fn clear(&mut self) {
        self.shells.clear();
        self.ticks.clear();
    }
}

//! Debris budget: which pieces fade first when the fragment pool fills.

use super::math::{Vec2, js_min};
use super::simulation::Simulation;
use super::types::Fragment;

/// The last second sinks and fades, without shrinking or blocking tanks.
pub const DEBRIS_CLEANUP_SECONDS: f64 = 1.0;
/// Pieces this near a player are kept in preference to distant ones.
const NEARBY_DISTANCE: f64 = 25.0;

pub fn debris_moving(simulation: &Simulation, fragment: &Fragment) -> bool {
    let body = &simulation.world.bodies[fragment.body];
    if body.is_sleeping() {
        return false;
    }
    let v = body.linvel();
    let w = body.angvel();
    let (vx, vy, vz) = (v.x as f64, v.y as f64, v.z as f64);
    let (wx, wy, wz) = (w.x as f64, w.y as f64, w.z as f64);
    vx * vx + vy * vy + vz * vz > 0.16 || wx * wx + wy * wy + wz * wz > 0.25
}

/// Prefer distant settled pieces, then distant moving pieces, preserving nearby action.
/// Chooses among `candidates` (indices into `fragments`), or all fragments; returns the
/// index into `fragments`.
pub fn cleanup_candidate(simulation: &Simulation, candidates: Option<&[usize]>) -> Option<usize> {
    let human = simulation.tanks.iter().find(|tank| tank.human);
    let focus = match human {
        Some(tank) if tank.alive => simulation.body_translation(tank.body).planar(),
        Some(tank) => tank.previous,
        None => Vec2::ZERO,
    };
    let players: Option<Vec<Vec2>> = simulation.multiplayer().then(|| {
        simulation
            .tanks
            .iter()
            .filter(|tank| tank.human)
            .map(|tank| {
                if tank.alive {
                    simulation.body_translation(tank.body).planar()
                } else {
                    tank.previous
                }
            })
            .collect()
    });
    let all: Vec<usize>;
    let candidates = match candidates {
        Some(candidates) => candidates,
        None => {
            all = (0..simulation.fragments.len()).collect();
            &all
        }
    };
    let mut best = None;
    let mut best_score = f64::INFINITY;
    for &index in candidates {
        let fragment = &simulation.fragments[index];
        let p = simulation.body_translation(fragment.body);
        let distance2 = match &players {
            Some(players) if !players.is_empty() => players
                .iter()
                .map(|point| (p.x - point.x).powi(2) + (p.z - point.z).powi(2))
                .fold(f64::INFINITY, js_min),
            _ => (p.x - focus.x).powi(2) + (p.z - focus.z).powi(2),
        };
        let priority = if distance2 < NEARBY_DISTANCE * NEARBY_DISTANCE {
            2.0
        } else {
            0.0
        } + if debris_moving(simulation, fragment) {
            1.0
        } else {
            0.0
        };
        // Discrete priority dominates; within it prefer already fading and older pieces.
        let score = priority * 1000.0 + fragment.life.min(100.0) - distance2.min(10000.0) * 0.00001;
        if score < best_score {
            best = Some(index);
            best_score = score;
        }
    }
    best
}

/// Start the normal fade before the hard budget forces an immediate eviction.
pub fn prepare_debris_cleanup(simulation: &mut Simulation) {
    let target = (simulation.max_fragments as f64 * 0.8).floor() as usize;
    if simulation.fragments.len() <= target {
        return;
    }
    let fading = simulation
        .fragments
        .iter()
        .filter(|f| f.life > DEBRIS_CLEANUP_SECONDS)
        .count();
    let mut excess = fading as i64 - target as i64;
    if excess <= 0 {
        return;
    }
    let mut candidates: Vec<usize> = (0..simulation.fragments.len())
        .filter(|&i| {
            let fragment = &simulation.fragments[i];
            fragment.life > DEBRIS_CLEANUP_SECONDS
                && fragment.life <= 3.0
                && !debris_moving(simulation, fragment)
        })
        .collect();
    while excess > 0 && !candidates.is_empty() {
        excess -= 1;
        let Some(index) = cleanup_candidate(simulation, Some(&candidates)) else {
            break;
        };
        simulation.fragments[index].life = DEBRIS_CLEANUP_SECONDS;
        candidates.retain(|&candidate| candidate != index);
    }
}

/// Smoothstep progress through the final fade, for presentation.
pub fn debris_cleanup_progress(life: f64) -> f64 {
    let t = 0f64.max(1f64.min(1.0 - life / DEBRIS_CLEANUP_SECONDS));
    t * t * (3.0 - 2.0 * t)
}

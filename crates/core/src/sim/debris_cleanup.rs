//! Debris budget: which pieces fade first when the fragment pool fills.

use super::math::{Vec2, best_by, js_min};
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

/// Eviction order for debris around the players: lower scores go first.
struct CleanupScores {
    focus: Vec2,
    players: Option<Vec<Vec2>>,
}

impl CleanupScores {
    fn new(simulation: &Simulation) -> Self {
        let human = simulation.tanks.iter().find(|tank| tank.human);
        let focus = human.map_or(Vec2::ZERO, |tank| simulation.tank_position(tank).planar());
        let players = simulation.multiplayer().then(|| {
            simulation
                .tanks
                .iter()
                .filter(|tank| tank.human)
                .map(|tank| simulation.tank_position(tank).planar())
                .collect()
        });
        Self { focus, players }
    }

    fn score(&self, simulation: &Simulation, fragment: &Fragment) -> f64 {
        let p = simulation.body_translation(fragment.body);
        let distance2 = match &self.players {
            Some(players) if !players.is_empty() => players
                .iter()
                .map(|point| (p.x - point.x).powi(2) + (p.z - point.z).powi(2))
                .fold(f64::INFINITY, js_min),
            _ => (p.x - self.focus.x).powi(2) + (p.z - self.focus.z).powi(2),
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
        priority * 1000.0 + fragment.life.min(100.0) - distance2.min(10000.0) * 0.00001
    }
}

/// Prefer distant settled pieces, then distant moving pieces, preserving nearby action.
/// Returns the index into `fragments` of the lowest score, the first of equal ones; a NaN
/// or infinite score is never picked.
pub fn cleanup_candidate(simulation: &Simulation) -> Option<usize> {
    let scores = CleanupScores::new(simulation);
    best_by(0..simulation.fragments.len(), |&index| {
        -scores.score(simulation, &simulation.fragments[index])
    })
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
    let excess = fading.saturating_sub(target);
    if excess == 0 {
        return;
    }
    // Fading one piece leaves the others' scores unchanged, so one ranking by score, ties
    // to the lower index, picks what a `cleanup_candidate` scan per piece would. Like those
    // scans, it never picks a NaN or infinite score.
    let scores = CleanupScores::new(simulation);
    let mut ranked: Vec<(f64, usize)> = simulation
        .fragments
        .iter()
        .enumerate()
        .filter(|(_, fragment)| {
            fragment.life > DEBRIS_CLEANUP_SECONDS
                && fragment.life <= 3.0
                && !debris_moving(simulation, fragment)
        })
        .map(|(index, fragment)| (scores.score(simulation, fragment), index))
        .filter(|&(score, _)| score < f64::INFINITY)
        .collect();
    ranked.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .expect("ranked scores are not NaN")
            .then(a.1.cmp(&b.1))
    });
    for &(_, index) in ranked.iter().take(excess) {
        simulation.fragments[index].life = DEBRIS_CLEANUP_SECONDS;
    }
}

/// Smoothstep progress through the final fade, for presentation.
pub fn debris_cleanup_progress(life: f64) -> f64 {
    let t = 0f64.max(1f64.min(1.0 - life / DEBRIS_CLEANUP_SECONDS));
    t * t * (3.0 - 2.0 * t)
}

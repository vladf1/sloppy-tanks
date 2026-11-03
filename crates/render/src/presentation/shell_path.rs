//! Where a shell fired now would fly, traced through the displayed scene for the touch
//! aim line: a thumb hides no part of a line drawn from the tank, where a reticle under
//! the finger or past the screen's edge was hard to find. The trace stops at the first
//! tank or standing cover in the lane and bounces as many times as the selected
//! ammunition does. It approximates the simulation's contacts (every cover a box of its
//! footprint, every hull a box about its heading), which is enough to show the lane.

use glam::DVec3;
use sloppy_core::sim::RenderState;
use sloppy_core::sim::combat_rules::COMBAT;
use sloppy_core::sim::data::weapon;
use sloppy_core::sim::hitboxes::SHELL_HIT_RADIUS;
use sloppy_core::sim::render_state::{RenderCover, RenderTank};
use sloppy_core::sim::tank_dimensions::{tank_hull, tank_muzzle};

/// Covers lower than the sight line (`Simulation::visible`) let a shell over them.
const SIGHT_HEIGHT: f64 = 1.0;
/// The most legs a path draws: ricochet shells bounce three times.
pub const MAX_LEGS: usize = 4;

/// One straight leg of the path on the ground plane, from `start` to `end`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leg {
    pub start: (f64, f64),
    pub end: (f64, f64),
}

/// A box on the ground plane: its centre, its yaw (the game's bearing of its local +z)
/// and its half extents along its local x and z.
struct Footprint {
    center: (f64, f64),
    yaw: f64,
    half: (f64, f64),
}

impl Footprint {
    fn local(&self, (x, z): (f64, f64)) -> (f64, f64) {
        let (dx, dz) = (x - self.center.0, z - self.center.1);
        let (sin, cos) = self.yaw.sin_cos();
        (dx * cos - dz * sin, dx * sin + dz * cos)
    }

    fn world_direction(&self, (x, z): (f64, f64)) -> (f64, f64) {
        let (sin, cos) = self.yaw.sin_cos();
        (x * cos + z * sin, -x * sin + z * cos)
    }

    /// Where along the segment from `a` to `b` (0 to 1) it enters the box, and the
    /// face's outward normal in the world; a start inside the box enters at once with
    /// no face.
    fn entry(&self, a: (f64, f64), b: (f64, f64)) -> Option<(f64, Option<(f64, f64)>)> {
        let (a, b) = (self.local(a), self.local(b));
        let mut enter = f64::NEG_INFINITY;
        let mut exit = f64::INFINITY;
        let mut normal = None;
        for (axis, start, end, extent) in [(0, a.0, b.0, self.half.0), (1, a.1, b.1, self.half.1)] {
            let delta = end - start;
            if delta.abs() < 1e-12 {
                if start.abs() > extent {
                    return None;
                }
                continue;
            }
            let (t0, t1) = ((-extent - start) / delta, (extent - start) / delta);
            let near = t0.min(t1);
            if near > enter {
                enter = near;
                let side = -delta.signum();
                normal = Some(if axis == 0 { (side, 0.0) } else { (0.0, side) });
            }
            exit = exit.min(t0.max(t1));
        }
        if enter > exit || exit < 0.0 || enter > 1.0 {
            return None;
        }
        if enter < 0.0 {
            return Some((0.0, None));
        }
        Some((enter, normal.map(|normal| self.world_direction(normal))))
    }
}

fn cover_footprint(cover: &RenderCover) -> Footprint {
    let rotation = glam::DQuat::from_xyzw(
        cover.rotation.x,
        cover.rotation.y,
        cover.rotation.z,
        cover.rotation.w,
    );
    let forward = rotation * DVec3::Z;
    Footprint {
        center: (cover.position.x, cover.position.z),
        yaw: forward.x.atan2(forward.z),
        half: (
            cover.w / 2.0 + SHELL_HIT_RADIUS,
            cover.d / 2.0 + SHELL_HIT_RADIUS,
        ),
    }
}

/// A hull's hit box (`hitboxes::hit_shape`) about its measured centre.
fn hull_footprint(tank: &RenderTank) -> Footprint {
    let hull = tank_hull(tank.kind);
    let (sin, cos) = tank.heading.sin_cos();
    let (cx, cz) = (hull.center.x, hull.center.z);
    Footprint {
        center: (
            tank.position.x + cx * cos + cz * sin,
            tank.position.z - cx * sin + cz * cos,
        ),
        yaw: tank.heading,
        half: (
            hull.size.x / 2.0 + SHELL_HIT_RADIUS,
            hull.size.z / 2.0 + SHELL_HIT_RADIUS,
        ),
    }
}

/// The legs of the path a shell of `viewer`'s selected ammunition takes when fired at
/// `angle` from the hull at `from`, into `legs` (cleared first). The first leg starts
/// at the muzzle and none is longer than the shell's whole flight.
pub fn trace(
    state: &RenderState,
    viewer: &RenderTank,
    from: (f64, f64),
    angle: f64,
    legs: &mut Vec<Leg>,
) {
    legs.clear();
    let stats = weapon(viewer.selected_ammo);
    let mut range = stats.speed * COMBAT.projectile_lifetime;
    let mut bounces = stats.bounces;
    let mut start = from;
    let mut direction = (angle.sin(), angle.cos());
    // The shell passes through the tank that fired it until it first bounces.
    let mut ricocheted = false;
    while legs.len() < MAX_LEGS && range > 0.0 {
        let end = (start.0 + direction.0 * range, start.1 + direction.1 * range);
        let covers = state
            .covers
            .iter()
            .filter(|cover| cover.alive && cover.position.y + cover.h / 2.0 >= SIGHT_HEIGHT)
            .filter_map(|cover| {
                let (t, normal) = cover_footprint(cover).entry(start, end)?;
                Some((t, normal))
            });
        let tanks = state
            .tanks
            .iter()
            .filter(|tank| tank.alive && (ricocheted || tank.id != viewer.id))
            .filter_map(|tank| Some((hull_footprint(tank).entry(start, end)?.0, None)));
        let hit = covers.chain(tanks).min_by(|a, b| a.0.total_cmp(&b.0));
        let t = hit.map_or(1.0, |(t, _)| t);
        let stop = (
            start.0 + (end.0 - start.0) * t,
            start.1 + (end.1 - start.1) * t,
        );
        legs.push(Leg { start, end: stop });
        range *= 1.0 - t;
        // Only a cover's face turns a shell; a tank, a start inside a cover or the end
        // of its flight stops it.
        let Some((_, Some(normal))) = hit else { break };
        if bounces == 0 {
            break;
        }
        bounces -= 1;
        ricocheted = true;
        let dot = direction.0 * normal.0 + direction.1 * normal.1;
        direction = (
            direction.0 - 2.0 * dot * normal.0,
            direction.1 - 2.0 * dot * normal.1,
        );
        start = (
            stop.0 + normal.0 * COMBAT.bounce_clearance,
            stop.1 + normal.1 * COMBAT.bounce_clearance,
        );
    }
    // Drawn from the muzzle, not the hull's middle.
    let muzzle = tank_muzzle(viewer.kind).z;
    if let Some(first) = legs.first_mut() {
        let (dx, dz) = (first.end.0 - first.start.0, first.end.1 - first.start.1);
        let length = dx.hypot(dz);
        let skip = muzzle.min(length);
        if length > 0.0 {
            first.start = (
                first.start.0 + dx / length * skip,
                first.start.1 + dz / length * skip,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_core::sim::math::{Point3, Quat4};
    use sloppy_core::sim::{CoverKind, Team, Weapon};

    fn tank(id: u32, team: Team, x: f64, z: f64) -> RenderTank {
        RenderTank {
            id,
            team,
            alive: true,
            position: Point3 { x, y: 0.65, z },
            ..RenderTank::default()
        }
    }

    fn wall(x: f64, z: f64, w: f64, d: f64) -> RenderCover {
        RenderCover {
            kind: CoverKind::Concrete,
            alive: true,
            position: Point3 { x, y: 1.0, z },
            rotation: Quat4::yaw(0.0),
            w,
            h: 2.0,
            d,
            ..RenderCover::default()
        }
    }

    fn scene(tanks: Vec<RenderTank>, covers: Vec<RenderCover>) -> RenderState {
        RenderState {
            viewer_id: 1,
            tanks,
            covers,
            ..RenderState::default()
        }
    }

    fn path(state: &RenderState, angle: f64) -> Vec<Leg> {
        let viewer = state.viewer().unwrap();
        let mut legs = Vec::new();
        trace(
            state,
            viewer,
            (viewer.position.x, viewer.position.z),
            angle,
            &mut legs,
        );
        legs
    }

    #[test]
    fn the_path_stops_at_the_first_wall_and_starts_at_the_muzzle() {
        let state = scene(
            vec![tank(1, Team::Blue, 0.0, 0.0)],
            vec![wall(0.0, 10.0, 6.0, 1.0), wall(0.0, 20.0, 6.0, 1.0)],
        );
        let legs = path(&state, 0.0);
        assert_eq!(legs.len(), 1);
        let leg = legs[0];
        assert!((leg.start.1 - tank_muzzle(Default::default()).z).abs() < 1e-9);
        assert!(
            (leg.end.1 - (9.5 - SHELL_HIT_RADIUS)).abs() < 1e-9,
            "{leg:?}"
        );
        assert!(leg.end.0.abs() < 1e-9);
    }

    #[test]
    fn the_path_stops_at_a_tank_of_either_team_but_not_its_own() {
        let state = scene(
            vec![
                tank(1, Team::Blue, 0.0, 0.0),
                tank(2, Team::Blue, 0.0, 12.0),
                tank(3, Team::Red, 0.0, 24.0),
            ],
            vec![],
        );
        let legs = path(&state, 0.0);
        assert_eq!(legs.len(), 1);
        assert!(legs[0].end.1 < 12.0 && legs[0].end.1 > 9.0, "{legs:?}");
    }

    #[test]
    fn low_cover_lets_the_path_over_it_and_open_ground_ends_at_the_shells_range() {
        let mut low = wall(0.0, 10.0, 6.0, 1.0);
        low.position.y = 0.3;
        low.h = 0.6;
        let state = scene(vec![tank(1, Team::Blue, 0.0, 0.0)], vec![low]);
        let legs = path(&state, std::f64::consts::FRAC_PI_2);
        let range = weapon(Weapon::default()).speed * COMBAT.projectile_lifetime;
        assert_eq!(legs.len(), 1);
        assert!((legs[0].end.0 - range).abs() < 1e-9, "{legs:?}");
    }

    #[test]
    fn ricochet_shells_bounce_off_walls_and_may_come_back_to_their_tank() {
        let mut viewer = tank(1, Team::Blue, 0.0, 0.0);
        viewer.selected_ammo = Weapon::Ricochet;
        let state = scene(
            vec![viewer],
            vec![wall(0.0, 6.0, 200.0, 1.0), wall(0.0, -6.0, 200.0, 1.0)],
        );
        let legs = path(&state, 0.0);
        // Off the far wall and straight back into its own hull.
        assert_eq!(legs.len(), 2, "{legs:?}");
        assert!(legs[1].end.1 > 0.0 && legs[1].end.1 < 3.0, "{legs:?}");
        // A slanted shot zigzags between the walls until its bounces run out.
        let legs = path(&state, 0.6);
        assert_eq!(legs.len(), MAX_LEGS, "{legs:?}");
        assert!(legs[1].end.1 < -5.0 && legs[2].end.1 > 5.0, "{legs:?}");
    }
}

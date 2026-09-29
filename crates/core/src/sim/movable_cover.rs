//! Keeps movable cover (drums, teeth, hedgehogs) in sync with navigation as it is pushed.

use super::navigation::Footprint;
use super::simulation::Simulation;
use super::types::Cover;

/// Seconds between navigation checks for one movable cover (4 Hz).
const NAV_CHECK_SECONDS: f64 = 0.25;
/// Footprint change that rebuilds navigation while moving, and once settled.
const MOVING_REBUILD_DISTANCE: f64 = 0.5;
const SETTLED_REBUILD_DISTANCE: f64 = 0.02;

/// Clear both the last baked footprint and the current one, even between navigation ticks.
pub fn moved_cover_region(cover: &Cover) -> Footprint {
    let Some(m) = cover.motion else {
        return Footprint::from(cover);
    };
    let left = (m.x - m.nav_w / 2.0).min(cover.x - cover.w / 2.0);
    let right = (m.x + m.nav_w / 2.0).max(cover.x + cover.w / 2.0);
    let near = (m.z - m.nav_d / 2.0).min(cover.z - cover.d / 2.0);
    let far = (m.z + m.nav_d / 2.0).max(cover.z + cover.d / 2.0);
    Footprint {
        x: (left + right) / 2.0,
        z: (near + far) / 2.0,
        w: right - left,
        d: far - near,
    }
}

/// Keep AI footprints current, including tipped blocks. Navigation patches are throttled
/// to 4 Hz and only rebuilt after a half-metre change, or once a block settles.
pub fn update_movable_cover(simulation: &mut Simulation) {
    for m in 0..simulation.movable_covers.len() {
        let index = simulation.movable_covers[m];
        if !simulation.covers[index].alive {
            continue;
        }
        let body = &simulation.world.bodies[simulation.covers[index].body];
        let p = body.translation();
        let q = *body.rotation();
        let sleeping = body.is_sleeping();
        let (qx, qy, qz, qw) = (q.x as f64, q.y as f64, q.z as f64, q.w as f64);
        let elapsed = simulation.elapsed;
        let cover = &mut simulation.covers[index];
        let motion = cover.motion.expect("movable cover tracks its motion");
        cover.x = p.x as f64;
        cover.z = p.z as f64;
        // Conservative rotated-box projection of the original barrier bounds.
        cover.w = (1.0 - 2.0 * (qy * qy + qz * qz)).abs() * motion.w
            + (2.0 * (qx * qy - qz * qw)).abs() * cover.h
            + (2.0 * (qx * qz + qy * qw)).abs() * motion.d;
        cover.d = (2.0 * (qx * qz - qy * qw)).abs() * motion.w
            + (2.0 * (qy * qz + qx * qw)).abs() * cover.h
            + (1.0 - 2.0 * (qx * qx + qy * qy)).abs() * motion.d;
        if elapsed < motion.check_at {
            continue;
        }
        let motion = cover.motion.as_mut().expect("movable cover tracks its motion");
        motion.check_at = elapsed + NAV_CHECK_SECONDS;
        let change = (cover.x - motion.x)
            .abs()
            .max((cover.z - motion.z).abs())
            .max((cover.w - motion.nav_w).abs() / 2.0)
            .max((cover.d - motion.nav_d).abs() / 2.0);
        if change < if sleeping { SETTLED_REBUILD_DISTANCE } else { MOVING_REBUILD_DISTANCE } {
            continue;
        }
        let region = moved_cover_region(cover);
        let cover = &mut simulation.covers[index];
        let (x, z, w, d) = (cover.x, cover.z, cover.w, cover.d);
        simulation.nav.rebuild(&simulation.covers, Some(region));
        let motion = simulation.covers[index].motion.as_mut().expect("movable cover tracks its motion");
        motion.x = x;
        motion.z = z;
        motion.nav_w = w;
        motion.nav_d = d;
    }
}

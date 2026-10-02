//! Pose interpolation between received scene samples (`src/net/render-timeline.ts`).

use std::collections::VecDeque;

use crate::sim::math::{Point3, Quat4, Vec2, angle_delta};
use crate::sim::render_state::{RenderState, RenderTank};

const MAX_SAMPLES: usize = 32;
/// Longest a hull is carried past its newest authoritative pose, local or remote.
pub const MAX_EXTRAPOLATION_SECONDS: f64 = 0.1;
const CORRECTION_RATE: f64 = 20.0;

fn interpolate_rotation(a: Quat4, b: Quat4, alpha: f64) -> Quat4 {
    let sign = if a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w < 0.0 {
        -1.0
    } else {
        1.0
    };
    let x = a.x + (b.x * sign - a.x) * alpha;
    let y = a.y + (b.y * sign - a.y) * alpha;
    let z = a.z + (b.z * sign - a.z) * alpha;
    let w = a.w + (b.w * sign - a.w) * alpha;
    let length = (x * x + y * y + z * z + w * w).sqrt();
    let length = if length == 0.0 { 1.0 } else { length };
    Quat4 {
        x: x / length,
        y: y / length,
        z: z / length,
        w: w / length,
    }
}

fn lerp(a: Point3, b: Option<Point3>, fraction: f64) -> Point3 {
    let b = b.unwrap_or(a);
    Point3::new(
        a.x + (b.x - a.x) * fraction,
        a.y + (b.y - a.y) * fraction,
        a.z + (b.z - a.z) * fraction,
    )
}

/// Interpolates received scene samples at a delayed display time. Membership comes from
/// the older sample, so removals wait for the display clock. The local hull instead
/// follows the newest authority, extrapolated toward the present and smoothed at frame
/// rate. Samples are never modified.
#[derive(Clone, Debug, Default)]
pub struct RenderTimeline {
    samples: VecDeque<RenderState>,
    local: Option<RenderTank>,
}

impl RenderTimeline {
    pub fn reset(&mut self, state: RenderState) {
        self.samples.clear();
        self.samples.push_back(state);
        self.local = None;
    }

    pub fn push(&mut self, state: RenderState) {
        self.samples.push_back(state);
        self.samples.retain_back(MAX_SAMPLES);
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Fills `output` for the delayed display `time` (seconds); `local_time` is where the
    /// local hull aims. Needs at least one sample.
    pub fn read(&mut self, time: f64, local_time: f64, dt: f64, output: &mut RenderState) {
        let newest = self.samples.back().expect("the timeline has a sample");
        // A late packet carries remote hulls along their velocity briefly instead of freezing them.
        let overrun = (time - newest.elapsed).clamp(0.0, MAX_EXTRAPOLATION_SECONDS);
        let time = time.min(newest.elapsed);
        let mut index = 0;
        while index + 1 < self.samples.len() && self.samples[index + 1].elapsed <= time {
            index += 1;
        }
        let before = &self.samples[index];
        let after = self.samples.get(index + 1).unwrap_or(before);
        let fraction = if after.elapsed > before.elapsed {
            ((time - before.elapsed) / (after.elapsed - before.elapsed)).clamp(0.0, 1.0)
        } else {
            0.0
        };

        output.tanks.clear();
        for tank in &before.tanks {
            let next = after.tanks.iter().find(|candidate| {
                candidate.id == tank.id
                    && candidate.life == tank.life
                    && candidate.alive == tank.alive
            });
            let mut out = tank.clone();
            out.position = lerp(tank.position, next.map(|n| n.position), fraction);
            out.heading = tank.heading
                + angle_delta(tank.heading, next.map_or(tank.heading, |n| n.heading)) * fraction;
            out.aim = tank.aim + angle_delta(tank.aim, next.map_or(tank.aim, |n| n.aim)) * fraction;
            if overrun > 0.0 && tank.alive {
                out.position.x += tank.velocity.x * overrun;
                out.position.z += tank.velocity.z * overrun;
            }
            out.previous = Vec2::new(out.position.x, out.position.z);
            output.tanks.push(out);
        }
        output.covers.clear();
        for cover in &before.covers {
            let next = after
                .covers
                .iter()
                .find(|candidate| candidate.id == cover.id && candidate.alive == cover.alive);
            let mut out = cover.clone();
            out.position = lerp(cover.position, next.map(|n| n.position), fraction);
            out.rotation = interpolate_rotation(
                cover.rotation,
                next.map_or(cover.rotation, |n| n.rotation),
                fraction,
            );
            output.covers.push(out);
        }
        output.fragments.clear();
        for fragment in &before.fragments {
            let next = after
                .fragments
                .iter()
                .find(|candidate| candidate.id == fragment.id);
            let mut out = fragment.clone();
            out.position = lerp(fragment.position, next.map(|n| n.position), fraction);
            out.rotation = interpolate_rotation(
                fragment.rotation,
                next.map_or(fragment.rotation, |n| n.rotation),
                fraction,
            );
            output.fragments.push(out);
        }
        output.shots.clear();
        for shot in &before.shots {
            let next = after.shots.iter().find(|candidate| candidate.id == shot.id);
            let mut out = *shot;
            let ahead = (time + overrun - before.elapsed).clamp(0.0, 0.05);
            match next {
                Some(next) => {
                    out.x = shot.x + (next.x - shot.x) * fraction;
                    out.z = shot.z + (next.z - shot.z) * fraction;
                }
                None => {
                    out.x = shot.x + shot.vx * ahead;
                    out.z = shot.z + shot.vz * ahead;
                }
            }
            output.shots.push(out);
        }

        // A death or respawn waits for the display clock, so the wreck and its effects agree.
        let before_viewer = before.viewer();
        let newest_viewer = newest.viewer();
        let authoritative = match (before_viewer, newest_viewer) {
            (Some(old), Some(new)) if old.life != new.life || old.alive != new.alive => Some(old),
            (_, Some(new)) => Some(new),
            (old, None) => old,
        };
        if let Some(authoritative) = authoritative {
            let continuous = self.local.as_ref().is_some_and(|local| {
                local.life == authoritative.life && local.alive == authoritative.alive
            });
            let mut target = authoritative.position;
            if authoritative.alive {
                let ahead = (local_time - newest.elapsed).clamp(0.0, MAX_EXTRAPOLATION_SECONDS);
                target.x += authoritative.velocity.x * ahead;
                target.z += authoritative.velocity.z * ahead;
            }
            let blend = if continuous {
                1.0 - (-CORRECTION_RATE * dt).exp()
            } else {
                1.0
            };
            // Local translation and hull rotation need the same frame-rate smoothing.
            // Copying heading from authority here made only our own tank turn at packet Hz.
            let heading = match (&self.local, continuous) {
                (Some(local), true) => {
                    local.heading + angle_delta(local.heading, authoritative.heading) * blend
                }
                _ => authoritative.heading,
            };
            if let (Some(local), true) = (&self.local, continuous) {
                target.x = local.position.x + (target.x - local.position.x) * blend;
                target.z = local.position.z + (target.z - local.position.z) * blend;
            }
            let mut local = authoritative.clone();
            local.position = target;
            local.previous = Vec2::new(target.x, target.z);
            local.heading = heading;
            if let Some(slot) = output.tanks.iter_mut().find(|tank| tank.id == local.id) {
                *slot = local.clone();
            }
            self.local = Some(local);
        }

        output.viewer_id = before.viewer_id;
        output.elapsed = before.elapsed.max(time);
        output.mines.clone_from(&before.mines);
        output.pickups.clone_from(&before.pickups);
        output.match_state.clone_from(&before.match_state);
        output.map_theme.clone_from(&before.map_theme);
        output.map_floor = before.map_floor;
        output.map_outer_floor = before.map_outer_floor;
        output.map_outer_floor_extent = before.map_outer_floor_extent;
        output.map_scale = before.map_scale;
        output.custom_map = before.custom_map;
        // Keep one predecessor for interpolation, with all delayed lifecycle data intact.
        self.samples.drain(..index);
    }

    /// The smoothed local hull of the last read.
    pub fn local(&self) -> Option<&RenderTank> {
        self.local.as_ref()
    }
}

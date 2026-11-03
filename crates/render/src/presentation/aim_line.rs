//! The touch aim line, drawn overhead in place of the reticle while touch aims the
//! turret: a finger hides a reticle under it and the fire stick's reticle could sit
//! past the screen's edge, but a line from the tank is always in view. Chevrons drift
//! along the shell's path (`shell_path`) the way it flies, light up from the muzzle as
//! the gun reloads, and mark each ricochet with a diamond and a wall or a friendly hull
//! that takes the shell with a bar. The line turns hot when the shell is on an enemy,
//! and brackets close in round that tank: the one it would hit, or the one the fire
//! stick tracks when the line passes just ahead of it (the assist leads a moving tank).
//! A tracked tank whose lane a wall blocks keeps pale brackets.

use std::f64::consts::FRAC_PI_2;

use glam::{Mat4, Quat, Vec3};
use sloppy_core::geometry::math::smoothstep;
use sloppy_core::scene::Node;
use sloppy_core::sim::RenderState;
use sloppy_core::sim::render_state::RenderTank;
use sloppy_core::sim::tank_dimensions::tank_hull;
use sloppy_core::sim::weapons::fire_interval;

use super::models as own;
use super::posing::lerp;
use super::shell_path::{self, Leg, MAX_LEGS, Stop};
use super::view_settings::RETICLE_HEIGHT;
use crate::color::hex_to_linear;
use crate::gpu::{InstanceId, Lifetime, Renderer};

/// The marks' ink: the reticle's pale cream, and hot when the shell would hit an enemy.
const PALE: u32 = 0xfff9da;
const HOT: u32 = 0xff5a2e;
/// Metres between chevrons at the default zoom, and how fast they drift along the
/// path, metres a second: slow enough to read as the shell's way, not as motion.
const SPACING: f64 = 1.5;
const MARCH_SPEED: f64 = 2.2;
/// Metres of path drawn at the default zoom: about the screen's reach from the tank,
/// fading over the last `FADE`. A path onto a tank is drawn up to `TANK_REACH`, so the
/// line meets the brackets.
const REACH: f64 = 26.0;
const TANK_REACH: f64 = 70.0;
const FADE: f64 = 7.0;
/// The chevrons fade in over this many metres from the muzzle, keep this far clear of
/// a ricochet's diamond and stop this far short of the end of the path.
const MUZZLE_FADE: f64 = 1.2;
const BOUNCE_GAP: f64 = 0.8;
const END_GAP: f64 = 0.7;
/// The chevrons' opacity ahead of the reload's front.
const RELOADING: f64 = 0.3;
/// The most chevrons a path draws: `TANK_REACH` at the default spacing.
const MAX_MARKS: usize = 48;
/// Metres the brackets stand off a hull's hit box, how much wider they start when a
/// tank becomes the target, and how long they take to close in, seconds.
const BRACKET_MARGIN: f64 = 0.35;
const BRACKET_OPEN: f64 = 1.6;
const BRACKET_CLOSE_SECONDS: f64 = 0.18;
/// The brackets' opacity round a tracked tank the shell cannot reach.
const BLOCKED_OPACITY: f32 = 0.65;

/// What touch aims this frame: the turret's angle and the enemy the fire stick tracks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchAim {
    pub angle: f64,
    pub target: Option<u32>,
}

pub struct AimLine {
    marks: [InstanceId; MAX_MARKS],
    bounces: [InstanceId; MAX_LEGS - 1],
    stop: InstanceId,
    brackets: [InstanceId; 4],
    path: Vec<Leg>,
    /// Where the chevrons stand, as a share of the spacing past the muzzle.
    march: f64,
    /// The bracketed tank, and how far its brackets have closed in (0 to 1).
    bracketed: Option<u32>,
    closed: f64,
}

fn instances<const N: usize>(renderer: &mut Renderer, node: &Node) -> [InstanceId; N] {
    let model = renderer.add_fading_model(node, Lifetime::Shared);
    std::array::from_fn(|_| {
        let instance = renderer
            .add_instance(model, Mat4::IDENTITY, Lifetime::Shared)
            .expect("model just added");
        renderer.set_visible(instance, false);
        renderer.set_reflected(instance, false);
        instance
    })
}

/// Lying flat at `at` on the reticle's plane, turned to `bearing`, `scale` times its
/// size.
fn flat_pose(at: (f64, f64), bearing: f64, scale: f64) -> Mat4 {
    Mat4::from_scale_rotation_translation(
        Vec3::splat(scale as f32),
        Quat::from_rotation_y(bearing as f32),
        Vec3::new(at.0 as f32, RETICLE_HEIGHT as f32, at.1 as f32),
    )
}

fn bearing(leg: &Leg) -> f64 {
    (leg.end.0 - leg.start.0).atan2(leg.end.1 - leg.start.1)
}

/// How far the gun has reloaded, 0 just fired to 1 ready.
fn reload_charge(tank: &RenderTank) -> f64 {
    let interval = fire_interval(tank.selected_ammo, tank.rapid, tank.human, tank.xp);
    if tank.cooldown <= 0.0 || interval <= 0.0 {
        1.0
    } else {
        (1.0 - tank.cooldown / interval).clamp(0.0, 1.0)
    }
}

impl AimLine {
    pub fn new(renderer: &mut Renderer) -> Self {
        Self {
            marks: instances(renderer, &own::aim_chevron()),
            bounces: instances(renderer, &own::aim_bounce()),
            stop: instances::<1>(renderer, &own::aim_stop())[0],
            brackets: instances(renderer, &own::aim_bracket()),
            path: Vec::with_capacity(MAX_LEGS),
            march: 0.0,
            bracketed: None,
            closed: 0.0,
        }
    }

    /// Lay the line for `aim` from the viewer's drawn hull at `from`, or hide it.
    /// `scale` is the reticle's zoom scale (the marks keep their size on screen), and
    /// `alpha` the frame's interpolation, for the brackets' drawn tank.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        renderer: &mut Renderer,
        state: &RenderState,
        aim: Option<TouchAim>,
        from: (f64, f64),
        scale: f64,
        alpha: f64,
        dt: f64,
    ) {
        self.path.clear();
        let viewer = state.viewer();
        if let (Some(aim), Some(viewer)) = (aim, viewer) {
            shell_path::trace(state, viewer, from, aim.angle, &mut self.path);
        }
        let tracked = aim
            .and_then(|aim| aim.target)
            .and_then(|id| state.tanks.iter().find(|tank| tank.id == id && tank.alive));
        let on = viewer.and_then(|viewer| {
            shell_path::on_target(state, viewer, &self.path, aim.and_then(|aim| aim.target))
        });
        let target = on.or(tracked);
        let on_target = on.is_some();
        let charge = viewer.map_or(1.0, reload_charge);
        self.march = (self.march + dt * MARCH_SPEED / SPACING).fract();
        self.lay_marks(renderer, scale, charge, on_target);
        self.lay_brackets(renderer, target, on_target, scale, alpha, dt);
    }

    fn lay_marks(&mut self, renderer: &mut Renderer, scale: f64, charge: f64, hot: bool) {
        let tint = hex_to_linear(if hot { HOT } else { PALE });
        let total: f64 = self.path.iter().map(Leg::length).sum();
        let last = self.path.last().map(|leg| leg.stop);
        let reach = if matches!(last, Some(Stop::Tank(_))) {
            TANK_REACH
        } else {
            REACH
        } * scale;
        let shown = total.min(reach);
        let fade_from = if shown < total || last == Some(Stop::Range) {
            shown - FADE * scale
        } else {
            shown
        };
        // The reload's front runs out from the muzzle, softened over a chevron.
        let spacing = SPACING * scale;
        let front = charge * shown;
        let lit = |at: f64| {
            if charge >= 1.0 {
                1.0
            } else {
                let lit = 1.0 - smoothstep(at, front - spacing / 2.0, front + spacing / 2.0);
                RELOADING + (1.0 - RELOADING) * lit
            }
        };
        let fade = |at: f64| {
            smoothstep(at, 0.0, MUZZLE_FADE * scale) * (1.0 - smoothstep(at, fade_from, shown))
        };

        // The ricochets within the drawn path, by their distance along it.
        let mut bounce_at = [f64::INFINITY; MAX_LEGS - 1];
        let mut along = 0.0;
        for (index, leg) in self.path.iter().enumerate() {
            along += leg.length();
            if leg.stop == Stop::Bounce && along < shown {
                bounce_at[index] = along;
            }
        }
        for (index, &instance) in self.bounces.iter().enumerate() {
            let at = bounce_at[index];
            renderer.set_visible(instance, at.is_finite());
            if at.is_finite() {
                let leg = &self.path[index];
                renderer.set_transform(instance, flat_pose(leg.end, 0.0, scale));
                renderer.set_opacity(instance, (fade(at) * lit(at)) as f32);
                renderer.set_tint(instance, tint);
            }
        }

        // A wall, a friendly hull or a protected enemy that takes the shell gets a bar.
        let stopped = self
            .path
            .last()
            .filter(|leg| leg.stop != Stop::Range && shown >= total && !hot);
        renderer.set_visible(self.stop, stopped.is_some());
        if let Some(leg) = stopped {
            renderer.set_transform(self.stop, flat_pose(leg.end, bearing(leg), scale));
            renderer.set_opacity(self.stop, lit(total) as f32);
            renderer.set_tint(self.stop, tint);
        }

        let mut legs = self.path.iter().peekable();
        let mut leg_start = 0.0;
        for (index, &instance) in self.marks.iter().enumerate() {
            let at = (self.march + index as f64) * spacing;
            while let Some(leg) = legs.next_if(|leg| at > leg_start + leg.length()) {
                leg_start += leg.length();
            }
            let near_bounce = bounce_at
                .iter()
                .any(|bounce| (bounce - at).abs() < BOUNCE_GAP * scale);
            let shown_here = at < shown - END_GAP * scale && !near_bounce;
            let Some(leg) = legs.peek().filter(|_| shown_here) else {
                renderer.set_visible(instance, false);
                continue;
            };
            let length = leg.length().max(1e-9);
            let into = (at - leg_start) / length;
            let point = (
                leg.start.0 + (leg.end.0 - leg.start.0) * into,
                leg.start.1 + (leg.end.1 - leg.start.1) * into,
            );
            renderer.set_visible(instance, true);
            renderer.set_transform(instance, flat_pose(point, bearing(leg), scale));
            renderer.set_opacity(instance, (fade(at) * lit(at)) as f32);
            renderer.set_tint(instance, tint);
        }
    }

    /// Brackets round `target`'s drawn hull: hot when the shell is on it, paler when
    /// the fire stick tracks it behind cover. A new target's brackets close in from wider.
    fn lay_brackets(
        &mut self,
        renderer: &mut Renderer,
        tank: Option<&RenderTank>,
        hot: bool,
        scale: f64,
        alpha: f64,
        dt: f64,
    ) {
        if tank.map(|tank| tank.id) != self.bracketed {
            self.closed = 0.0;
        }
        self.bracketed = tank.map(|tank| tank.id);
        self.closed = (self.closed + dt / BRACKET_CLOSE_SECONDS).min(1.0);
        let Some(tank) = tank else {
            for &instance in &self.brackets {
                renderer.set_visible(instance, false);
            }
            return;
        };
        let hull = tank_hull(tank.kind);
        let open = (1.0 - self.closed).powi(2) * BRACKET_OPEN;
        let half = (
            hull.size.x / 2.0 + BRACKET_MARGIN + open,
            hull.size.z / 2.0 + BRACKET_MARGIN + open,
        );
        let x = lerp(tank.previous.x, tank.position.x, alpha);
        let z = lerp(tank.previous.z, tank.position.z, alpha);
        let (sin, cos) = tank.heading.sin_cos();
        let opacity = if hot { 1.0 } else { BLOCKED_OPACITY } * self.closed as f32;
        let tint = hex_to_linear(if hot { HOT } else { PALE });
        // Each corner a quarter turn on from the last, clockwise from front right.
        for (index, (&instance, (sx, sz))) in self
            .brackets
            .iter()
            .zip([(1.0, 1.0), (1.0, -1.0), (-1.0, -1.0), (-1.0, 1.0)])
            .enumerate()
        {
            let (lx, lz) = (hull.center.x + sx * half.0, hull.center.z + sz * half.1);
            let corner = (x + lx * cos + lz * sin, z - lx * sin + lz * cos);
            let turn = tank.heading + index as f64 * FRAC_PI_2;
            renderer.set_visible(instance, true);
            renderer.set_transform(instance, flat_pose(corner, turn, scale));
            renderer.set_opacity(instance, opacity);
            renderer.set_tint(instance, tint);
        }
    }
}

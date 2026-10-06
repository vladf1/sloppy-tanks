//! Shells, pellets, rockets and TOW missiles in flight (`projectile-visuals.ts`):
//! per munition a painted body, a team-colored band and, for rockets and TOWs, an
//! attached flickering flame. Each layer is one bounded instanced draw.

use std::f64::consts::{FRAC_PI_2, PI};

use glam::{Mat4, Vec2, Vec3};
use sloppy_core::geometry::{
    ExtrudeOptions, Mesh, Path, Shape, box_geometry, cone_geometry, cylinder_geometry,
    extrude_geometry, icosahedron_geometry, merge_geometries, sphere_geometry, torus_geometry,
};
use sloppy_core::sim::ammunition::PROJECTILE_ORDER;
use sloppy_core::sim::data::{STEP, TEAM_COLORS, weapon};
use sloppy_core::sim::render_state::RenderShot;
use sloppy_core::sim::{Team, Weapon};

use super::pool::{PoolBuffer, record};
use crate::color::hex_to_linear;

/// Shots drawn at once, across every munition.
pub const PROJECTILE_CAPACITY: usize = 600;
/// Ricochet discs spin in flight (rad/s).
const RICOCHET_SPIN: f64 = 12.0;
/// Rocket flames flicker along their length (two beating rates) and a little in
/// width.
const FLAME_FLICKER_RATE: [f64; 2] = [47.0, 29.0];
const FLAME_FLICKER: f64 = 0.22;
const FLAME_PULSE: f64 = 0.1;
/// The flame's colours from root to tip, in linear HDR: the frame's ACES tone
/// mapping bleaches bright saturated colours toward cream, so these are the
/// inputs it maps to sRGB #ffd060, #ff9a28, #ff5c14 and #c0280a.
const FLAME_STOPS: [[f32; 3]; 4] = [
    [2.185, 0.54, 0.008],
    [1.434, 0.198, 0.0],
    [1.174, 0.06, 0.0],
    [0.44, 0.037, 0.003],
];
/// Where the flame leaves the motor, behind the munition's centre (model units).
const FLAME_ROOT: f64 = 0.5;

/// Preserve the original shell scale: rocket body is ~0.94 m, standard ~0.70 m.
pub fn model_scale(kind: Weapon) -> f64 {
    match kind {
        Weapon::Standard => 1.0,
        Weapon::Spread => 0.9,
        Weapon::Rocket => 0.65,
        Weapon::Ricochet => 0.65,
        Weapon::Piercing => 0.8,
        Weapon::Tow => 0.74,
    }
}

/// A munition's meshes, authored along +Z.
#[derive(Clone, Debug)]
pub struct ProjectileModel {
    pub body: Mesh,
    pub team: Mesh,
    pub exhaust: Option<Mesh>,
}

fn tube(radius: f64, length: f64, z: f64) -> Mesh {
    let mut mesh = cylinder_geometry(radius, radius, length, 10);
    mesh.rotate_x(FRAC_PI_2).translate(0.0, 0.0, z);
    mesh
}

fn point(radius: f64, length: f64, z: f64) -> Mesh {
    let mut mesh = cone_geometry(radius, length, 10);
    mesh.rotate_x(FRAC_PI_2).translate(0.0, 0.0, z);
    mesh
}

fn translated(mut mesh: Mesh, x: f64, y: f64, z: f64) -> Mesh {
    mesh.translate(x, y, z);
    mesh
}

/// Bake each part's color into vertex colors and merge (the TS `painted`).
fn painted(parts: Vec<(Mesh, u32)>) -> Mesh {
    let meshes: Vec<Mesh> = parts
        .into_iter()
        .map(|(mesh, hex)| {
            let mut mesh = mesh.to_non_indexed();
            mesh.colors = vec![hex_to_linear(hex); mesh.positions.len()];
            mesh
        })
        .collect();
    let refs: Vec<&Mesh> = meshes.iter().collect();
    merge_geometries(&refs).expect("projectile parts share one layout")
}

fn rocket_fins() -> Mesh {
    let mut profile = Path::new();
    profile
        .move_to(0.14, -0.12)
        .line_to(0.4, -0.6)
        .line_to(0.14, -0.48)
        .close_path();
    let options = ExtrudeOptions {
        depth: 0.055,
        bevel_enabled: false,
        steps: 1,
        ..ExtrudeOptions::default()
    };
    let mut fin = extrude_geometry(&[Shape::new(profile)], &options);
    fin.translate(0.0, 0.0, -0.0275).rotate_x(FRAC_PI_2);
    fin
}

fn four_fins(parts: &mut Vec<(Mesh, u32)>, color: u32) {
    let fin = rocket_fins();
    for i in 0..4 {
        let mut copy = fin.clone();
        copy.rotate_z(i as f64 * PI / 2.0);
        parts.push((copy, color));
    }
}

/// The motor's flame: a plume from the nozzle, white-hot at its root through
/// yellow and orange to a red tip, unlit so it glows.
fn rocket_exhaust() -> Mesh {
    const ROOT: f64 = FLAME_ROOT;
    const LENGTH: f64 = 1.05;
    let mut cone = point(0.21, LENGTH, 0.0);
    cone.rotate_y(PI)
        .translate(0.0, 0.0, -(ROOT + LENGTH / 2.0));
    let mut mesh = painted(vec![(cone, 0xff671c)]);
    let stops = FLAME_STOPS;
    for (color, position) in mesh.colors.iter_mut().zip(&mesh.positions) {
        // Hot colour gives way to orange early: the root is the cone's widest part.
        let t = ((-f64::from(position[2]) - ROOT) / LENGTH)
            .clamp(0.0, 1.0)
            .sqrt()
            * 3.0;
        let (from, to) = (stops[(t as usize).min(2)], stops[(t as usize + 1).min(3)]);
        let k = (t - t.floor()) as f32;
        *color = [0, 1, 2].map(|i| from[i] + (to[i] - from[i]) * k);
    }
    mesh
}

/// The unscaled model of a munition.
fn authored(kind: Weapon) -> ProjectileModel {
    let accent = weapon(kind).color;
    let dark = 0x263344;
    let steel = 0xe2e9ef;
    match kind {
        Weapon::Standard => ProjectileModel {
            body: painted(vec![
                (tube(0.13, 0.38, -0.08), 0xcdaa55),
                (point(0.13, 0.26, 0.24), 0xffedb4),
                (tube(0.15, 0.08, -0.29), dark),
            ]),
            team: tube(0.138, 0.27, -0.07),
            exhaust: None,
        },
        Weapon::Spread => ProjectileModel {
            body: painted(vec![
                (icosahedron_geometry(0.18, 1), dark),
                (
                    translated(sphere_geometry(0.14, 8, 6), 0.0, 0.055, 0.035),
                    accent,
                ),
            ]),
            team: translated(sphere_geometry(0.115, 8, 6), 0.0, 0.105, 0.055),
            exhaust: None,
        },
        Weapon::Rocket => {
            let mut parts = vec![
                (tube(0.2, 0.82, -0.06), steel),
                (point(0.2, 0.5, 0.6), accent),
                (tube(0.215, 0.12, -0.46), dark),
            ];
            four_fins(&mut parts, dark);
            ProjectileModel {
                body: painted(parts),
                team: tube(0.208, 0.28, -0.08),
                exhaust: Some(rocket_exhaust()),
            }
        }
        Weapon::Tow => {
            let mut parts = vec![
                (tube(0.14, 0.74, -0.06), 0x56645d),
                (point(0.14, 0.36, 0.49), accent),
                (tube(0.15, 0.08, -0.46), dark),
            ];
            four_fins(&mut parts, dark);
            ProjectileModel {
                body: painted(parts),
                team: tube(0.145, 0.3, -0.08),
                exhaust: Some(rocket_exhaust()),
            }
        }
        Weapon::Ricochet => {
            let mut ring = torus_geometry(0.255, 0.037, 4, 12);
            ring.rotate_x(FRAC_PI_2).translate(0.0, 0.15, 0.0);
            ProjectileModel {
                body: painted(vec![
                    (cylinder_geometry(0.31, 0.31, 0.16, 6), dark),
                    (
                        translated(cylinder_geometry(0.24, 0.31, 0.075, 6), 0.0, 0.117, 0.0),
                        steel,
                    ),
                    (ring, accent),
                ]),
                team: translated(cylinder_geometry(0.19, 0.19, 0.022, 6), 0.0, 0.17, 0.0),
                exhaust: None,
            }
        }
        Weapon::Piercing => ProjectileModel {
            body: painted(vec![
                (tube(0.085, 0.65, -0.15), dark),
                (point(0.085, 0.55, 0.45), 0xeaffff),
                (
                    translated(box_geometry(0.34, 0.055, 0.23), 0.0, 0.0, -0.41),
                    accent,
                ),
                (
                    translated(box_geometry(0.055, 0.28, 0.23), 0.0, 0.0, -0.41),
                    accent,
                ),
                (tube(0.09, 0.25, 0.1), accent),
            ]),
            team: tube(0.095, 0.3, -0.2),
            exhaust: None,
        },
    }
}

/// A munition's model at its in-flight scale.
pub fn projectile_model(kind: Weapon) -> ProjectileModel {
    let mut model = authored(kind);
    let size = model_scale(kind);
    model.body.scale(size, size, size);
    model.team.scale(size, size, size);
    if let Some(exhaust) = &mut model.exhaust {
        exhaust.scale(size, size, size);
    }
    model
}

/// One munition's instance layers; slot `i` of each layer is the same shot.
#[derive(Clone, Debug)]
pub struct ProjectileBatch {
    pub body: PoolBuffer,
    pub team: PoolBuffer,
    pub exhaust: Option<PoolBuffer>,
}

#[derive(Clone, Debug)]
pub struct ProjectileVisuals {
    /// In `PROJECTILE_ORDER`.
    pub batches: Vec<ProjectileBatch>,
    team_colors: [[f32; 4]; 2],
}

impl Default for ProjectileVisuals {
    fn default() -> Self {
        let batches = PROJECTILE_ORDER
            .iter()
            .map(|&kind| ProjectileBatch {
                body: PoolBuffer::new(PROJECTILE_CAPACITY),
                team: PoolBuffer::new(PROJECTILE_CAPACITY),
                exhaust: matches!(kind, Weapon::Rocket | Weapon::Tow)
                    .then(|| PoolBuffer::new(PROJECTILE_CAPACITY)),
            })
            .collect();
        let team_colors = TEAM_COLORS.map(|hex| {
            let [r, g, b] = hex_to_linear(hex);
            [r, g, b, 1.0]
        });
        Self {
            batches,
            team_colors,
        }
    }
}

fn batch_index(kind: Weapon) -> usize {
    PROJECTILE_ORDER
        .iter()
        .position(|&k| k == kind)
        .expect("every munition has a batch")
}

impl ProjectileVisuals {
    pub fn batch(&self, kind: Weapon) -> &ProjectileBatch {
        &self.batches[batch_index(kind)]
    }

    pub fn reset(&mut self) {
        for batch in &mut self.batches {
            batch.body.clear();
            batch.team.clear();
            if let Some(exhaust) = &mut batch.exhaust {
                exhaust.clear();
            }
        }
    }

    /// Rebuild every layer from the shots. `alpha` places each shot between its
    /// previous and current physics pose (straight flight within one tick), so
    /// shells stay level with the interpolated tanks that fired them.
    pub fn update(&mut self, shots: &[RenderShot], time: f64, alpha: f64) {
        self.reset();
        let behind = (1.0 - alpha.clamp(0.0, 1.0)) * STEP;
        for shot in shots.iter().take(PROJECTILE_CAPACITY) {
            let batch = &mut self.batches[batch_index(shot.weapon)];
            let position = Vec3::new(
                (shot.x - shot.vx * behind) as f32,
                shot.visual_y.or(shot.y).unwrap_or(1.0) as f32,
                (shot.z - shot.vz * behind) as f32,
            );
            let spin = if shot.weapon == Weapon::Ricochet {
                time * RICOCHET_SPIN + f64::from(shot.id)
            } else {
                0.0
            };
            let yaw = shot.vx.atan2(shot.vz) + spin;
            let world = Mat4::from_translation(position) * Mat4::from_rotation_y(yaw as f32);
            batch.body.push(record(world, [1.0; 4], [0.0; 4]));
            let team = self.team_colors[match shot.team {
                Team::Blue => 0,
                Team::Red => 1,
            }];
            batch.team.push(record(world, team, [0.0; 4]));
            if let Some(exhaust) = &mut batch.exhaust {
                // An attached flame; `rocket_smoke` lays the trail behind it.
                let phase = f64::from(shot.id);
                let flicker = 1.0 - FLAME_FLICKER
                    + FLAME_FLICKER
                        * 0.5
                        * ((time * FLAME_FLICKER_RATE[0] + phase).sin()
                            + (time * FLAME_FLICKER_RATE[1] + phase * 1.7).sin());
                let pulse = 1.0 + FLAME_PULSE * (time * FLAME_FLICKER_RATE[1] + phase).cos();
                // Scaled about the nozzle, so the flame stays attached to it.
                let nozzle = Vec3::new(0.0, 0.0, -(FLAME_ROOT * model_scale(shot.weapon)) as f32);
                let flame = world
                    * Mat4::from_translation(nozzle)
                    * Mat4::from_scale(Vec3::new(pulse as f32, pulse as f32, flicker as f32))
                    * Mat4::from_translation(-nozzle);
                exhaust.push(record(flame, [1.0; 4], [0.0; 4]));
            }
        }
    }
}

/// Planar bounds (x width, z length) of a mesh, for the compactness check.
pub fn footprint(mesh: &Mesh) -> Vec2 {
    let (mut min, mut max) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for p in &mesh.positions {
        let v = Vec2::new(p[0], p[2]);
        min = min.min(v);
        max = max.max(v);
    }
    max - min
}

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_core::sim::ammunition::AMMO_ORDER;

    fn shot(id: u32, kind: Weapon, team: Team, x: f64, z: f64) -> RenderShot {
        RenderShot {
            id,
            weapon: kind,
            team,
            x,
            z,
            y: Some(1.0),
            visual_y: None,
            vx: 12.0,
            vz: -18.0,
        }
    }

    /// Both teams fire every player munition; spread is three pellets.
    fn volley() -> Vec<RenderShot> {
        let mut shots = Vec::new();
        for team in [Team::Blue, Team::Red] {
            for (i, &kind) in AMMO_ORDER.iter().enumerate() {
                let pellets = if kind == Weapon::Spread { 3 } else { 1 };
                for pellet in 0..pellets {
                    let id = shots.len() as u32;
                    shots.push(shot(
                        id,
                        kind,
                        team,
                        (i as f64 - 2.0) * 4.0 + pellet as f64 * 0.4,
                        team as u8 as f64 * 4.0 - 5.0,
                    ));
                }
            }
        }
        shots
    }

    #[test]
    fn every_munition_draws_aligned_body_team_and_exhaust_instances() {
        let mut visuals = ProjectileVisuals::default();
        visuals.update(&volley(), 2.0, 1.0);
        let counts: Vec<_> = visuals.batches.iter().map(|b| b.body.len()).collect();
        assert_eq!(counts, [2, 6, 2, 2, 2, 0]);
        for (batch, kind) in visuals.batches.iter().zip(PROJECTILE_ORDER) {
            assert_eq!(batch.team.len(), batch.body.len(), "{kind:?} team layer");
            assert_eq!(
                batch
                    .exhaust
                    .as_ref()
                    .map_or(batch.body.len(), PoolBuffer::len),
                batch.body.len()
            );
            assert_eq!(
                batch.exhaust.is_some(),
                matches!(kind, Weapon::Rocket | Weapon::Tow)
            );
        }
        // Instances follow shot order within a batch, so team colors follow teams.
        let standard = &visuals.batch(Weapon::Standard).team;
        for (index, hex) in TEAM_COLORS.iter().enumerate() {
            let [r, g, b] = hex_to_linear(*hex);
            assert_eq!(standard.records()[index].tint, [r, g, b, 1.0]);
        }
    }

    #[test]
    fn instances_sit_at_the_shot_position_facing_its_velocity() {
        let mut visuals = ProjectileVisuals::default();
        let mut piercing = shot(1, Weapon::Piercing, Team::Blue, 3.0, -4.0);
        piercing.visual_y = Some(1.4);
        visuals.update(&[piercing], 0.0, 1.0);
        let world = visuals.batch(Weapon::Piercing).body.records()[0].world();
        let (_, rotation, position) = world.to_scale_rotation_translation();
        assert!(
            position.distance(Vec3::new(3.0, 1.4, -4.0)) < 1e-6,
            "visual height wins over y"
        );
        let forward = rotation * Vec3::Z;
        let expected = 12f32.atan2(-18.0);
        assert!((forward.x.atan2(forward.z) - expected).abs() < 1e-6);
        // Between ticks the shell sits back along its flight, level with the tanks.
        visuals.update(&[piercing], 0.0, 0.5);
        let world = visuals.batch(Weapon::Piercing).body.records()[0].world();
        let back = world.w_axis.truncate();
        let expected = Vec3::new(
            (3.0 - 12.0 * 0.5 * STEP) as f32,
            1.4,
            (-4.0 + 18.0 * 0.5 * STEP) as f32,
        );
        assert!(back.distance(expected) < 1e-5);
    }

    #[test]
    fn player_munition_models_stay_compact() {
        for kind in AMMO_ORDER {
            let size = footprint(&projectile_model(kind).body);
            assert!(size.x <= 0.53, "{kind:?} width {}", size.x);
            assert!(size.y <= 1.01, "{kind:?} length {}", size.y);
        }
    }

    #[test]
    fn a_flood_of_shots_is_capped_and_cleared_by_the_next_empty_update() {
        let mut visuals = ProjectileVisuals::default();
        let template = volley();
        let shots: Vec<_> = (0..650)
            .map(|i| RenderShot {
                id: i as u32,
                x: ((i % 30) as f64 - 15.0) * 1.5,
                z: ((i / 30) as f64 - 10.0) * 1.5,
                ..template[i % template.len()]
            })
            .collect();
        visuals.update(&shots, 3.0, 1.0);
        let drawn: usize = visuals.batches.iter().map(|b| b.body.len()).sum();
        assert_eq!(drawn, PROJECTILE_CAPACITY);
        for batch in &visuals.batches {
            for layer in [Some(&batch.body), Some(&batch.team), batch.exhaust.as_ref()]
                .into_iter()
                .flatten()
            {
                assert!(
                    layer.records().iter().all(|r| r
                        .world_rows
                        .as_flattened()
                        .iter()
                        .all(|v| v.is_finite()))
                );
            }
        }
        visuals.update(&[], 4.0, 1.0);
        assert!(visuals.batches.iter().all(|b| b.body.is_empty()
            && b.team.is_empty()
            && b.exhaust.as_ref().is_none_or(PoolBuffer::is_empty)));
    }

    #[test]
    fn every_model_merges_into_one_colored_mesh() {
        for kind in PROJECTILE_ORDER {
            let model = projectile_model(kind);
            assert_eq!(model.body.colors.len(), model.body.positions.len());
            assert!(model.body.triangle_count() > 10);
            if let Some(exhaust) = &model.exhaust {
                assert_eq!(exhaust.colors.len(), exhaust.positions.len());
            }
        }
    }
}

//! Port of `scenery.ts` and the scenery half of `presentation.ts` (`buildScenery`,
//! `customFloor`, `customSpawnPad`): the per-theme entry points, shared spawn pads,
//! arena floors and the village yard.
//!
//! Presentation builds each theme's [`Scenery`] once (it needs no physics world, so
//! startup can build it while physics loads), keeps it across rounds, shows only the
//! current theme's root, and calls [`Scenery::update`] every frame with its clock.
//! Extra levels have no themed scenery: they show [`create_spawn_pads`] for their
//! scale and one or two [`create_arena_floor`]s.

use std::sync::Arc;

use crate::geometry::{Path, RingGeometry, Shape, plane_geometry_segments, shape_geometry};
use crate::scene::{Material, Node};
use crate::sim::arena::spawn_positions;
use crate::sim::kinds::Team;
use crate::sim::math::Vec2;
use crate::sim::model_data::ARENA;
use glam::DVec3;

use super::batching::{batch, paint_mesh};
use super::ground_surfaces::{GroundKind, ground_material, ground_uvs, village_ground_material};
use super::harbor_scenery::HarborScenery;
use super::model_primitives::{
    TEAM_COLORS, box_part, cylinder_part, paint, put, rotated, shadow_receiver, shadowed,
};
use super::quarry_scenery::quarry_scenery;
use super::village_atmosphere::CHIMNEY_SMOKE_NODE;
use super::village_scenery::VillageScenery;

/// A standard map's scenery theme (`RenderState["mapTheme"]`).
pub use crate::sim::maps::MapTheme;

/// One theme's retained scenery.
pub enum Scenery {
    Village(Box<VillageScenery>),
    Harbor(Box<HarborScenery>),
    Quarry(Box<Node>),
}

/// `Presentation.buildScenery(theme)`: build a theme's scenery. Callers cache it; it
/// never changes between rounds except through [`Scenery::update`] and, for the
/// village, [`VillageScenery::set_covers`].
pub fn build_scenery(theme: MapTheme) -> Scenery {
    match theme {
        MapTheme::Village => Scenery::Village(Box::default()),
        MapTheme::Harbor => Scenery::Harbor(Box::default()),
        MapTheme::Quarry => Scenery::Quarry(Box::new(quarry_scenery())),
    }
}

impl Scenery {
    /// A theme's scenery around a root [`build_scenery`] built before (read back from
    /// a page's geometry cache, `scene_codec`).
    pub fn from_root(theme: MapTheme, root: Node) -> Scenery {
        match theme {
            MapTheme::Village => Scenery::Village(Box::new(VillageScenery::from_root(root))),
            MapTheme::Harbor => Scenery::Harbor(Box::new(HarborScenery { root })),
            MapTheme::Quarry => Scenery::Quarry(Box::new(root)),
        }
    }

    pub fn root(&self) -> &Node {
        match self {
            Scenery::Village(scenery) => &scenery.root,
            Scenery::Harbor(scenery) => &scenery.root,
            Scenery::Quarry(root) => root,
        }
    }

    /// Animate transforms for the presentation clock (seconds). Shader-animated
    /// parts (water, meadow, smoke) read the same clock as a uniform; see
    /// `effects_scenery`. The quarry has no animated scenery.
    pub fn update(&mut self, time: f64) {
        match self {
            Scenery::Village(scenery) => scenery.update(time),
            Scenery::Harbor(scenery) => scenery.update(time),
            Scenery::Quarry(_) => {}
        }
    }

    /// Drop every drawable but the chimney smoke's, once presentation has taken its
    /// own copy of the tree to draw: [`update`](Self::update) and the movers' poses
    /// need only the nodes' transforms, and [`VillageScenery::set_covers`] the smoke.
    /// The retained tree otherwise kept a theme's whole scenery (20-odd MB of
    /// vertices) on the CPU for the session after the GPU had it.
    pub fn release_drawables(&mut self) {
        let root = match self {
            Scenery::Village(scenery) => &mut scenery.root,
            Scenery::Harbor(scenery) => &mut scenery.root,
            Scenery::Quarry(root) => root,
        };
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if node.name == CHIMNEY_SMOKE_NODE {
                continue;
            }
            node.drawable = None;
            stack.extend(node.children.iter_mut());
        }
    }
}

// ---------------------------------------------------------------------------
// Spawn pads, floors and the village yard.

/// `createSpawnPads(scale)`: both teams' octagonal deployment plinths, batched. Used
/// by the village yard (scale 1) and by extra levels at their own scale.
pub fn create_spawn_pads(scale: f64) -> Node {
    let mut details = Node::group("spawn-pads");
    let mut rim = RingGeometry {
        inner_radius: 2.05,
        outer_radius: 2.3,
        theta_segments: 12,
        theta_start: 0.06,
        theta_length: std::f64::consts::FRAC_PI_4 - 0.12,
    }
    .build();
    rim.rotate_x(-std::f64::consts::FRAC_PI_2);
    let rim = Arc::new(rim);
    let mut arrow_path = Path::new();
    arrow_path
        .move_to(-0.28, -0.55)
        .line_to(0.28, 0.0)
        .line_to(-0.28, 0.55)
        .line_to(-0.48, 0.37)
        .line_to(-0.1, 0.0)
        .line_to(-0.48, -0.37)
        .close_path();
    let mut arrow = shape_geometry(&[Shape::new(arrow_path)]);
    arrow.rotate_x(-std::f64::consts::FRAC_PI_2);
    let arrow = Arc::new(arrow);
    for team in [Team::Blue, Team::Red] {
        let side = if team == Team::Blue { -1.0 } else { 1.0 };
        let color = TEAM_COLORS[team.index()];
        for Vec2 { x, z } in spawn_positions(team, scale) {
            // Low octagonal deployment plinth with a recessed deck and segmented team lights.
            for (r, h, color, sides, y) in [
                (2.75, 0.1, 0x283c4e, 8, 0.08),
                (2.52, 0.045, 0x718898, 8, 0.135),
                (2.37, 0.035, 0x223d51, 32, 0.17),
                (1.98, 0.025, 0x455e70, 8, 0.193),
            ] {
                put(&mut details, cylinder_part(r, h, color, sides), x, y, z);
            }
            for i in 0..8 {
                let angle = (f64::from(i) * std::f64::consts::PI) / 4.0;
                let segment = rotated(Node::mesh(rim.clone(), paint(color)), 0.0, angle, 0.0);
                put(&mut details, segment, x, 0.198, z);
                put(
                    &mut details,
                    cylinder_part(0.075, 0.025, 0xc9d6dd, 8),
                    x + angle.cos() * 2.58,
                    0.175,
                    z + angle.sin() * 2.58,
                );
            }
            for dz in [-1.25, 1.25] {
                for i in 0..5 {
                    put(
                        &mut details,
                        box_part(0.18, 0.02, 0.4, 0x1a2b3c, 0.005),
                        x - 0.52 + f64::from(i) * 0.26,
                        0.218,
                        z + dz,
                    );
                }
            }
            // Concentric paint and inward chevrons make the pad legible when unoccupied.
            let badge = rotated(
                box_part(0.7, 0.025, 0.7, color, 0.035),
                0.0,
                std::f64::consts::FRAC_PI_4,
                0.0,
            );
            put(&mut details, badge, x, 0.22, z);
            for offset in [3.1, 3.8] {
                let yaw = if team == Team::Blue {
                    0.0
                } else {
                    std::f64::consts::PI
                };
                let chevron = rotated(Node::mesh(arrow.clone(), paint(color)), 0.0, yaw, 0.0);
                put(&mut details, chevron, x - side * offset, 0.09, z);
            }
        }
    }
    batch(&mut details);
    details
}

/// `createArenaFloor(renderer, kind, extent)` raised to `y`: a flat ground square.
/// Dry grass is subdivided every ~2.5 m and tinted by broad vertex-color patches.
/// Each floor gets its own material, like the TypeScript (the village terrain
/// shares it). Extra levels use it as `customFloor(kind, extent, y)`: the floor
/// (`y` 0.008) or outer floor (`y` -0.002), cached by the caller per key.
pub fn create_arena_floor(kind: GroundKind, extent: f64, y: f64) -> Node {
    let mut surface = ground_material(kind);
    let grass = kind == GroundKind::DryGrass;
    let segments = if grass {
        crate::geometry::math::js_round(extent / 2.5).max(1.0) as u32
    } else {
        1
    };
    let mut geometry = plane_geometry_segments(extent, extent, segments, segments);
    geometry.rotate_x(-std::f64::consts::FRAC_PI_2);
    ground_uvs(&mut geometry, 0.0, 0.0);
    if grass {
        surface.color = crate::scene::Color(0xaee6a6);
        surface.vertex_colors = true;
        geometry.colors = geometry
            .positions
            .iter()
            .map(|p| {
                let (x, z) = (f64::from(p[0]), f64::from(p[2]));
                let patch =
                    0.5 + 0.25 * (x * 0.18 + z * 0.09).sin() + 0.25 * (z * 0.22 - x * 0.1).sin();
                [
                    (0.68 + patch * 0.28) as f32,
                    (0.83 + patch * 0.14) as f32,
                    (0.42 + patch * 0.36) as f32,
                ]
            })
            .collect();
    }
    let mut floor = shadow_receiver(Arc::new(geometry), Arc::new(surface));
    floor.position.y = y;
    floor
}

/// `createTerrain(scene)` plus `createYardDetails`: the village board, the grass
/// floor (whose shader wears the roads into it), spawn pads and team fences,
/// appended to `scene`. Returns the ground material, which the valley terrain
/// shares.
pub(crate) fn create_terrain(scene: &mut Node) -> Arc<Material> {
    let mut board = box_part(ARENA * 2.0 + 6.0, 1.2, ARENA * 2.0 + 6.0, 0x947c4d, 0.4);
    paint_mesh(&mut board);
    put(scene, board, 0.0, -0.8, 0.0);
    let extent = ARENA * 2.0;
    let segments = crate::geometry::math::js_round(extent / 2.5) as u32;
    let mut geometry = plane_geometry_segments(extent, extent, segments, segments);
    geometry.rotate_x(-std::f64::consts::FRAC_PI_2);
    ground_uvs(&mut geometry, 0.0, 0.0);
    geometry.colors = vec![[1.0; 3]; geometry.positions.len()];
    let ground = village_ground_material();
    let mut floor = shadow_receiver(Arc::new(geometry), ground.clone());
    floor.position.y = 0.008;
    scene.children.push(floor);
    create_yard_details(scene);
    ground
}

/// `createYardDetails`: static village dressing; collidable objects are created
/// separately from the arena layout.
fn create_yard_details(scene: &mut Node) {
    let mut details = Node::group("");
    details.children.push(create_spawn_pads(1.0));
    batch(&mut details);
    scene.children.push(details);
    scene.children.push(team_bunting());
}

/// Bunting poles stand this high above a pier's weathering, and the line sags
/// this far between them.
const BUNTING_POLE: f64 = 1.3;
const BUNTING_SAG: f64 = 0.62;
/// Pennants along the line: spacing, width along it and drop.
const PENNANT_STEP: f64 = 0.74;
const PENNANT_WIDTH: f64 = 0.56;
const PENNANT_DROP: f64 = 0.68;
const BUNTING_HEIGHT: f64 = 2.2;

/// Each team's side wall flies its colours: short poles on the buttress piers
/// carry a sagging line of pennants, team paint alternating with a paler tint of
/// it, so the
/// sides read from every camera. Poles skip the piers where team flags stand.
fn team_bunting() -> Node {
    use super::building_kit::{PIER_RISE, Palette, Surface, add_building, wall_piers};
    use super::tank_kit::Kit;
    use crate::sim::arena::{BOUNDARY_THICKNESS, spawn_positions};
    let x_wall = ARENA + BOUNDARY_THICKNESS / 2.0;
    let pole_base = BUNTING_HEIGHT + PIER_RISE + 0.18;
    let line = pole_base + BUNTING_POLE - 0.06;
    let flags: Vec<f64> = spawn_positions(Team::Blue, 1.0)
        .iter()
        .map(|s| s.z)
        .collect();
    let length = ARENA * 2.0 + BOUNDARY_THICKNESS * 2.0;
    let poles: Vec<f64> = wall_piers(length)
        .into_iter()
        .filter(|z| flags.iter().all(|f| (f - z).abs() > 1.5))
        .collect();
    let mut kit: Kit<Surface> = Kit::new();
    let mut positions: Vec<f64> = Vec::new();
    let mut colors: Vec<[f32; 3]> = Vec::new();
    for team in [Team::Blue, Team::Red] {
        let side = if team == Team::Blue { -1.0 } else { 1.0 };
        let x = side * x_wall;
        let paint =
            crate::geometry::math::hex_to_linear(TEAM_COLORS[team.index()]).map(|c| c as f32);
        let pale = paint.map(|c| c * 0.45 + 0.55 * 0.9);
        for &z in &poles {
            kit.post(
                Surface::Wood,
                0.045,
                BUNTING_POLE,
                DVec3::new(x, pole_base, z),
                6,
            );
            kit.post(
                Surface::Brass,
                0.07,
                0.1,
                DVec3::new(x, pole_base + BUNTING_POLE, z),
                6,
            );
        }
        let mut pennant = 0usize;
        for pair in poles.windows(2) {
            let (z0, z1) = (pair[0], pair[1]);
            let span = z1 - z0;
            let at =
                |t: f64| DVec3::new(x, line - BUNTING_SAG * 4.0 * t * (1.0 - t), z0 + span * t);
            let segments = 8;
            for i in 0..segments {
                let (a, b) = (
                    f64::from(i) / f64::from(segments),
                    f64::from(i + 1) / f64::from(segments),
                );
                kit.rod(Surface::Iron, 0.012, at(a), at(b), 4);
            }
            let count = (span / PENNANT_STEP).floor() as usize;
            for k in 0..count {
                let t = (k as f64 + 0.5) / count as f64;
                let half = PENNANT_WIDTH / 2.0 / span;
                let (left, right) = (at(t - half), at(t + half));
                let mut tip = at(t);
                tip.y -= PENNANT_DROP;
                // A little flutter out of the line's plane.
                tip.x += side * 0.05 * ((pennant as f64) * 1.7).sin();
                let color = if pennant.is_multiple_of(2) {
                    paint
                } else {
                    pale
                };
                for p in [left, right, tip] {
                    positions.extend([p.x, p.y - 0.015, p.z]);
                    colors.push(color);
                }
                pennant += 1;
            }
        }
    }
    let mut group = Node::group("");
    let palette = Palette {
        siding: 0xffffff,
        trim: 0xffffff,
        accent: 0xffffff,
        roof: 0xffffff,
    };
    add_building(&mut group, kit.finish(), &palette);
    let normals: Vec<f64> = (0..positions.len() / 3)
        .flat_map(|_| [1.0, 0.0, 0.0])
        .collect();
    let mut cloth = crate::geometry::Mesh::from_f64(&positions, &normals, &[], None);
    cloth.colors = colors;
    let material = Material {
        vertex_colors: true,
        side: crate::scene::Side::Double,
        ..Material::standard(0xffffff, 0.0, 0.85)
    };
    group
        .children
        .push(shadowed(Arc::new(cloth), Arc::new(material)));
    group
}

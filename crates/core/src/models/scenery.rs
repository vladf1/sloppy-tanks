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
use crate::sim::data::ARENA;
use crate::sim::types::{Team, Vec2};

use super::batching::{batch, paint_mesh};
use super::ground_surfaces::{GroundKind, ground_material, ground_uvs, road_geometry};
use super::harbor_scenery::HarborScenery;
use super::model_primitives::{
    TEAM_COLORS, box_part, cylinder_part, paint, put, rotated, shadow_receiver,
};
use super::quarry_scenery::quarry_scenery;
use super::village_roads::village_roads;
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
/// floor, roads, spawn pads and team fences, appended to `scene`. Returns the
/// grass material, which the valley terrain shares.
pub(crate) fn create_terrain(scene: &mut Node) -> Arc<Material> {
    let mut board = box_part(ARENA * 2.0 + 6.0, 1.2, ARENA * 2.0 + 6.0, 0x947c4d, 0.4);
    paint_mesh(&mut board);
    put(scene, board, 0.0, -0.8, 0.0);
    let floor = create_arena_floor(GroundKind::DryGrass, ARENA * 2.0, 0.008);
    let grass = floor
        .drawable
        .as_ref()
        .expect("floor mesh")
        .material
        .clone();
    scene.children.push(floor);
    create_yard_details(scene);
    grass
}

/// `createYardDetails`: static village dressing; collidable objects are created
/// separately from the arena layout.
fn create_yard_details(scene: &mut Node) {
    let mut roads = Node::group("");
    let road_material = Arc::new(Material {
        vertex_colors: true,
        transparent: true,
        depth_write: false,
        ..ground_material(GroundKind::PackedDirt)
    });
    for road in village_roads() {
        let geometry = road_geometry(road.w, road.d, road.x, road.z);
        put(
            &mut roads,
            Node::mesh(Arc::new(geometry), road_material.clone()),
            road.x,
            road.y,
            road.z,
        );
    }
    batch(&mut roads);
    for mesh in &mut roads.children {
        if let Some(drawable) = &mut mesh.drawable {
            drawable.cast_shadow = false;
            // Ground blending must precede shields, pickup glows and track decals.
            drawable.render_order = -1;
        }
    }
    scene.children.push(roads);
    let mut details = Node::group("");
    details.children.push(create_spawn_pads(1.0));
    for team in [0usize, 1] {
        let side = if team == 0 { -1.0 } else { 1.0 };
        let color = TEAM_COLORS[team];
        for z in (-57..=57).step_by(2) {
            let z = f64::from(z);
            put(
                &mut details,
                box_part(0.16, 1.7, 0.16, color, 0.015),
                side * ARENA,
                2.7,
                z,
            );
            put(
                &mut details,
                box_part(0.13, 0.16, 2.2, color, 0.01),
                side * ARENA,
                3.0,
                z,
            );
        }
    }
    batch(&mut details);
    scene.children.push(details);
}

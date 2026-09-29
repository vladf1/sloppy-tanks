//! Port of `scenery.ts` and the scenery half of `presentation.ts` (`buildScenery`,
//! `customFloor`, `customSpawnPad`): the per-theme entry points, shared spawn pads,
//! arena floors, the village yard, sun shadow fitting, and the JavaScript-exact math
//! helpers the scenery modules share.
//!
//! Presentation builds each theme's [`Scenery`] once (it needs no physics world, so
//! startup can build it while physics loads), keeps it across rounds, shows only the
//! current theme's root, and calls [`Scenery::update`] every frame with its clock.
//! Extra levels have no themed scenery: they show [`custom_spawn_pads`] for their
//! scale and one or two [`custom_floor`]s.

use std::sync::Arc;

use glam::{DMat4, DQuat, DVec2, DVec3};

use crate::geometry::math::{compose, decompose, hex_to_linear, normalize};
use crate::geometry::{Mesh, Path, RingGeometry, Shape, plane_geometry_segments, shape_geometry};
use crate::scene::{Material, Node};

use super::batching::batch;
use super::ground_surfaces::{GroundKind, ground_material, ground_uvs, road_geometry};
use super::harbor_scenery::HarborScenery;
use super::model_primitives::{TEAM_COLORS, box_part, cylinder_part, paint, put, rotated};
use super::pending_scenery::{ARENA, spawn_positions};
use super::quarry_scenery::QuarryScenery;
use super::village_roads::village_roads;
use super::village_scenery::VillageScenery;

/// A standard map's scenery theme (`RenderState["mapTheme"]`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MapTheme {
    Village,
    Harbor,
    Quarry,
}

/// One theme's retained scenery.
pub enum Scenery {
    Village(Box<VillageScenery>),
    Harbor(Box<HarborScenery>),
    Quarry(Box<QuarryScenery>),
}

/// `Presentation.buildScenery(theme)`: build a theme's scenery. Callers cache it; it
/// never changes between rounds except through [`Scenery::update`] and, for the
/// village, [`VillageScenery::set_covers`].
pub fn build_scenery(theme: MapTheme) -> Scenery {
    match theme {
        MapTheme::Village => Scenery::Village(Box::default()),
        MapTheme::Harbor => Scenery::Harbor(Box::default()),
        MapTheme::Quarry => Scenery::Quarry(Box::default()),
    }
}

impl Scenery {
    pub fn root(&self) -> &Node {
        match self {
            Scenery::Village(scenery) => &scenery.root,
            Scenery::Harbor(scenery) => &scenery.root,
            Scenery::Quarry(scenery) => &scenery.root,
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
        phi_segments: 1,
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
    let mut arrow = shape_geometry(&[Shape::new(arrow_path)], 12);
    arrow.rotate_x(-std::f64::consts::FRAC_PI_2);
    let arrow = Arc::new(arrow);
    for team in [0u8, 1] {
        let side = if team == 0 { -1.0 } else { 1.0 };
        let color = TEAM_COLORS[usize::from(team)];
        for (x, z) in spawn_positions(team, scale) {
            // Low octagonal deployment plinth with a recessed deck and segmented team lights.
            put(
                &mut details,
                cylinder_part(2.75, 0.1, 0x283c4e, 8),
                x,
                0.08,
                z,
            );
            put(
                &mut details,
                cylinder_part(2.52, 0.045, 0x718898, 8),
                x,
                0.135,
                z,
            );
            put(
                &mut details,
                cylinder_part(2.37, 0.035, 0x223d51, 32),
                x,
                0.17,
                z,
            );
            put(
                &mut details,
                cylinder_part(1.98, 0.025, 0x455e70, 8),
                x,
                0.193,
                z,
            );
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
                let yaw = if team == 0 { 0.0 } else { std::f64::consts::PI };
                let chevron = rotated(Node::mesh(arrow.clone(), paint(color)), 0.0, yaw, 0.0);
                put(&mut details, chevron, x - side * offset, 0.09, z);
            }
        }
    }
    batch(&mut details);
    details
}

/// `createArenaFloor(renderer, kind, extent)`: a flat ground square. Dry grass is
/// subdivided every ~2.5 m and tinted by broad vertex-color patches. Each floor
/// gets its own material, like the TypeScript (the village terrain shares it).
pub fn create_arena_floor(kind: GroundKind, extent: f64) -> Node {
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
    let mut floor = Node::mesh(Arc::new(geometry), Arc::new(surface));
    if let Some(drawable) = &mut floor.drawable {
        drawable.receive_shadow = true;
    }
    floor
}

/// `customSpawnPad(scale)`: an extra level's pads (cache one per scale).
pub fn custom_spawn_pads(scale: f64) -> Node {
    create_spawn_pads(scale)
}

/// `customFloor(kind, extent, y)`: an extra level's floor (`y` 0.008) or outer floor
/// (`y` -0.002), `extent` defaulting to the standard arena (cache one per key).
pub fn custom_floor(kind: GroundKind, extent: Option<f64>, y: f64) -> Node {
    let mut floor = create_arena_floor(kind, extent.unwrap_or(ARENA * 2.0));
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
    let floor = create_arena_floor(GroundKind::DryGrass, ARENA * 2.0);
    let grass = floor
        .drawable
        .as_ref()
        .expect("floor mesh")
        .material
        .clone();
    put(scene, floor, 0.0, 0.008, 0.0);
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

// ---------------------------------------------------------------------------
// Lighting and sun shadows (createLighting, defaultSunShadow, fitSunShadow).

/// The sun shadow camera's depth span; the tuned bias assumes it.
pub const SHADOW_DEPTH: f64 = 219.5;

/// An orthographic shadow camera box, in the light camera's view space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowBox {
    pub left: f64,
    pub right: f64,
    pub bottom: f64,
    pub top: f64,
    pub near: f64,
    pub far: f64,
}

/// `createLighting`'s fixed settings (presentation overrides colors, sun position
/// and intensities per theme on reset).
pub mod lighting {
    pub const BACKGROUND: u32 = 0x59bbed;
    pub const FOG_NEAR: f64 = 150.0;
    pub const FOG_FAR: f64 = 260.0;
    pub const FILL_SKY: u32 = 0xbdd5f5;
    pub const FILL_GROUND: u32 = 0x75859b;
    pub const FILL_INTENSITY: f64 = 1.65;
    pub const SUN_COLOR: u32 = 0xffd59b;
    pub const SUN_INTENSITY: f64 = 2.8;
    pub const SUN_POSITION: [f64; 3] = [-45.0, 85.0, 25.0];
    pub const SHADOW_MAP_SIZE: u32 = 2048;
    pub const SHADOW_NORMAL_BIAS: f64 = 0.05;
    pub const SHADOW_BIAS: f64 = -0.0002;
}

/// `defaultSunShadow`: the original square sun shadow box shared by the village
/// and harbor.
pub fn default_sun_shadow() -> ShadowBox {
    ShadowBox {
        left: -(ARENA + 10.0),
        bottom: -(ARENA + 10.0),
        right: ARENA + 10.0,
        top: ARENA + 10.0,
        near: 0.5,
        far: 0.5 + SHADOW_DEPTH,
    }
}

/// `fitSunShadow(sun, half, low, high)`: fit the sun's orthographic shadow box
/// around a ground square (`|x|, |z| <= half`, `low <= y <= high`) for the sun's
/// direction from `sun` towards `target`. A square box aimed at a low, diagonal
/// sun lands on the ground as a tilted strip that misses two arena corners; this
/// one covers the whole square. The depth range keeps its default span.
pub fn fit_sun_shadow(sun: DVec3, target: DVec3, half: f64, low: f64, high: f64) -> ShadowBox {
    // Object3D.lookAt for a camera: Matrix4.lookAt(eye, target, up) with the
    // camera looking down its -z.
    let mut z = sun - target;
    if z.length_squared() == 0.0 {
        z.z = 1.0;
    }
    z = normalize(z);
    let up = DVec3::Y;
    let mut x = up.cross(z);
    if x.length_squared() == 0.0 {
        if up.z.abs() == 1.0 {
            z.x += 0.0001;
        } else {
            z.z += 0.0001;
        }
        z = normalize(z);
        x = up.cross(z);
    }
    x = normalize(x);
    let y = z.cross(x);
    let world = DMat4::from_cols(x.extend(0.0), y.extend(0.0), z.extend(0.0), sun.extend(1.0));
    let inverse = world.inverse();
    let mut min = DVec3::splat(f64::INFINITY);
    let mut max = DVec3::splat(f64::NEG_INFINITY);
    for cx in [-half, half] {
        for cy in [low, high] {
            for cz in [-half, half] {
                let corner =
                    crate::geometry::math::transform_point(&inverse, DVec3::new(cx, cy, cz));
                min = min.min(corner);
                max = max.max(corner);
            }
        }
    }
    // The camera looks down -z; orthographic near may sit behind the light.
    let near = -max.z - 2.0;
    ShadowBox {
        left: min.x,
        right: max.x,
        bottom: min.y,
        top: max.y,
        near,
        far: near + SHADOW_DEPTH,
    }
}

// ---------------------------------------------------------------------------
// Shared helpers: JavaScript/Three.js-exact math and node utilities.

/// V8's `Math.hypot`: scale by the largest magnitude and Kahan-sum the squares.
pub(crate) fn js_hypot(values: &[f64]) -> f64 {
    let max = values.iter().fold(0.0f64, |max, v| max.max(v.abs()));
    if max == f64::INFINITY {
        return f64::INFINITY;
    }
    if values.iter().any(|v| v.is_nan()) {
        return f64::NAN;
    }
    if max == 0.0 {
        return 0.0;
    }
    let mut sum = 0.0;
    let mut compensation = 0.0;
    for value in values {
        let n = value.abs() / max;
        let summand = n * n - compensation;
        let preliminary = sum + summand;
        compensation = (preliminary - sum) - summand;
        sum = preliminary;
    }
    sum.sqrt() * max
}

/// `MathUtils.smoothstep(x, min, max)`.
pub(crate) fn smoothstep(x: f64, min: f64, max: f64) -> f64 {
    if x <= min {
        return 0.0;
    }
    if x >= max {
        return 1.0;
    }
    let x = (x - min) / (max - min);
    x * x * (3.0 - 2.0 * x)
}

/// `MathUtils.lerp(x, y, t)`.
pub(crate) fn lerp(x: f64, y: f64, t: f64) -> f64 {
    (1.0 - t) * x + t * y
}

/// `MathUtils.clamp(value, min, max)`.
pub(crate) fn clamp(value: f64, min: f64, max: f64) -> f64 {
    min.max(max.min(value))
}

/// `Color.lerp(target, alpha)` on linear channels.
pub(crate) fn lerp_color(color: [f64; 3], target: [f64; 3], alpha: f64) -> [f64; 3] {
    [
        color[0] + (target[0] - color[0]) * alpha,
        color[1] + (target[1] - color[1]) * alpha,
        color[2] + (target[2] - color[2]) * alpha,
    ]
}

/// `new THREE.Color(hex)` channels (linear).
pub(crate) fn linear(hex: u32) -> [f64; 3] {
    hex_to_linear(hex)
}

/// `Quaternion.setFromUnitVectors(from, to)` for normalised vectors.
pub(crate) fn quat_from_unit_vectors(from: DVec3, to: DVec3) -> DQuat {
    let r = from.x * to.x + from.y * to.y + from.z * to.z + 1.0;
    let (x, y, z, w) = if r < 1e-8 {
        if from.x.abs() > from.z.abs() {
            (-from.y, from.x, 0.0, 0.0)
        } else {
            (0.0, -from.z, from.y, 0.0)
        }
    } else {
        (
            from.y * to.z - from.z * to.y,
            from.z * to.x - from.x * to.z,
            from.x * to.y - from.y * to.x,
            r,
        )
    };
    let length = (x * x + y * y + z * z + w * w).sqrt();
    if length == 0.0 {
        return DQuat::IDENTITY;
    }
    let inverse = 1.0 / length;
    DQuat::from_xyzw(x * inverse, y * inverse, z * inverse, w * inverse)
}

/// A part stretched between two points along its local y axis (the scenery's
/// `beam` helpers): positioned at the midpoint and turned from +y to the segment.
pub(crate) fn span_between(mut part: Node, from: DVec3, to: DVec3) -> Node {
    part.position = DVec3::new(
        (from.x + to.x) * 0.5,
        (from.y + to.y) * 0.5,
        (from.z + to.z) * 0.5,
    );
    part.rotation = quat_from_unit_vectors(DVec3::Y, normalize(to - from));
    part
}

/// `Vector3.distanceTo`.
pub(crate) fn distance(a: DVec3, b: DVec3) -> f64 {
    let d = a - b;
    (d.x * d.x + d.y * d.y + d.z * d.z).sqrt()
}

/// `Object3D.applyMatrix4(matrix)` for each of `group`'s children, appended to
/// `parent`: bakes an assembly's transform into its parts so a later batch merges
/// them with the parent's other parts.
pub(crate) fn adopt_children(parent: &mut Node, group: Node) {
    let matrix = compose(group.position, group.rotation, group.scale);
    for mut child in group.children {
        let local = matrix * compose(child.position, child.rotation, child.scale);
        let (position, rotation, scale) = decompose(&local);
        child.position = position;
        child.rotation = rotation;
        child.scale = scale;
        parent.children.push(child);
    }
}

/// `paintMesh(mesh)`: bake an opaque paint color into a standalone mesh's vertices
/// and switch it to the shared vertex-color material, so it shares the batched
/// parts' shader. The geometry keeps its indices.
pub(crate) fn paint_mesh(node: &mut Node) {
    let Some(drawable) = &mut node.drawable else {
        return;
    };
    let painted = shared_vertex_material(&drawable.material);
    if Arc::ptr_eq(&painted, &drawable.material) {
        return;
    }
    let [r, g, b] = hex_to_linear(drawable.material.color.0);
    let mut mesh = (*drawable.mesh).clone();
    mesh.colors = vec![[r as f32, g as f32, b as f32]; mesh.positions.len()];
    drawable.mesh = Arc::new(mesh);
    drawable.material = painted;
}

/// The vertex-color material `batch` would draw `source` with (its private
/// `vertexMaterial`), found by batching a one-triangle probe.
fn shared_vertex_material(source: &Arc<Material>) -> Arc<Material> {
    let probe = Mesh::from_f64(
        &[0.0; 9],
        &[0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0],
        &[0.0; 6],
        None,
    );
    let mut group = Node::group("");
    group
        .children
        .push(Node::mesh(Arc::new(probe), source.clone()));
    batch(&mut group);
    group.children[0]
        .drawable
        .as_ref()
        .expect("batched probe")
        .material
        .clone()
}

/// `Vector3.applyQuaternion`.
pub(crate) fn apply_quaternion(v: DVec3, q: DQuat) -> DVec3 {
    let tx = 2.0 * (q.y * v.z - q.z * v.y);
    let ty = 2.0 * (q.z * v.x - q.x * v.z);
    let tz = 2.0 * (q.x * v.y - q.y * v.x);
    DVec3::new(
        v.x + q.w * tx + q.y * tz - q.z * ty,
        v.y + q.w * ty + q.z * tx - q.x * tz,
        v.z + q.w * tz + q.x * ty - q.y * tx,
    )
}

/// A 2D point.
pub(crate) fn v2(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

//! Models presentation owns: world-space HUD (tank bars, reticle, player ring
//! and spawn pulse), pickups and their collection glow, mines, debris pieces,
//! spawn pads, custom floors and flags. Ports of `tank-bars.ts`, `reticle.ts`,
//! `pickup-visuals.ts`, the model parts of `presentation.ts`, `barrel-debris.ts`,
//! `scenery.ts` (`createSpawnPads`, `createArenaFloor`) and `flags.ts`.
//!
//! Named nodes are joints that instances pose or hide; their names are the
//! constants below. Materials are built once per model; presentation caches the
//! prepared models for the whole session.

use std::f64::consts::PI;
use std::sync::Arc;

use glam::{DVec2, DVec3};
use sloppy_core::geometry::{
    CylinderGeometry, Mesh, RingGeometry, Shape, box_geometry, circle_geometry,
    cylinder_geometry, plane_geometry, plane_geometry_segments, ring_geometry,
    rounded_box_geometry, shape_geometry, sphere_geometry, tetrahedron_geometry,
    torus_geometry,
};
use sloppy_core::models::{DEFAULT_BOX_RADIUS, TEAM_COLORS, box_part, cylinder_part, paint, put};
use sloppy_core::scene::{
    Blending, Color, Effect, Material, Node, Side, TextureRef, Wrap,
};
use sloppy_core::sim::arena::spawn_positions;
use sloppy_core::sim::data::{MINE_RADIUS, SHIELD_CAPACITY, pickup};
use sloppy_core::sim::maps::GroundKind;
use sloppy_core::sim::{FragmentShape, PickupKind, Team};

/// Joint names.
pub mod joint {
    /// Tank bar hull fill, one per health color; scaled along x by the ratio.
    pub const BAR_FILLS: [&str; 3] = ["bar-fill-team", "bar-fill-mid", "bar-fill-low"];
    pub const BAR_RANKS: [&str; 3] = ["bar-rank-1", "bar-rank-2", "bar-rank-3"];
    pub const BAR_SHIELD: &str = "bar-shield";
    pub const BAR_SHIELD_FILL: &str = "bar-shield-fill";
    pub const BAR_SPAWN: &str = "bar-spawn";
    pub const BAR_SPAWN_FILL: &str = "bar-spawn-fill";
    /// Reticle ink and centre, one set per state.
    pub const RETICLE_READY: &str = "reticle-ready";
    pub const RETICLE_RELOADING: &str = "reticle-reloading";
    pub const RETICLE_CONFIRMED: &str = "reticle-confirmed";
    /// Pickup ring at full or cooldown opacity, and the refill arc.
    pub const PICKUP_RING: &str = "pickup-ring";
    pub const PICKUP_RING_DIM: &str = "pickup-ring-dim";
    pub const PICKUP_REFILL: &str = "pickup-refill";
    /// The blinking cap of a mine.
    pub const MINE_LIGHT: &str = "mine-light";
}

/// Custom effects presentation registers (WGSL beside this file).
pub const FLAG_CLOTH_EFFECT: &str = "flag-cloth";
pub const PICKUP_FACE_EFFECT: &str = "pickup-face";
pub const PICKUP_REFILL_EFFECT: &str = "pickup-refill";
/// Segments of the refill arc; `instance_data.x` says how many draw.
pub const REFILL_SEGMENTS: u32 = 48;

pub const PICKUP_ATLAS: &str = "textures/pickups/atlas.webp";
const PICKUP_ICON_SIZE: f64 = 256.0;
const PICKUP_ATLAS_PADDING: f64 = 16.0;
const PICKUP_ATLAS_STRIDE: f64 = PICKUP_ICON_SIZE + PICKUP_ATLAS_PADDING * 2.0;
const PICKUP_ATLAS_SIZE: f64 = PICKUP_ATLAS_STRIDE * 3.0;
/// Pickup faces glow with their own pictogram at this strength.
const PICKUP_FACE_GLOW: f32 = 0.3;

pub const SPAWN_PROTECTION_COLOR: u32 = 0xffdf86;
const BAR_WIDTH: f64 = 1.75;
const METER_FILL_WIDTH: f64 = 1.34;
const METER_FILL_X: f64 = -0.51;

fn arc(mesh: Mesh) -> Arc<Mesh> {
    Arc::new(mesh)
}

/// World-space HUD paint: unlit, drawn over everything in the transparent pass.
fn hud_material(color: u32) -> Arc<Material> {
    Arc::new(Material {
        depth_test: false,
        depth_write: false,
        transparent: true,
        tone_mapped: false,
        ..Material::basic(color)
    })
}

fn mesh(mesh: Arc<Mesh>, material: Arc<Material>, render_order: i32) -> Node {
    let mut node = Node::mesh(mesh, material);
    if let Some(drawable) = &mut node.drawable {
        drawable.render_order = render_order;
    }
    node
}

fn at(mut node: Node, x: f64, y: f64, z: f64) -> Node {
    node.position = DVec3::new(x, y, z);
    node
}

fn named(mut node: Node, name: &str) -> Node {
    node.name = name.into();
    node
}

fn flat(mut node: Node) -> Node {
    node.set_rotation_euler(-PI / 2.0, 0.0, 0.0);
    node
}

fn polygon(points: &[[f64; 2]]) -> Mesh {
    let points: Vec<DVec2> = points.iter().map(|p| DVec2::new(p[0], p[1])).collect();
    shape_geometry(&[Shape::from_points(&points)], 12)
}

fn translated(mut mesh: Mesh, x: f64, y: f64, z: f64) -> Mesh {
    mesh.translate(x, y, z);
    mesh
}

/// Pickup armor badge or spawn timer: a rimmed badge and a fill bar.
fn protection_meter(name: &str, fill_name: &str, color: u32, segmented: bool) -> Node {
    let paint = hud_material(color);
    let dark = hud_material(0x07141f);
    let badge_shape = arc(polygon(&[
        [-0.13, 0.12],
        [0.13, 0.12],
        [0.11, -0.035],
        [0.0, -0.14],
        [-0.11, -0.035],
    ]));
    let mut group = Node::group(name);
    let mut rim = at(mesh(badge_shape.clone(), dark.clone(), 13), -0.8, 0.0, 0.0);
    rim.scale = DVec3::splat(1.3);
    group.children.push(rim);
    group
        .children
        .push(at(mesh(badge_shape, paint.clone(), 14), -0.8, 0.0, 0.0));
    group.children.push(at(
        mesh(arc(plane_geometry(1.42, 0.18)), dark.clone(), 13),
        0.16,
        0.0,
        0.0,
    ));
    let fill = arc(translated(
        plane_geometry(METER_FILL_WIDTH, 0.1),
        METER_FILL_WIDTH / 2.0,
        0.0,
        0.0,
    ));
    group.children.push(named(
        at(mesh(fill, paint, 14), METER_FILL_X, 0.0, 0.0),
        fill_name,
    ));
    // Three charge sections distinguish pickup armor from the continuous spawn timer.
    if segmented {
        let divider = arc(plane_geometry(0.035, 0.12));
        for fraction in [1.0 / 3.0, 2.0 / 3.0] {
            group.children.push(at(
                mesh(divider.clone(), dark.clone(), 15),
                METER_FILL_X + METER_FILL_WIDTH * fraction,
                0.0,
                0.0,
            ));
        }
    }
    group.visible = false;
    group
}

/// `createTankBar(team)`: the hull bar with its three health colors, the shield
/// and spawn meters, and one to three rank chevrons. Billboarded by the instance.
pub fn tank_bar(team: Team) -> Node {
    let mut bar = Node::group("tank-bar");
    bar.children.push(mesh(
        arc(plane_geometry(1.87, 0.28)),
        hud_material(0x9eb8ab),
        10,
    ));
    bar.children.push(at(
        mesh(arc(plane_geometry(1.81, 0.22)), hud_material(0x010504), 11),
        0.0,
        0.0,
        0.005,
    ));
    let fill = arc(translated(
        plane_geometry(BAR_WIDTH, 0.16),
        BAR_WIDTH / 2.0,
        0.0,
        0.0,
    ));
    let colors = [TEAM_COLORS[team.index()], 0xffe27a, 0xff7c73];
    for (name, color) in joint::BAR_FILLS.iter().zip(colors) {
        let mut node = named(
            at(
                mesh(fill.clone(), hud_material(color), 12),
                -BAR_WIDTH / 2.0,
                0.0,
                0.01,
            ),
            name,
        );
        node.visible = false;
        bar.children.push(node);
    }
    bar.children.push(protection_meter(
        joint::BAR_SHIELD,
        joint::BAR_SHIELD_FILL,
        pickup(PickupKind::Shield).color,
        true,
    ));
    bar.children.push(protection_meter(
        joint::BAR_SPAWN,
        joint::BAR_SPAWN_FILL,
        SPAWN_PROTECTION_COLOR,
        false,
    ));
    let chevron = arc(polygon(&[
        [-0.16, -0.015],
        [0.0, 0.075],
        [0.16, -0.015],
        [0.16, -0.075],
        [0.0, 0.015],
        [-0.16, -0.075],
    ]));
    let gold = hud_material(0xffd477);
    for (i, name) in joint::BAR_RANKS.iter().enumerate() {
        let mut node = named(
            at(
                mesh(chevron.clone(), gold.clone(), 13),
                -1.18,
                0.15 - i as f64 * 0.15,
                0.015,
            ),
            name,
        );
        node.visible = false;
        bar.children.push(node);
    }
    bar
}

/// The fraction of a protection meter; the shield's capacity is fixed.
pub fn shield_fraction(points: f64) -> f64 {
    (points / SHIELD_CAPACITY).clamp(0.0, 1.0)
}

fn reticle_material(color: u32, opacity: f32) -> Arc<Material> {
    Arc::new(Material {
        side: Side::Double,
        depth_test: false,
        depth_write: false,
        transparent: true,
        tone_mapped: false,
        opacity,
        ..Material::basic(color)
    })
}

/// Ring, ticks and centre in one paint (`createReticle`); the rings lie flat.
fn reticle_layer(outline: bool, ink: Arc<Material>, center: Option<Arc<Material>>) -> Vec<Node> {
    let order = if outline { 50 } else { 51 };
    let (inner, outer) = if outline { (0.4, 0.64) } else { (0.46, 0.57) };
    let mut nodes = vec![flat(mesh(
        arc(ring_geometry(inner, outer, 40)),
        ink.clone(),
        order,
    ))];
    let (length, width) = if outline { (0.43, 0.18) } else { (0.31, 0.075) };
    let across = arc(plane_geometry(length, width));
    let along = arc(plane_geometry(width, length));
    for (x, z) in [(-0.83, 0.0), (0.83, 0.0), (0.0, -0.83), (0.0, 0.83)] {
        let geometry = if x == 0.0 { along.clone() } else { across.clone() };
        nodes.push(at(flat(mesh(geometry, ink.clone(), order)), x, 0.0, z));
    }
    if let Some(center) = center {
        nodes.push(flat(mesh(arc(circle_geometry(0.075, 16)), center, 52)));
    }
    nodes
}

/// `createReticle()`: a two-tone reticle, legible over bright ground, paint and
/// cover. The ink has three looks (ready, reloading, hit confirmed), toggled by
/// joint visibility.
pub fn reticle() -> Node {
    let mut root = Node::group("reticle");
    root.children
        .extend(reticle_layer(true, reticle_material(0x12263c, 1.0), None));
    for (name, ink, center, opacity) in [
        (joint::RETICLE_READY, 0xfff9da, 0xffdf38, 1.0),
        (joint::RETICLE_RELOADING, 0xfff9da, 0xffdf38, 0.3),
        (joint::RETICLE_CONFIRMED, 0xffffff, 0xffffff, 1.0),
    ] {
        let mut group = Node::group(name);
        group.children = reticle_layer(
            false,
            reticle_material(ink, opacity),
            Some(reticle_material(center, opacity)),
        );
        group.visible = name == joint::RETICLE_READY;
        root.children.push(group);
    }
    root
}

/// A thin dark rim reads on sand; the yellow ring identifies the player on either team.
pub fn player_ring() -> Node {
    let mut root = Node::group("player-ring");
    for (inner, outer, color, y) in [(1.48, 1.9, 0x172f4a, 0.1), (1.56, 1.8, 0xffe522, 0.12)] {
        let material = Arc::new(Material {
            side: Side::Double,
            tone_mapped: false,
            ..Material::basic(color)
        });
        root.children.push(at(
            flat(mesh(arc(ring_geometry(inner, outer, 48)), material, 0)),
            0.0,
            y,
            0.0,
        ));
    }
    root
}

/// The spawn cue: a yellow ring pulsing outward; instance opacity fades it.
pub fn spawn_pulse() -> Node {
    let material = Arc::new(Material {
        transparent: true,
        depth_write: false,
        side: Side::Double,
        tone_mapped: false,
        ..Material::basic(0xffe522)
    });
    let mut root = Node::group("spawn-pulse");
    root.children
        .push(flat(mesh(arc(ring_geometry(1.8, 1.96, 48)), material, 0)));
    root
}

/// `pickupAtlasUV`: map a box face's 0..1 UV into the kind's atlas tile.
pub fn pickup_atlas_uv(kind: PickupKind, u: f64, v: f64) -> [f64; 2] {
    let (column, row) = match kind {
        PickupKind::Spread => (0.0, 0.0),
        PickupKind::Rocket => (1.0, 0.0),
        PickupKind::Ricochet => (2.0, 0.0),
        PickupKind::Piercing => (0.0, 1.0),
        PickupKind::Rapid => (1.0, 1.0),
        PickupKind::Shield => (2.0, 1.0),
        PickupKind::Speed => (0.0, 2.0),
        PickupKind::Repair => (1.0, 2.0),
        PickupKind::Laser => (2.0, 2.0),
    };
    [
        (column * PICKUP_ATLAS_STRIDE + PICKUP_ATLAS_PADDING + u * PICKUP_ICON_SIZE)
            / PICKUP_ATLAS_SIZE,
        1.0 - (row * PICKUP_ATLAS_STRIDE + PICKUP_ATLAS_PADDING + (1.0 - v) * PICKUP_ICON_SIZE)
            / PICKUP_ATLAS_SIZE,
    ]
}

/// Pickup materials are always transparent so first person can fade them by
/// instance opacity alone, without switching pipelines mid-round.
fn pickup_face_material() -> Arc<Material> {
    let map = TextureRef {
        anisotropy: 4,
        ..TextureRef::file(PICKUP_ATLAS)
    };
    Arc::new(Material {
        map: Some(map),
        roughness: 0.55,
        metalness: 0.15,
        tone_mapped: false,
        transparent: true,
        effect: Effect::Custom {
            name: PICKUP_FACE_EFFECT,
            params: vec![PICKUP_FACE_GLOW],
        },
        ..Material::default()
    })
}

/// The hovering cube (power-ups) or crate (special ammo) with the kind's
/// pictogram on every face (`pickupCube`).
pub fn pickup_gem(kind: PickupKind) -> Node {
    let crate_kind = kind.special_ammo().is_some();
    let mut face = if crate_kind {
        box_geometry(1.8, 1.05, 1.2)
    } else {
        box_geometry(1.25, 1.25, 1.25)
    };
    for uv in &mut face.uvs {
        let [u, v] = pickup_atlas_uv(kind, f64::from(uv[0]), f64::from(uv[1]));
        *uv = [u as f32, v as f32];
    }
    let mut gem = Node::group("pickup-gem");
    let mut body = Node::mesh(arc(face), pickup_face_material());
    if let Some(drawable) = &mut body.drawable {
        drawable.cast_shadow = true;
    }
    gem.children.push(body);
    if crate_kind {
        // A raised rim leaves the top-face symbol visible from the overhead camera.
        let parts = [
            translated(box_geometry(1.94, 0.14, 0.09), 0.0, 0.49, -0.615),
            translated(box_geometry(1.94, 0.14, 0.09), 0.0, 0.49, 0.615),
            translated(box_geometry(0.09, 0.14, 1.14), -0.925, 0.49, 0.0),
            translated(box_geometry(0.09, 0.14, 1.14), 0.925, 0.49, 0.0),
            translated(box_geometry(1.9, 0.1, 1.28), 0.0, -0.5, 0.0),
            translated(box_geometry(0.2, 0.28, 0.08), 0.0, 0.38, 0.64),
        ];
        let merged = sloppy_core::geometry::merge_geometries(&parts.iter().collect::<Vec<_>>())
            .expect("crate parts share attributes");
        let hardware = Arc::new(Material {
            transparent: true,
            ..Material::standard(0x273544, 0.5, 0.55)
        });
        let mut node = Node::mesh(arc(merged), hardware);
        if let Some(drawable) = &mut node.drawable {
            drawable.cast_shadow = true;
        }
        gem.children.push(node);
    }
    gem
}

/// The pad under a pickup: painted base, kind-colored ring (bright or dimmed
/// while recharging) and the refill arc that grows back.
pub fn pickup_base(kind: PickupKind) -> Node {
    let color = pickup(kind).color;
    let mut root = Node::group("pickup");
    put(&mut root, cylinder_part(1.05, 0.12, 0x25435f, 24), 0.0, 0.08, 0.0);
    let torus = arc(torus_geometry(0.94, 0.045, 5, 24));
    for (name, opacity) in [(joint::PICKUP_RING, 1.0), (joint::PICKUP_RING_DIM, 0.2)] {
        let material = Arc::new(Material {
            transparent: true,
            opacity,
            ..(*paint(color)).clone()
        });
        let mut ring = named(at(mesh(torus.clone(), material, 0), 0.0, 0.2, 0.0), name);
        ring.set_rotation_euler(PI / 2.0, 0.0, 0.0);
        ring.visible = name == joint::PICKUP_RING;
        root.children.push(ring);
    }
    let refill = RingGeometry {
        inner_radius: 0.89,
        outer_radius: 1.02,
        theta_segments: REFILL_SEGMENTS,
        phi_segments: 1,
        theta_start: PI / 2.0,
        theta_length: PI * 2.0,
    }
    .build();
    let material = Arc::new(Material {
        transparent: true,
        opacity: 0.9,
        side: Side::Double,
        depth_write: false,
        tone_mapped: false,
        effect: Effect::Custom {
            name: PICKUP_REFILL_EFFECT,
            params: vec![REFILL_SEGMENTS as f32, 1.02],
        },
        ..Material::basic(color)
    });
    let mut node = named(
        at(flat(mesh(arc(refill), material, 0)), 0.0, 0.23, 0.0),
        joint::PICKUP_REFILL,
    );
    node.visible = false;
    root.children.push(node);
    root
}

fn additive(side: Side) -> Arc<Material> {
    Arc::new(Material {
        transparent: true,
        depth_write: false,
        side,
        blending: Blending::Additive,
        ..Material::basic(0xffffff)
    })
}

/// The collection ring (colored by instance tint); it spreads and fades.
pub fn pickup_effect_ring() -> Node {
    let mut root = Node::group("pickup-effect-ring");
    root.children.push(at(
        flat(mesh(
            arc(ring_geometry(0.88, 1.0, 48)),
            additive(Side::Double),
            0,
        )),
        0.0,
        0.08,
        0.0,
    ));
    root
}

/// The glow around the tank that collected a pickup.
pub fn pickup_effect_glow() -> Node {
    let mut root = Node::group("pickup-effect-glow");
    root.children.push(mesh(
        arc(sphere_geometry(1.0, 16, 10)),
        additive(Side::Back),
        0,
    ));
    root
}

/// A mine painted like the pickups' bases, with a blinking team cap.
pub fn mine(team: Team) -> Node {
    let mut root = Node::group("mine");
    put(&mut root, cylinder_part(MINE_RADIUS, 0.17, 0x384f47, 12), 0.0, 0.12, 0.0);
    put(
        &mut root,
        named(
            cylinder_part(0.17, 0.07, TEAM_COLORS[team.index()], 12),
            joint::MINE_LIGHT,
        ),
        0.0,
        0.24,
        0.0,
    );
    root
}

/// `barrelScrapGeometry`: closed, low-poly torn steel, unit bounds so instanced
/// dimensions align with the simple colliders.
pub fn barrel_scrap(lid: bool) -> Mesh {
    let mut geometry = if lid {
        CylinderGeometry {
            radius_top: 0.5,
            radius_bottom: 0.5,
            height: 0.12,
            radial_segments: 10,
            height_segments: 1,
            ..CylinderGeometry::default()
        }
        .build()
    } else {
        sloppy_core::geometry::BoxGeometry {
            width: 1.0,
            height: 1.0,
            depth: 0.12,
            width_segments: 4,
            height_segments: 3,
            depth_segments: 1,
        }
        .build()
    };
    for position in &mut geometry.positions {
        let [x, y, z] = position.map(f64::from);
        let moved = if lid {
            let angle = z.atan2(x);
            let radius = 0.83 + 0.17 * (angle * 5.0).cos();
            [x * radius, y + 0.45 * x.abs() - 0.18 * z, z * radius]
        } else {
            [
                x * (0.83 + 0.17 * (y * 19.0).cos()),
                y + 0.07 * (x * 23.0).sin(),
                z + 0.7 * x * x + 0.12 * (y * 8.0 + x * 5.0).sin(),
            ]
        };
        *position = moved.map(|v| v as f32);
    }
    let bounds = sloppy_core::geometry::Aabb::from_points(
        geometry.positions.iter().map(|p| sloppy_core::geometry::widen(*p)),
    );
    let size = bounds.size();
    geometry.center();
    geometry.scale(1.0 / size.x, 1.0 / size.y, 1.0 / size.z);
    geometry.compute_vertex_normals();
    geometry
}

/// The instanced debris pieces presentation owns; wood, planks and logs use
/// scenery surfaces and come from the model catalog. `None` for those.
pub fn debris_piece(shape: FragmentShape) -> Option<Node> {
    let white = paint(0xffffff);
    let geometry = match shape {
        FragmentShape::Armor => arc(rounded_box_geometry(1.25, 0.16, 0.85, 1, DEFAULT_BOX_RADIUS)),
        FragmentShape::Wheel => arc(cylinder_geometry(0.48, 0.48, 0.28, 10)),
        FragmentShape::Track => arc(rounded_box_geometry(0.5, 0.2, 1.5, 1, DEFAULT_BOX_RADIUS)),
        FragmentShape::Shard => arc(tetrahedron_geometry(0.75, 0)),
        FragmentShape::DrumShell => arc(barrel_scrap(false)),
        FragmentShape::DrumLid => arc(barrel_scrap(true)),
        FragmentShape::Wood | FragmentShape::Panel | FragmentShape::Beam | FragmentShape::Log => {
            return None;
        }
    };
    let mut node = Node::mesh(geometry, white);
    if let Some(drawable) = &mut node.drawable {
        drawable.cast_shadow = true;
        drawable.receive_shadow = true;
    }
    let mut root = Node::group("debris");
    root.children.push(node);
    Some(root)
}

/// `createSpawnPads(scale)`: low octagonal deployment plinths with segmented
/// team lights and inward chevrons, for yards without themed scenery.
pub fn spawn_pads(scale: f64) -> Node {
    let mut details = Node::group("spawn-pads");
    let mut rim = RingGeometry {
        inner_radius: 2.05,
        outer_radius: 2.3,
        theta_segments: 12,
        phi_segments: 1,
        theta_start: 0.06,
        theta_length: PI / 4.0 - 0.12,
    }
    .build();
    rim.rotate_x(-PI / 2.0);
    let rim = arc(rim);
    let mut arrow = polygon(&[
        [-0.28, -0.55],
        [0.28, 0.0],
        [-0.28, 0.55],
        [-0.48, 0.37],
        [-0.1, 0.0],
        [-0.48, -0.37],
    ]);
    arrow.rotate_x(-PI / 2.0);
    let arrow = arc(arrow);
    for team in [Team::Blue, Team::Red] {
        let side = if team == Team::Blue { -1.0 } else { 1.0 };
        let color = TEAM_COLORS[team.index()];
        for position in spawn_positions(team, scale) {
            let (x, z) = (position.x, position.z);
            put(&mut details, cylinder_part(2.75, 0.1, 0x283c4e, 8), x, 0.08, z);
            put(&mut details, cylinder_part(2.52, 0.045, 0x718898, 8), x, 0.135, z);
            put(&mut details, cylinder_part(2.37, 0.035, 0x223d51, 32), x, 0.17, z);
            put(&mut details, cylinder_part(1.98, 0.025, 0x455e70, 8), x, 0.193, z);
            for i in 0..8 {
                let angle = f64::from(i) * PI / 4.0;
                let mut segment = Node::mesh(rim.clone(), paint(color));
                if let Some(drawable) = &mut segment.drawable {
                    drawable.cast_shadow = false;
                }
                segment.set_rotation_euler(0.0, angle, 0.0);
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
            let mut badge = box_part(0.7, 0.025, 0.7, color, 0.035);
            badge.set_rotation_euler(0.0, PI / 4.0, 0.0);
            put(&mut details, badge, x, 0.22, z);
            for offset in [3.1, 3.8] {
                let mut chevron = Node::mesh(arrow.clone(), paint(color));
                chevron.set_rotation_euler(0.0, if team == Team::Blue { 0.0 } else { PI }, 0.0);
                put(&mut details, chevron, x - side * offset, 0.09, z);
            }
        }
    }
    details
}

/// The ground albedo with Three's settings (`groundMaterial`): mirrored repeat,
/// mipmaps, 4× anisotropy; one tile per eight metres.
fn ground_texture(kind: GroundKind) -> TextureRef {
    let path = match kind {
        GroundKind::DryGrass => "textures/ground/dry-grass.webp",
        GroundKind::PackedDirt => "textures/ground/packed-dirt.webp",
    };
    TextureRef {
        wrap: Wrap::Mirror,
        anisotropy: 4,
        ..TextureRef::file(path)
    }
}

/// `createArenaFloor(kind, extent)`: a textured square floor; dry grass adds a
/// patchy vertex tint.
pub fn arena_floor(kind: GroundKind, extent: f64) -> Node {
    let grass = kind == GroundKind::DryGrass;
    let segments = if grass {
        ((extent / 2.5).round() as u32).max(1)
    } else {
        1
    };
    let mut geometry = plane_geometry_segments(extent, extent, segments, segments);
    geometry.rotate_x(-PI / 2.0);
    // groundUVs: one tile per eight world metres.
    for (uv, position) in geometry.uvs.iter_mut().zip(&geometry.positions) {
        *uv = [position[0] / 8.0, position[2] / 8.0];
    }
    if grass {
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
    let material = Arc::new(Material {
        color: Color(if grass { 0xaee6a6 } else { 0xe5dbcc }),
        roughness: 1.0,
        map: Some(ground_texture(kind)),
        vertex_colors: grass,
        ..Material::default()
    });
    let mut floor = Node::mesh(arc(geometry), material);
    if let Some(drawable) = &mut floor.drawable {
        drawable.receive_shadow = true;
    }
    let mut root = Node::group("arena-floor");
    root.children.push(floor);
    root
}

/// Flag poles beside each team's spawn column, with cloth that the flag effect
/// ripples (`Flags`). One model per team; each instance is one flag, placed at
/// its pole's foot. Instance data carries (gust, wind x, wind z, phase).
pub fn flag(team: Team) -> Node {
    let mut root = Node::group("flag");
    put(&mut root, cylinder_part(0.055, 4.8, 0x59656a, 8), 0.0, 2.4, 0.0);
    let material = Arc::new(Material {
        color: Color(TEAM_COLORS[team.index()]),
        roughness: 1.0,
        side: Side::Double,
        effect: Effect::Custom {
            name: FLAG_CLOTH_EFFECT,
            params: Vec::new(),
        },
        ..Material::default()
    });
    let mut cloth = Node::mesh(arc(plane_geometry_segments(1.4, 0.9, 16, 6)), material);
    if let Some(drawable) = &mut cloth.drawable {
        drawable.cast_shadow = true;
        drawable.receive_shadow = true;
        // Wind moves vertices in the shader; the bounds grow with the gust.
        drawable.frustum_culled = false;
    }
    put(&mut root, cloth, 0.0, 4.6, 0.0);
    root
}

/// Where the flags stand: beside each team's spawn column, with a phase per flag.
pub fn flag_placements() -> Vec<(Team, DVec3, f32)> {
    let mut placements = Vec::new();
    for team in [Team::Blue, Team::Red] {
        let x = if team == Team::Blue { -62.0 } else { 62.0 };
        for position in spawn_positions(team, 1.0) {
            let phase = position.z * 0.12 + x * 0.04;
            placements.push((team, DVec3::new(x, 0.0, position.z), phase as f32));
        }
    }
    placements
}

/// The flat plane for a round of plain-yard harbor water (`HarborWater`).
pub fn water_plane(size: f64) -> Mesh {
    let mut plane = plane_geometry(size, size);
    plane.rotate_x(-PI / 2.0);
    plane
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(node: &Node, out: &mut Vec<String>) {
        if !node.name.is_empty() {
            out.push(node.name.clone());
        }
        for child in &node.children {
            names(child, out);
        }
    }

    #[test]
    fn hud_models_name_their_joints() {
        let mut found = Vec::new();
        names(&tank_bar(Team::Red), &mut found);
        for name in joint::BAR_FILLS.iter().chain(&joint::BAR_RANKS).chain(&[
            joint::BAR_SHIELD,
            joint::BAR_SHIELD_FILL,
            joint::BAR_SPAWN,
            joint::BAR_SPAWN_FILL,
        ]) {
            assert!(found.iter().any(|n| n == name), "{name}");
        }
        let mut found = Vec::new();
        names(&reticle(), &mut found);
        assert!(found.iter().any(|n| n == joint::RETICLE_CONFIRMED));
        let mut found = Vec::new();
        names(&pickup_base(PickupKind::Laser), &mut found);
        assert!(found.iter().any(|n| n == joint::PICKUP_REFILL));
    }

    #[test]
    fn scrap_has_unit_bounds_and_pads_cover_both_teams() {
        for lid in [false, true] {
            let mesh = barrel_scrap(lid);
            let bounds = sloppy_core::geometry::Aabb::from_points(
                mesh.positions.iter().map(|p| sloppy_core::geometry::widen(*p)),
            );
            let size = bounds.size();
            assert!((size.x - 1.0).abs() < 1e-5 && (size.y - 1.0).abs() < 1e-5);
        }
        assert_eq!(spawn_pads(1.0).children.len(), 10 * (4 + 16 + 10 + 1 + 2));
        assert_eq!(flag_placements().len(), 10);
        let uv = pickup_atlas_uv(PickupKind::Spread, 0.0, 1.0);
        assert!((uv[0] - 16.0 / 864.0).abs() < 1e-12 && (uv[1] - (1.0 - 16.0 / 864.0)).abs() < 1e-12);
    }
}

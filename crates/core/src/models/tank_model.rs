//! Port of `tank-model.ts`: the three tracked chassis (Booker-like scout, Abrams-like
//! balanced tank, Type 99-like heavy) and the dispatch to the Humvee.
//!
//! Tree: `root` (named by kind, scaled by the vehicle scale) holds `hull` and
//! `turret`. The hull holds the tracks group `track-group` (top treads that scroll)
//! and every fixed hull part; the turret holds `barrel`, whose bore mesh is
//! `muzzle`. See [`super::part`] for the names.

use std::f64::consts::PI;
use std::sync::Arc;

use glam::{DVec2, DVec3};

use super::humvee_model::humvee_model;
use super::model_primitives::{
    Cache, TEAM_COLORS, box_part, cylinder_part, material, paint, put, rotated, shadow_receiver,
    shadowed,
};
use super::tank_surfaces::{Finish, apply_tank_surface};
use super::{Team, VehicleKind, part};
use crate::geometry::math::scale_hex_color;
use crate::geometry::{
    CylinderGeometry, ExtrudeOptions, Mesh, Path, Shape, extrude_geometry, plane_geometry,
    ring_geometry, triangulate_shape, widen,
};
use crate::scene::Node;

/// Wreck paint and the shared dark/steel trim colors.
pub(crate) const WRECK_PAINT: u32 = 0x3c4650;
pub(crate) const DARK: u32 = 0x13232c;
pub(crate) const STEEL: u32 = 0x637581;
pub(crate) const WRECK_STEEL: u32 = 0x37424c;
/// Shade paint is team paint darkened in linear space by this factor.
pub(crate) const SHADE_FACTOR: f64 = 0.62;
const GLASS_TINT: u32 = 0x8adeec;
const MARKING_WHITE: u32 = 0xdce7ee;
const HEADLIGHT: u32 = 0xd9e6df;

/// The darker variant of a team paint (`new THREE.Color(c).multiplyScalar(0.62)`).
pub(crate) fn shade_of(color: u32) -> u32 {
    scale_hex_color(color, SHADE_FACTOR)
}

static ARMOR: Cache<(u64, u64, u64, u64), Mesh> = Cache::new();
static MODELS: Cache<(VehicleKind, Team, bool, bool), Node> = Cache::new();

/// Chamfered rectangular plate outline in unit x/z, shared by every armor block.
const ARMOR_OUTLINE: [[f64; 2]; 8] = [
    [-0.38, -0.5],
    [0.38, -0.5],
    [0.5, -0.36],
    [0.5, 0.32],
    [0.32, 0.5],
    [-0.32, 0.5],
    [-0.5, 0.32],
    [-0.5, -0.36],
];
const TURRET_RING_HOLE_POINTS: u32 = 24;

/// Unit armor geometry: a tapered prism with a recessed roof and sloping front
/// glacis, optionally with a round turret-ring opening cut through the roof.
/// `opening` is in metres and needs the final `width`/`depth` to stay round.
fn armor_geometry(taper: f64, width: f64, depth: f64, opening: f64) -> Mesh {
    let mut vertices: Vec<f64> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    for top in [false, true] {
        for [x, z] in ARMOR_OUTLINE {
            if top {
                vertices.extend_from_slice(&[x * taper, 0.5, z * taper - 0.04]);
            } else {
                vertices.extend_from_slice(&[x, -0.5, z]);
            }
        }
    }
    for i in 0..8 {
        let next = (i + 1) % 8;
        indices.extend_from_slice(&[i, i + 8, next, next, i + 8, next + 8]);
    }
    for i in 1..7 {
        indices.extend_from_slice(&[0, i, i + 1]);
        if opening == 0.0 {
            indices.extend_from_slice(&[8, i + 9, i + 8]);
        }
    }
    if opening != 0.0 {
        let mut contour: Vec<DVec2> = ARMOR_OUTLINE
            .iter()
            .map(|[x, z]| DVec2::new(x * taper, z * taper - 0.04))
            .collect();
        let hole: Vec<DVec2> = (0..TURRET_RING_HOLE_POINTS)
            .map(|i| {
                let angle = (f64::from(i) / f64::from(TURRET_RING_HOLE_POINTS)) * PI * 2.0;
                DVec2::new(
                    (angle.cos() * opening) / width,
                    (angle.sin() * opening - 0.12) / depth,
                )
            })
            .collect();
        let offset = (vertices.len() / 3) as u32;
        for p in contour.iter().chain(&hole) {
            vertices.extend_from_slice(&[p.x, 0.5, p.y]);
        }
        for [a, b, c] in triangulate_shape(&mut contour, &mut [hole]) {
            indices.extend_from_slice(&[offset + a as u32, offset + c as u32, offset + b as u32]);
        }
    }
    let indexed = Mesh::from_f64(&vertices, &[], &[], Some(indices));
    let mut mesh = indexed.to_non_indexed();
    mesh.compute_vertex_normals();
    // Planar UVs per face keep armor texture visible on tops, cheeks and sides.
    mesh.uvs = mesh
        .positions
        .iter()
        .zip(&mesh.normals)
        .map(|(p, n)| {
            let (p, n) = (widen(*p), widen(*n).abs());
            let top = n.y >= n.x && n.y >= n.z;
            let u = if top {
                p.x
            } else if n.x > n.z {
                p.z
            } else {
                p.x
            };
            let v = if top { p.z } else { p.y };
            [(u + 0.5) as f32, (v + 0.5) as f32]
        })
        .collect();
    mesh
}

/// `armor(w, h, d, color, taper, opening)`: an armor block scaled to size, with
/// its unit geometry shared per taper (and per size when it has an opening).
fn armor(width: f64, height: f64, depth: f64, color: u32, taper: f64, opening: f64) -> Node {
    let key = if opening != 0.0 {
        [taper, width, depth, opening].map(f64::to_bits)
    } else {
        [taper, 0.0, 0.0, 0.0].map(f64::to_bits)
    };
    let mesh = ARMOR.get_or_insert(key.into(), || armor_geometry(taper, width, depth, opening));
    let mut node = shadowed(mesh, material(color, 0.18, 0.58));
    node.scale = DVec3::new(width, height, depth);
    node
}

static BELT: std::sync::OnceLock<Arc<Mesh>> = std::sync::OnceLock::new();
static TRACK_PAD: std::sync::OnceLock<Arc<Mesh>> = std::sync::OnceLock::new();

/// The stadium-shaped track belt, extruded across the track width along x.
fn belt_geometry() -> Arc<Mesh> {
    BELT.get_or_init(|| {
        let mut outline = Path::new();
        outline
            .move_to(-0.9, -0.33)
            .line_to(0.9, -0.33)
            .absarc(0.9, 0.0, 0.33, -PI / 2.0, PI / 2.0, false)
            .line_to(-0.9, 0.33)
            .absarc(-0.9, 0.0, 0.33, PI / 2.0, PI * 1.5, false);
        let options = ExtrudeOptions {
            depth: 0.54,
            bevel_enabled: false,
            curve_segments: 6,
            ..ExtrudeOptions::default()
        };
        let mut mesh = extrude_geometry(&[Shape::new(outline)], &options);
        mesh.translate(0.0, 0.0, -0.27).rotate_y(PI / 2.0);
        Arc::new(mesh)
    })
    .clone()
}

/// Rubber pads only need an exposed face; their backing is the solid track shoe.
fn track_pad_geometry() -> Arc<Mesh> {
    TRACK_PAD
        .get_or_init(|| {
            let mut mesh = plane_geometry(1.0, 1.0);
            mesh.rotate_x(PI / 2.0);
            Arc::new(mesh)
        })
        .clone()
}

/// The turret-ring well's wall: an open cylinder turned inside out. Reversed
/// triangles and normals face the wall inward, so it draws with ordinary
/// front-side paint instead of a back-side shader of its own.
fn inward_wall_geometry(radius: f64, height: f64) -> Mesh {
    let mut mesh = CylinderGeometry {
        radius_top: radius,
        radius_bottom: radius,
        height,
        radial_segments: 24,
        height_segments: 1,
        open_ended: true,
        ..CylinderGeometry::default()
    }
    .build();
    if let Some(indices) = &mut mesh.indices {
        for triangle in indices.as_chunks_mut::<3>().0 {
            triangle.swap(1, 2);
        }
    }
    for normal in &mut mesh.normals {
        *normal = normal.map(|n| -n);
    }
    mesh
}

/// The vehicle model for a kind and team: shared, built once. Clone the node to
/// move its parts independently.
pub fn tank_model(kind: VehicleKind, team: Team) -> Arc<Node> {
    tank_model_variant(kind, team, false, false)
}

/// `tankModel(kind, team, wreck, openTurretRing)`: `wreck` swaps the paint for
/// burnt colors; `open_turret_ring` cuts the ring well into the hull roof (used by
/// hull-only wrecks whose turret flew off).
pub fn tank_model_variant(
    kind: VehicleKind,
    team: Team,
    wreck: bool,
    open_turret_ring: bool,
) -> Arc<Node> {
    MODELS.get_or_insert((kind, team, wreck, open_turret_ring), || {
        if kind == VehicleKind::Humvee {
            humvee_model(team, wreck)
        } else {
            tracked_model(kind, team, wreck, open_turret_ring)
        }
    })
}

/// Chassis measurements shared by the hull and turret builders.
struct Chassis {
    scout: bool,
    heavy: bool,
    color: u32,
    shade: u32,
    steel: u32,
    overall_width: f64,
    width: f64,
    length: f64,
    deck: f64,
}

fn tracked_model(kind: VehicleKind, team: Team, wreck: bool, open_turret_ring: bool) -> Node {
    let scout = kind == VehicleKind::Scout;
    let heavy = kind == VehicleKind::Heavy;
    let color = if wreck {
        WRECK_PAINT
    } else {
        TEAM_COLORS[team.index()]
    };
    // Dimensions include the tracks and skirts, not just the center armor slab.
    // Hull length/overall width: compact scout ~1.94, Abrams/Type 99 ~2.17.
    let overall_width = if scout {
        2.3
    } else if heavy {
        2.5
    } else {
        2.42
    };
    let chassis = Chassis {
        scout,
        heavy,
        color,
        shade: if wreck { DARK } else { shade_of(color) },
        steel: if wreck { WRECK_STEEL } else { STEEL },
        overall_width,
        width: overall_width - 0.42,
        length: if scout {
            4.415
        } else if heavy {
            5.425
        } else {
            5.244
        },
        deck: if scout {
            0.58
        } else if heavy {
            0.67
        } else {
            0.65
        },
    };
    let mut root = Node::group(kind.name());
    root.children.push(hull(&chassis, open_turret_ring));
    root.children.push(turret(&chassis, team));
    root.scale = DVec3::splat(kind.scale());
    let finish = if wreck {
        Finish::Wrecked
    } else {
        Finish::Fresh {
            steel: chassis.steel,
        }
    };
    apply_tank_surface(
        &mut root,
        &[chassis.color, chassis.shade, chassis.steel],
        finish,
    );
    root
}

fn hull(c: &Chassis, open_turret_ring: bool) -> Node {
    let (width, length, deck, shade, steel) = (c.width, c.length, c.deck, c.shade, c.steel);
    let mut hull = Node::group(part::HULL);
    // The lower hull narrows toward the belly, leaving room for the running gear.
    let lower = rotated(armor(width, 0.32, length, shade, 0.8, 0.0), 0.0, 0.0, PI);
    put(&mut hull, lower, 0.0, 0.22, 0.0);
    let opening = if c.scout { 0.5 } else { 0.65 };
    let ring_radius = if c.scout { 0.59 } else { 0.76 };
    let taper = if c.scout { 0.72 } else { 0.86 };
    let ring = if open_turret_ring { ring_radius } else { 0.0 };
    let upper = armor(width, deck - 0.2, length, c.color, taper, ring);
    put(&mut hull, upper, 0.0, deck / 2.0 + 0.16, 0.0);
    if open_turret_ring {
        turret_ring_well(&mut hull, deck, opening, ring_radius);
    }
    // Shallow armored belly and service covers stay above the track contact plane.
    // These are hull meshes so the same details survive on overturned wrecks.
    let belly = rotated(
        armor(width * 0.7, 0.1, length * 0.83, shade, 0.91, 0.0),
        0.0,
        0.0,
        PI,
    );
    put(&mut hull, belly, 0.0, 0.035, 0.0);
    for z in [-length * 0.25, length * 0.18] {
        let cover_width = width * if z < 0.0 { 0.43 } else { 0.3 };
        let cover_length = length * if z < 0.0 { 0.2 } else { 0.15 };
        // Dark seams outline raised, bolted access plates without coplanar faces.
        let seam = box_part(cover_width + 0.045, 0.025, cover_length + 0.045, DARK, 0.0);
        put(&mut hull, seam, 0.0, -0.025, z);
        let plate = box_part(cover_width, 0.035, cover_length, shade, 0.0);
        put(&mut hull, plate, 0.0, -0.045, z);
        for side in [-1.0, 1.0] {
            for end in [-1.0, 1.0] {
                put(
                    &mut hull,
                    box_part(0.04, 0.018, 0.04, steel, 0.0),
                    side * (cover_width / 2.0 - 0.055),
                    -0.068,
                    z + end * (cover_length / 2.0 - 0.055),
                );
            }
        }
    }
    let track_group_index = hull.children.len();
    let mut track_group = Node::group(part::TRACK_GROUP);
    for side in [-1.0, 1.0] {
        running_gear(&mut hull, &mut track_group, c, side);
    }
    hull.children.insert(track_group_index, track_group);
    // Exposed rear engine deck gives the hull a direction even with the turret turned.
    for i in 0..7 {
        put(
            &mut hull,
            box_part(width * 0.53, 0.025, 0.05, DARK, 0.0),
            0.0,
            deck + 0.025,
            -length / 2.0 + 0.12 + f64::from(i) * 0.075,
        );
    }
    let hatch = box_part(0.32, 0.045, 0.3, steel, 0.0);
    put(&mut hull, hatch, 0.0, deck + 0.02, length / 2.0 - 0.45);
    if c.heavy {
        for side in [-1.0, 1.0] {
            for j in 0..3 {
                let tile = rotated(box_part(0.31, 0.1, 0.24, shade, 0.0), -0.18, 0.0, 0.0);
                put(
                    &mut hull,
                    tile,
                    side * (0.24 + f64::from(j) * 0.32),
                    deck - 0.06,
                    length / 2.0 - 0.3,
                );
            }
        }
    }
    hull
}

/// A real cutout with visible inner walls and a dark recessed floor, not a black
/// decal on the closed deck. Kept inside the original hull bounds.
fn turret_ring_well(hull: &mut Node, deck: f64, opening: f64, ring_radius: f64) {
    let roof_y = deck + 0.06;
    let floor_y = 0.395;
    let mut rim_mesh = ring_geometry(opening, ring_radius, 24);
    rim_mesh.rotate_x(-PI / 2.0);
    let rim = shadowed(Arc::new(rim_mesh), material(0x4a5358, 0.55, 0.8));
    put(hull, rim, 0.0, roof_y, -0.12);
    let wall_mesh = inward_wall_geometry(opening, roof_y - floor_y);
    let wall = shadow_receiver(Arc::new(wall_mesh), material(0x3b454a, 0.15, 0.9));
    put(hull, wall, 0.0, (roof_y + floor_y) / 2.0, -0.12);
    let floor = cylinder_part(opening, 0.02, 0x293238, 24);
    put(hull, floor, 0.0, floor_y - 0.01, -0.12);
}

/// Belt, road wheels, treads, shoes, skirts and a headlight on one side.
fn running_gear(hull: &mut Node, track_group: &mut Node, c: &Chassis, side: f64) {
    let (length, deck, shade, steel) = (c.length, c.deck, c.shade, c.steel);
    let track_x = side * (c.overall_width / 2.0 - 0.23);
    let belt_stretch = length / 2.46;
    let mut belt = shadowed(belt_geometry(), paint(DARK));
    belt.scale = DVec3::new(0.7, 1.12, belt_stretch);
    put(hull, belt, track_x, 0.19, 0.0);
    let wheels = if c.scout || c.heavy { 6 } else { 7 };
    for j in 0..wheels {
        let z = -length * 0.37 + (f64::from(j) * length * 0.74) / f64::from(wheels - 1);
        let radius = if c.scout { 0.3 } else { 0.34 };
        let wheel = rotated(cylinder_part(radius, 0.055, shade, 10), 0.0, 0.0, PI / 2.0);
        put(hull, wheel, track_x + side * 0.1925, 0.18, z);
        let hub = rotated(cylinder_part(0.1, 0.04, steel, 8), 0.0, 0.0, PI / 2.0);
        put(hull, hub, track_x + side * 0.205, 0.18, z);
    }
    for j in 0..18 {
        let tread = box_part(0.42, 0.03, 0.075, steel, 0.0);
        put(
            track_group,
            tread,
            track_x,
            0.565,
            -length * 0.44 + f64::from(j) * length * 0.052,
        );
    }
    // Lower shoes and their rubber pads break up the otherwise smooth belt bottom.
    // Keep them on the hull: the top tread group's small scrolling motion must not
    // slide these shoes away from the curved ends of the track.
    for j in 0..14 {
        let z = -length * 0.35 + (f64::from(j) * length * 0.7) / 13.0;
        let shoe = box_part(0.41, 0.035, length * 0.044, steel, 0.0);
        put(hull, shoe, track_x, -0.177, z);
        let mut pad = shadow_receiver(track_pad_geometry(), paint(DARK));
        pad.scale = DVec3::new(0.28, 1.0, length * 0.03);
        put(hull, pad, track_x, -0.197, z);
    }
    for end in [-1.0, 1.0] {
        for j in 1..4 {
            let angle = (f64::from(j) * PI) / 4.0;
            let pitch = (-end * 1.12 * angle.sin()).atan2(belt_stretch * angle.cos());
            let tread = rotated(
                box_part(0.41, 0.035, length * 0.038, steel, 0.0),
                pitch,
                0.0,
                0.0,
            );
            put(
                hull,
                tread,
                track_x,
                0.19 - 0.375 * angle.cos(),
                end * (0.9 + 0.335 * angle.sin()) * belt_stretch,
            );
        }
    }
    put(
        hull,
        box_part(0.46, 0.07, length, c.color, 0.0),
        track_x,
        deck,
        0.0,
    );
    // Booker: short modular skirts. Abrams: long panels. Type 99: heavy blocks.
    let panels = if c.scout {
        4
    } else if c.heavy {
        5
    } else {
        3
    };
    for j in 0..panels {
        let skirt = box_part(
            if c.heavy { 0.12 } else { 0.07 },
            if c.heavy { 0.3 } else { 0.26 },
            length / f64::from(panels) - 0.04,
            if c.heavy && j % 2 == 1 {
                shade
            } else {
                c.color
            },
            0.0,
        );
        put(
            hull,
            skirt,
            track_x + side * if c.heavy { 0.17 } else { 0.19 },
            deck - 0.17,
            -length / 2.0 + ((f64::from(j) + 0.5) * length) / f64::from(panels),
        );
    }
    let headlight = box_part(0.16, 0.11, 0.1, HEADLIGHT, 0.0);
    put(
        hull,
        headlight,
        side * 0.65,
        deck - 0.1,
        length / 2.0 - 0.05,
    );
}

/// The turret with its barrel and team marking.
fn turret(c: &Chassis, team: Team) -> Node {
    let (deck, steel) = (c.deck, c.steel);
    let mut turret = Node::group(part::TURRET);
    let ring = cylinder_part(if c.scout { 0.59 } else { 0.76 }, 0.1, DARK, 16);
    put(&mut turret, ring, 0.0, deck + 0.055, -0.12);
    let roof = if c.scout {
        scout_turret(&mut turret, c)
    } else if !c.heavy {
        balanced_turret(&mut turret, c)
    } else {
        heavy_turret(&mut turret, c)
    };
    for side in [-1.0, 1.0] {
        let hatch = cylinder_part(if c.scout { 0.18 } else { 0.21 }, 0.065, steel, 12);
        put(&mut turret, hatch, side * 0.28, roof + 0.025, -0.24);
        for j in 0..3 {
            let smoke = rotated(cylinder_part(0.055, 0.2, steel, 8), PI / 3.0, 0.0, 0.0);
            put(
                &mut turret,
                smoke,
                side * if c.scout { 0.72 } else { 0.88 },
                roof - 0.32,
                0.06 - f64::from(j) * 0.15,
            );
        }
    }
    // A compact roof gun and antenna distinguish equipment without expensive meshes.
    put(
        &mut turret,
        box_part(0.09, 0.18, 0.1, DARK, 0.0),
        -0.28,
        roof + 0.16,
        -0.24,
    );
    let roof_gun = box_part(0.07, 0.07, if c.scout { 0.42 } else { 0.6 }, steel, 0.0);
    put(&mut turret, roof_gun, -0.28, roof + 0.25, -0.02);
    let antenna = cylinder_part(0.018, if c.scout { 0.5 } else { 0.7 }, DARK, 5);
    put(&mut turret, antenna, 0.53, roof + 0.25, -0.67);
    turret.children.push(barrel(c));
    if team == Team::Blue {
        let badge = rotated(
            box_part(0.18, 0.025, 0.18, MARKING_WHITE, 0.0),
            0.0,
            PI / 4.0,
            0.0,
        );
        put(&mut turret, badge, 0.0, roof + 0.018, 0.12);
    } else {
        for x in [-0.07, 0.07] {
            let stripe = box_part(0.06, 0.025, 0.2, MARKING_WHITE, 0.0);
            put(&mut turret, stripe, x, roof + 0.018, 0.12);
        }
    }
    turret
}

/// M10 Booker-inspired compact welded turret, smooth armor and enclosed bustle.
fn scout_turret(turret: &mut Node, c: &Chassis) -> f64 {
    let (deck, color, shade, steel) = (c.deck, c.color, c.shade, c.steel);
    put(
        turret,
        armor(1.66, 0.43, 2.03, color, 0.8, 0.0),
        0.0,
        deck + 0.28,
        -0.12,
    );
    for side in [-1.0, 1.0] {
        let cheek = rotated(
            armor(0.5, 0.33, 0.72, shade, 0.67, 0.0),
            0.0,
            side * -0.16,
            0.0,
        );
        put(turret, cheek, side * 0.56, deck + 0.27, 0.48);
        put(
            turret,
            box_part(0.18, 0.27, 0.67, color, 0.0),
            side * 0.74,
            deck + 0.23,
            -0.64,
        );
    }
    let roof = deck + 0.5;
    put(
        turret,
        box_part(1.22, 0.3, 0.45, shade, 0.0),
        0.0,
        deck + 0.22,
        -1.12,
    );
    put(
        turret,
        box_part(0.28, 0.19, 0.27, steel, 0.0),
        0.35,
        roof + 0.095,
        0.11,
    );
    put(
        turret,
        box_part(0.2, 0.08, 0.025, GLASS_TINT, 0.0),
        0.35,
        roof + 0.11,
        0.26,
    );
    roof
}

/// Abrams: broad trapezoidal cheeks and a long, boxy bustle behind the ring.
fn balanced_turret(turret: &mut Node, c: &Chassis) -> f64 {
    let (deck, color, shade, steel) = (c.deck, c.color, c.shade, c.steel);
    put(
        turret,
        armor(1.97, 0.46, 2.65, color, 0.83, 0.0),
        0.0,
        deck + 0.31,
        -0.3,
    );
    for side in [-1.0, 1.0] {
        let cheek = rotated(
            armor(0.76, 0.4, 1.08, color, 0.66, 0.0),
            0.0,
            side * -0.2,
            0.0,
        );
        put(turret, cheek, side * 0.58, deck + 0.3, 0.58);
    }
    let roof = deck + 0.56;
    put(
        turret,
        box_part(1.64, 0.36, 0.65, shade, 0.0),
        0.0,
        deck + 0.27,
        -1.58,
    );
    // Open rear stowage basket is a large, recognizable silhouette feature.
    for y in [deck + 0.18, deck + 0.49] {
        put(
            turret,
            box_part(1.92, 0.055, 0.055, steel, 0.0),
            0.0,
            y,
            -1.96,
        );
        for side in [-1.0, 1.0] {
            put(
                turret,
                box_part(0.055, 0.055, 0.69, steel, 0.0),
                side * 0.93,
                y,
                -1.64,
            );
        }
    }
    for x in [-0.93, -0.46, 0.0, 0.46, 0.93] {
        put(
            turret,
            box_part(0.04, 0.31, 0.04, steel, 0.0),
            x,
            deck + 0.335,
            -1.96,
        );
    }
    put(
        turret,
        cylinder_part(0.19, 0.25, shade, 10),
        0.49,
        roof + 0.13,
        0.14,
    );
    put(
        turret,
        box_part(0.19, 0.09, 0.08, GLASS_TINT, 0.0),
        0.49,
        roof + 0.2,
        0.3,
    );
    roof
}

/// Type 99: compact center, sharply pointed twin wedges and tiled armor.
fn heavy_turret(turret: &mut Node, c: &Chassis) -> f64 {
    let (deck, color, shade, steel) = (c.deck, c.color, c.shade, c.steel);
    put(
        turret,
        armor(1.5, 0.47, 2.25, shade, 0.73, 0.0),
        0.0,
        deck + 0.32,
        -0.23,
    );
    let roof = deck + 0.58;
    for side in [-1.0, 1.0] {
        let wedge = rotated(
            armor(0.82, 0.48, 1.65, color, 0.48, 0.0),
            0.0,
            side * -0.36,
            0.0,
        );
        put(turret, wedge, side * 0.64, deck + 0.31, 0.38);
        for j in 0..4 {
            let j = f64::from(j);
            let tile = rotated(
                box_part(0.36, 0.1, 0.22, shade, 0.0),
                -0.28,
                side * -0.36,
                side * 0.12,
            );
            put(
                turret,
                tile,
                side * (0.36 + j * 0.17),
                roof - 0.06 - j * 0.07,
                0.72 - j * 0.28,
            );
        }
        put(
            turret,
            box_part(0.3, 0.38, 0.65, color, 0.0),
            side * 0.73,
            deck + 0.27,
            -1.15,
        );
    }
    put(
        turret,
        box_part(0.34, 0.3, 0.32, steel, 0.0),
        0.4,
        roof + 0.15,
        -0.52,
    );
    put(
        turret,
        box_part(0.19, 0.1, 0.04, GLASS_TINT, 0.0),
        0.4,
        roof + 0.2,
        -0.35,
    );
    roof
}

/// The gun: tube, sleeves, the scout's squared muzzle brake, and the bore disc
/// named `muzzle` whose world position is the launch point.
fn barrel(c: &Chassis) -> Node {
    let (deck, shade, steel) = (c.deck, c.shade, c.steel);
    let mut barrel = Node::group(part::BARREL);
    let gun_y = deck + if c.scout { 0.27 } else { 0.31 };
    let tube_radius = if c.scout {
        0.062
    } else if c.heavy {
        0.074
    } else {
        0.068
    };
    let muzzle_z = if c.scout {
        3.72
    } else if c.heavy {
        4.44
    } else {
        3.84
    };
    let tube = rotated(
        cylinder_part(tube_radius, muzzle_z - 0.6, steel, 12),
        PI / 2.0,
        0.0,
        0.0,
    );
    put(&mut barrel, tube, 0.0, gun_y, (muzzle_z + 0.6) / 2.0);
    for [z, radius, length] in [
        [0.67, tube_radius * 2.1, 0.42],
        [muzzle_z * 0.58, tube_radius * 1.5, 0.36],
    ] {
        let sleeve = rotated(cylinder_part(radius, length, shade, 12), PI / 2.0, 0.0, 0.0);
        put(&mut barrel, sleeve, 0.0, gun_y, z);
    }
    if c.scout {
        // Squared muzzle brake is visually distinct from the two smoothbore guns.
        put(
            &mut barrel,
            box_part(0.22, 0.17, 0.27, steel, 0.0),
            0.0,
            gun_y,
            muzzle_z - 0.135,
        );
        for side in [-1.0, 1.0] {
            for z in [muzzle_z - 0.2, muzzle_z - 0.09] {
                put(
                    &mut barrel,
                    box_part(0.012, 0.1, 0.05, DARK, 0.0),
                    side * 0.111,
                    gun_y,
                    z,
                );
            }
        }
    }
    let mut bore = rotated(
        cylinder_part(tube_radius * 0.76, 0.008, DARK, 12),
        PI / 2.0,
        0.0,
        0.0,
    );
    bore.name = part::MUZZLE.into();
    put(&mut barrel, bore, 0.0, gun_y, muzzle_z + 0.005);
    barrel
}

//! Harbor Havoc's far shore, so the world ends in a coast rather than grey: green
//! headlands with wind turbines, a city across the bay, a neighbouring container
//! terminal, a tank farm and a breakwater with its lighthouse, all well outside
//! the playfield and softened by the fog. Variation comes from a fixed hash of the
//! angle, never the simulation's random stream.
//!
//! The coast is split into quarter sectors, each batched on its own, so a view
//! draws only the sectors in front of it. Nothing here casts or receives shadows.

use std::f64::consts::{FRAC_PI_2, PI, TAU};
use std::sync::Arc;

use crate::geometry::Mesh;
use crate::geometry::math::{hex_to_linear, scale_hex_color};
use crate::scene::{Material, Node};

use super::batching::batch;
use super::model_primitives::{adopt_children, beam, box_part, cylinder_part, put, rotated};

/// The four quarter sectors being filled, indexed by [`sector_of`].
type Sectors = [Node; 4];

/// The quarter sector an angle falls in: [-π, -π/2) north-west, [-π/2, 0)
/// north-east, [0, π/2) south-east, [π/2, π) south-west.
fn sector_of(theta: f64) -> usize {
    (((theta + PI) / FRAC_PI_2) as usize).min(3)
}

/// Place an assembly in the sector of `theta`, its nested groups baked flat so the
/// sector batches into a few draws.
fn place(sectors: &mut Sectors, theta: f64, assembly: Node) {
    fn flatten(parent: &mut Node, group: Node) {
        let mut moved = Node::group("");
        adopt_children(&mut moved, group);
        for child in moved.children {
            if child.drawable.is_none() {
                flatten(parent, child);
            } else {
                parent.children.push(child);
            }
        }
    }
    let target = &mut sectors[sector_of(theta)];
    if assembly.drawable.is_some() {
        target.children.push(assembly);
    } else {
        flatten(target, assembly);
    }
}

/// A part at a world position, in the sector of `theta`.
fn place_at(sectors: &mut Sectors, theta: f64, part: Node, x: f64, y: f64, z: f64) {
    let mut part = part;
    part.position = glam::DVec3::new(x, y, z);
    place(sectors, theta, part);
}

/// Ring segments around the bay.
const SEGMENTS: usize = 240;
/// Mean radius of the shoreline from the arena centre, metres.
const SHORE_RADIUS: f64 = 232.0;
/// Distance haze baked into the far shore's colors, so it reads as miles away
/// whatever the fog: the linear color it fades toward and how far.
const HAZE: [f32; 3] = [0.36, 0.47, 0.55];
const HAZE_AMOUNT: f32 = 0.48;
/// How far the idle terminal cranes have raised their booms from level, radians.
const BOOM_RAISE: f64 = 1.3;
/// The open sea to the north (−z), as an angle range with no land.
const OPEN_SEA: (f64, f64) = (-2.25, -0.95);
/// Water level the shore rises from (`harbor_water::HARBOR_WATER_HEIGHT`).
const SEA_LEVEL: f64 = -2.2;

/// A smooth pseudo-random value in 0..1 for an angle, periodic over the circle.
fn wave(theta: f64, seed: f64) -> f64 {
    let s = (theta * 3.0 + seed).sin() * 0.45
        + (theta * 7.0 + seed * 2.3).sin() * 0.3
        + (theta * 13.0 + seed * 4.1).sin() * 0.25;
    s * 0.5 + 0.5
}

/// A fixed hash of an index in 0..1.
fn hash(i: u32, seed: u32) -> f64 {
    let mut x = i.wrapping_mul(0x9e37_79b9) ^ seed.wrapping_mul(0x85eb_ca6b);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2c1b_3c6d);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297a_2d39);
    x ^= x >> 15;
    f64::from(x) / f64::from(u32::MAX)
}

fn in_open_sea(theta: f64) -> bool {
    theta > OPEN_SEA.0 && theta < OPEN_SEA.1
}

/// How much of the headland stands at this angle: 0 at the open sea's edges,
/// rising over a quarter radian.
fn land_amount(theta: f64) -> f64 {
    let edge = (theta - OPEN_SEA.1).min(OPEN_SEA.0 + TAU - theta);
    let edge = if theta < OPEN_SEA.0 {
        (OPEN_SEA.0 - theta).min(theta + TAU - OPEN_SEA.1)
    } else {
        edge
    };
    (edge / 0.3).clamp(0.0, 1.0)
}

/// Ridge height of the hills behind the shore at an angle: low where the city and
/// the terminal stand (east), high headlands west.
fn ridge_height(theta: f64) -> f64 {
    let west = (-theta.cos()).max(0.0);
    let base = 7.0 + 26.0 * west + 12.0 * wave(theta, 1.7);
    base * land_amount(theta)
}

fn shore_radius(theta: f64) -> f64 {
    SHORE_RADIUS + 22.0 * wave(theta, 4.2) - 8.0
}

/// Profile of the coast from the waterline inland: (metres past the shoreline,
/// fraction of the ridge height, extra height, color).
const PROFILE: [(f64, f64, f64, u32); 6] = [
    (-3.0, 0.0, -1.2, 0x6f6a5d),
    (2.0, 0.0, 2.4, 0x9a907a),
    (9.0, 0.12, 3.0, 0x7d8f5e),
    (30.0, 0.62, 2.0, 0x6a8452),
    (55.0, 1.0, 0.0, 0x56714a),
    (110.0, 0.55, 0.0, 0x6c8160),
];

/// One sector of the shore ring as a flat-shaded, vertex-colored mesh.
fn shore_mesh(from: usize, to: usize) -> Option<Node> {
    let rows = PROFILE.len();
    let mut positions = Vec::new();
    let mut colors = Vec::new();
    for step in from..=to {
        let theta = -PI + TAU * step as f64 / SEGMENTS as f64;
        let radius = shore_radius(theta);
        let ridge = ridge_height(theta);
        for (row, &(offset, fraction, lift, color)) in PROFILE.iter().enumerate() {
            let jitter = hash(step as u32 * 8 + row as u32, 11) - 0.5;
            let r = radius + offset + jitter * 4.0;
            let y = SEA_LEVEL
                + (ridge * fraction + lift) * land_amount(theta).max(0.2)
                + jitter * ridge * 0.08;
            positions.extend([r * theta.cos(), y, r * theta.sin()]);
            let shade = 0.88 + hash(step as u32 * 8 + row as u32, 23) * 0.22;
            let [cr, cg, cb] = hex_to_linear(scale_hex_color(color, shade));
            colors.push([cr as f32, cg as f32, cb as f32]);
        }
    }
    let mut indices = Vec::new();
    for column in 0..(to - from) {
        let theta = -PI + TAU * (from + column) as f64 / SEGMENTS as f64;
        let next = -PI + TAU * (from + column + 1) as f64 / SEGMENTS as f64;
        if in_open_sea(theta) && in_open_sea(next) {
            continue;
        }
        for row in 0..rows - 1 {
            let a = (column * rows + row) as u32;
            let b = a + rows as u32;
            indices.extend([a, b, a + 1, b, b + 1, a + 1]);
        }
    }
    if indices.is_empty() {
        return None;
    }
    let mut mesh = Mesh::from_f64(&positions, &[], &[], Some(indices));
    mesh.colors = colors;
    let mut flat = mesh.to_non_indexed();
    flat.compute_vertex_normals();
    flat.uvs = vec![[0.0, 0.0]; flat.positions.len()];
    Some(Node::mesh(
        Arc::new(flat),
        Arc::new(Material {
            vertex_colors: true,
            flat_shading: true,
            ..Material::standard(0xffffff, 0.0, 1.0)
        }),
    ))
}

/// A point on the land `inland` metres past the shoreline at `theta`, at ground
/// height.
fn on_land(theta: f64, inland: f64) -> (f64, f64, f64) {
    let r = shore_radius(theta) + inland;
    let ridge = ridge_height(theta);
    let height = PROFILE
        .windows(2)
        .find(|pair| inland <= pair[1].0)
        .map_or(0.55 * ridge, |pair| {
            let (a, b) = (pair[0], pair[1]);
            let t = ((inland - a.0) / (b.0 - a.0)).clamp(0.0, 1.0);
            let ya = ridge * a.1 + a.2;
            let yb = ridge * b.1 + b.2;
            ya + (yb - ya) * t
        });
    (r * theta.cos(), SEA_LEVEL + height, r * theta.sin())
}

const CITY_PAINT: [u32; 7] = [
    0xdfe3e4, 0xb8c6cf, 0x8fa4b4, 0xe8ddc9, 0x9db8c4, 0x70869a, 0xc9b9a3,
];

/// Towers across the bay to the east, banded with floors of glazing.
fn city(sectors: &mut Sectors) {
    for i in 0..44u32 {
        let theta = -0.62 + 1.1 * hash(i, 3);
        let inland = 10.0 + 50.0 * hash(i, 5);
        let (x, y, z) = on_land(theta, inland);
        let centre = 1.0 - (theta - (-0.05)).abs() / 0.6;
        let height = 6.0 + hash(i, 7).powi(2) * 30.0 * (0.4 + 0.6 * centre.max(0.0));
        let (w, d) = (5.0 + 5.0 * hash(i, 9), 5.0 + 5.0 * hash(i, 13));
        let color = CITY_PAINT[(hash(i, 17) * CITY_PAINT.len() as f64) as usize % CITY_PAINT.len()];
        let yaw = -theta + hash(i, 19) * 0.3;
        let mut tower = Node::group("");
        put(
            &mut tower,
            box_part(w, height, d, color, 0.0),
            0.0,
            height / 2.0,
            0.0,
        );
        let glass = scale_hex_color(color, 0.55);
        let mut floor = 3.2;
        while floor < height - 2.0 {
            put(
                &mut tower,
                box_part(w + 0.3, 1.1, d + 0.3, glass, 0.0),
                0.0,
                floor,
                0.0,
            );
            floor += 3.4;
        }
        put(
            &mut tower,
            box_part(w * 0.4, 2.4, d * 0.4, 0x9aa0a0, 0.0),
            w * 0.15,
            height + 1.2,
            0.0,
        );
        place_at(sectors, theta, rotated(tower, 0.0, yaw, 0.0), x, y - 0.5, z);
    }
}

const BOX_PAINT: [u32; 6] = [0xc96f3b, 0x3d8a8c, 0x5f7fa6, 0xb7a26e, 0x9b4f45, 0xe0dccd];

/// A neighbouring container terminal to the south-east: stacked boxes and three
/// ship-to-shore cranes with their booms raised.
fn terminal(sectors: &mut Sectors) {
    for i in 0..60u32 {
        let theta = 0.55 + 0.5 * hash(i, 31);
        let inland = 6.0 + 26.0 * hash(i, 37);
        let (x, y, z) = on_land(theta, inland);
        let levels = 1 + (hash(i, 41) * 4.0) as u32;
        let color = BOX_PAINT[(hash(i, 43) * 6.0) as usize % 6];
        let stack = rotated(
            box_part(12.0, 2.6 * f64::from(levels), 2.5, color, 0.0),
            0.0,
            -theta,
            0.0,
        );
        place_at(sectors, theta, stack, x, y + 1.3 * f64::from(levels), z);
    }
    // Idle ship-to-shore cranes, booms raised: the terminal's silhouette.
    for i in 0..4u32 {
        let theta = 0.58 + 0.15 * f64::from(i);
        let (x, y, z) = on_land(theta, 3.0);
        let mut crane = Node::group("");
        let (red, white) = (0xa8342c, 0xe5e2da);
        for (lx, lz) in [(-5.0, -6.0), (5.0, -6.0), (-5.0, 6.0), (5.0, 6.0)] {
            put(&mut crane, box_part(1.0, 24.0, 1.0, red, 0.0), lx, 12.0, lz);
        }
        for lz in [-6.0, 6.0] {
            put(
                &mut crane,
                box_part(11.0, 1.2, 1.0, red, 0.0),
                0.0,
                23.5,
                lz,
            );
            beam(&mut crane, [-5.0, 9.0, lz], [5.0, 23.0, lz], 0.5, red);
        }
        for lx in [-4.0, 4.0] {
            beam(
                &mut crane,
                [lx, 24.0, -6.0],
                [lx * 0.3, 36.0, 1.0],
                0.8,
                red,
            );
            beam(&mut crane, [lx, 24.0, 6.0], [lx * 0.3, 36.0, 1.0], 0.8, red);
            // The boom, raised on its hinge, and the stay holding it.
            let tip = [
                lx * 0.3,
                24.5 + 36.0 * BOOM_RAISE.sin(),
                -6.0 - 36.0 * BOOM_RAISE.cos(),
            ];
            beam(&mut crane, [lx * 0.3, 24.5, -6.0], tip, 1.0, white);
            beam(&mut crane, [lx * 0.3, 36.0, 1.0], tip, 0.3, white);
        }
        put(
            &mut crane,
            box_part(12.0, 1.4, 14.0, white, 0.0),
            0.0,
            24.6,
            0.0,
        );
        put(
            &mut crane,
            box_part(6.0, 3.4, 5.0, white, 0.0),
            0.0,
            26.9,
            5.0,
        );
        let mut crane = rotated(crane, 0.0, FRAC_PI_2 - theta, 0.0);
        crane.scale = glam::DVec3::splat(0.75);
        place_at(sectors, theta, crane, x, y, z);
    }
}

/// Fuel tanks and a pair of banded stacks to the south.
fn tank_farm(sectors: &mut Sectors) {
    for i in 0..12u32 {
        let theta = 1.35 + 0.55 * hash(i, 51);
        let inland = 14.0 + 30.0 * hash(i, 53);
        let (x, y, z) = on_land(theta, inland);
        let radius = 2.5 + 2.0 * hash(i, 57);
        let height = 4.0 + 3.0 * hash(i, 59);
        let tank = cylinder_part(radius, height, 0xe6e4dc, 16);
        place_at(sectors, theta, tank, x, y + height / 2.0, z);
        let roof = cylinder_part(radius + 0.2, 0.8, 0x9fa6a6, 16);
        place_at(sectors, theta, roof, x, y + height, z);
    }
    for (i, theta) in [1.62, 1.71].into_iter().enumerate() {
        let (x, y, z) = on_land(theta, 22.0 + 6.0 * i as f64);
        for band in 0..6 {
            let color = if band % 2 == 0 { 0xe9e5dc } else { 0xc0463c };
            let stack = cylinder_part(1.3 - band as f64 * 0.06, 5.0, color, 12);
            place_at(sectors, theta, stack, x, y + 2.5 + band as f64 * 5.0, z);
        }
    }
}

/// Wind turbines along the western headland ridge.
fn turbines(sectors: &mut Sectors) {
    for i in 0..7u32 {
        let theta = 2.35 + 0.22 * f64::from(i) + 0.08 * hash(i, 61);
        let theta = if theta > PI { theta - TAU } else { theta };
        if in_open_sea(theta) || land_amount(theta) < 0.9 {
            continue;
        }
        let (x, y, z) = on_land(theta, 55.0);
        let mut turbine = Node::group("");
        put(
            &mut turbine,
            cylinder_part(0.7, 32.0, 0xf1f1ec, 10),
            0.0,
            16.0,
            0.0,
        );
        put(
            &mut turbine,
            box_part(3.4, 1.8, 1.8, 0xf1f1ec, 0.0),
            0.0,
            32.4,
            0.5,
        );
        let spin = hash(i, 67) * TAU;
        for blade in 0..3 {
            let angle = spin + f64::from(blade) * TAU / 3.0;
            let mut arm = Node::group("");
            put(
                &mut arm,
                box_part(0.55, 15.0, 0.2, 0xf4f4f0, 0.0),
                0.0,
                7.5,
                0.0,
            );
            let arm = rotated(arm, 0.0, 0.0, angle);
            put(&mut turbine, arm, 0.0, 32.4, 1.5);
        }
        let mut turbine = rotated(turbine, 0.0, -theta - FRAC_PI_2, 0.0);
        turbine.scale = glam::DVec3::splat(0.65);
        place_at(sectors, theta, turbine, x, y, z);
    }
}

/// A rubble breakwater across the northern sea with a lighthouse at its head.
fn breakwater(sectors: &mut Sectors) {
    let z = -158.0;
    let mut group = Node::group("");
    let group = &mut group;
    put(
        group,
        box_part(150.0, 4.0, 9.0, 0x8c8a80, 0.0),
        -55.0,
        SEA_LEVEL + 1.0,
        z,
    );
    for i in 0..40u32 {
        let x = -128.0 + f64::from(i) * 3.7;
        let size = 2.2 + hash(i, 71) * 1.8;
        let rock = rotated(
            box_part(
                size,
                size * 0.8,
                size,
                scale_hex_color(0x8b877b, 0.85 + hash(i, 73) * 0.3),
                0.0,
            ),
            hash(i, 79) * 0.6,
            hash(i, 83) * TAU,
            hash(i, 89) * 0.6,
        );
        put(group, rock, x, SEA_LEVEL + 1.2, z + 4.2 + hash(i, 97) * 1.2);
    }
    let (lx, lz) = (22.0, z);
    put(
        group,
        cylinder_part(5.0, 3.0, 0x8c8a80, 16),
        lx,
        SEA_LEVEL + 2.0,
        lz,
    );
    for band in 0..4 {
        let color = if band % 2 == 0 { 0xf2efe6 } else { 0xc8433a };
        put(
            group,
            cylinder_part(2.3 - f64::from(band) * 0.2, 4.5, color, 16),
            lx,
            SEA_LEVEL + 5.75 + f64::from(band) * 4.5,
            lz,
        );
    }
    put(
        group,
        cylinder_part(1.6, 2.4, 0xffe7a8, 12),
        lx,
        SEA_LEVEL + 23.0,
        lz,
    );
    put(
        group,
        cylinder_part(1.9, 0.7, 0x2f3b40, 12),
        lx,
        SEA_LEVEL + 24.5,
        lz,
    );
    let group = std::mem::replace(group, Node::group(""));
    place(sectors, -FRAC_PI_2 + 0.1, group);
}

/// Merge a sector, fade its paint into the distance haze and keep it out of the
/// shadow pass.
fn finish(mut sector: Node) -> Node {
    batch(&mut sector);
    for child in &mut sector.children {
        if let Some(drawable) = &mut child.drawable {
            drawable.cast_shadow = false;
            drawable.receive_shadow = false;
            let mut mesh = (*drawable.mesh).clone();
            for color in &mut mesh.colors {
                for (channel, haze) in color.iter_mut().zip(HAZE) {
                    *channel += (haze - *channel) * HAZE_AMOUNT;
                }
            }
            drawable.mesh = Arc::new(mesh);
        }
    }
    sector
}

/// The far shore around the harbor, as four batched sectors.
pub fn harbor_backdrop() -> Node {
    let mut root = Node::group("harbor-backdrop");
    let quarter = SEGMENTS / 4;
    let mut sectors: Sectors = std::array::from_fn(|_| Node::group(""));
    for (index, sector) in sectors.iter_mut().enumerate() {
        if let Some(shore) = shore_mesh(index * quarter, (index + 1) * quarter) {
            sector.children.push(shore);
        }
    }
    city(&mut sectors);
    terminal(&mut sectors);
    tank_farm(&mut sectors);
    turbines(&mut sectors);
    breakwater(&mut sectors);
    for sector in sectors {
        root.children.push(finish(sector));
    }
    root
}

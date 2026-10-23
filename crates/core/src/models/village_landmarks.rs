//! Port of `village-landmarks.ts`: the watermill with its turning wheel, two timber
//! footbridges over the creek and a log cart, all outside the combat arena.

use std::f64::consts::{FRAC_PI_2, PI};
use std::sync::Arc;

use crate::geometry::{cylinder_geometry, torus_geometry};
use crate::scene::Node;

use super::batching::batch;
use super::concrete_surfaces::concrete_wall;
use super::house_model::house_facing;
use super::house_surfaces::{shingle_roof, siding_box, siding_gable};
use super::model_primitives::{
    DEFAULT_ROUGHNESS, beam, box_part, cylinder_part, material, put, rotated,
};
use super::village_ground::{FARM_FENCES, FARMYARD, hay_field, north_wheat};
use super::village_landscape::valley_height;

/// The watermill wheel's node name; [`VillageScenery::update`] turns it.
///
/// [`VillageScenery::update`]: super::village_scenery::VillageScenery::update
pub const WATERWHEEL: &str = "turning-waterwheel";
const TIMBER: u32 = 0x725236;
/// The waterwheel's hub relative to the mill: far enough out over the creek that
/// its paddles dip into the water (the mill stands 0.25 m below the valley floor).
const WHEEL_OFFSET: f64 = -13.6;
const WHEEL_HUB: f64 = 1.1;

fn footbridge() -> Node {
    let mut group = Node::group("village-timber-bridge");
    let arch = |z: f64| -0.65 + 2.0 * (1.0 - (z / 9.0) * (z / 9.0));
    let mut z: f64 = -9.0;
    while z < 9.0 {
        let plank = rotated(
            siding_box(5.4, 0.18, 0.57, 0xb18c5d),
            ((4.0 * z) / 81.0).atan(),
            0.0,
            0.0,
        );
        put(&mut group, plank, 0.0, arch(z), z);
        z += 0.6;
    }
    for side in [-1.0, 1.0] {
        for step in 0..=6 {
            let z = -9.0 + f64::from(step) * 3.0;
            put(
                &mut group,
                box_part(0.22, 1.5, 0.22, 0x6b5036, 0.0),
                side * 2.5,
                arch(z) + 0.75,
                z,
            );
            put(
                &mut group,
                box_part(0.3, 0.12, 0.3, 0xc4aa7e, 0.0),
                side * 2.5,
                arch(z) + 1.55,
                z,
            );
            if z < 9.0 {
                for h in [0.55, 1.25] {
                    beam(
                        &mut group,
                        [side * 2.5, arch(z) + h, z],
                        [side * 2.5, arch(z + 3.0) + h, z + 3.0],
                        0.14,
                        TIMBER,
                    );
                }
                beam(
                    &mut group,
                    [side * 2.5, arch(z) + 0.4, z],
                    [side * 2.5, arch(z + 3.0) + 1.25, z + 3.0],
                    0.1,
                    TIMBER,
                );
            }
        }
        put(
            &mut group,
            concrete_wall(6.1, 2.6, 2.0),
            0.0,
            -1.85,
            side * 9.6,
        );
    }
    batch(&mut group);
    group
}

fn watermill() -> Node {
    let mut group = Node::group("pine-watermill");
    let mut building = Node::group("");
    let mut wheel = Node::group(WATERWHEEL);
    for (part, x, y, z) in [
        (concrete_wall(11.4, 3.0, 8.4), 0.0, 0.4, 0.0),
        (siding_box(11.0, 6.2, 8.0, 0xac8255), 0.0, 4.7, 0.0),
        (siding_gable(12.0, 3.6, 9.0, 0x934e3d), 0.0, 7.8, 0.0),
        (shingle_roof(12.0, 3.6, 9.0, 0x934e3d), 0.0, 7.8, 0.0),
        (concrete_wall(0.8, 2.4, 0.8), 3.0, 10.2, -2.0),
        (box_part(1.1, 0.2, 1.1, 0x796f5b, 0.0), 3.0, 11.5, -2.0),
        (box_part(0.53, 0.025, 0.53, 0x34392c, 0.0), 3.0, 11.62, -2.0),
    ] {
        put(&mut building, part, x, y, z);
    }
    for x in [-5.5, 0.0, 5.5] {
        put(
            &mut building,
            siding_box(0.27, 6.4, 0.25, 0x62472e),
            x,
            4.7,
            4.04,
        );
        if x != 0.0 {
            put(
                &mut building,
                siding_box(0.27, 6.4, 0.25, 0x62472e),
                x,
                4.7,
                -4.04,
            );
        }
    }
    for y in [2.0, 5.6, 7.65] {
        put(
            &mut building,
            siding_box(11.3, 0.23, 8.25, 0x62472e),
            0.0,
            y,
            0.0,
        );
    }
    for x in [-3.2, 3.2] {
        for (w, h, d, color, z) in [
            (1.8, 1.7, 0.12, 0xe0cc9c, 4.12),
            (1.5, 1.4, 0.1, 0x384f48, 4.2),
            (0.1, 1.4, 0.12, 0xc5b483, 4.27),
            (1.5, 0.1, 0.12, 0xc5b483, 4.27),
        ] {
            put(&mut building, box_part(w, h, d, color, 0.0), x, 4.15, z);
        }
        for side in [-1.0, 1.0] {
            let shutter = rotated(siding_box(0.65, 1.8, 0.14, 0x66877b), 0.0, side * 0.16, 0.0);
            put(&mut building, shutter, x + side * 1.25, 4.15, 4.13);
        }
        beam(
            &mut building,
            [x - 1.7, 5.8, 4.14],
            [x + 1.7, 7.5, 4.14],
            0.18,
            TIMBER,
        );
    }
    for (part, x, y, z) in [
        (siding_box(2.0, 3.2, 0.15, 0x584731), 0.0, 2.4, 4.17),
        (box_part(0.13, 0.13, 0.2, 0xc8a963, 0.0), 0.65, 2.3, 4.3),
    ] {
        put(&mut building, part, x, y, z);
    }
    for i in 0..4 {
        let i = f64::from(i);
        put(
            &mut building,
            concrete_wall(3.2, 0.3 + i * 0.15, 0.75),
            0.0,
            -0.55 + i * 0.19,
            6.6 - i * 0.7,
        );
    }
    put(
        &mut building,
        shingle_roof(4.0, 1.0, 2.8, 0x607563),
        0.0,
        4.35,
        5.2,
    );
    for x in [-1.9, 1.9] {
        put(
            &mut building,
            box_part(0.18, 4.5, 0.18, 0x6c5036, 0.0),
            x,
            1.9,
            6.45,
        );
    }
    // The axle runs from the mill wall out over the bank to the wheel, which turns
    // in the creek between two stone piers (the creek's water line is about 11.4 m
    // out from the mill's centre, its bed 4 m below the floor).
    let (start, end) = (-3.0, WHEEL_OFFSET - 1.4);
    let axle = rotated(
        cylinder_part(0.27, start - end, 0x514a3d, 10),
        FRAC_PI_2,
        0.0,
        0.0,
    );
    put(&mut building, axle, -8.0, WHEEL_HUB, (start + end) / 2.0);
    put(
        &mut building,
        concrete_wall(2.4, 2.5, 3.0),
        -8.0,
        -0.3,
        -4.9,
    );
    for (z, bottom) in [(WHEEL_OFFSET + 1.6, -3.4), (WHEEL_OFFSET - 1.6, -4.1)] {
        let top = WHEEL_HUB - 0.25;
        put(
            &mut building,
            concrete_wall(1.3, top - bottom, 1.1),
            -8.0,
            (top + bottom) / 2.0,
            z,
        );
    }
    for (part, y) in [
        (siding_box(3.2, 2.7, 3.4, 0x88683f), 1.85),
        (shingle_roof(3.7, 1.0, 3.9, 0x6c7354), 3.2),
    ] {
        put(&mut building, part, -6.9, y, -3.9);
    }
    for side in [-1.0, 1.0] {
        let ring = Node::mesh(
            Arc::new(torus_geometry(4.0, 0.16, 6, 48)),
            material(0x514a39, 0.25, DEFAULT_ROUGHNESS),
        );
        put(&mut wheel, ring, 0.0, 0.0, side * 0.72);
        for i in 0..12 {
            let a = (f64::from(i) * PI) / 6.0;
            beam(
                &mut wheel,
                [0.0, 0.0, side * 0.72],
                [a.cos() * 4.0, a.sin() * 4.0, side * 0.72],
                0.16,
                0xa98858,
            );
        }
    }
    for i in 0..20 {
        let a = (f64::from(i) * PI) / 10.0;
        let paddle = rotated(siding_box(0.28, 0.72, 1.8, 0x8a6941), 0.0, 0.0, a);
        put(&mut wheel, paddle, a.cos() * 4.0, a.sin() * 4.0, 0.0);
    }
    let hub = rotated(cylinder_part(0.56, 1.8, 0x6a6854, 12), FRAC_PI_2, 0.0, 0.0);
    wheel.children.push(hub);
    batch(&mut building);
    batch(&mut wheel);
    put(&mut group, building, 0.0, 0.0, 0.0);
    put(&mut group, wheel, -8.0, WHEEL_HUB, WHEEL_OFFSET);
    group
}

fn log_cart() -> Node {
    let mut group = Node::group("village-log-cart");
    put(
        &mut group,
        siding_box(4.0, 0.24, 5.0, 0x9f7c4e),
        0.0,
        1.05,
        0.0,
    );
    for x in [-1.8, 1.8] {
        for z in [-1.7, 1.7] {
            let wheel = rotated(cylinder_part(0.7, 0.23, 0x453d31, 12), 0.0, 0.0, FRAC_PI_2);
            put(&mut group, wheel, x, 0.68, z);
        }
        beam(
            &mut group,
            [x, 0.9, 2.3],
            [x * 0.55, 0.6, 6.0],
            0.16,
            TIMBER,
        );
    }
    for row in 0..3 {
        for i in 0..4 - row {
            let log = rotated(cylinder_part(0.38, 5.6, 0x80613e, 9), FRAC_PI_2, 0.0, 0.0);
            let x = (f64::from(i) - f64::from(3 - row) / 2.0) * 0.8;
            let y = 1.5 + f64::from(row) * 0.65;
            put(&mut group, log, x, y, 0.0);
            for side in [-1.0, 1.0] {
                let end = rotated(cylinder_part(0.31, 0.02, 0xc1a274, 9), FRAC_PI_2, 0.0, 0.0);
                put(&mut group, end, x, y, side * 2.81);
            }
        }
    }
    batch(&mut group);
    group
}

/// Lowest valley ground under a footprint (centre, half extents), so a building
/// set there never hangs over its downhill corner.
fn footing(x: f64, z: f64, half_x: f64, half_z: f64) -> f64 {
    [
        (-1.0, -1.0),
        (1.0, -1.0),
        (-1.0, 1.0),
        (1.0, 1.0),
        (0.0, 0.0),
    ]
    .iter()
    .map(|(sx, sz)| valley_height(x + sx * half_x, z + sz * half_z))
    .fold(f64::INFINITY, f64::min)
}

const BARN_RED: u32 = 0x963a2b;
const BARN_TRIM: u32 = 0xe9e1cf;
const BARN_ROOF: u32 = 0x4c4a47;
const STRAW: u32 = 0xc7ad62;

/// A red bank barn with white trim, sliding doors braced with an X, a hayloft
/// door in the gable and a louvred cupola on the ridge; the ridge runs along z.
fn barn(w: f64, d: f64, wall: f64, rise: f64) -> Node {
    let mut barn = Node::group("");
    put(
        &mut barn,
        concrete_wall(w + 0.4, 1.6, d + 0.4),
        0.0,
        -0.5,
        0.0,
    );
    put(
        &mut barn,
        siding_box(w, wall, d, BARN_RED),
        0.0,
        0.3 + wall / 2.0,
        0.0,
    );
    put(
        &mut barn,
        siding_gable(w, rise, d, BARN_RED),
        0.0,
        0.3 + wall,
        0.0,
    );
    put(
        &mut barn,
        shingle_roof(w + 0.9, rise + 0.3, d + 0.8, BARN_ROOF),
        0.0,
        0.3 + wall - 0.3,
        0.0,
    );
    // Corner boards and a band under the eaves.
    for sx in [-1.0, 1.0] {
        for sz in [-1.0, 1.0] {
            put(
                &mut barn,
                box_part(0.22, wall, 0.22, BARN_TRIM, 0.0),
                sx * w / 2.0,
                0.3 + wall / 2.0,
                sz * d / 2.0,
            );
        }
        put(
            &mut barn,
            box_part(0.12, 0.2, d, BARN_TRIM, 0.0),
            sx * (w / 2.0 + 0.04),
            0.3 + wall - 0.12,
            0.0,
        );
    }
    // Sliding doors on the yard side (-x), each braced with a white X.
    for (z, width) in [(-1.6, 3.0), (1.6, 3.0)] {
        let height = 3.6;
        put(
            &mut barn,
            siding_box(0.12, height, width, 0x7d2f23),
            -w / 2.0 - 0.08,
            0.3 + height / 2.0,
            z,
        );
        let (x, low, high) = (-w / 2.0 - 0.16, 0.45, 0.3 + height - 0.15);
        for (a, b) in [(-1.0, 1.0), (1.0, -1.0)] {
            beam(
                &mut barn,
                [x, low, z + a * (width / 2.0 - 0.15)],
                [x, high, z + b * (width / 2.0 - 0.15)],
                0.12,
                BARN_TRIM,
            );
        }
        for y in [low - 0.05, high + 0.05] {
            put(
                &mut barn,
                box_part(0.1, 0.16, width, BARN_TRIM, 0.0),
                x,
                y,
                z,
            );
        }
    }
    put(
        &mut barn,
        box_part(0.14, 0.14, 6.8, 0x3a3532, 0.0),
        -w / 2.0 - 0.1,
        0.3 + 3.75,
        0.0,
    );
    // The hayloft door high in the front gable, hay showing in it.
    for (sz, ww, hh, color, depth) in [
        (-1.0, 2.2, 2.0, BARN_TRIM, 0.08),
        (-1.0, 1.8, 1.6, 0x4a3a28, 0.14),
        (-1.0, 1.5, 0.6, STRAW, 0.16),
    ] {
        put(
            &mut barn,
            box_part(ww, hh, depth, color, 0.0),
            0.0,
            0.3 + wall + if color == STRAW { 0.0 } else { 0.5 },
            sz * (d / 2.0 + depth / 2.0),
        );
    }
    // Cupola.
    let top = 0.3 + wall + rise;
    put(
        &mut barn,
        siding_box(1.1, 1.0, 1.1, BARN_TRIM),
        0.0,
        top + 0.2,
        0.0,
    );
    for x in [-0.56, 0.56] {
        put(
            &mut barn,
            box_part(0.04, 0.6, 0.8, 0x3a3532, 0.0),
            x,
            top + 0.25,
            0.0,
        );
    }
    put(
        &mut barn,
        shingle_roof(1.6, 0.6, 1.6, BARN_ROOF),
        0.0,
        top + 0.68,
        0.0,
    );
    barn
}

/// A rail fence along a polyline: split posts every few metres, two rails, all
/// following the ground.
fn rail_fence(group: &mut Node, points: &[[f64; 2]]) {
    const SPACING: f64 = 2.6;
    const POST: u32 = 0x6d5a43;
    const RAIL: u32 = 0x8a7357;
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let length = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        let count = (length / SPACING).ceil().max(1.0) as usize;
        let at = |t: f64| {
            let (x, z) = (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t);
            [x, valley_height(x, z), z]
        };
        for i in 0..=count {
            let [x, y, z] = at(i as f64 / count as f64);
            let lean = ((x * 1.7 + z * 0.9).sin()) * 0.05;
            let post = rotated(box_part(0.14, 1.35, 0.14, POST, 0.0), lean, 0.0, lean * 0.6);
            put(group, post, x, y + 0.5, z);
            if i < count {
                let [nx, ny, nz] = at((i + 1) as f64 / count as f64);
                for h in [0.45, 0.95] {
                    beam(group, [x, y + h, z], [nx, ny + h, nz], 0.09, RAIL);
                }
            }
        }
    }
}

/// The farmstead east of the village: a barn and silo by the yard, a farmhouse
/// on the lane, round bales on the mown field, rail fences and a log cart.
fn farmstead() -> Node {
    let mut group = Node::group("village-farmstead");
    let mut parts = Node::group("");
    let ([yard_x, yard_z], _) = FARMYARD;
    let (bx, bz) = (yard_x + 11.5, yard_z + 3.0);
    put(
        &mut parts,
        barn(9.0, 13.0, 4.6, 3.4),
        bx,
        footing(bx, bz, 4.5, 6.5),
        bz,
    );
    // A stave silo with a conical cap beside the barn.
    let (sx, sz) = (bx + 1.5, bz - 9.5);
    let silo_ground = footing(sx, sz, 2.0, 2.0);
    put(
        &mut parts,
        cylinder_part(2.1, 10.0, 0xbcb3a3, 16),
        sx,
        silo_ground + 4.6,
        sz,
    );
    for i in 0..5 {
        put(
            &mut parts,
            cylinder_part(2.14, 0.12, 0x5d5a55, 16),
            sx,
            silo_ground + 1.0 + f64::from(i) * 2.0,
            sz,
        );
    }
    let cap = Node::mesh(
        Arc::new(cylinder_geometry(0.15, 2.4, 1.6, 16)),
        material(0x8e9493, 0.3, 0.5),
    );
    put(&mut parts, cap, sx, silo_ground + 10.4, sz);
    // Haystacks by the barn.
    for (dx, dz, r) in [(-7.0, 9.0, 1.9), (-9.5, 6.5, 1.5)] {
        let (x, z) = (yard_x + dx, yard_z + dz);
        let stack = Node::mesh(
            Arc::new(cylinder_geometry(0.25, r, r * 1.5, 12)),
            material(STRAW, 0.0, 1.0),
        );
        put(&mut parts, stack, x, valley_height(x, z) + r * 0.7, z);
    }
    // Round bales along the hay field's windrows.
    let (centre, size, angle) = hay_field();
    let (sin, cos) = angle.sin_cos();
    let mut seed = 0.37f64;
    for row in 0..5 {
        let across = -size[1] / 2.0 + 4.0 + f64::from(row) * 5.6;
        for k in 0..4 {
            seed = (seed * 9301.0 + 49297.0) % 233280.0 / 233280.0;
            let along = -size[0] / 2.0 + 4.0 + f64::from(k) * 6.4 + seed * 2.5;
            let x = centre[0] + along * cos - across * sin;
            let z = centre[1] + along * sin + across * cos;
            let bale = rotated(
                cylinder_part(0.72, 1.25, STRAW, 14),
                0.0,
                seed * 3.0,
                std::f64::consts::FRAC_PI_2,
            );
            put(&mut parts, bale, x, valley_height(x, z) + 0.66, z);
        }
    }
    for fence in FARM_FENCES {
        rail_fence(&mut parts, fence);
    }
    // Sheaves stooked in rows across the north wheat field.
    let (centre, size, angle) = north_wheat();
    let (sin, cos) = angle.sin_cos();
    for row in 0..2 {
        for k in 0..7 {
            let along = -size[0] / 2.0 + 3.0 + f64::from(k) * 4.6 + f64::from(row) * 2.3;
            let across = -2.5 + f64::from(row) * 5.0;
            let x = centre[0] + along * cos - across * sin;
            let z = centre[1] + along * sin + across * cos;
            let stook = Node::mesh(
                Arc::new(cylinder_geometry(0.12, 0.55, 1.15, 7)),
                material(STRAW, 0.0, 1.0),
            );
            put(&mut parts, stook, x, valley_height(x, z) + 0.5, z);
        }
    }
    // A field hut where the ploughed strip meets the trees.
    let (hx, hz) = (-41.0, -99.0);
    let hut_ground = footing(hx, hz, 1.6, 1.8);
    for (part, y) in [
        (siding_box(3.0, 2.2, 3.4, 0x7c6142), 1.1),
        (siding_gable(3.0, 1.1, 3.4, 0x7c6142), 2.2),
        (shingle_roof(3.5, 1.2, 3.9, 0x5a5650), 2.15),
        (box_part(0.9, 1.7, 0.08, 0x3e3226, 0.0), 0.85),
    ] {
        let z = if y < 1.0 { hz + 1.72 } else { hz };
        put(&mut parts, part, hx, hut_ground + y, z);
    }
    batch(&mut parts);
    group.children.push(parts);
    // The farmhouse faces the lane.
    let (hx, hz) = (yard_x - 7.0, yard_z - 18.0);
    let mut farmhouse = Node::group("");
    house_facing(&mut farmhouse, hx, hz, 6.0, 7.0, 5.4, 0xd8cdb4, 0);
    put(&mut farmhouse, concrete_wall(6.3, 1.6, 7.3), 0.0, -0.8, 0.0);
    put(&mut group, farmhouse, hx, footing(hx, hz, 3.0, 3.5), hz);
    let cart = rotated(log_cart(), 0.0, -0.9, 0.0);
    let (cx, cz) = (yard_x - 2.0, yard_z + 4.0);
    put(&mut group, cart, cx, valley_height(cx, cz) - 0.1, cz);
    group
}

/// `villageLandmarks().group`: the mill, both bridges and the log cart. The mill's
/// wheel is the descendant named [`WATERWHEEL`].
pub fn village_landmarks() -> Node {
    let mut group = Node::group("");
    group.children.push(farmstead());
    put(&mut group, watermill(), -35.0, -0.25, -69.0);
    put(&mut group, footbridge(), 0.0, 0.0, -82.0);
    let bridge = rotated(footbridge(), 0.0, FRAC_PI_2, 0.0);
    put(&mut group, bridge, -85.0, 0.0, 38.0);
    let camp = rotated(log_cart(), 0.0, 0.17, 0.0);
    put(&mut group, camp, -68.0, -0.65, -24.0);
    group
}

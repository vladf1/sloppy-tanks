//! Port of `village-landmarks.ts`: the watermill with its turning wheel, two timber
//! footbridges over the creek and a log cart, all outside the combat arena.

use std::f64::consts::{FRAC_PI_2, PI};
use std::sync::Arc;

use glam::DVec3;

use crate::geometry::torus_geometry;
use crate::scene::Node;

use super::batching::batch;
use super::concrete_surfaces::concrete_wall;
use super::house_surfaces::{shingle_roof, siding_box, siding_gable};
use super::model_primitives::span_between;
use super::model_primitives::{DEFAULT_ROUGHNESS, box_part, cylinder_part, material, put, rotated};

/// The watermill wheel's node name; [`VillageScenery::update`] turns it.
///
/// [`VillageScenery::update`]: super::village_scenery::VillageScenery::update
pub const WATERWHEEL: &str = "turning-waterwheel";
const TIMBER: u32 = 0x725236;
/// The waterwheel's hub relative to the mill: far enough out over the creek that
/// its paddles dip into the water (the mill stands 0.25 m below the valley floor).
const WHEEL_OFFSET: f64 = -13.6;
const WHEEL_HUB: f64 = 1.1;

/// `beam(group, a, b, width, color)`: a square timber between two points.
fn beam(group: &mut Node, a: [f64; 3], b: [f64; 3], width: f64, color: u32) {
    let (from, to) = (DVec3::from_array(a), DVec3::from_array(b));
    let part = box_part(width, to.distance(from), width, color, 0.0);
    group.children.push(span_between(part, from, to));
}

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
    put(&mut building, concrete_wall(11.4, 3.0, 8.4), 0.0, 0.4, 0.0);
    put(
        &mut building,
        siding_box(11.0, 6.2, 8.0, 0xac8255),
        0.0,
        4.7,
        0.0,
    );
    put(
        &mut building,
        siding_gable(12.0, 3.6, 9.0, 0x934e3d),
        0.0,
        7.8,
        0.0,
    );
    put(
        &mut building,
        shingle_roof(12.0, 3.6, 9.0, 0x934e3d),
        0.0,
        7.8,
        0.0,
    );
    put(&mut building, concrete_wall(0.8, 2.4, 0.8), 3.0, 10.2, -2.0);
    put(
        &mut building,
        box_part(1.1, 0.2, 1.1, 0x796f5b, 0.0),
        3.0,
        11.5,
        -2.0,
    );
    put(
        &mut building,
        box_part(0.53, 0.025, 0.53, 0x34392c, 0.0),
        3.0,
        11.62,
        -2.0,
    );
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
        put(
            &mut building,
            box_part(1.8, 1.7, 0.12, 0xe0cc9c, 0.0),
            x,
            4.15,
            4.12,
        );
        put(
            &mut building,
            box_part(1.5, 1.4, 0.1, 0x384f48, 0.0),
            x,
            4.15,
            4.2,
        );
        put(
            &mut building,
            box_part(0.1, 1.4, 0.12, 0xc5b483, 0.0),
            x,
            4.15,
            4.27,
        );
        put(
            &mut building,
            box_part(1.5, 0.1, 0.12, 0xc5b483, 0.0),
            x,
            4.15,
            4.27,
        );
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
    put(
        &mut building,
        siding_box(2.0, 3.2, 0.15, 0x584731),
        0.0,
        2.4,
        4.17,
    );
    put(
        &mut building,
        box_part(0.13, 0.13, 0.2, 0xc8a963, 0.0),
        0.65,
        2.3,
        4.3,
    );
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
    put(
        &mut building,
        siding_box(3.2, 2.7, 3.4, 0x88683f),
        -6.9,
        1.85,
        -3.9,
    );
    put(
        &mut building,
        shingle_roof(3.7, 1.0, 3.9, 0x6c7354),
        -6.9,
        3.2,
        -3.9,
    );
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

/// `villageLandmarks().group`: the mill, both bridges and the log cart. The mill's
/// wheel is the descendant named [`WATERWHEEL`].
pub fn village_landmarks() -> Node {
    let mut group = Node::group("");
    put(&mut group, watermill(), -35.0, -0.25, -69.0);
    put(&mut group, footbridge(), 0.0, 0.0, -82.0);
    let bridge = rotated(footbridge(), 0.0, FRAC_PI_2, 0.0);
    put(&mut group, bridge, -85.0, 0.0, 38.0);
    let camp = rotated(log_cart(), 0.0, 0.17, 0.0);
    put(&mut group, camp, -68.0, -0.65, -24.0);
    group
}

//! Port of `harbor-vessels.ts`: moored container ships and quay gantry cranes
//! beyond the harbor wall. Ships bob and crane loads sway; see [`HarborFleet`].

use std::f64::consts::FRAC_PI_2;
use std::sync::Arc;

use glam::DVec3;

use crate::geometry::{Mesh, torus_geometry};
use crate::scene::Node;

use super::batching::batch;
use super::harbor_models::{CargoShape, shipping_container};
use super::harbor_surfaces::{HarborSurface, harbor_box, harbor_material, steel_box};
use super::model_primitives::{DEFAULT_BOX_RADIUS, box_part, cylinder_part, paint, put, rotated};
use super::scenery::{adopt_children, distance, span_between};

/// `harborBeam(group, a, b, width, color)`: a square beam between authored
/// endpoints (crane braces, rails, rigging and mooring lines).
pub fn harbor_beam(group: &mut Node, a: [f64; 3], b: [f64; 3], width: f64, color: u32) {
    let (from, to) = (DVec3::from_array(a), DVec3::from_array(b));
    let beam = box_part(width, distance(from, to), width, color, 0.0);
    group.children.push(span_between(beam, from, to));
}

/// Hull cross-section outline (x along the ship, z across): a tapered stern and a
/// pointed bow.
const OUTLINE: [[f64; 2]; 9] = [
    [-34.0, -4.8],
    [-30.0, -7.0],
    [24.0, -7.0],
    [31.0, -4.8],
    [36.0, 0.0],
    [31.0, 4.8],
    [24.0, 7.0],
    [-30.0, 7.0],
    [-34.0, 4.8],
];

/// One flat-shaded hull band between `bottom` and `top`, its lower outline scaled
/// by `lower_scale` (the inset lower hull), with a deck cap.
fn hull_section(bottom: f64, top: f64, lower_scale: f64, color: u32) -> Node {
    let mut positions = Vec::with_capacity(OUTLINE.len() * 6);
    for (y, scale) in [(bottom, lower_scale), (top, 1.0)] {
        for [x, z] in OUTLINE {
            positions.extend([x * scale, y, z * scale]);
        }
    }
    let n = OUTLINE.len() as u32;
    let mut indices = Vec::new();
    for i in 0..n {
        let j = (i + 1) % n;
        indices.extend([i, i + n, j, j, i + n, j + n]);
    }
    for i in 1..n - 1 {
        indices.extend([n, n + i + 1, n + i]);
    }
    let mut flat = Mesh::from_f64(&positions, &[], &[], Some(indices)).to_non_indexed();
    flat.compute_vertex_normals();
    flat.uvs = flat
        .positions
        .iter()
        .map(|p| {
            [
                ((f64::from(p[0]) + f64::from(p[2])) / 5.0) as f32,
                (f64::from(p[1]) / 5.0) as f32,
            ]
        })
        .collect();
    Node::mesh(Arc::new(flat), harbor_material(HarborSurface::Steel, color))
}

const CARGO_PAINT: [u32; 5] = [0xcb7d43, 0x3b9394, 0x6e8eae, 0xbaad7c, 0xa76155];

fn container_ship(color: u32, variant: usize) -> Node {
    let mut ship = Node::group("container-ship");
    ship.children.push(hull_section(-3.8, -1.3, 0.85, 0xa95443));
    ship.children.push(hull_section(-1.3, 2.6, 1.0, color));
    put(
        &mut ship,
        steel_box(58.0, 0.2, 12.7, 0xbbbaa5),
        -2.0,
        2.72,
        0.0,
    );
    for bay in 0..4 {
        for row in [-1.0, 1.0] {
            let levels = if bay == variant % 4 { 1 } else { 2 };
            for level in 0..levels {
                let mut cargo = Node::group("");
                let paint_index = (bay + level * 2 + variant + usize::from(row > 0.0)) % 5;
                shipping_container(
                    &mut cargo,
                    CargoShape {
                        w: 9.6,
                        d: 4.5,
                        h: 3.2,
                        color: CARGO_PAINT[paint_index],
                    },
                );
                // Bake cargo into its ship before batching the whole vessel.
                cargo.position = DVec3::new(
                    -14.0 + bay as f64 * 10.2,
                    2.85 + level as f64 * 3.27,
                    row * 2.5,
                );
                adopt_children(&mut ship, cargo);
            }
        }
    }
    // Accommodation block, wraparound bridge glazing, deck rails and twin exhausts.
    put(
        &mut ship,
        steel_box(8.0, 7.5, 10.5, 0xf0e6c9),
        -26.0,
        6.55,
        0.0,
    );
    put(
        &mut ship,
        steel_box(9.3, 2.3, 11.4, 0xf0e6c9),
        -25.5,
        11.35,
        0.0,
    );
    put(
        &mut ship,
        box_part(9.7, 0.22, 11.8, 0xd9d4b9, DEFAULT_BOX_RADIUS),
        -25.5,
        12.62,
        0.0,
    );
    for side in [-1.0, 1.0] {
        for i in 0..6 {
            put(
                &mut ship,
                box_part(1.05, 1.15, 0.08, 0x335b6c, 0.0),
                -29.2 + f64::from(i) * 1.45,
                11.4,
                side * 5.72,
            );
        }
        put(
            &mut ship,
            box_part(0.08, 1.15, 8.8, 0x335b6c, 0.0),
            -20.82,
            11.4,
            0.0,
        );
        for i in 0..3 {
            put(
                &mut ship,
                box_part(0.08, 1.1, 1.1, 0x526e75, DEFAULT_BOX_RADIUS),
                -21.95,
                5.0 + f64::from(i) * 1.7,
                side * 2.8,
            );
        }
        harbor_beam(
            &mut ship,
            [-32.0, 3.65, side * 6.3],
            [25.0, 3.65, side * 6.3],
            0.09,
            0xd8d2b9,
        );
        let mut x = -32.0;
        while x < 26.0 {
            harbor_beam(
                &mut ship,
                [x, 2.8, side * 6.3],
                [x, 3.65, side * 6.3],
                0.08,
                0xd8d2b9,
            );
            x += 3.0;
        }
        for x in [-30.0, 26.0] {
            put(
                &mut ship,
                cylinder_part(0.5, 0.35, 0x293e4a, 12),
                x,
                3.1,
                side * 4.5,
            );
        }
        for x in [-22.0, 24.0] {
            let lifering = Node::mesh(Arc::new(torus_geometry(0.48, 0.12, 6, 12)), paint(0xe88b4a));
            put(&mut ship, lifering, x, 3.8, side * 6.42);
        }
    }
    put(
        &mut ship,
        steel_box(2.8, 4.1, 3.2, 0xc57943),
        -27.0,
        14.2,
        -1.7,
    );
    for z in [-2.3, -1.1] {
        put(
            &mut ship,
            cylinder_part(0.43, 1.2, 0x2d4149, 12),
            -27.0,
            16.5,
            z,
        );
    }
    harbor_beam(
        &mut ship,
        [-22.0, 12.7, 0.0],
        [-22.0, 18.0, 0.0],
        0.1,
        0xe6dec4,
    );
    harbor_beam(
        &mut ship,
        [-22.0, 16.8, -2.0],
        [-22.0, 16.8, 2.0],
        0.08,
        0xe6dec4,
    );
    put(
        &mut ship,
        box_part(3.2, 0.18, 0.35, 0xe1d8bf, DEFAULT_BOX_RADIUS),
        -22.0,
        17.7,
        0.0,
    );
    // Bow windlass and anchor-chain guide.
    put(
        &mut ship,
        cylinder_part(0.75, 0.8, 0x465558, 12),
        29.0,
        3.1,
        0.0,
    );
    harbor_beam(
        &mut ship,
        [29.0, 3.0, 0.0],
        [34.0, 2.8, 0.0],
        0.16,
        0x687271,
    );
    batch(&mut ship);
    ship
}

const CRANE_GOLD: u32 = 0xe9b347;
const CRANE_DARK: u32 = 0x394f5d;

/// A quay gantry crane; its hanging load is the group's last child.
fn gantry_crane() -> Node {
    let mut group = Node::group("quay-crane");
    let (gold, dark) = (CRANE_GOLD, CRANE_DARK);
    put(
        &mut group,
        harbor_box(14.0, 1.4, 9.0, 0xb9b9a7, HarborSurface::Dock),
        0.0,
        -0.65,
        0.0,
    );
    for side in [-1.0, 1.0] {
        put(
            &mut group,
            steel_box(2.0, 0.7, 8.0, dark),
            side * 5.0,
            0.5,
            0.0,
        );
        for z in [-2.8, 2.8] {
            let wheel = rotated(cylinder_part(0.65, 0.6, 0x2b3940, 12), 0.0, 0.0, FRAC_PI_2);
            put(&mut group, wheel, side * 5.0, 0.6, z);
            harbor_beam(
                &mut group,
                [side * 5.0, 1.0, z],
                [side * 3.4, 18.0, z * 0.7],
                0.65,
                gold,
            );
        }
        for step in 0..4 {
            let y = 2.0 + f64::from(step) * 4.0;
            harbor_beam(
                &mut group,
                [side * 4.8, y, -2.5],
                [side * 4.1, y + 4.0, 2.5],
                0.23,
                gold,
            );
            harbor_beam(
                &mut group,
                [side * 4.8, y, 2.5],
                [side * 4.1, y + 4.0, -2.5],
                0.23,
                gold,
            );
        }
        // Parallel boom trusses extend over water, never across the playfield.
        for y in [18.0, 20.0] {
            harbor_beam(&mut group, [side, y, 3.0], [side, y, -21.0], 0.32, gold);
        }
        for step in 0..8 {
            let z = 3.0 - f64::from(step) * 3.0;
            harbor_beam(
                &mut group,
                [side, 18.0, z],
                [side, 20.0, z - 3.0],
                0.19,
                gold,
            );
            harbor_beam(
                &mut group,
                [side, 20.0, z],
                [side, 18.0, z - 3.0],
                0.19,
                gold,
            );
        }
        harbor_beam(
            &mut group,
            [side * 3.4, 17.7, 0.0],
            [side, 20.0, -13.0],
            0.12,
            0xb8b9a6,
        );
    }
    put(&mut group, steel_box(9.0, 1.1, 4.8, gold), 0.0, 17.6, 0.0);
    put(&mut group, steel_box(4.0, 3.2, 4.0, dark), 0.0, 19.0, 3.8);
    put(&mut group, steel_box(2.5, 2.0, 2.3, gold), 2.0, 16.2, -4.0);
    put(
        &mut group,
        box_part(2.55, 1.15, 0.07, 0x6ca6b5, 0.0),
        2.0,
        16.4,
        -5.18,
    );
    // Maintenance ladder and high-visibility railing.
    let mut y = 1.0;
    while y < 17.0 {
        put(
            &mut group,
            box_part(0.7, 0.07, 0.1, 0xd2cdb8, 0.0),
            -4.7,
            y,
            2.85,
        );
        y += 0.55;
    }
    batch(&mut group);
    let mut load = Node::group("");
    for x in [-0.8, 0.8] {
        harbor_beam(&mut load, [x, 0.0, 0.0], [x, -8.0, 0.0], 0.045, dark);
    }
    put(&mut load, steel_box(4.5, 0.5, 2.8, gold), 0.0, -8.0, 0.0);
    for x in [-1.8, 1.8] {
        put(
            &mut load,
            box_part(0.22, 0.7, 0.2, dark, DEFAULT_BOX_RADIUS),
            x,
            -8.5,
            0.0,
        );
    }
    batch(&mut load);
    put(&mut group, load, 0.0, 18.0, -15.0);
    group
}

/// Ship berths: x, z, yaw, scale, hull color. Side berths make ships visible from
/// both teams' normal deployment cameras.
const SHIPS: [(f64, f64, f64, f64, u32); 3] = [
    (-8.0, -80.0, 0.0, 1.0, 0x3f6d80),
    (-77.0, -8.0, FRAC_PI_2, 0.85, 0x527f79),
    (77.0, 15.0, -FRAC_PI_2, 0.72, 0x9a6257),
];
/// Crane positions: x, z, yaw, scale.
const CRANES: [(f64, f64, f64, f64); 4] = [
    (-40.0, -65.0, 0.0, 1.0),
    (40.0, -65.0, 0.0, 1.0),
    (-65.5, 16.0, FRAC_PI_2, 0.75),
    (65.5, -20.0, -FRAC_PI_2, 0.75),
];

/// `HarborFleet`: the ships (first children) then the cranes.
pub struct HarborFleet;

impl HarborFleet {
    /// The fleet group: three ships, then four cranes.
    pub fn build() -> Node {
        let mut group = Node::group("");
        for (i, &(x, z, yaw, scale, color)) in SHIPS.iter().enumerate() {
            let mut ship = rotated(container_ship(color, i), 0.0, yaw, 0.0);
            ship.scale = DVec3::splat(scale);
            put(&mut group, ship, x, 0.0, z);
        }
        for &(x, z, yaw, scale) in &CRANES {
            let mut crane = rotated(gantry_crane(), 0.0, yaw, 0.0);
            crane.scale = DVec3::splat(scale);
            put(&mut group, crane, x, 0.0, z);
        }
        group
    }

    /// `update(time)`: ships bob and roll slightly, crane loads sway.
    pub fn update(fleet: &mut Node, time: f64) {
        for (i, &(_, _, yaw, _, _)) in SHIPS.iter().enumerate() {
            let ship = &mut fleet.children[i];
            let i = i as f64;
            ship.position.y = (time * 0.55 + i * 2.0).sin() * 0.07;
            ship.set_rotation_euler((time * 0.42 + i).sin() * 0.0018, yaw, 0.0);
        }
        for i in 0..CRANES.len() {
            let crane = &mut fleet.children[SHIPS.len() + i];
            let load = crane.children.last_mut().expect("crane load");
            load.set_rotation_euler(0.0, 0.0, (time * 0.7 + i as f64).sin() * 0.025);
        }
    }
}

//! Port of `quarry-site-details.ts`: the work-floor gravel, dry scrub on the
//! apron, the screening conveyor, site office dressing, the site sign and two
//! mobile lighting towers. All tall dressing is outside the playable wall;
//! in-arena gravel is only 2–4 cm high.

use std::f64::consts::{FRAC_PI_2, PI};
use std::sync::Arc;

use glam::DVec3;

use crate::geometry::{Mesh, icosahedron_geometry, plane_geometry, widen};
use crate::scene::{Material, Node, Side, TextureRef};

use super::effects_scenery::QUARRY_SIGN_TEXTURE;
use super::harbor_surfaces::{steel_beam, steel_box};
use super::model_primitives::adopt_children;
use super::model_primitives::{box_part, cylinder_part, put, rotated};
use super::quarry_surfaces::{RubbleStone, sandstone_rubble};
use super::quarry_terrain::quarry_ground_drop;
use crate::geometry::math::{hex_to_linear, js_hypot};
use crate::sim::arena::spawn_positions;
use crate::sim::math::Random;
use crate::sim::types::Team;

/// The "DUSTY DIG / 03 — ACTIVE QUARRY" board (drawn by the browser, see
/// [`QUARRY_SIGN_TEXTURE`]) on two posts.
fn site_sign(group: &mut Node) {
    let face = Node::mesh(
        Arc::new(plane_geometry(5.8, 2.9)),
        Arc::new(Material {
            map: Some(TextureRef::generated(QUARRY_SIGN_TEXTURE)),
            ..Material::standard(0xffffff, 0.0, 0.95)
        }),
    );
    put(group, face, 49.0, 2.6, -66.9);
    for x in [46.7, 51.3] {
        put(group, steel_box(0.14, 5.0, 0.14, 0x535953), x, 0.7, -67.1);
    }
}

/// Machinery, stockpiles and haul lanes on the apron that scrub never grows on.
const SCRUB_KEEP_OUT: [[f64; 4]; 7] = [
    [-31.0, -16.0, -76.0, -60.0], // excavator
    [-66.0, -38.0, -74.0, -65.0], // screening conveyor and hopper
    [5.0, 27.0, -78.0, -59.0],    // sentinel butte
    [23.0, 54.0, -76.0, -62.0],   // site office, water tank, drums and sign
    [-55.0, -36.0, 63.0, 74.0],   // cut stone stacks
    [62.0, 77.0, 8.0, 28.0],      // haul truck bay
    [63.0, 86.0, 24.0, 70.0],     // east haul ramp
];

/// `quarrySiteDetails(equipment, gravel)`: append the site dressing to
/// `equipment` and the work-floor gravel rubble to `gravel`.
pub fn quarry_site_details(equipment: &mut Node, gravel: &mut Node) {
    let mut rng = Random::new(62541.0);
    // Spilled haul loads leave tight clusters of flat gravel across the work floor,
    // with a scatter of strays between them. Spawn pads stay clean.
    let pads: Vec<_> = spawn_positions(Team::Blue, 1.0)
        .into_iter()
        .chain(spawn_positions(Team::Red, 1.0))
        .collect();
    let mut stones = Vec::new();
    let mut stone = |rng: &mut Random, x: f64, z: f64, size: f64| {
        if x.abs().max(z.abs()) > 58.5 || pads.iter().any(|p| js_hypot(&[x - p.x, z - p.z]) < 3.4) {
            return;
        }
        let h = rng.range(0.04, 0.07);
        let d = size * rng.range(0.6, 1.1);
        let rot_y = rng.range(-PI, PI);
        let shade = rng.range(0.55, 0.95);
        stones.push(RubbleStone {
            x,
            y: 0.008,
            z,
            w: size,
            h,
            d,
            rot_y,
            shade,
        });
    };
    for _ in 0..40 {
        let cx = rng.range(-56.0, 56.0);
        let cz = rng.range(-56.0, 56.0);
        let spread = rng.range(0.8, 2.6);
        let count = 10 + rng.range(0.0, 16.0).floor() as usize;
        for _ in 0..count {
            let angle = rng.range(0.0, PI * 2.0);
            let r = spread * rng.next().sqrt();
            let size = rng.range(0.1, 0.3);
            stone(&mut rng, cx + angle.cos() * r, cz + angle.sin() * r, size);
        }
    }
    // Strays between the clusters, then a few flat spalls knocked off the rock
    // islands by earlier shelling.
    for (count, min_size, max_size) in [(240, 0.12, 0.42), (30, 0.45, 0.8)] {
        for _ in 0..count {
            let (x, z, size) = (
                rng.range(-57.0, 57.0),
                rng.range(-57.0, 57.0),
                rng.range(min_size, max_size),
            );
            stone(&mut rng, x, z, size);
        }
    }
    gravel.children.push(sandstone_rubble(&stones));
    equipment.children.push(scrub(&mut rng));
    conveyor_and_yard(equipment);
    site_sign(equipment);
    // Mobile lighting plants stand by for night shifts, giving the apron some height.
    light_tower(equipment, 57.0, -67.5, -0.4);
    light_tower(equipment, -67.0, 58.0, 2.2);
}

/// Dry scrub holds the undisturbed shoulders, clear of traffic, machinery and the
/// combat lanes: straw and sage grass tufts plus low rounded saltbush.
fn scrub(rng: &mut Random) -> Node {
    let mut scrub: Vec<f64> = Vec::new();
    let mut tints: Vec<[f32; 3]> = Vec::new();
    let (sage, straw) = (hex_to_linear(0x87866a), hex_to_linear(0xa99571));
    for i in 0..190 {
        let side = if i % 2 == 1 { -1.0 } else { 1.0 };
        let along = rng.range(-72.0, 72.0);
        let out = side * rng.range(64.0, 72.5);
        let (x, z) = if i % 4 < 2 {
            (along, out)
        } else {
            (out, along * 0.75)
        };
        let y = -quarry_ground_drop(x, z) + 0.01;
        let blocked = SCRUB_KEEP_OUT
            .iter()
            .any(|&[x0, x1, z0, z1]| x > x0 && x < x1 && z > z0 && z < z1);
        let mix = rng.range(0.0, 1.0);
        let bright = rng.range(0.8, 1.05);
        let tint = [0, 1, 2].map(|c| (sage[c] + (straw[c] - sage[c]) * mix) * bright);
        if blocked {
            continue;
        }
        if i % 5 == 0 {
            // Saltbush: a squat, lumpy faceted clump, darker toward its underside.
            let radius = rng.range(0.35, 0.65);
            let mut bush = icosahedron_geometry(radius, 0);
            bush.rotate_y(rng.range(0.0, PI));
            // Shared corners move together so the clump stays closed.
            let lumps: Vec<f64> = (0..12).map(|_| rng.range(0.78, 1.18)).collect();
            let mut corners: Vec<DVec3> = Vec::new();
            for p in &bush.positions {
                let vertex = widen(*p);
                let corner = match corners
                    .iter()
                    .position(|c| c.distance_squared(vertex) < 1e-6)
                {
                    Some(corner) => corner,
                    None => {
                        corners.push(vertex);
                        corners.len() - 1
                    }
                };
                let vertex = vertex * lumps[corner % lumps.len()];
                let shade = 0.66 + 0.34 * 0.0f64.max(vertex.y / radius + 0.2);
                scrub.extend([
                    x + vertex.x,
                    y + radius * 0.3 + vertex.y * 0.62,
                    z + vertex.z,
                ]);
                tints.push([
                    (tint[0] * shade * 0.92) as f32,
                    (tint[1] * shade) as f32,
                    (tint[2] * shade * 0.94) as f32,
                ]);
            }
            continue;
        }
        for _ in 0..7 {
            let angle = rng.range(0.0, PI * 2.0);
            let dx = angle.cos() * 0.05;
            let dz = angle.sin() * 0.05;
            let lean = rng.range(0.15, 0.4);
            let tip = rng.range(0.22, 0.6);
            scrub.extend([
                x - dz,
                y,
                z + dx,
                x + dz,
                y,
                z - dx,
                x + angle.cos() * lean,
                y + tip,
                z + angle.sin() * lean,
            ]);
            // Blades fade from shaded base to sunlit, straw-bleached tips.
            let base = tint.map(|c| (c * 0.6) as f32);
            tints.extend([
                base,
                base,
                [
                    (tint[0] * 1.15) as f32,
                    (tint[1] * 1.1) as f32,
                    tint[2] as f32,
                ],
            ]);
        }
    }
    let mut geometry = Mesh::from_f64(&scrub, &[], &[], None);
    geometry.colors = tints;
    geometry.compute_vertex_normals();
    Node::mesh(
        Arc::new(geometry),
        Arc::new(Material {
            side: Side::Double,
            vertex_colors: true,
            ..Material::standard(0xffffff, 0.0, 1.0)
        }),
    )
}

/// The idle screening conveyor, its hopper and the site office dressing.
fn conveyor_and_yard(equipment: &mut Node) {
    // Idle screening conveyor: rust-red chords, dusty truss and a faded feed hopper.
    for z in [-71.4, -68.6] {
        steel_beam(equipment, [-62.0, 0.3, z], [-42.0, 6.3, z], 0.24, 0x8a5136);
        steel_beam(equipment, [-62.0, 1.6, z], [-42.0, 7.6, z], 0.14, 0xb08d46);
        for i in 0..8 {
            let x = -62.0 + f64::from(i) * 2.5;
            let y = 0.3 + f64::from(i) * 0.75;
            steel_beam(equipment, [x, y, z], [x + 2.5, y + 2.05, z], 0.09, 0x6b5a48);
            steel_beam(equipment, [x, y, z], [x, y + 1.3, z], 0.085, 0x6b5a48);
        }
    }
    // Muted teal drive motor and rust head drum mark the working head end.
    put(
        equipment,
        steel_box(1.2, 1.0, 1.1, 0x4e7d7c),
        -41.6,
        6.9,
        -70.0,
    );
    let drum = rotated(cylinder_part(0.5, 2.9, 0x8a4f2e, 12), FRAC_PI_2, 0.0, 0.0);
    put(equipment, drum, -42.1, 6.35, -70.0);
    let belt = rotated(
        box_part(21.0, 0.14, 2.5, 0x3d413b, 0.0),
        0.0,
        0.0,
        6.0f64.atan2(20.0),
    );
    put(equipment, belt, -52.0, 3.45, -70.0);
    for i in 0..15 {
        let i = f64::from(i);
        let roller = rotated(cylinder_part(0.18, 3.1, 0x5b625b, 8), FRAC_PI_2, 0.0, 0.0);
        put(equipment, roller, -62.0 + i * 1.4, 0.35 + i * 0.42, -70.0);
    }
    for z in [-71.3, -68.7] {
        steel_beam(equipment, [-46.0, -1.7, z], [-46.0, 5.4, z], 0.22, 0x68766e);
        steel_beam(equipment, [-53.0, -1.7, z], [-46.0, 5.4, z], 0.17, 0x68766e);
    }
    put(
        equipment,
        steel_box(4.2, 2.1, 3.7, 0xb08d46),
        -63.0,
        0.2,
        -70.0,
    );
    put(
        equipment,
        box_part(3.7, 0.07, 3.2, 0x42483c, 0.0),
        -63.0,
        1.29,
        -70.0,
    );
    // Office access, air conditioner, water tank and stacked sawn blocks.
    put(
        equipment,
        steel_box(1.5, 2.7, 0.12, 0x515f59),
        41.2,
        -0.3,
        -67.4,
    );
    for i in 0..3 {
        let i = f64::from(i);
        put(
            equipment,
            steel_box(2.2, 0.22, 0.6, 0x8c9081),
            41.2,
            -1.15 - i * 0.22,
            -66.9 + i * 0.6,
        );
    }
    put(
        equipment,
        steel_box(1.7, 0.9, 0.65, 0xb3b0a0),
        33.5,
        -0.6,
        -67.1,
    );
    for i in 0..5 {
        put(
            equipment,
            box_part(1.4, 0.045, 0.04, 0x596058, 0.0),
            33.5,
            -0.9 + f64::from(i) * 0.14,
            -66.75,
        );
    }
    put(
        equipment,
        cylinder_part(1.55, 3.8, 0xa8aaa0, 20),
        27.0,
        0.1,
        -70.0,
    );
    for y in [-1.2, 1.35] {
        put(
            equipment,
            cylinder_part(1.6, 0.13, 0x7e4a2c, 20),
            27.0,
            y,
            -70.0,
        );
    }
    // Rust-skirted office, faded generator and a tight teal/rust drum cluster.
    put(
        equipment,
        steel_box(11.2, 0.5, 5.2, 0x7e4a2c),
        37.0,
        -1.55,
        -70.0,
    );
    put(
        equipment,
        steel_box(2.2, 1.4, 1.2, 0xb08d46),
        30.5,
        -1.1,
        -66.6,
    );
    put(
        equipment,
        steel_box(2.3, 0.18, 1.3, 0x4a4238),
        30.5,
        -0.32,
        -66.6,
    );
    for (x, z, color) in [
        (44.2, -65.2, 0x4e7d7c),
        (45.35, -66.5, 0x4e7d7c),
        (44.75, -65.75, 0x8a4f2e),
    ] {
        put(equipment, cylinder_part(0.55, 1.3, color, 12), x, -1.15, z);
    }
    for i in 0..8 {
        put(
            equipment,
            steel_box(3.2, 1.1, 2.2, 0xb9b09a),
            -50.0 + f64::from(i % 4) * 3.4,
            -1.2 + f64::from(i / 4) * 1.15,
            69.0,
        );
    }
}

/// `lightTower(group, x, z, yaw)`: a towed mast light (trailer, outriggers, a tall
/// mast and a lamp bar), baked into `group`'s parts.
fn light_tower(group: &mut Node, x: f64, z: f64, yaw: f64) {
    let mut tower = Node::group("");
    put(
        &mut tower,
        steel_box(2.6, 1.05, 1.35, 0xc69a4b),
        0.0,
        0.95,
        0.0,
    );
    put(
        &mut tower,
        steel_box(2.7, 0.12, 1.45, 0x4a4238),
        0.0,
        0.42,
        0.0,
    );
    for side in [-1.0, 1.0] {
        let wheel = rotated(cylinder_part(0.36, 0.26, 0x343431, 10), FRAC_PI_2, 0.0, 0.0);
        put(&mut tower, wheel, -0.3, 0.36, side * 0.8);
        steel_beam(
            &mut tower,
            [side * 1.2, 0.5, -0.6],
            [side * 1.55, 0.0, -1.1],
            0.08,
            0x5a564c,
        );
        steel_beam(
            &mut tower,
            [side * 1.2, 0.5, 0.6],
            [side * 1.55, 0.0, 1.1],
            0.08,
            0x5a564c,
        );
    }
    steel_beam(
        &mut tower,
        [1.3, 0.45, 0.0],
        [2.3, 0.35, 0.0],
        0.1,
        0x5a564c,
    );
    put(
        &mut tower,
        steel_box(0.2, 7.4, 0.2, 0x9a9a92),
        -0.9,
        5.1,
        0.0,
    );
    put(
        &mut tower,
        steel_box(0.12, 0.12, 2.1, 0x5a564c),
        -0.9,
        8.8,
        0.0,
    );
    for dz in [-0.78, -0.26, 0.26, 0.78] {
        let lamp = rotated(steel_box(0.34, 0.3, 0.42, 0x353e3c), 0.0, 0.0, -0.35);
        put(&mut tower, lamp, -0.76, 8.62, dz);
        let lens = rotated(box_part(0.02, 0.24, 0.34, 0xe8e2c8, 0.0), 0.0, 0.0, -0.35);
        put(&mut tower, lens, -0.57, 8.55, dz);
    }
    tower.set_rotation_euler(0.0, yaw, 0.0);
    tower.position = DVec3::new(x, -1.79, z);
    adopt_children(group, tower);
}

//! Port of `quarry-site-details.ts`: dry scrub on the apron, the screening
//! conveyor, site office dressing, the site sign and two mobile lighting towers.
//! All of it stands outside the playable wall.

use std::f64::consts::{FRAC_PI_2, PI};
use std::sync::Arc;

use glam::DVec3;

use crate::geometry::{Mesh, icosahedron_geometry, plane_geometry, widen};
use crate::scene::{Material, Node, Side, TextureRef};

use super::effects_scenery::QUARRY_SIGN_TEXTURE;
use super::harbor_surfaces::{steel_beam, steel_box};
use super::model_primitives::adopt_children;
use super::model_primitives::{box_part, cylinder_part, put, rotated};
use super::quarry_machinery::{PAINT, weather_paint};
use super::quarry_terrain::quarry_ground_drop;
use crate::geometry::math::hex_to_linear;
use crate::sim::math::Random;

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
const SCRUB_KEEP_OUT: [[f64; 4]; 10] = [
    [-31.0, -16.0, -76.0, -60.0], // excavator
    [-77.0, -65.0, 24.0, 44.0],   // wheel loader and its heap
    [3.0, 26.0, 65.0, 76.0],      // drill rig and its drilled line
    [-31.0, -15.0, 65.0, 78.0],   // south spoil heap
    [-66.0, -38.0, -74.0, -65.0], // screening conveyor and hopper
    [5.0, 27.0, -78.0, -59.0],    // sentinel butte
    [23.0, 54.0, -76.0, -62.0],   // site office, water tank, drums and sign
    [-55.0, -36.0, 63.0, 74.0],   // cut stone stacks
    [62.0, 77.0, 8.0, 28.0],      // haul truck bay
    [63.0, 86.0, 24.0, 70.0],     // east haul ramp
];

/// `quarrySiteDetails(equipment)`: append the site dressing to `equipment`. The
/// work floor's loose gravel is the soil shader's (`quarry_soil.wgsl`): stones a
/// few centimetres high cost a GPU nothing there, and gather where the bake says.
pub fn quarry_site_details(equipment: &mut Node) {
    let mut rng = Random::new(62541.0);
    equipment.children.push(scrub(&mut rng));
    conveyor_and_yard(equipment);
    site_sign(equipment);
    // Mobile lighting plants stand by for night shifts, giving the apron some height.
    light_tower(equipment, 57.0, -67.5, -0.4);
    light_tower(equipment, -67.0, 58.0, 2.2);
    // A blast-hole drill works the south apron, its mast up over the line of
    // holes it has drilled (their cuttings are the scenery's spoil).
    drill_rig(equipment, DRILL_RIG[0], DRILL_RIG[1], DRILL_RIG[2]);
    // A wheel loader works the west apron's heap, bucket down at its toe.
    wheel_loader(equipment, WHEEL_LOADER[0], WHEEL_LOADER[1], WHEEL_LOADER[2]);
}

/// The wheel loader's spot on the west apron: x, z and yaw (its bucket faces
/// the heap beside it, toward +z).
const WHEEL_LOADER: [f64; 3] = [-70.6, 29.5, -1.6];

/// `wheelLoader(group, x, z, yaw)`: an articulated wheel loader in fleet yellow,
/// bucket lowered: rear engine bay and counterweight, cab over the hinge, lift
/// arms and a toothed bucket out front on four big tyres. Built along +x.
fn wheel_loader(group: &mut Node, x: f64, z: f64, yaw: f64) {
    const DARK: u32 = 0x4a4238;
    const FRAME: u32 = 0x5a564c;
    let mut loader = Node::group("");
    for (wx, wz) in [(-1.7, -1.15), (-1.7, 1.15), (1.6, -1.15), (1.6, 1.15)] {
        let tyre = rotated(cylinder_part(0.85, 0.7, 0x343431, 12), FRAC_PI_2, 0.0, 0.0);
        put(&mut loader, tyre, wx, 0.85, wz);
        let hub = rotated(cylinder_part(0.36, 0.74, 0x7e4a2c, 8), FRAC_PI_2, 0.0, 0.0);
        put(&mut loader, hub, wx, 0.85, wz);
    }
    put(&mut loader, steel_box(5.6, 0.5, 1.5, FRAME), 0.0, 1.05, 0.0);
    // Engine bay and counterweight at the back, a grille in its tail.
    put(
        &mut loader,
        box_part(2.6, 1.3, 1.9, PAINT, 0.0),
        -1.75,
        1.95,
        0.0,
    );
    put(
        &mut loader,
        box_part(0.5, 0.9, 2.1, PAINT, 0.0),
        -3.1,
        1.55,
        0.0,
    );
    put(
        &mut loader,
        box_part(0.05, 0.75, 1.5, 0x3d413b, 0.0),
        -3.37,
        1.65,
        0.0,
    );
    put(
        &mut loader,
        cylinder_part(0.08, 0.9, 0x7e4a2c, 8),
        -1.2,
        3.0,
        0.6,
    );
    // Cab over the hinge, dark glass all round under a pale roof.
    put(
        &mut loader,
        box_part(1.5, 1.7, 1.6, PAINT, 0.0),
        0.05,
        2.65,
        0.0,
    );
    put(
        &mut loader,
        box_part(1.25, 1.0, 1.66, 0x42483c, 0.0),
        0.08,
        2.85,
        0.0,
    );
    put(
        &mut loader,
        box_part(1.56, 1.0, 1.35, 0x42483c, 0.0),
        0.05,
        2.85,
        0.0,
    );
    put(
        &mut loader,
        steel_box(1.75, 0.12, 1.8, DARK),
        0.05,
        3.56,
        0.0,
    );
    // Front frame, lift arms down to the bucket and the tilt ram.
    put(
        &mut loader,
        box_part(1.6, 0.9, 1.4, PAINT, 0.0),
        1.6,
        1.6,
        0.0,
    );
    for side in [-0.55, 0.55] {
        steel_beam(
            &mut loader,
            [1.1, 2.2, side],
            [3.55, 0.75, side],
            0.28,
            FRAME,
        );
    }
    steel_beam(
        &mut loader,
        [1.6, 2.0, 0.0],
        [3.3, 1.35, 0.0],
        0.18,
        0x9a9a92,
    );
    put(
        &mut loader,
        steel_box(1.15, 0.12, 2.8, DARK),
        3.95,
        0.2,
        0.0,
    );
    put(
        &mut loader,
        steel_box(0.14, 1.05, 2.8, DARK),
        3.4,
        0.68,
        0.0,
    );
    for side in [-1.0, 1.0] {
        put(
            &mut loader,
            steel_box(1.1, 0.95, 0.1, DARK),
            3.9,
            0.62,
            side * 1.35,
        );
    }
    for k in 0..6 {
        let tooth_z = -1.15 + f64::from(k) * 0.46;
        put(
            &mut loader,
            steel_box(0.32, 0.08, 0.16, 0x9a9a92),
            4.6,
            0.17,
            tooth_z,
        );
    }
    weather_paint(&mut loader);
    loader.set_rotation_euler(0.0, yaw, 0.0);
    loader.position = DVec3::new(x, -1.79, z);
    adopt_children(group, loader);
}

/// The drill rig's spot on the south apron: x, z and yaw.
const DRILL_RIG: [f64; 3] = [9.0, 70.2, 0.08];
/// The drilled holes run east of the rig along the apron, metres apart.
const DRILL_HOLES: usize = 6;
const DRILL_HOLE_SPACING: f64 = 2.6;

/// `drillRig(group, x, z, yaw)`: a crawler blast-hole drill in fleet yellow on
/// dark tracks: compressor house, cab, and a tall feed mast with its rotation
/// head, rod carousel and dust hood at the collar. Painted in the colours the
/// site's other steel already uses, so it joins their batches.
fn drill_rig(group: &mut Node, x: f64, z: f64, yaw: f64) {
    const YELLOW: u32 = PAINT;
    const DARK: u32 = 0x4a4238;
    const FRAME: u32 = 0x5a564c;
    let mut rig = Node::group("");
    for side in [-1.0, 1.0] {
        put(
            &mut rig,
            steel_box(5.2, 0.95, 0.72, DARK),
            0.0,
            0.48,
            side * 1.2,
        );
        for dx in [-2.2, 2.2] {
            let idler = rotated(cylinder_part(0.42, 0.76, 0x343431, 10), FRAC_PI_2, 0.0, 0.0);
            put(&mut rig, idler, dx, 0.48, side * 1.2);
        }
    }
    put(&mut rig, steel_box(4.8, 0.45, 2.5, FRAME), 0.0, 1.18, 0.0);
    // Compressor and engine house at the back, grille facing aft.
    put(
        &mut rig,
        box_part(2.7, 1.7, 2.4, YELLOW, 0.0),
        -1.0,
        2.25,
        0.0,
    );
    put(
        &mut rig,
        box_part(0.05, 1.15, 1.9, 0x3d413b, 0.0),
        -2.38,
        2.25,
        0.0,
    );
    put(
        &mut rig,
        cylinder_part(0.09, 1.1, 0x7e4a2c, 8),
        -1.6,
        3.6,
        0.8,
    );
    // Cab forward on one side, dark glass all round.
    put(
        &mut rig,
        box_part(1.45, 1.85, 1.3, YELLOW, 0.0),
        1.25,
        2.33,
        -0.58,
    );
    put(
        &mut rig,
        box_part(1.2, 1.0, 1.36, 0x42483c, 0.0),
        1.28,
        2.6,
        -0.58,
    );
    put(
        &mut rig,
        box_part(1.51, 1.0, 1.05, 0x42483c, 0.0),
        1.25,
        2.6,
        -0.58,
    );
    put(
        &mut rig,
        steel_box(1.65, 0.12, 1.5, DARK),
        1.25,
        3.32,
        -0.58,
    );
    // The feed mast, standing on its cradle at the front, braced back to the deck.
    let mast_x = 2.65;
    put(
        &mut rig,
        box_part(0.55, 10.2, 0.6, YELLOW, 0.0),
        mast_x,
        6.0,
        0.55,
    );
    // Lattice bracing zigzags up both sides of the mast.
    for side in [0.2, 0.9] {
        for k in 0..6 {
            let y = 1.4 + f64::from(k) * 1.55;
            let (from, to) = if k % 2 == 0 {
                (-0.26, 0.26)
            } else {
                (0.26, -0.26)
            };
            steel_beam(
                &mut rig,
                [mast_x + from, y, side],
                [mast_x + to, y + 1.55, side],
                0.06,
                FRAME,
            );
        }
    }
    put(
        &mut rig,
        steel_box(0.16, 9.8, 0.2, 0x9a9a92),
        mast_x + 0.34,
        5.9,
        0.55,
    );
    for dz in [0.15, 0.95] {
        steel_beam(
            &mut rig,
            [0.9, 1.4, dz],
            [mast_x - 0.2, 4.4, dz],
            0.16,
            FRAME,
        );
    }
    put(
        &mut rig,
        steel_box(0.75, 0.9, 0.8, 0x4e7d7c),
        mast_x + 0.5,
        7.2,
        0.55,
    );
    put(
        &mut rig,
        cylinder_part(0.3, 3.4, 0x8a4f2e, 10),
        mast_x - 0.45,
        3.9,
        1.15,
    );
    put(
        &mut rig,
        steel_box(0.95, 0.5, 0.95, 0x515f59),
        mast_x + 0.4,
        0.3,
        0.55,
    );
    put(
        &mut rig,
        cylinder_part(0.06, 6.6, 0x5b625b, 6),
        mast_x + 0.45,
        3.85,
        0.55,
    );
    // Hydraulic jacks down to the apron and a hose loop off the mast head.
    for (jx, jz) in [(-2.3, -1.45), (-2.3, 1.45), (2.2, -1.45)] {
        put(&mut rig, steel_box(0.26, 1.2, 0.26, FRAME), jx, 0.6, jz);
        put(&mut rig, steel_box(0.6, 0.08, 0.6, DARK), jx, 0.04, jz);
    }
    steel_beam(
        &mut rig,
        [mast_x - 0.2, 11.0, 0.55],
        [0.6, 3.5, 0.9],
        0.07,
        0x343431,
    );
    weather_paint(&mut rig);
    rig.set_rotation_euler(0.0, yaw, 0.0);
    rig.position = DVec3::new(x, -1.79, z);
    adopt_children(group, rig);
}

/// Where the rig's drilled holes stand on the apron, east of it along its line.
pub(super) fn drill_holes() -> Vec<(f64, f64)> {
    let [x, z, yaw] = DRILL_RIG;
    let (sin, cos) = yaw.sin_cos();
    (0..DRILL_HOLES)
        .map(|i| {
            let along = 4.2 + i as f64 * DRILL_HOLE_SPACING;
            let across = 0.55 + if i % 2 == 0 { 0.0 } else { 1.3 };
            (
                x + along * cos + across * sin,
                z - along * sin + across * cos,
            )
        })
        .collect()
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

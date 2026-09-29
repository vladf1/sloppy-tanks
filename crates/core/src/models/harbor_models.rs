//! Port of `harbor-models.ts`: shipping containers (cover and ship cargo) and
//! strapped cargo crates with their damage stages. Corrugated steel, corner
//! castings, double doors and locking bars share cached meshes.

use std::f64::consts::{FRAC_PI_2, PI};
use std::sync::{Arc, OnceLock};

use glam::{DVec2, DVec3};

use crate::geometry::math::{
    apply_quaternion, js_round, js_sign, quat_from_euler, scale_hex_color,
};
use crate::geometry::{Mesh, Shape, shape_geometry};
use crate::scene::Node;
use crate::sim::math::Random;

use super::harbor_surfaces::steel_box;
use super::house_surfaces::siding_box;
use super::model_primitives::{box_part, material, put, rotated};

/// `Pick<Cover, "w" | "d" | "h" | "color">`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CargoShape {
    pub w: f64,
    pub d: f64,
    pub h: f64,
    pub color: u32,
}

/// `Pick<Cover, "x" | "z" | "w" | "d" | "h" | "color">`: crates also seed their
/// cosmetic randomness from their position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrateShape {
    pub x: f64,
    pub z: f64,
    pub w: f64,
    pub d: f64,
    pub h: f64,
    pub color: u32,
}

/// Node name of each torn-wood decal on a damaged crate.
pub const CARGO_SPLIT: &str = "cargo-split";
const LOCK_SILVER: u32 = 0xc4c4ad;

/// Corner radius of a container's small fittings (ribs, rails, doors, bars).
const FITTING_RADIUS: f64 = 0.025;

/// `shippingContainer(group, c)`: append a container's parts to `group`, long
/// axis along x unless `d > w`.
pub fn shipping_container(group: &mut Node, c: CargoShape) {
    container_parts(group, c, FITTING_RADIUS);
}

/// A container stacked on a moored ship. Seen only across the basin, its fittings
/// are square boxes: rounded ones (108 triangles each instead of 12) made every
/// ship about 131,000 triangles, drawn again for shadows and the water reflection,
/// and bound the harbor's frame time on the GPU for no visible difference.
pub fn ship_container(group: &mut Node, c: CargoShape) {
    container_parts(group, c, 0.0);
}

fn container_parts(group: &mut Node, c: CargoShape, fitting_radius: f64) {
    let along_z = c.d > c.w;
    let width = if along_z { c.d } else { c.w };
    let depth = if along_z { c.w } else { c.d };
    let mut part = |w: f64, h: f64, d: f64, color: u32, x: f64, y: f64, z: f64| {
        let mut mesh = if w * h * d > 1.0 {
            steel_box(w, h, d, color)
        } else {
            box_part(w, h, d, color, fitting_radius)
        };
        if along_z {
            mesh = rotated(mesh, 0.0, FRAC_PI_2, 0.0);
        }
        put(
            group,
            mesh,
            if along_z { z } else { x },
            y,
            if along_z { -x } else { z },
        );
    };
    part(width, c.h, depth, c.color, 0.0, c.h / 2.0, 0.0);
    let dark = scale_hex_color(c.color, 0.67);
    for s in [-1.0, 1.0] {
        let mut x = -width / 2.0 + 0.4;
        while x < width / 2.0 - 0.2 {
            part(
                0.12,
                c.h - 0.35,
                0.09,
                dark,
                x,
                c.h / 2.0,
                s * (depth / 2.0 + 0.025),
            );
            x += 0.55;
        }
        for y in [0.12, c.h - 0.12] {
            part(width + 0.1, 0.18, 0.15, dark, 0.0, y, (s * depth) / 2.0);
            part(0.18, 0.18, depth, dark, (s * width) / 2.0, y, 0.0);
        }
        for z in [-depth / 2.0 + 0.13, depth / 2.0 - 0.13] {
            // Keep corner caps above the roof and its ribs; coplanar tops flicker as the camera moves.
            let post_height = c.h + 0.08;
            part(
                0.22,
                post_height,
                0.22,
                LOCK_SILVER,
                s * (width / 2.0 - 0.1),
                post_height / 2.0,
                z,
            );
        }
        // Recessed double-door panels and silver locking rods on both ends.
        for z in [-depth / 4.0, depth / 4.0] {
            part(
                0.08,
                c.h - 0.55,
                depth / 2.0 - 0.15,
                dark,
                s * (width / 2.0 + 0.025),
                c.h / 2.0,
                z,
            );
            part(
                0.13,
                c.h - 0.6,
                0.08,
                LOCK_SILVER,
                s * (width / 2.0 + 0.08),
                c.h / 2.0,
                z,
            );
            part(0.16, 0.09, 0.5, 0xd6d1b2, s * (width / 2.0 + 0.1), 1.15, z);
        }
        part(
            2.0,
            0.5,
            0.035,
            0xe1d9b7,
            -width * 0.27,
            c.h * 0.65,
            s * (depth / 2.0 + 0.09),
        );
        for i in 0..4 {
            part(
                0.09,
                0.24,
                0.045,
                dark,
                -width * 0.27 - 0.5 + f64::from(i) * 0.3,
                c.h * 0.65,
                s * (depth / 2.0 + 0.12),
            );
        }
    }
    let mut x = -width / 2.0 + 0.4;
    while x < width / 2.0 {
        part(0.12, 0.04, depth - 0.3, dark, x, c.h + 0.015, 0.0);
        x += 0.55;
    }
}

const STRAP: u32 = 0x6a624d;
const STRAP_BASE: u32 = 0x66503a;
const BATTEN: u32 = 0xdec18b;

/// `cargoStack(group, c, damageStage)`: a strapped wooden crate. Cosmetic
/// randomness is stable per crate and never advances the simulation RNG.
pub fn cargo_stack(group: &mut Node, c: CrateShape, damage_stage: u32) {
    let mut rng = Random::new(js_round(c.x * 73_856_093.0 + c.z * 19_349_663.0));
    let broken_strap = if rng.next() < 0.5 { -1.0 } else { 1.0 };
    let curl_side = if rng.next() < 0.5 { -1.0 } else { 1.0 };
    let curl_angle = rng.range(0.28, 0.55);
    put(
        group,
        siding_box(c.w, c.h - 0.22, c.d, c.color),
        0.0,
        (c.h + 0.22) / 2.0,
        0.0,
    );
    // Straps wrap above the lid; flush tops compete with its textured face in the depth buffer.
    let strap_bottom = 0.2;
    let strap_top = c.h + 0.06;
    for x in [-c.w * 0.34, c.w * 0.34] {
        put(
            group,
            box_part(0.25, 0.22, c.d, STRAP_BASE, 0.0),
            x,
            0.11,
            0.0,
        );
        if damage_stage == 2 && js_sign(x) == broken_strap {
            // A broken top band curls up at its free end; the side bands still hold the box.
            for side in [-1.0, 1.0] {
                put(
                    group,
                    box_part(0.15, c.h - 0.2, 0.04, STRAP, 0.0),
                    x,
                    (c.h + 0.2) / 2.0,
                    side * (c.d / 2.0 + 0.02),
                );
                put(
                    group,
                    box_part(0.15, 0.045, c.d * 0.32, STRAP, 0.0),
                    x,
                    strap_top,
                    side * c.d * 0.34,
                );
            }
            let loose_end = rotated(
                box_part(0.15, 0.045, c.d * 0.17, STRAP, 0.0),
                curl_side * curl_angle,
                0.0,
                0.0,
            );
            put(
                group,
                loose_end,
                x,
                strap_top + c.d * 0.035,
                curl_side * c.d * 0.1,
            );
            continue;
        }
        put(
            group,
            box_part(0.15, strap_top - strap_bottom, c.d + 0.04, STRAP, 0.0),
            x,
            (strap_top + strap_bottom) / 2.0,
            0.0,
        );
    }
    for z in [-1.0, 1.0] {
        for y in [0.42, c.h - 0.2] {
            put(
                group,
                siding_box(c.w, 0.2, 0.12, BATTEN),
                0.0,
                y,
                z * (c.d / 2.0 + 0.04),
            );
        }
        let brace = rotated(siding_box(c.w * 0.9, 0.2, 0.12, BATTEN), 0.0, 0.0, 0.58);
        put(group, brace, 0.0, c.h / 2.0, z * (c.d / 2.0 + 0.08));
    }
    if damage_stage > 0 {
        cargo_damage(group, c, damage_stage, &mut rng);
    }
}

/// Shared jagged split outlines; light torn fibres surround a darker, narrower recess.
fn split_geometries() -> &'static [Arc<Mesh>; 4] {
    static SPLITS: OnceLock<[Arc<Mesh>; 4]> = OnceLock::new();
    SPLITS.get_or_init(|| {
        std::array::from_fn(|variant| {
            let mut rng = Random::new(variant as f64 + 179.0);
            let points: Vec<_> = [
                [0.0, -0.5],
                [-0.22, -0.24],
                [-1.0, -0.1],
                [-0.34, -0.06],
                [0.15, 0.22],
                [-0.15, 0.5],
                [0.55, 0.23],
                [0.24, 0.03],
                [0.85, -0.08],
                [0.18, -0.03],
                [0.03, -0.25],
            ]
            .iter()
            .map(|&[x, y]| DVec2::new(x * rng.range(0.65, 1.35), y))
            .collect();
            Arc::new(shape_geometry(&[Shape::from_points(&points)], 12))
        })
    })
}

fn cargo_damage(group: &mut Node, c: CrateShape, stage: u32, rng: &mut Random) {
    let width = if stage == 1 { 0.095 } else { 0.17 };
    let split = |group: &mut Node, rng: &mut Random, at: [f64; 3], length: f64, r: [f64; 3]| {
        let rotation = quat_from_euler(r[0], r[1], r[2]);
        let normal = apply_quaternion(DVec3::Z, rotation);
        let geometry = &split_geometries()[(rng.next() * 4.0).floor() as usize];
        let breadth = width * rng.range(0.75, 1.2);
        for (i, color) in [0xc9a271u32, 0x35291c].into_iter().enumerate() {
            let i = i as f64;
            let mut mesh = Node::mesh(geometry.clone(), material(color, 0.0, 0.9));
            mesh.name = CARGO_SPLIT.into();
            mesh.rotation = rotation;
            mesh.scale = DVec3::new(breadth * if i == 0.0 { 1.9 } else { 1.0 }, length, 1.0);
            // Separate both layers from the wood and each other to avoid flickering.
            put(
                group,
                mesh,
                at[0] + normal.x * i * 0.018,
                at[1] + normal.y * i * 0.018,
                at[2] + normal.z * i * 0.018,
            );
        }
    };
    // Arguments draw in call order, before each split draws its own variation.
    let x = c.w * rng.range(-0.15, 0.08);
    let z = c.d * rng.range(-0.04, 0.04);
    let length = c.d * rng.range(0.58, 0.8);
    let rz = rng.range(-0.18, 0.18);
    split(
        group,
        rng,
        [x, c.h + 0.025, z],
        length,
        [-FRAC_PI_2, 0.0, rz],
    );
    let x = c.w * rng.range(0.12, 0.24);
    let z = c.d * rng.range(-0.2, 0.2);
    let length = c.d * rng.range(0.28, 0.44);
    let rz = rng.range(-0.55, 0.55);
    split(
        group,
        rng,
        [x, c.h + 0.025, z],
        length,
        [-FRAC_PI_2, 0.0, rz],
    );
    for side in [-1.0, 1.0] {
        let x = c.w * rng.range(-0.2, 0.2);
        let y = c.h * rng.range(0.48, 0.6);
        let length = c.h * rng.range(0.52, 0.7);
        let rz = rng.range(-0.3, 0.3);
        let ry = if side < 0.0 { PI } else { 0.0 };
        split(
            group,
            rng,
            [x, y, side * (c.d / 2.0 + 0.025)],
            length,
            [0.0, ry, rz],
        );
        let y = c.h * rng.range(0.48, 0.6);
        let z = c.d * rng.range(-0.2, 0.2);
        let length = c.h * rng.range(0.52, 0.7);
        let rz = rng.range(-0.3, 0.3);
        split(
            group,
            rng,
            [side * (c.w / 2.0 + 0.025), y, z],
            length,
            [0.0, (side * PI) / 2.0, rz],
        );
    }
    if stage == 2 {
        // Lift and twist short lid boards over the split, exposing their raw edges.
        let board_count = if rng.next() < 0.5 { 1 } else { 2 };
        for i in 0..board_count {
            let side = if i == 0 { -1.0 } else { 1.0 };
            let mut board = siding_box(c.w * 0.22, 0.11, c.d * 0.42, c.color);
            let (rx, ry) = (rng.range(-0.1, 0.1), rng.range(-0.2, 0.2));
            let rz = side * rng.range(0.08, 0.14);
            board.set_rotation_euler(rx, ry, rz);
            let x = side * c.w * rng.range(0.1, 0.17);
            let z = c.d * rng.range(-0.18, 0.18);
            put(group, board, x, c.h + 0.22, z);
        }
        let splinters = rng.range(2.0, 5.0).floor() as usize;
        for _ in 0..splinters {
            let mut splinter = box_part(0.055, 0.09, c.d * 0.18, 0xd6b17a, 0.0);
            let (ry, rz) = (rng.range(-0.6, 0.6), rng.range(-0.5, 0.5));
            splinter.set_rotation_euler(0.0, ry, rz);
            let x = c.w * rng.range(-0.24, 0.24);
            let z = c.d * rng.range(-0.3, 0.3);
            put(group, splinter, x, c.h + 0.16, z);
        }
    }
}

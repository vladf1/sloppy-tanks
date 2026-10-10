//! Port of `harbor-models.ts`: shipping containers (cover and ship cargo).
//! Corrugated steel, corner castings, double doors and locking bars share cached
//! meshes. Cargo crates are in [`super::crate_model`].

use std::f64::consts::FRAC_PI_2;

use crate::geometry::math::scale_hex_color;
use crate::scene::Node;

use super::harbor_surfaces::steel_box;
use super::model_primitives::{box_part, put, rotated};

/// `Pick<Cover, "w" | "d" | "h" | "color">`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CargoShape {
    pub w: f64,
    pub d: f64,
    pub h: f64,
    pub color: u32,
}

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

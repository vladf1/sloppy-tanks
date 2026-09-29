//! Ports of `quarry-rock-shape.ts` and the shape half of `quarry-barrier-shapes.ts`:
//! the rock and dragon-tooth shapes that rendering and collision share.
//!
//! Shared with `sim::quarry_rock_shape` and `sim::quarry_barrier_shapes`;
//! de-duplicate at integration (the simulation also owns the barrier hulls).

use super::prop_support::Random;
use crate::geometry::math::js_round;

/// A rock's render and collision mesh: f32 positions (a `Float32Array`) and triangle
/// indices.
#[derive(Clone, Debug, PartialEq)]
pub struct RockShape {
    pub positions: Vec<f32>,
    pub indices: Vec<u32>,
}

/// Octagon corners of the unit rock outline before jitter.
const ROCK_CORNERS: [[f64; 2]; 8] = [
    [-0.62, -1.0],
    [0.51, -0.95],
    [1.0, -0.48],
    [0.94, 0.57],
    [0.48, 1.0],
    [-0.61, 0.91],
    [-1.0, 0.43],
    [-0.94, -0.54],
];
/// (height fraction, outline scale) of each sediment shelf ring.
const ROCK_RINGS: [[f64; 2]; 7] = [
    [0.0, 1.0],
    [0.07, 1.0],
    [0.28, 0.96],
    [0.44, 0.94],
    [0.72, 0.89],
    [0.88, 0.84],
    [1.0, 0.68],
];

/// `quarryRockShape(w, h, d, variant)`: broken sediment shelves and an uneven
/// eroded cap.
pub fn quarry_rock_shape(w: f64, h: f64, d: f64, variant: u32) -> RockShape {
    let mut rng = Random::new(812.0 + f64::from(variant));
    let corners: Vec<[f64; 2]> = ROCK_CORNERS
        .iter()
        .map(|&[x, z]| {
            let sx = rng.range(0.89, 1.0);
            let sz = rng.range(0.89, 1.0);
            [x * sx, z * sz]
        })
        .collect();
    let mut outline = Vec::with_capacity(corners.len() * 2);
    for (i, p) in corners.iter().enumerate() {
        let next = corners[(i + 1) % corners.len()];
        let split = rng.range(0.35, 0.65);
        let inset = rng.range(0.91, 1.02);
        outline.push(*p);
        outline.push([
            (p[0] + (next[0] - p[0]) * split) * inset,
            (p[1] + (next[1] - p[1]) * split) * inset,
        ]);
    }
    let tilt_x = rng.range(-0.11, 0.11);
    let tilt_z = rng.range(-0.09, 0.09);
    let corner_heights: Vec<f64> = corners.iter().map(|_| rng.range(0.87, 0.98)).collect();
    let crowns: Vec<f64> = (0..outline.len())
        .map(|i| {
            if i % 2 == 0 {
                corner_heights[i / 2]
            } else {
                (corner_heights[i / 2] + corner_heights[(i / 2 + 1) % 8]) / 2.0
            }
        })
        .collect();
    let erosion: Vec<f64> = outline.iter().map(|_| rng.range(-0.025, 0.025)).collect();
    let mut positions: Vec<f64> = Vec::new();
    for [level, scale] in ROCK_RINGS {
        for (side, &[x, z]) in outline.iter().enumerate() {
            let worn_scale =
                scale + (level * 8.0 + side as f64 * 0.7).sin() * erosion[side] * level;
            positions.push((x * w * worn_scale) / 2.0);
            positions.push(if level == 0.0 {
                -0.12
            } else {
                h * level * (crowns[side] + x * tilt_x + z * tilt_z)
            });
            positions.push((z * d * worn_scale) / 2.0);
        }
    }
    let count = outline.len() as u32;
    let mut indices = Vec::new();
    for ring in 0..ROCK_RINGS.len() as u32 - 1 {
        for side in 0..count {
            let a = ring * count + side;
            let b = ring * count + (side + 1) % count;
            indices.extend_from_slice(&[a, a + count, b, b, a + count, b + count]);
        }
    }
    let center = (positions.len() / 3) as u32;
    let cx = w * rng.range(-0.08, 0.08);
    let cz = d * rng.range(-0.08, 0.08);
    positions.extend_from_slice(&[cx, h * 0.96, cz]);
    let cap = (ROCK_RINGS.len() as u32 - 1) * count;
    for i in 0..count {
        indices.extend_from_slice(&[center, cap + (i + 1) % count, cap + i]);
    }
    RockShape {
        positions: positions.iter().map(|&v| v as f32).collect(),
        indices,
    }
}

/// `quarryRockVariant(x, z)`.
pub fn quarry_rock_variant(x: f64, z: f64) -> u32 {
    (js_round(x * 17.0 + z * 31.0).abs() % 97.0) as u32
}

/// A dragon tooth's lifting-crown yaw and the scale of its top relative to its foot
/// (`TOOTH_PROFILES`); a zero top scale is a pointed casting without a lifting eye.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToothProfile {
    pub yaw: f64,
    pub top_scale: f64,
}

const TOOTH_PROFILES: [ToothProfile; 4] = [
    ToothProfile {
        yaw: -0.24,
        top_scale: 0.12,
    },
    ToothProfile {
        yaw: 0.34,
        top_scale: 0.0,
    },
    ToothProfile {
        yaw: -0.1,
        top_scale: 0.16,
    },
    ToothProfile {
        yaw: 0.46,
        top_scale: 0.1,
    },
];

/// `dragonToothVariant(x, z)`.
pub fn dragon_tooth_variant(x: f64, z: f64) -> u32 {
    (js_round(x.abs() * 23.0 + z.abs() * 37.0) % TOOTH_PROFILES.len() as f64) as u32
}

/// `dragonToothProfile(variant)`.
pub fn dragon_tooth_profile(variant: u32) -> ToothProfile {
    TOOTH_PROFILES[variant as usize % TOOTH_PROFILES.len()]
}

/// `dragonToothPoint`: a unit-box coordinate on the rotated, tapered pyramid, fitted
/// inside `w`/`d` so navigation keeps the exact ground footprint.
pub fn dragon_tooth_point(
    x: f64,
    y: f64,
    z: f64,
    w: f64,
    h: f64,
    d: f64,
    variant: u32,
) -> [f64; 3] {
    let ToothProfile { yaw, top_scale } = dragon_tooth_profile(variant);
    let (cos, sin) = (yaw.cos(), yaw.sin());
    let scale = (1.0 - (1.0 - top_scale) * (y + 0.5)) / (cos.abs() + sin.abs());
    [
        (x * cos + z * sin) * w * scale,
        y * h,
        (z * cos - x * sin) * d * scale,
    ]
}

/// One steel hedgehog I-section (`HEDGEHOG_BEAMS`): its length and Euler x/z tilt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HedgehogBeam {
    pub length: f64,
    pub rx: f64,
    pub rz: f64,
}

pub const HEDGEHOG_BEAMS: [HedgehogBeam; 3] = [
    HedgehogBeam {
        length: 3.1,
        rx: 0.0,
        rz: std::f64::consts::FRAC_PI_4,
    },
    HedgehogBeam {
        length: 3.1,
        rx: 0.0,
        rz: -std::f64::consts::FRAC_PI_4,
    },
    HedgehogBeam {
        length: 3.1,
        rx: std::f64::consts::FRAC_PI_2,
        rz: 0.0,
    },
];

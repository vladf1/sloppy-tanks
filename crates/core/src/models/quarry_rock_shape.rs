//! Port of `quarry-rock-shape.ts`: the shared render/collision mesh of quarry
//! rocks, with broken sediment shelves and an uneven eroded cap. The simulation
//! builds rock colliders from the same positions and indices.

use super::pending_scenery::Random;

/// Rings of the rock profile: height fraction and outline scale, toe to cap.
const RINGS: [[f64; 2]; 7] = [
    [0.0, 1.0],
    [0.07, 1.0],
    [0.28, 0.96],
    [0.44, 0.94],
    [0.72, 0.89],
    [0.88, 0.84],
    [1.0, 0.68],
];

/// Octagonal footprint corners before jitter.
const CORNERS: [[f64; 2]; 8] = [
    [-0.62, -1.0],
    [0.51, -0.95],
    [1.0, -0.48],
    [0.94, 0.57],
    [0.48, 1.0],
    [-0.61, 0.91],
    [-1.0, 0.43],
    [-0.94, -0.54],
];

/// `quarryRockShape(w, h, d, variant)`: stored (f32) positions and triangle indices.
/// The first 16 vertices are the buried toe ring, then six more rings and a cap
/// centre.
#[derive(Clone, Debug, PartialEq)]
pub struct RockShape {
    pub positions: Vec<f32>,
    pub indices: Vec<u32>,
}

pub fn quarry_rock_shape(w: f64, h: f64, d: f64, variant: u32) -> RockShape {
    let mut rng = Random::new(812.0 + f64::from(variant));
    let corners: Vec<[f64; 2]> = CORNERS
        .iter()
        .map(|&[x, z]| {
            let x = x * rng.range(0.89, 1.0);
            [x, z * rng.range(0.89, 1.0)]
        })
        .collect();
    let mut outline = Vec::with_capacity(16);
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
    let mut positions = Vec::with_capacity((RINGS.len() * outline.len() + 1) * 3);
    for [level, scale] in RINGS {
        for (side, &[x, z]) in outline.iter().enumerate() {
            // Local erosion breaks a few faces, without wrapping every rock in identical steps.
            let worn_scale =
                scale + (level * 8.0 + side as f64 * 0.7).sin() * erosion[side] * level;
            positions.push(((x * w * worn_scale) / 2.0) as f32);
            positions.push(if level == 0.0 {
                -0.12
            } else {
                (h * level * (crowns[side] + x * tilt_x + z * tilt_z)) as f32
            });
            positions.push(((z * d * worn_scale) / 2.0) as f32);
        }
    }
    let count = outline.len() as u32;
    let mut indices = Vec::new();
    for ring in 0..RINGS.len() as u32 - 1 {
        for side in 0..count {
            let a = ring * count + side;
            let b = ring * count + (side + 1) % count;
            indices.extend([a, a + count, b, b, a + count, b + count]);
        }
    }
    let center = (positions.len() / 3) as u32;
    let cx = w * rng.range(-0.08, 0.08);
    let cz = d * rng.range(-0.08, 0.08);
    positions.extend([cx as f32, (h * 0.96) as f32, cz as f32]);
    let cap = (RINGS.len() as u32 - 1) * count;
    for i in 0..count {
        indices.extend([center, cap + (i + 1) % count, cap + i]);
    }
    RockShape { positions, indices }
}

/// `quarryRockVariant(x, z)`: a stable rock variant for a cover position.
pub fn quarry_rock_variant(x: f64, z: f64) -> u32 {
    (crate::geometry::math::js_round(x * 17.0 + z * 31.0).abs() % 97.0) as u32
}

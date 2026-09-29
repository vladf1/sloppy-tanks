//! Shared render/collision mesh for quarry rocks: broken sediment shelves and an uneven
//! eroded cap. Positions are f32 like the Float32Array the renderer and Rapier consumed.

use super::math::{Random, js_round};

pub struct RockShape {
    /// xyz triples.
    pub positions: Vec<f32>,
    /// Triangle vertex indices, three per face.
    pub indices: Vec<u32>,
}

pub fn quarry_rock_shape(w: f64, h: f64, d: f64, variant: u32) -> RockShape {
    let mut rng = Random::new(812.0 + variant as f64);
    let corners: Vec<[f64; 2]> = [
        [-0.62, -1.0],
        [0.51, -0.95],
        [1.0, -0.48],
        [0.94, 0.57],
        [0.48, 1.0],
        [-0.61, 0.91],
        [-1.0, 0.43],
        [-0.94, -0.54],
    ]
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
    let rings = [
        [0.0, 1.0],
        [0.07, 1.0],
        [0.28, 0.96],
        [0.44, 0.94],
        [0.72, 0.89],
        [0.88, 0.84],
        [1.0, 0.68],
    ];
    let mut positions: Vec<f64> = Vec::new();
    for [level, scale] in rings {
        for (side, &[x, z]) in outline.iter().enumerate() {
            // Local erosion breaks a few faces, without wrapping every rock in identical steps.
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
    for ring in 0..rings.len() as u32 - 1 {
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
    let cap = (rings.len() as u32 - 1) * count;
    for i in 0..count {
        indices.extend_from_slice(&[center, cap + (i + 1) % count, cap + i]);
    }
    RockShape {
        positions: positions.into_iter().map(|value| value as f32).collect(),
        indices,
    }
}

pub fn quarry_rock_variant(x: f64, z: f64) -> u32 {
    (js_round(x * 17.0 + z * 31.0).abs() % 97.0) as u32
}

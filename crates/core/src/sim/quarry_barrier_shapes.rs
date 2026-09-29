//! Dragon's teeth and steel hedgehog shapes, shared by rendering and physics.

use std::f64::consts::PI;

use super::math::js_round;
use super::types::CoverKind;

pub const DRAGON_TOOTH_SCALE: f64 = 0.9;
pub const DRAGON_TOOTH_MASS: f64 =
    9.6 * (DRAGON_TOOTH_SCALE * DRAGON_TOOTH_SCALE * DRAGON_TOOTH_SCALE);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToothProfile {
    pub yaw: f64,
    pub top_scale: f64,
}

// Narrow lifting crowns and pointed castings, with imperfect field orientation.
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

pub fn dragon_tooth_variant(x: f64, z: f64) -> usize {
    (js_round(x.abs() * 23.0 + z.abs() * 37.0) as i64 % TOOTH_PROFILES.len() as i64) as usize
}

pub fn dragon_tooth_profile(variant: usize) -> ToothProfile {
    TOOTH_PROFILES[variant % TOOTH_PROFILES.len()]
}

/// Unit-box coordinates become the same rotated pyramid in rendering and physics.
/// Fit the rotation inside w/d so navigation retains the exact ground footprint.
pub fn dragon_tooth_point(
    x: f64,
    y: f64,
    z: f64,
    w: f64,
    h: f64,
    d: f64,
    variant: usize,
) -> [f64; 3] {
    let ToothProfile { yaw, top_scale } = dragon_tooth_profile(variant);
    let cos = yaw.cos();
    let sin = yaw.sin();
    let scale = (1.0 - (1.0 - top_scale) * (y + 0.5)) / (cos.abs() + sin.abs());
    [
        (x * cos + z * sin) * w * scale,
        y * h,
        (z * cos - x * sin) * d * scale,
    ]
}

pub struct HedgehogBeam {
    pub length: f64,
    pub rx: f64,
    pub rz: f64,
}

pub const HEDGEHOG_BEAMS: [HedgehogBeam; 3] = [
    HedgehogBeam {
        length: 3.1,
        rx: 0.0,
        rz: PI / 4.0,
    },
    HedgehogBeam {
        length: 3.1,
        rx: 0.0,
        rz: -PI / 4.0,
    },
    HedgehogBeam {
        length: 3.1,
        rx: PI / 2.0,
        rz: 0.0,
    },
];

/// Convex hull points (xyz triples, f32 like the physics input) matching sloping concrete
/// and open steel shapes, so shots pass through visible gaps. Each steel flange/web is
/// convex, preserving the open gaps on a dynamic body.
pub fn quarry_barrier_hulls(
    kind: CoverKind,
    w: f64,
    h: f64,
    d: f64,
    variant: usize,
) -> Vec<Vec<f32>> {
    if kind == CoverKind::Teeth {
        let mut points = Vec::with_capacity(24);
        for y in [-0.5, 0.5] {
            for x in [-0.5, 0.5] {
                for z in [-0.5, 0.5] {
                    points.extend(dragon_tooth_point(x, y, z, w, h, d, variant).map(|v| v as f32));
                }
            }
        }
        return vec![points];
    }
    let mut hulls = Vec::with_capacity(9);
    for beam in &HEDGEHOG_BEAMS {
        for [offset, width, depth] in [[0.0, 0.12, 0.44], [-0.22, 0.1, 0.52], [0.22, 0.1, 0.52]] {
            let mut vertices = Vec::with_capacity(24);
            for z in [-depth / 2.0, depth / 2.0] {
                for y in [-beam.length / 2.0, beam.length / 2.0] {
                    for x in [-width / 2.0, width / 2.0] {
                        let bx = x + offset;
                        let by = y * beam.rx.cos() - z * beam.rx.sin();
                        let bz = y * beam.rx.sin() + z * beam.rx.cos();
                        vertices.push(((bx * beam.rz.cos() - by * beam.rz.sin()) * w / 2.9) as f32);
                        vertices.push(
                            (((bx * beam.rz.sin() + by * beam.rz.cos() + 1.3) * h) / 2.7 - h / 2.0)
                                as f32,
                        );
                        vertices.push(((bz * d) / 3.2) as f32);
                    }
                }
            }
            hulls.push(vertices);
        }
    }
    hulls
}

//! Senda's clipped street planting as low-poly solids rather than leaf cards: rounded
//! shrubs swelling in two or three tiers and clipped cones, each a lathe of a few sides
//! shaded smooth, its leaves' tint varying from its shaded foot to its sunlit top and
//! from vertex to vertex, some with a scatter of orange or red blossom over their tops.
//! Each is its own small mesh in world space, painted in its vertices, so a cell of them
//! merges with the other painted parts into one draw.

use std::sync::Arc;

use glam::DVec2;

use crate::geometry::Mesh;
use crate::geometry::math::hex_to_linear;
use crate::scene::Node;

use super::batching::vertex_material;
use super::model_primitives::{paint, shadowed};

/// A shrub's form: its profile from foot to crown as (radius, height) shares of its
/// size, each ledge where a tier's skirt tucks in under the next.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum ShrubForm {
    /// A clipped mound in two tiers.
    Mound,
    /// A clipped cone (a juniper or boxwood spire) in three tiers.
    Cone,
}

const MOUND: [(f64, f64); 6] = [
    (0.8, 0.0),
    (1.0, 0.24),
    (0.82, 0.48),
    (0.86, 0.54),
    (0.55, 0.84),
    (0.22, 0.98),
];
const CONE: [(f64, f64); 7] = [
    (0.75, 0.0),
    (1.0, 0.16),
    (0.66, 0.38),
    (0.74, 0.43),
    (0.46, 0.66),
    (0.5, 0.7),
    (0.2, 0.92),
];

/// Sides round each lathe, and how far each ring's vertices wander in and out (a share
/// of the radius) so no two shrubs show the same clean solid.
const SIDES: usize = 10;
const RING_WANDER: f64 = 0.1;
/// Tint from the foot up: this share of the leaves' colour at the foot, rising by this
/// much to the crown; and each vertex's own swing either way.
const FOOT_SHADE: f64 = 0.62;
const CROWN_LIFT: f64 = 0.5;
const VERTEX_SWING: f64 = 0.12;
/// Blossom over a flowering shrub's upper half: the share of its vertices there that
/// carry it.
const BLOSSOM_SHARE: f64 = 0.3;
/// How far up (share of its height) the blossom starts, and how much of a vertex's
/// colour it takes: speckled among the leaves, not a solid cap.
const BLOSSOM_FROM: f64 = 0.6;
const BLOSSOM_MIX: f64 = 0.65;

/// One shrub of `form` at `at` on ground `base` metres up, `radius` across its widest
/// tier and `height` tall, in `leaves` (sRGB), with `blossom` scattered over its top if
/// any; `seed` varies its outline and tint.
#[allow(clippy::too_many_arguments)]
pub(super) fn rounded_shrub(
    group: &mut Node,
    (at, base): (DVec2, f64),
    (radius, height): (f64, f64),
    form: ShrubForm,
    leaves: u32,
    blossom: Option<u32>,
    seed: f64,
) {
    let profile: &[(f64, f64)] = match form {
        ShrubForm::Mound => &MOUND,
        ShrubForm::Cone => &CONE,
    };
    let turn = hash(seed) * std::f64::consts::TAU;
    let mut positions: Vec<f64> = Vec::with_capacity((profile.len() * SIDES + 1) * 3);
    let mut shares: Vec<f64> = Vec::new();
    for (ring, &(r, h)) in profile.iter().enumerate() {
        for side in 0..SIDES {
            let angle = turn + side as f64 / SIDES as f64 * std::f64::consts::TAU;
            let wander = 1.0
                + RING_WANDER * (hash(seed + ring as f64 * 7.3 + side as f64 * 1.9) - 0.5) * 2.0;
            let reach = radius * r * wander;
            positions.extend([
                at.x + angle.cos() * reach,
                base + height * h,
                at.y + angle.sin() * reach,
            ]);
            shares.push(h);
        }
    }
    let apex = positions.len() / 3;
    positions.extend([at.x, base + height, at.y]);
    shares.push(1.0);
    let mut indices: Vec<u32> = Vec::new();
    for ring in 0..profile.len() - 1 {
        for side in 0..SIDES {
            let next = (side + 1) % SIDES;
            let (a, b) = ((ring * SIDES + side) as u32, (ring * SIDES + next) as u32);
            let (c, d) = (
                ((ring + 1) * SIDES + next) as u32,
                ((ring + 1) * SIDES + side) as u32,
            );
            indices.extend([a, d, c, a, c, b]);
        }
    }
    let top = (profile.len() - 1) * SIDES;
    for side in 0..SIDES {
        let next = (side + 1) % SIDES;
        indices.extend([(top + side) as u32, apex as u32, (top + next) as u32]);
    }
    let count = positions.len() / 3;
    let normals = vec![0.0; count * 3];
    let uvs = vec![0.0; count * 2];
    let mut mesh = Mesh::from_f64(&positions, &normals, &uvs, Some(indices));
    mesh.compute_vertex_normals();
    let green = hex_to_linear(leaves);
    let flower = blossom.map(hex_to_linear);
    mesh.colors = shares
        .iter()
        .enumerate()
        .map(|(k, &h)| {
            let swing = 1.0 + VERTEX_SWING * (hash(seed + k as f64 * 0.37) - 0.5) * 2.0;
            let light = (FOOT_SHADE + CROWN_LIFT * h) * swing;
            let color = match flower {
                Some(flower)
                    if h > BLOSSOM_FROM && hash(seed * 1.3 + k as f64 * 2.1) < BLOSSOM_SHARE =>
                {
                    std::array::from_fn(|i| green[i] + (flower[i] - green[i]) * BLOSSOM_MIX)
                }
                _ => green,
            };
            color.map(|c| (c * light) as f32)
        })
        .collect();
    group
        .children
        .push(shadowed(Arc::new(mesh), vertex_material(&paint(0xffffff))));
}

/// A fraction from 0 to 1 that follows from `a` alone.
fn hash(a: f64) -> f64 {
    ((a * 12.9898).sin() * 43_758.545).fract().abs()
}

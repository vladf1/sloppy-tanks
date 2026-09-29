//! Port of `quarry-ramp.ts`: the haul ramp climbing the east cut from the pit floor
//! to the first shelf. The deck rises northward against the wall; a rock berm
//! guards the open edge and fill slopes bury into the apron. Scenery only: no
//! collider or navigation.

use std::f64::consts::PI;

use crate::geometry::Mesh;

use super::quarry_soil::QUARRY_TERRAIN_EXTENT;
use crate::geometry::math::{js_hypot, lerp, smoothstep};
use crate::sim::math::{Random, clamp};

/// `QUARRY_RAMP`: the ramp's authored extents.
pub struct QuarryRamp {
    /// Apron floor level beyond the playable boundary.
    pub floor: f64,
    /// Landing height, a few centimetres proud of the lowest stretch of shelf.
    pub crest: f64,
    /// North edge of the landing, where the berm turns along the end.
    pub z_crest: f64,
    /// Where the landing ends and the grade begins.
    pub z_landing: f64,
    /// Where the deck meets the apron floor.
    pub z_foot: f64,
    /// Berm centreline along the open (west) edge of the deck.
    pub x_berm: f64,
    /// Past the shelf lip the landing tucks under; the wall hides the grade.
    pub x_inner: f64,
    /// Grid bounds, including the buried toes of every fill slope.
    pub x0: f64,
    pub x1: f64,
    pub z0: f64,
    pub z1: f64,
}

pub const QUARRY_RAMP: QuarryRamp = QuarryRamp {
    floor: 0.008 - 1.8,
    crest: 1.9,
    z_crest: 33.0,
    z_landing: 38.0,
    z_foot: 66.0,
    x_berm: 72.2,
    x_inner: 80.9,
    x0: 66.0,
    x1: 84.5,
    z0: 26.0,
    z1: 68.0,
};

const BERM_HALF: f64 = 0.6;
const BERM_HEIGHT: f64 = 0.55;
/// Rise per metre of the loose fill flanks, near the angle of repose.
const FILL: f64 = 0.8;
const RUTS: [f64; 2] = [74.3, 76.9];

fn deck_height(z: f64) -> f64 {
    let r = &QUARRY_RAMP;
    if z >= r.z_foot {
        return r.floor - (z - r.z_foot) * 0.3;
    }
    let t = clamp((z - r.z_landing) / (r.z_foot - r.z_landing), 0.0, 1.0);
    lerp(r.crest, r.floor, smoothstep(t, 0.0, 1.0))
}

/// `quarryRampHeight(x, z)`: surface elevation including the berm and fill slopes;
/// buried points clamp below the apron.
pub fn quarry_ramp_height(x: f64, z: f64) -> f64 {
    let r = &QUARRY_RAMP;
    let u = x - r.x_berm;
    let v = z - (r.z_crest - BERM_HALF);
    let outside = js_hypot(&[0.0f64.max(-BERM_HALF - u), 0.0f64.max(-BERM_HALF - v)]);
    let bump = |s: f64| 0.0f64.max(1.0 - (s / BERM_HALF) * (s / BERM_HALF));
    let fade = 1.0 - smoothstep(z, r.z_foot - 8.0, r.z_foot);
    let berm = BERM_HEIGHT * fade * bump(u).max(bump(v));
    let relief = if outside > 0.0 {
        (x * 1.7 + z * 0.9).sin() * (z * 2.3 - x).sin() * 0.09
    } else {
        0.0
    };
    let y = deck_height(z.max(r.z_crest)) + berm - outside * FILL + relief
        - 0.0f64.max(x - r.x_inner) * 2.5;
    y.max(r.floor - 0.25)
}

fn ramp_color(x: f64, z: f64, y: f64) -> [f64; 3] {
    let r = &QUARRY_RAMP;
    let deck = x > r.x_berm + BERM_HALF && z > r.z_crest;
    if !deck {
        // Loose sandstone spoil: warmer than the apron, fading into it at the toe.
        let lift = smoothstep(y - r.floor, 0.0, 1.4);
        return [1.0 - 0.02 * lift, 1.0 - 0.1 * lift, 1.0 - 0.2 * lift];
    }
    // Compacted haul deck with two darker wheel ruts; it lightens into the apron
    // as it reaches floor level so the shared texture meets without a seam.
    let rut = RUTS
        .iter()
        .map(|rut| {
            let s = (x - rut) / 0.45;
            (-(s * s)).exp()
        })
        .fold(f64::NEG_INFINITY, f64::max);
    let compacted = 0.74 - rut * 0.14;
    let k = smoothstep(y - r.floor, 0.0, 0.6);
    [
        lerp(1.0, compacted, k),
        lerp(1.0, compacted * 0.97, k),
        lerp(1.0, compacted * 0.93, k),
    ]
}

/// `quarryRampGeometry()`: the heightfield deck, sharing the apron texture at world
/// coordinates (UVs map world x/z onto the soil bake).
pub fn quarry_ramp_geometry() -> Mesh {
    let r = &QUARRY_RAMP;
    let cols = ((r.x1 - r.x0) / 0.4).ceil() as u32;
    let rows = ((r.z1 - r.z0) / 0.7).ceil() as u32;
    let mut positions = Vec::new();
    let mut colors = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    for row in 0..=rows {
        let z = r.z0 + ((r.z1 - r.z0) * f64::from(row)) / f64::from(rows);
        for col in 0..=cols {
            let x = r.x0 + ((r.x1 - r.x0) * f64::from(col)) / f64::from(cols);
            let y = quarry_ramp_height(x, z);
            positions.extend([x, y, z]);
            colors.push(ramp_color(x, z, y).map(|c| c as f32));
            uvs.extend([
                x / QUARRY_TERRAIN_EXTENT + 0.5,
                0.5 - z / QUARRY_TERRAIN_EXTENT,
            ]);
            if row < rows && col < cols {
                let a = row * (cols + 1) + col;
                let b = a + cols + 1;
                indices.extend([a, b, a + 1, b, b + 1, a + 1]);
            }
        }
    }
    let mut geometry = Mesh::from_f64(&positions, &[], &uvs, Some(indices));
    geometry.colors = colors;
    geometry.compute_vertex_normals();
    geometry
}

/// A rock placed on the ramp, in world coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RampRock {
    pub x: f64,
    pub z: f64,
    pub size: f64,
    pub rot_y: f64,
}

/// `quarryRampBoulders()`: windrow boulders spaced along the berm crest.
pub fn quarry_ramp_boulders() -> Vec<RampRock> {
    let r = &QUARRY_RAMP;
    let mut rng = Random::new(4417.0);
    let mut boulders = Vec::new();
    let mut z = r.z_crest + 1.5;
    while z < r.z_foot - 7.0 {
        let x = r.x_berm + rng.range(-0.25, 0.25);
        let size = rng.range(0.7, 1.25);
        let rot_y = rng.range(-PI, PI);
        boulders.push(RampRock { x, z, size, rot_y });
        z += rng.range(3.2, 5.4);
    }
    let mut x = r.x_berm + 2.5;
    while x < r.x_inner - 1.0 {
        let size = rng.range(0.7, 1.1);
        let rot_y = rng.range(-1.0, 1.0);
        boulders.push(RampRock {
            x,
            z: r.z_crest - BERM_HALF,
            size,
            rot_y,
        });
        x += rng.range(2.6, 3.4);
    }
    boulders
}

/// `quarryRampSpoil()`: loose spoil chips strewn down the open fill flank, denser
/// toward the toe.
pub fn quarry_ramp_spoil() -> Vec<RampRock> {
    let r = &QUARRY_RAMP;
    let mut rng = Random::new(9023.0);
    (0..70)
        .map(|_| {
            let z = rng.range(r.z_crest - 3.0, r.z_foot - 6.0);
            let height = deck_height(z.max(r.z_crest)) - r.floor;
            let reach = (height + BERM_HEIGHT) / FILL;
            let x = r.x_berm - BERM_HALF - reach * rng.range(0.05, 1.1).sqrt();
            let size = rng.range(0.25, 0.7);
            let rot_y = rng.range(-PI, PI);
            RampRock { x, z, size, rot_y }
        })
        .collect()
}

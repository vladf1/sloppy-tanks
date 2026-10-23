//! Dusty Dig's floor relief, baked into its soil: broad basins, swells and
//! hummocks, and sand drifts across the +x wind with a long windward back and a
//! short slip face. The floor stays the plane the tanks drive on; the relief only
//! shades it. The quarry soil shader used to work it out for every pixel (a fifth
//! of the floor's time in WebKit); it is static and broad, so the bake carries it
//! instead: the damp hollows, bleached crests and paler drift sand as tints, and
//! the raking sun on the slopes as shade.
//!
//! The relief is evaluated on a lattice of [`RELIEF_STEP`] soil pixels (about
//! 41 cm, under a fifth of the slip faces' 2 m) and interpolated between, so it
//! costs a few percent of the soil bake. Like the bake it is a pure function of
//! the pixel, so any row split gives the same pixels.

use super::quarry_soil::{QUARRY_SOIL_SIZE, QUARRY_TERRAIN_EXTENT, bake_quarry_soil};

/// Soil pixels between the relief's lattice points.
pub const RELIEF_STEP: usize = 4;

// The floor's broad basins, swells and hollows: wavelengths and heights, metres.
const BASIN_SPAN: f64 = 46.0;
const BASIN_HEIGHT: f64 = 2.2;
const SWELL_SPAN: f64 = 17.0;
const SWELL_HEIGHT: f64 = 1.1;
const HUMMOCK_SPAN: f64 = 6.5;
const HUMMOCK_HEIGHT: f64 = 0.24;
// Sand drifts: crest spacing along the wind, crest height, the share of each
// wavelength the steep lee face takes, how far the crests bow, the size of the
// fields they gather in and the length of one drift tongue along its crest, metres.
const DRIFT_SPACING: f64 = 6.0;
const DRIFT_HEIGHT: f64 = 0.2;
const DRIFT_LEE: f64 = 0.34;
const DRIFT_BOW: f64 = 8.0;
const DRIFT_FIELD: f64 = 19.0;
const DRIFT_TONGUE: f64 = 7.0;

/// The quarry's sun (`theme_look`: from (-50, 34, 29) toward the origin), the
/// direction toward it.
const SUN_DIRECTION: [f64; 3] = [-0.745_6, 0.507_0, 0.432_4];
/// The share of the floor's light that comes straight from the sun, which the
/// slopes turn toward or away from it; the sky's and the bounce's barely change.
const DIRECT_SHARE: f64 = 0.65;
/// Exponent from a linear multiplier to the sRGB-encoded one that has the same
/// effect (the sRGB curve's power-law fit).
const SRGB_EXPONENT: f64 = 1.0 / 2.2;

/// The bake's soil with its relief, rows `[start, end)`: what the quarry soil
/// texture holds.
pub fn bake_quarry_ground(accum: &[f32], start: usize, end: usize) -> Vec<u8> {
    let mut pixels = bake_quarry_soil(accum, start, end);
    shade_relief(&mut pixels, start);
    pixels
}

/// Multiply the relief's tint and slope shade into soil rows starting at `start`
/// (RGBA8, sRGB color; alpha, the ground kind, is left alone).
pub fn shade_relief(pixels: &mut [u8], start: usize) {
    let size = QUARRY_SOIL_SIZE;
    let rows = pixels.len() / (size * 4);
    if rows == 0 {
        return;
    }
    let columns = (size - 1) / RELIEF_STEP + 2;
    let first = start / RELIEF_STEP;
    let last = (start + rows - 1) / RELIEF_STEP + 1;
    let lattice: Vec<[f32; 3]> = (first..=last)
        .flat_map(|r| {
            (0..columns).map(move |c| relief_multiplier(c * RELIEF_STEP, r * RELIEF_STEP))
        })
        .collect();
    let mut between = vec![[0.0f32; 3]; columns];
    for (offset, row) in pixels.chunks_exact_mut(size * 4).enumerate() {
        let y = start + offset;
        let r = y / RELIEF_STEP - first;
        let fy = (y % RELIEF_STEP) as f32 / RELIEF_STEP as f32;
        // The lattice interpolated to this row once, then along it per pixel.
        let (above, below) = lattice[r * columns..(r + 2) * columns].split_at(columns);
        for ((out, a), b) in between.iter_mut().zip(above).zip(below) {
            *out = std::array::from_fn(|k| a[k] + (b[k] - a[k]) * fy);
        }
        for (cell, pixels) in row.chunks_mut(4 * RELIEF_STEP).enumerate() {
            let (a, b) = (between[cell], between[cell + 1]);
            for (step, pixel) in pixels.chunks_exact_mut(4).enumerate() {
                let fx = step as f32 / RELIEF_STEP as f32;
                for k in 0..3 {
                    let scale = a[k] + (b[k] - a[k]) * fx;
                    pixel[k] = (f32::from(pixel[k]) * scale).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
}

/// The sRGB multiplier the relief applies at soil pixel (`col`, `row`).
fn relief_multiplier(col: usize, row: usize) -> [f32; 3] {
    let to_world =
        |i: usize| (i as f64 / (QUARRY_SOIL_SIZE - 1) as f64 - 0.5) * QUARRY_TERRAIN_EXTENT;
    let relief = soil_relief(to_world(col), to_world(row));
    // Hollows hold damp, darker fines; crests are bleached, paler sand.
    let hollow = smooth(-0.2, -1.4, relief.height);
    let crest = smooth(0.3, 1.4, relief.height);
    let value = (1.0 - 0.14 * hollow) * (1.0 + 0.07 * crest);
    let damp = [0.96, 0.93, 0.91];
    // Drift sand is finer and paler than the worked ground, palest on the crests.
    let drift = [
        mix(1.04, 1.1, relief.drift),
        mix(1.02, 1.07, relief.drift),
        mix(0.98, 1.01, relief.drift),
    ];
    // The raking sun lights the slopes turned toward it and leaves the slip faces
    // in shade (the normal leans against the slope).
    let [sx, sz] = relief.slope;
    let [lx, ly, lz] = SUN_DIRECTION;
    let facing = (ly - sx * lx - sz * lz) / (1.0 + sx * sx + sz * sz).sqrt() / ly;
    let sun = 1.0 + DIRECT_SHARE * (facing - 1.0);
    std::array::from_fn(|k| {
        let linear = value
            * mix(1.0, damp[k], 0.6 * hollow)
            * mix(1.0, drift[k], 0.8 * relief.drift_field)
            * sun;
        linear.max(0.0).powf(SRGB_EXPONENT) as f32
    })
}

struct Relief {
    /// Height (metres) of the broad relief and its slope (d/dx, d/dz).
    height: f64,
    slope: [f64; 2],
    /// How far up a drift the point lies (0 trough, 1 crest), and how much drift
    /// sand covers it.
    drift: f64,
    drift_field: f64,
}

/// The floor's shaded relief at world x/z.
fn soil_relief(x: f64, z: f64) -> Relief {
    let basin = value_grad(x / BASIN_SPAN - 4.3, z / BASIN_SPAN + 1.9);
    let swell = value_grad(x / SWELL_SPAN + 3.1, z / SWELL_SPAN + 7.7);
    let hummock = value_grad(x / HUMMOCK_SPAN + 11.3, z / HUMMOCK_SPAN + 2.9);
    let mut height = (basin[0] - 0.5) * BASIN_HEIGHT
        + (swell[0] - 0.5) * SWELL_HEIGHT
        + (hummock[0] - 0.5) * HUMMOCK_HEIGHT;
    let mut slope = [0, 1].map(|k| {
        basin[k + 1] * (BASIN_HEIGHT / BASIN_SPAN)
            + swell[k + 1] * (SWELL_HEIGHT / SWELL_SPAN)
            + hummock[k + 1] * (HUMMOCK_HEIGHT / HUMMOCK_SPAN)
    });
    // Drifts: crests run across the wind, bowing and wandering as they go, with
    // a long gentle back up from the windward side and a short steep slip face.
    let bow = value_grad(z / 13.0 + 2.3, x / 37.0 - 1.9);
    let u = (x + (bow[0] - 0.5) * DRIFT_BOW) / DRIFT_SPACING;
    let du = [
        (1.0 + bow[2] * DRIFT_BOW / 37.0) / DRIFT_SPACING,
        bow[1] * DRIFT_BOW / 13.0 / DRIFT_SPACING,
    ];
    let phase = u - u.floor();
    let back = 1.0 - DRIFT_LEE;
    let on_lee = phase > back;
    let t = if on_lee {
        (1.0 - phase) / DRIFT_LEE
    } else {
        phase / back
    };
    let rise = t * t * (3.0 - 2.0 * t);
    let rate = 6.0 * t * (1.0 - t) * if on_lee { -1.0 / DRIFT_LEE } else { 1.0 / back };
    // Fields of drift sand come and go; each crest has its own size, and the
    // crests break into tongues that swell and die out along their length.
    let field = smooth(
        0.5,
        0.8,
        value(x / DRIFT_FIELD + 7.1, z / DRIFT_FIELD - 3.3),
    );
    let tongue = smooth(0.3, 0.75, value(u.floor() * 3.7, z / DRIFT_TONGUE));
    let crest_size = (0.5 + 0.7 * hash(u.floor(), 5.0)) * tongue;
    let amount = DRIFT_HEIGHT * field * crest_size;
    height += rise * amount;
    slope[0] += du[0] * rate * amount;
    slope[1] += du[1] * rate * amount;
    Relief {
        height,
        slope,
        drift: rise,
        drift_field: field,
    }
}

fn mix(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Smoothstep from `e0` to `e1`, either way round.
fn smooth(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A hash in [0, 1) of an integer lattice cell (an integer mix: the lattice
/// takes most of the relief's time).
fn hash(x: f64, y: f64) -> f64 {
    let mut h =
        (x as i32 as u32).wrapping_mul(0x9e37_79b1) ^ (y as i32 as u32).wrapping_mul(0x85eb_ca77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^= h >> 15;
    f64::from(h) / 4_294_967_296.0
}

/// Smooth value noise in [0, 1) on a unit lattice.
fn value(x: f64, y: f64) -> f64 {
    value_grad(x, y)[0]
}

/// Smooth value noise and its gradient: (value, d/dx, d/dy).
fn value_grad(x: f64, y: f64) -> [f64; 3] {
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = (x - ix, y - iy);
    let (ux, uy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (dx, dy) = (6.0 * fx * (1.0 - fx), 6.0 * fy * (1.0 - fy));
    let a = hash(ix, iy);
    let b = hash(ix + 1.0, iy);
    let c = hash(ix, iy + 1.0);
    let d = hash(ix + 1.0, iy + 1.0);
    let k = a - b - c + d;
    [
        a + (b - a) * ux + (c - a) * uy + k * ux * uy,
        dx * ((b - a) + k * uy),
        dy * ((c - a) + k * ux),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Any band split gives the same pixels, as the soil bake's own rows do.
    #[test]
    fn relief_is_independent_of_the_band_split() {
        let size = QUARRY_SOIL_SIZE;
        let rows = 11;
        let base: Vec<u8> = (0..rows * size * 4)
            .map(|i| (i * 37 % 200 + 30) as u8)
            .collect();
        let mut whole = base.clone();
        shade_relief(&mut whole, 1001);
        let mut split = base.clone();
        let (upper, lower) = split.split_at_mut(5 * size * 4);
        shade_relief(upper, 1001);
        shade_relief(lower, 1006);
        assert_eq!(whole, split);
    }

    /// The relief only shades: a tint and slope shade near one, alpha untouched.
    #[test]
    fn relief_shades_within_bounds_and_keeps_alpha() {
        let size = QUARRY_SOIL_SIZE;
        let mut pixels = vec![160u8; 8 * size * 4];
        shade_relief(&mut pixels, 640);
        let mut lowest = 255;
        let mut highest = 0;
        for pixel in pixels.chunks_exact(4) {
            assert_eq!(pixel[3], 160);
            lowest = lowest.min(pixel[0]);
            highest = highest.max(pixel[0]);
        }
        assert!(lowest >= 120 && highest <= 200, "{lowest}..{highest}");
        assert!(
            highest - lowest >= 4,
            "the relief shows: {lowest}..{highest}"
        );
    }

    /// The drift slip faces turn from the sun (darker) and their backs toward it.
    #[test]
    fn drifts_shade_their_slip_faces() {
        let mut darker = 0;
        let mut lighter = 0;
        for i in 0..40_000 {
            let (x, z) = (
                -100.0 + (i % 4000) as f64 * 0.05,
                -90.0 + (i / 4000) as f64 * 20.0,
            );
            let r = soil_relief(x, z);
            if r.drift_field > 0.5 {
                let [sx, sz] = r.slope;
                let [lx, ly, lz] = SUN_DIRECTION;
                let facing = (ly - sx * lx - sz * lz) / (1.0 + sx * sx + sz * sz).sqrt() / ly;
                if facing < 0.97 {
                    darker += 1;
                } else if facing > 1.03 {
                    lighter += 1;
                }
            }
        }
        assert!(darker > 0 && lighter > 0, "{darker} {lighter}");
    }
}

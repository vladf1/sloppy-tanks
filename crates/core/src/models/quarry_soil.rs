//! Port of `quarry-soil.ts` (and its worker): the baked 2048² work-yard soil of
//! Dusty Dig. Pure and deterministic: any row split produces the same pixels, so
//! the page can bake it in bands (workers, or slices between frames) and the
//! result is independent of the split.
//!
//! The bake is heavy (4.2 M pixels). Callers that must keep the menu responsive
//! should bake [`bake_quarry_soil`] in bands; [`quarry_soil_pixels`] bakes it
//! whole and caches it.

use std::sync::OnceLock;

use crate::geometry::math::{js_hypot, lerp, smoothstep};
use crate::sim::math::Random;

use super::quarry_terrain::QUARRY_BANK_TOP;

/// Side of the square quarry terrain and soil bake, in metres.
pub const QUARRY_TERRAIN_EXTENT: f64 = 210.0;
/// Side of the soil bake, in pixels (about 10 cm per pixel).
pub const QUARRY_SOIL_SIZE: usize = 2048;
/// Side of the sand accumulation grid (about 2 m per cell).
pub const ACCUM_CELLS: usize = 105;
const EXTENT: f64 = QUARRY_TERRAIN_EXTENT;

/// The 256² value-noise table (seeded, stored as f32 like the Float32Array).
fn soil_noise() -> &'static [f32] {
    static NOISE: OnceLock<Vec<f32>> = OnceLock::new();
    NOISE.get_or_init(|| {
        let mut seed = Random::new(38012.0);
        (0..256 * 256).map(|_| seed.next() as f32).collect()
    })
}

/// Smooth deterministic value noise, shared by the macro soil and fine aggregate.
fn noise(table: &[f32], x: f64, z: f64) -> f64 {
    let ix = x.floor();
    let iz = z.floor();
    let hash = |a: f64, b: f64| {
        let a = (a as i64 & 255) as usize;
        let b = (b as i64 & 255) as usize;
        f64::from(table[a + b * 256])
    };
    let fx = x - ix;
    let fz = z - iz;
    let u = fx * fx * (3.0 - 2.0 * fx);
    let v = fz * fz * (3.0 - 2.0 * fz);
    lerp(
        lerp(hash(ix, iz), hash(ix + 1.0, iz), u),
        lerp(hash(ix, iz + 1.0), hash(ix + 1.0, iz + 1.0), u),
        v,
    )
}

fn sample_accum(grid: &[f32], x: f64, z: f64) -> f64 {
    let cell = EXTENT / (ACCUM_CELLS - 1) as f64;
    let limit = ACCUM_CELLS as f64 - 1.001;
    let fx = 0.0f64.max(limit.min((x + EXTENT / 2.0) / cell));
    let fz = 0.0f64.max(limit.min((z + EXTENT / 2.0) / cell));
    let i = fx.floor();
    let j = fz.floor();
    let u = fx - i;
    let v = fz - j;
    let (i, j) = (i as usize, j as usize);
    let at = |i: usize, j: usize| f64::from(grid[j * ACCUM_CELLS + i]);
    at(i, j) * (1.0 - u) * (1.0 - v)
        + at(i + 1, j) * u * (1.0 - v)
        + at(i, j + 1) * (1.0 - u) * v
        + at(i + 1, j + 1) * u * v
}

/// `Uint8ClampedArray` element assignment (ECMAScript ToUint8Clamp): clamp, then
/// round half to even.
fn to_uint8_clamp(value: f64) -> u8 {
    if value.is_nan() || value <= 0.0 {
        return 0;
    }
    if value >= 255.0 {
        return 255;
    }
    let floor = value.floor();
    let half = floor + 0.5;
    let rounded = if half < value {
        floor + 1.0
    } else if value < half || floor % 2.0 == 0.0 {
        floor
    } else {
        floor + 1.0
    };
    rounded as u8
}

/// `bakeQuarrySoil(accum, start, end)`: sRGBA rows `[start, end)` of the soil.
/// Each pixel draws two or three values from one seeded stream, so a later band
/// first replays the earlier draws. Alpha carries how gritty the soil is (128 fine
/// sand ... 255 loose gravel) for the world-space detail shader; it never blends.
pub fn bake_quarry_soil(accum: &[f32], start: usize, end: usize) -> Vec<u8> {
    let size = QUARRY_SOIL_SIZE;
    let table = soil_noise();
    let mut pixels = vec![0u8; (end - start) * size * 4];
    let mut rng = Random::new(7391.0);
    for _ in 0..start * size {
        rng.next();
        if rng.next() > 0.975 {
            rng.next();
        }
    }
    for (row, out) in (start..end).zip(pixels.chunks_exact_mut(size * 4)) {
        let z = (row as f64 / (size - 1) as f64 - 0.5) * EXTENT;
        bake_row(accum, table, &mut rng, z, out);
    }
    pixels
}

/// One row of the soil at `z` into `out`, continuing `rng`. A function of its own,
/// called per row: a Wasm engine only swaps in optimized code between calls, so one
/// call baking the whole texture would run in its baseline tier throughout.
#[inline(never)]
fn bake_row(accum: &[f32], table: &[f32], rng: &mut Random, z: f64, out: &mut [u8]) {
    let size = QUARRY_SOIL_SIZE;
    let extent = EXTENT;
    for col in 0..size {
        let x = (col as f64 / (size - 1) as f64 - 0.5) * extent;
        let macro_field = noise(table, x * 0.065, z * 0.065);
        let grit = noise(table, x * 0.75, z * 0.75);
        // Broad irregular sheets: windblown sand where the macro field runs high,
        // exposed stony soil where it runs low. A sine warp plus two fixed diagonal
        // drift bands break any hint of cellular repetition without another octave.
        let warp = (x * 0.021 + 1.7).sin() * (z * 0.023 - 0.6).sin();
        let bands = (x * 0.045 + z * 0.031 + 1.2).sin() * (z * 0.052 - x * 0.013 + 0.4).sin();
        let sand = smoothstep(macro_field + warp * 0.18 + bands * 0.1, 0.54, 0.68);
        let rocky = 1.0 - smoothstep(macro_field - warp * 0.15, 0.27, 0.45);
        // Rounded outer haul loop and a gently wandering east/west crossing: a
        // graded, compacted bed with crisp shoulders and a windrow of loose spill.
        let qx = x.abs() - 39.0;
        let qz = z.abs() - 39.0;
        let loop_distance =
            (js_hypot(&[qx.max(0.0), qz.max(0.0)]) + qx.max(qz).min(0.0) - 12.0).abs();
        let crossing = (z - (x * 0.055).sin() * 1.6).abs();
        let distance = loop_distance.min(crossing);
        let mut road = 0.0;
        let mut windrow = 0.0;
        let mut rut = 0.0;
        if distance < 9.5 {
            let edge = (noise(table, x * 0.42, z * 0.42) - 0.5) * 1.3;
            road = 1.0 - smoothstep(distance + edge, 4.2, 5.5);
            let w = (distance + edge * 0.7 - 6.1) / 0.8;
            windrow = (-w * w).exp();
            // Two-lane dual-wheel haul tracks, pressed darker into the compacted bed.
            let inner = (distance - 0.8) / 0.26;
            let outer = (distance - 3.5) / 0.3;
            rut = (-inner * inner).exp().max((-outer * outer).exp() * 0.8) * road;
        }
        // Trampled work floor: one broad central apron plus two midfield patches
        // between the rock shoulders. Fixed smooth shapes, no extra noise.
        let wear = (1.0 - smoothstep(js_hypot(&[x / 30.0, z / 23.0]), 0.55, 1.0))
            .max(1.0 - smoothstep(js_hypot(&[(x.abs() - 25.0) / 13.0, z / 16.0]), 0.5, 1.0));
        // Beyond the wall the ground falls to the machinery apron: loose fill on
        // the embankment, then a working floor of darker quarry fines.
        let reach = x.abs().max(z.abs());
        let apron = smoothstep(reach, QUARRY_BANK_TOP + 0.2, QUARRY_BANK_TOP + 1.5);
        let fill = apron * (1.0 - smoothstep(reach, 65.0, 67.5));
        let fine = rng.range(-5.0, 5.0) * 0.6;
        let aggregate = if rng.next() > 0.975 {
            rng.range(-22.0, 17.0) * 0.6
        } else {
            0.0
        };
        // sRGB bytes: buff soil -> pale sand -> stony soil -> compacted grey-tan road.
        let (mut r, mut g, mut b) = (186.0, 165.0, 134.0);
        let off_road = 1.0 - road;
        let worn_sand = sand * (1.0 - wear * 0.55) * (1.0 - apron * 0.6);
        r += (213.0 - r) * worn_sand;
        g += (195.0 - g) * worn_sand;
        b += (163.0 - b) * worn_sand;
        let stony = (rocky + fill * 0.7).min(1.0) * off_road;
        r += (168.0 - r) * stony;
        g += (141.0 - g) * stony;
        b += (109.0 - b) * stony;
        r += (170.0 - r) * road;
        g += (155.0 - g) * road;
        b += (131.0 - b) * road;
        r += (198.0 - r) * windrow * 0.55;
        g += (180.0 - g) * windrow * 0.55;
        b += (150.0 - b) * windrow * 0.55;
        r += (171.0 - r) * wear * 0.45 * off_road;
        g += (151.0 - g) * wear * 0.45 * off_road;
        b += (122.0 - b) * wear * 0.45 * off_road;
        // Machinery apron: darker fines, spattered with the same stony patches.
        let working = apron * (1.0 - fill);
        r += (168.0 - r) * working * 0.5;
        g += (150.0 - g) * working * 0.5;
        b += (124.0 - b) * working * 0.5;
        // Wind ripples: sub-metre crests across the prevailing +x wind on open sand.
        let ripple = if worn_sand * off_road > 0.02 {
            (x * 8.3 + (z * 0.9 + x * 0.21).sin() * 2.4 + macro_field * 9.0).sin()
                * worn_sand
                * off_road
                * 3.5
        } else {
            0.0
        };
        let shade =
            (macro_field - 0.5) * 9.0 + (grit - 0.5) * 13.0 + fine + aggregate - rut * 9.0 + ripple;
        // Drifted sand against cover bases and along the quiet outer shoulders.
        let shoulder = smoothstep(reach, 48.0, 58.0) * (1.0 - apron) * off_road;
        let drift = 0.5f64.min(sample_accum(accum, x, z) * 0.55 + shoulder * 0.3);
        r += (216.0 - r) * drift;
        g += (199.0 - g) * drift;
        b += (167.0 - b) * drift;
        // Gravel shows through stony ground and spill; sand and traffic bury it.
        let gravel = 0.0f64.max(1.0f64.min(
            0.5 + stony * 0.45 + windrow * 0.35 - worn_sand * 0.4 - road * 0.25 - drift * 0.5,
        ));
        let i = col * 4;
        out[i] = to_uint8_clamp(r + shade);
        out[i + 1] = to_uint8_clamp(g + shade);
        out[i + 2] = to_uint8_clamp(b + shade);
        out[i + 3] = to_uint8_clamp(128.0 + gravel * 127.0);
    }
}

/// The whole bake for the current layout's drift, computed once and cached.
pub fn quarry_soil_pixels() -> &'static [u8] {
    static PIXELS: OnceLock<Vec<u8>> = OnceLock::new();
    PIXELS
        .get_or_init(|| bake_quarry_soil(super::quarry_terrain::sand_accum(), 0, QUARRY_SOIL_SIZE))
}

#[cfg(test)]
mod tests {
    use super::to_uint8_clamp;

    #[test]
    fn clamped_bytes_round_half_to_even() {
        assert_eq!(to_uint8_clamp(2.5), 2);
        assert_eq!(to_uint8_clamp(3.5), 4);
        assert_eq!(to_uint8_clamp(3.4), 3);
        assert_eq!(to_uint8_clamp(-1.0), 0);
        assert_eq!(to_uint8_clamp(300.0), 255);
        assert_eq!(to_uint8_clamp(254.5), 254);
    }
}

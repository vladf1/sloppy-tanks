//! Port of `quarry-soil.ts` (and its worker): the baked 2048² work-yard soil of
//! Dusty Dig. Pure and deterministic: any row split produces the same pixels, so
//! the page can bake it in bands (workers, or slices between frames) and the
//! result is independent of the split.
//!
//! The bake is heavy (4.2 M pixels), so callers that must keep the menu responsive
//! bake [`bake_quarry_soil`] in bands.

use std::sync::OnceLock;

use crate::geometry::math::{js_hypot, lerp, smoothstep};
use crate::sim::data::ARENA;
use crate::sim::math::Random;
use crate::sim::quarry_layout::quarry_layout;
use crate::sim::types::CoverKind;

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

/// Rock covers' footprints as rounded rectangles: centre x/z, half width and
/// depth, corner radius. The rocks are indestructible, so the shade and stone
/// baked round their feet never outlive them.
fn rock_footprints() -> &'static [[f64; 5]] {
    static ROCKS: OnceLock<Vec<[f64; 5]>> = OnceLock::new();
    ROCKS.get_or_init(|| {
        quarry_layout()
            .into_iter()
            .filter(|cover| cover.kind == CoverKind::Rock)
            .map(|c| [c.x, c.z, c.w / 2.0, c.d / 2.0, c.w.min(c.d) * 0.3])
            .collect()
    })
}

/// How far the shade pooled at a rock's foot and the stones it sheds reach from
/// its footprint, metres.
const ROCK_CONTACT_REACH: f64 = 2.4;
const ROCK_SHED_REACH: f64 = 3.6;
/// The sun's bearing across the floor (`theme_look`'s quarry sun, toward it):
/// the foot shade pools on the side turned from it.
const SUN_BEARING: [f64; 2] = [-0.865, 0.502];
/// How far the foot shade leans away from the sun, metres.
const CONTACT_LEAN: f64 = 1.1;
/// How far a rock's lee drift trails downwind (+x), metres.
const LEE_TAIL: f64 = 3.2;
/// Rows a rock's lee drift and leaning shade can reach beyond its footprint.
const ROCK_ROW_REACH: f64 = ROCK_SHED_REACH + CONTACT_LEAN;

/// The rocks whose footprints come within `reach` of the row at `z`: most rows
/// meet none, so the per-pixel distance below rarely has anything to test.
fn rocks_near_row(z: f64, reach: f64) -> Vec<[f64; 5]> {
    rock_footprints()
        .iter()
        .filter(|&&[_, cz, _, half_d, _]| (z - cz).abs() < half_d + reach)
        .copied()
        .collect()
}

/// Distance (metres) from x/z to the nearest of `rocks`' footprint edges,
/// negative inside, capped at `reach`.
fn rock_distance(rocks: &[[f64; 5]], x: f64, z: f64, reach: f64) -> f64 {
    let mut best = reach;
    for &[cx, cz, half_w, half_d, round] in rocks {
        let qx = (x - cx).abs() - (half_w - round);
        if qx > best + round {
            continue;
        }
        let qz = (z - cz).abs() - (half_d - round);
        let d = js_hypot(&[qx.max(0.0), qz.max(0.0)]) + qx.max(qz).min(0.0) - round;
        best = best.min(d);
    }
    best
}

/// The machines parked on the apron: centre x/z and half extents, metres
/// (the excavator, the haul truck, the drill rig and the wheel loader). The
/// ground they stand on is trodden dark, oil-stained and littered with spill.
const MACHINE_SPOTS: [(f64, f64, f64, f64); 4] = [
    (-24.0, -68.0, 5.0, 4.0),
    (69.0, 18.0, 4.0, 6.5),
    (9.0, 70.2, 4.0, 2.6),
    (-70.6, 29.5, 4.4, 2.8),
];

/// How much a machine's stand marks the ground at (x, z), 0..1: strongest
/// under it, fading out a couple of metres past its footprint.
fn machine_stand(x: f64, z: f64) -> f64 {
    MACHINE_SPOTS
        .iter()
        .map(|&(cx, cz, half_w, half_d)| {
            let d = js_hypot(&[(x - cx) / (half_w + 2.0), (z - cz) / (half_d + 2.0)]);
            1.0 - smoothstep(d, 0.45, 1.0)
        })
        .fold(0.0, f64::max)
}

/// Distance of the apron's haul road from the arena's centre lines, metres.
const HAUL_RING: f64 = 71.6;
/// The haul road's ruts across it: two lanes of dual-wheel tracks, metres.
const HAUL_RING_RUTS: [f64; 4] = [-3.4, -1.1, 1.1, 3.4];

/// Blast patterns set out on the open floor: centre x/z, turn (radians), rows
/// and columns of holes. Mirrored like the layout, clear of cover and roads.
const BLAST_PATTERNS: [(f64, f64, f64, usize, usize); 2] =
    [(9.0, -19.0, 0.25, 3, 4), (-9.0, 19.0, 0.25, 3, 4)];
/// Spacing of a pattern's holes, and the reach of each hole's cuttings, metres.
const BLAST_SPACING: f64 = 3.0;
const CUTTINGS_REACH: f64 = 0.85;

/// A drilled blast pattern at (x, z): (hole collar, cuttings, paint mark), each
/// 0..1. Each hole is a dark collar in a pale ring of rock flour, with a dab
/// of orange marking paint beside it.
fn blast_pattern(x: f64, z: f64) -> (f64, f64, f64) {
    for (cx, cz, turn, rows, columns) in BLAST_PATTERNS {
        let (dx, dz) = (x - cx, z - cz);
        let (sin, cos) = turn.sin_cos();
        let (u, v) = (dx * cos + dz * sin, -dx * sin + dz * cos);
        let (half_u, half_v) = (
            (columns as f64 - 1.0) * BLAST_SPACING / 2.0,
            (rows as f64 - 1.0) * BLAST_SPACING / 2.0,
        );
        if u.abs() > half_u + 1.0 || v.abs() > half_v + 1.0 {
            continue;
        }
        let (gu, gv) = (
            ((u + half_u) / BLAST_SPACING)
                .round()
                .clamp(0.0, columns as f64 - 1.0),
            ((v + half_v) / BLAST_SPACING)
                .round()
                .clamp(0.0, rows as f64 - 1.0),
        );
        let (ou, ov) = (
            u + half_u - gu * BLAST_SPACING,
            v + half_v - gv * BLAST_SPACING,
        );
        let d = js_hypot(&[ou, ov]);
        let collar = 1.0 - smoothstep(d, 0.08, 0.15);
        let cuttings = (1.0 - smoothstep(d, 0.15, CUTTINGS_REACH)).powf(0.7) * (1.0 - collar);
        let paint = 1.0 - smoothstep(js_hypot(&[ou - 0.5, ov + 0.35]), 0.1, 0.16);
        return (collar, cuttings, paint);
    }
    (0.0, 0.0, 0.0)
}

/// Loader tyre tracks across the open floor at (x, z): two wandering paths,
/// each a pair of ruts 1.9 m apart, 0..1 with a pressed tread.
fn loader_tracks(x: f64, z: f64) -> f64 {
    // Each path is a gentle curve across the yard: the point's offset from it
    // over the curve's local stretch approximates the distance.
    let paths = [
        (
            z - (14.0 * (x * 0.043 + 0.7).sin() + 0.22 * x - 6.0),
            14.0 * 0.043 * (x * 0.043 + 0.7).cos() + 0.22,
            1.0,
        ),
        (
            x - (17.0 * (z * 0.037 + 2.1).sin() - 0.18 * z + 9.0),
            17.0 * 0.037 * (z * 0.037 + 2.1).cos() - 0.18,
            1.0,
        ),
        (
            z - (11.0 * (x * 0.051 - 1.3).sin() - 0.3 * x + 28.0),
            11.0 * 0.051 * (x * 0.051 - 1.3).cos() - 0.3,
            0.7,
        ),
    ];
    let tread = 0.75 + 0.25 * ((x + z) * 7.0).sin();
    paths
        .into_iter()
        .map(|(offset, slope, strength)| {
            let across = offset / (1.0 + slope * slope).sqrt();
            let rut = ((across.abs() - 0.95) / 0.28).powi(2);
            (-rut).exp() * tread * strength
        })
        .fold(0.0, f64::max)
}

/// A per-pixel hash in [0, 1) for pebbles: a pure function of the pixel, so it
/// leaves the seeded stream's draw pattern (and band replay) untouched.
fn pixel_hash(col: usize, row: usize) -> f64 {
    let mut h = (col as u32).wrapping_mul(0x9e37_79b1) ^ (row as u32).wrapping_mul(0x85eb_ca77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^= h >> 15;
    f64::from(h) / 4_294_967_296.0
}

/// `Uint8ClampedArray` element assignment (ECMAScript ToUint8Clamp): round half to
/// even, then clamp; the saturating cast clamps to 0..=255 and maps NaN to 0.
fn to_uint8_clamp(value: f64) -> u8 {
    value.round_ties_even() as u8
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
        bake_row(accum, table, &mut rng, row, z, out);
    }
    pixels
}

/// One row of the soil at `z` into `out`, continuing `rng`. A function of its own,
/// called per row: a Wasm engine only swaps in optimized code between calls, so one
/// call baking the whole texture would run in its baseline tier throughout.
#[inline(never)]
fn bake_row(accum: &[f32], table: &[f32], rng: &mut Random, row: usize, z: f64, out: &mut [u8]) {
    let size = QUARRY_SOIL_SIZE;
    let extent = EXTENT;
    let rocks = rocks_near_row(z, ROCK_ROW_REACH);
    for col in 0..size {
        let x = (col as f64 / (size - 1) as f64 - 0.5) * extent;
        // Stretched along the +x wind, so sand sheets and deflated ground lie in
        // streaks behind the obstacles instead of round blotches.
        let macro_field = noise(table, x * 0.034, z * 0.085);
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
        let mut road_edge = 0.0;
        if distance < 9.5 {
            // The grader's line wanders in long, uneven runs with the odd bite
            // where a wheel has broken the shoulder, never a regular scallop that
            // reads as a cast shadow.
            let edge = (noise(table, x * 0.11 + 5.0, z * 0.11 - 2.0) - 0.5) * 0.9
                + (noise(table, x * 0.9, z * 0.9) - 0.5) * 0.25;
            road = 1.0 - smoothstep(distance + edge, 4.45, 4.75);
            // The bladed edge: a thin step of shade where the bed was cut down.
            let cut = (distance + edge - 4.6) / 0.22;
            road_edge = (-cut * cut).exp();
            // The grader's windrow: a ridge of pale spill along each shoulder,
            // its crest broken where tyres have cut through it.
            let w = (distance + edge * 0.7 - 6.0) / 0.95;
            let breach = smoothstep(noise(table, x * 0.19 + 31.0, z * 0.19 - 8.0), 0.18, 0.3);
            windrow = (-w * w).exp() * breach;
            // Two-lane dual-wheel haul tracks, pressed darker into the compacted bed.
            let inner = (distance - 0.8) / 0.26;
            let outer = (distance - 3.5) / 0.3;
            rut = (-inner * inner).exp().max((-outer * outer).exp() * 0.8) * road;
            // Chevron lugs pressed into the ruts by the haul tyres.
            let lugs = ((x + z) * 9.0).sin() * ((x - z) * 9.0).sin();
            rut *= 0.8 + 0.35 * lugs;
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
        g += (196.0 - g) * worn_sand;
        b += (164.0 - b) * worn_sand;
        let stony = (rocky + fill * 0.7).min(1.0) * off_road;
        r += (160.0 - r) * stony;
        g += (136.0 - g) * stony;
        b += (107.0 - b) * stony;
        // The compacted bed: grey crushed-stone fines rolled smooth by the
        // trucks, near the sand's own value so the road reads by its cooler colour,
        // its gravel and its ruts rather than as a shadow across the lane, with
        // tongues of sand blown across it between the wheel paths.
        let blown = smoothstep(
            noise(table, x * 0.06 + 17.0, z * 0.11 - 4.0) + warp * 0.2,
            0.55,
            0.78,
        ) * (1.0 - rut * 1.5).max(0.0);
        let packed = road * (1.0 - blown * 0.6);
        r += (176.0 - r) * packed;
        g += (167.0 - g) * packed;
        b += (151.0 - b) * packed;
        r += (201.0 - r) * windrow * 0.7;
        g += (188.0 - g) * windrow * 0.7;
        b += (163.0 - b) * windrow * 0.7;
        r += (171.0 - r) * wear * 0.45 * off_road;
        g += (151.0 - g) * wear * 0.45 * off_road;
        b += (122.0 - b) * wear * 0.45 * off_road;
        // Machinery apron: darker fines, spattered with the same stony patches.
        let working = apron * (1.0 - fill);
        r += (158.0 - r) * working * 0.6;
        g += (141.0 - g) * working * 0.6;
        b += (118.0 - b) * working * 0.6;
        // The loading floor between the stockpile and the excavator: trodden dark
        // with crushed fines, cut by the loader's ruts running out east along the
        // apron and fading as they go.
        let loading = (1.0
            - smoothstep(js_hypot(&[(x + 32.0) / 17.0, (z + 70.0) / 6.5]), 0.45, 1.0))
            * working;
        r += (142.0 - r) * loading * 0.55;
        g += (126.0 - g) * loading * 0.55;
        b += (104.0 - b) * loading * 0.55;
        // Under and round each parked machine: compacted dark fines, blotched
        // with oil and hydraulic leaks, so the steel stands on worked ground.
        let stand = if working > 0.0 {
            machine_stand(x, z) * working
        } else {
            0.0
        };
        let oil = if stand > 0.0 {
            smoothstep(noise(table, x * 0.7 + 51.0, z * 0.7 - 13.0), 0.55, 0.75) * stand
        } else {
            0.0
        };
        r += (128.0 - r) * stand * 0.5;
        g += (116.0 - g) * stand * 0.5;
        b += (100.0 - b) * stand * 0.5;
        r += (74.0 - r) * oil * 0.55;
        g += (68.0 - g) * oil * 0.55;
        b += (62.0 - b) * oil * 0.55;
        let haul = z - (-70.5 + 2.2 * (x * 0.085 + 0.6).sin());
        let haul_fade = smoothstep(x, -58.0, -48.0) * (1.0 - smoothstep(x, -6.0, 6.0)) * working;
        let haul_ruts = [-1.2, 1.2]
            .into_iter()
            .map(|lane| (-((haul - lane).abs() / 0.3).powi(2)).exp())
            .fold(0.0, f64::max)
            * haul_fade;
        // The apron's haul road rings the pit between the wall and the benches,
        // tying the trucks, the ramp and the loading floor together: a compacted
        // bed with two sets of dual-wheel ruts, wandering a little as it goes.
        // Only the apron carries it, so the arena's pixels skip the work.
        let (haul_bed, ring_ruts) = if working > 0.0 {
            let ring_corner = 14.0;
            let (rx, rz) = (
                x.abs() - (HAUL_RING - ring_corner),
                z.abs() - (HAUL_RING - ring_corner),
            );
            let ring = (rx.max(0.0).powi(2) + rz.max(0.0).powi(2)).sqrt() + rx.max(rz).min(0.0)
                - ring_corner
                + 0.9 * (x * 0.05 + z * 0.037).sin();
            let haul_bed = working * (1.0 - smoothstep(ring.abs(), 3.6, 5.0));
            let ruts = if haul_bed > 0.0 {
                HAUL_RING_RUTS
                    .into_iter()
                    .map(|lane| (-((ring - lane) / 0.34).powi(2)).exp())
                    .fold(0.0, f64::max)
                    * haul_bed
                    * (0.8 + 0.3 * ((x - z) * 6.0).sin() * ((x + z) * 6.0).sin())
            } else {
                0.0
            };
            (haul_bed, ruts)
        } else {
            (0.0, 0.0)
        };
        r += (150.0 - r) * haul_bed * 0.5;
        g += (135.0 - g) * haul_bed * 0.5;
        b += (112.0 - b) * haul_bed * 0.5;
        // Iron-stained ochre ground where one slow field runs high and dark damp
        // hollows where it runs low, away from traffic.
        let stain = noise(table, x * 0.034 + 40.0, z * 0.034 - 17.0);
        let iron = smoothstep(stain + warp * 0.12, 0.6, 0.74) * off_road * (1.0 - worn_sand * 0.5);
        r += (204.0 - r) * iron * 0.3;
        g += (150.0 - g) * iron * 0.3;
        b += (106.0 - b) * iron * 0.3;
        let damp =
            (1.0 - smoothstep(stain - bands * 0.08, 0.27, 0.41)) * (1.0 - worn_sand) * off_road;
        r += (156.0 - r) * damp * 0.28;
        g += (122.0 - g) * damp * 0.28;
        b += (96.0 - b) * damp * 0.28;
        // Beds of loose grey gravel where spill has been graded out.
        let spill = noise(table, x * 0.09 + 11.0, z * 0.09 - 5.0);
        let gravel_bed =
            smoothstep(spill, 0.62, 0.74) * off_road * (1.0 - worn_sand) * (1.0 - apron);
        r += (162.0 - r) * gravel_bed * 0.45;
        g += (150.0 - g) * gravel_bed * 0.45;
        b += (132.0 - b) * gravel_bed * 0.45;
        // Twin tyre ruts of loaders that cut across the open floor between the
        // haul roads, wandering, fading where sand has blown over them.
        let tyres = loader_tracks(x, z) * off_road * (1.0 - worn_sand * 0.6) * (1.0 - apron);
        r += (146.0 - r) * tyres * 0.6;
        g += (124.0 - g) * tyres * 0.6;
        b += (98.0 - b) * tyres * 0.6;
        // Loose stone gathers in clusters a few metres across, with clean sand
        // between them, instead of an even sprinkle.
        let clump = noise(table, x * 0.31 + 23.0, z * 0.31 - 9.0);
        let pebble = pixel_hash(col, row);
        let pebbles = if pebble > 0.84 - gravel_bed * 0.25 - stony * 0.08 - (clump - 0.5) * 0.5 {
            (pixel_hash(row + 7919, col) - 0.55) * (22.0 + gravel_bed * 26.0)
        } else {
            0.0
        };
        // Wind ripples: sub-metre crests across the prevailing +x wind, in
        // patches on open sand rather than combed over every sheet.
        let ripple = if worn_sand * off_road > 0.02 {
            let patch = smoothstep(noise(table, x * 0.11 + 7.0, z * 0.11 - 3.0), 0.42, 0.7);
            (x * 8.3 + (z * 0.9 + x * 0.21).sin() * 2.4 + macro_field * 9.0).sin()
                * worn_sand
                * off_road
                * patch
                * 7.0
        } else {
            0.0
        };
        let shade = (macro_field - 0.5) * 11.0 + (grit - 0.5) * 15.0 + fine + aggregate
            - rut * 30.0
            - road_edge * 14.0
            - tyres * 16.0
            - haul_ruts * 20.0
            - ring_ruts * 18.0
            - loading * 5.0
            + ripple
            + pebbles
            - damp * 4.0;
        // Drifted sand against cover bases and along the quiet outer shoulders.
        // Sand banks up against the inside of the boundary wall.
        let shoulder = (smoothstep(reach, 48.0, 58.0) * 0.6 + smoothstep(reach, 56.0, 59.3) * 0.9)
            * (1.0 - apron)
            * off_road;
        let drift = 0.5f64.min(sample_accum(accum, x, z) * 0.55 + shoulder * 0.3);
        // The foot of the block wall: a dark line where the stone meets the
        // floor, then a band of chips knocked off the blocks.
        let to_wall = ARENA - reach;
        let wall_shade = if to_wall > -0.2 {
            (1.0 - smoothstep(to_wall, 0.0, 0.9)) * (0.7 + 0.6 * noise(table, x * 1.3, z * 1.3))
        } else {
            0.0
        };
        let wall_scree = if to_wall > -0.2 {
            (1.0 - smoothstep(to_wall, 0.5, 2.2))
                * smoothstep(noise(table, x * 0.6 + 9.0, z * 0.6), 0.3, 0.6)
        } else {
            0.0
        };
        r += (216.0 - r) * drift;
        g += (199.0 - g) * drift;
        b += (167.0 - b) * drift;
        // Shade pools at the rocks' feet, where the sky is half hidden, and the
        // stones they shed lie in a ring round them.
        let (rock_edge, shade_edge, lee_edge) = if rocks.is_empty() {
            (ROCK_SHED_REACH, ROCK_SHED_REACH, ROCK_SHED_REACH)
        } else {
            (
                rock_distance(&rocks, x, z, ROCK_SHED_REACH),
                // Measured from a point moved toward the sun: ground on the shaded
                // side lies nearer the rock than it is.
                rock_distance(
                    &rocks,
                    x + SUN_BEARING[0] * CONTACT_LEAN,
                    z + SUN_BEARING[1] * CONTACT_LEAN,
                    ROCK_SHED_REACH,
                ),
                rock_distance(&rocks, x - LEE_TAIL, z, ROCK_SHED_REACH),
            )
        };
        // Under the rock itself (its caprock pans sample this bake) stays clear.
        // The foot shade leans away from the sun and breaks up over the ground.
        let breakup = 0.55 + 0.9 * noise(table, x * 0.8 + 3.0, z * 0.8 + 61.0);
        let lean_reach = rock_edge.min(shade_edge + 0.4);
        let contact = ((1.0 - smoothstep(lean_reach, -0.2, ROCK_CONTACT_REACH)).powi(2)
            * smoothstep(rock_edge, -1.6, -0.6)
            * breakup)
            .min(1.0);
        let shed = 1.0 - smoothstep(rock_edge, 0.2, ROCK_SHED_REACH);
        // Sand trails off each rock's lee in a tapering tongue; the windward foot
        // is scoured to bare gravel.
        let lee = (1.0 - smoothstep(lee_edge, -0.4, 1.4))
            * smoothstep(rock_edge, -0.3, 0.4)
            * (1.0 - road);
        r += (220.0 - r) * lee * 0.45;
        g += (203.0 - g) * lee * 0.45;
        b += (170.0 - b) * lee * 0.45;
        // Gravel shows through stony ground and spill; sand and traffic bury it.
        let gravel = 0.0f64.max(1.0f64.min(
            0.5 + stony * 0.45
                + windrow * 0.85
                + gravel_bed * 0.5
                + shed * 0.45
                + wall_scree * 0.7
                + stand * 0.4
                - lee * 0.8
                - worn_sand * 0.4
                - road * 0.05
                - drift * 0.5 * (1.0 - shed),
        ));
        // Under the wall line the joints between the blocks open onto shaded,
        // trodden fines, so each block stands apart on a dark seam rather than
        // floating on bright sand.
        let under_wall = smoothstep(reach, ARENA - 0.15, ARENA + 0.1)
            * (1.0 - smoothstep(reach, QUARRY_BANK_TOP - 0.1, QUARRY_BANK_TOP + 0.15));
        let occlusion = 1.0 - contact * 0.32 - wall_shade.min(1.0) * 0.26 - under_wall * 0.55;
        let (collar, cuttings, paint) = blast_pattern(x, z);
        r += (196.0 - r) * cuttings * 0.75;
        g += (188.0 - g) * cuttings * 0.75;
        b += (172.0 - b) * cuttings * 0.75;
        r += (214.0 - r) * paint;
        g += (98.0 - g) * paint;
        b += (42.0 - b) * paint;
        let occlusion = occlusion * (1.0 - collar * 0.6);
        let i = col * 4;
        out[i] = to_uint8_clamp((r + shade) * occlusion);
        out[i + 1] = to_uint8_clamp((g + shade) * occlusion);
        out[i + 2] = to_uint8_clamp((b + shade) * (occlusion - contact * 0.03));
        // Damp hollows dry out into cracked mud plates (alpha below 128).
        let mud = smoothstep(damp, 0.25, 0.55);
        out[i + 3] = to_uint8_clamp(128.0 + gravel * 127.0 * (1.0 - mud) - mud * 127.0);
    }
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

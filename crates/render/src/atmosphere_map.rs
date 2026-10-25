//! The atmosphere map the frame's lit shaders sample (`common.wgsl` `ground_macro`
//! and `cloud_shadow`), baked once: two value-noise fields every lit pixel used to
//! evaluate itself, now one filtered tap each.
//!
//! - RGB: the ground's macro relief over the square of `RELIEF_HALF_EXTENT` metres
//!   round the arena centre, one texel per `RELIEF_TEXEL` metres: the slope of its
//!   broad swells and finer hummocks along x and z (`RELIEF_SLOPE_RANGE` around 0.5)
//!   and their height -1..1 (as 0..1). It is the hashed noise itself, to 8 bits,
//!   fading out toward the square's edge (`RELIEF_FADE`), past anything the gameplay
//!   cameras see, so the taps the edge clamps beyond it hold no relief.
//! - A: the passing clouds' shadow noise, tiled: `CLOUD_PERIOD` cells of its broad
//!   octave across 255 texels, the last texel repeating the first, so a tap
//!   `fract`-wrapped into the first 255 texels filters across the seam. Its finer
//!   octave runs at `CLOUD_DETAIL` times the broad one's frequency, which tiles with it.

use crate::noise_lattice::lattice_hash;

/// Texels along each side of the map.
pub const ATMOSPHERE_MAP_SIZE: u32 = 256;
/// Half the side of the square the relief covers, metres from the arena centre.
pub const RELIEF_HALF_EXTENT: f32 = 192.0;
/// The relief's slopes as stored: 0.5 + slope / range.
pub const RELIEF_SLOPE_RANGE: f32 = 0.3;
/// Metres from the arena centre along either axis over which the relief fades out.
const RELIEF_FADE: [f32; 2] = [172.0, 188.0];
/// Cells of the cloud noise's broad octave in one tile of the map.
pub const CLOUD_PERIOD: f32 = 16.0;
/// The finer cloud octave's frequency over the broad one's: 34 cells per tile.
pub const CLOUD_DETAIL: f32 = 2.125;
/// Cloud shadow features per metre on the ground (cells of the broad octave), and
/// their drift with the wind in metres per second.
pub const CLOUD_SHADOW_SCALE: f32 = 0.018;
const CLOUD_SHADOW_DRIFT: [f64; 2] = [1.1, 0.45];

/// How far the cloud shadows have drifted after `time` seconds, in tiles of the map's
/// cloud noise, wrapped to 0..1 (in double precision: the clock grows all session).
pub fn cloud_tile_offset(time: f32) -> [f32; 2] {
    let tiles = f64::from(CLOUD_SHADOW_SCALE) / f64::from(CLOUD_PERIOD) * f64::from(time);
    CLOUD_SHADOW_DRIFT.map(|drift| (drift * tiles).rem_euclid(1.0) as f32)
}

// The ground's macro variation: broad swells and finer hummocks per metre, and
// their heights in metres before the theme's relief scales them.
const SWELL_SCALE: f32 = 0.045;
const SWELL_HEIGHT: f32 = 1.3;
const SWELL_OFFSET: [f32; 2] = [13.7, -5.3];
const HUMMOCK_SCALE: f32 = 0.16;
const HUMMOCK_HEIGHT: f32 = 0.22;
const HUMMOCK_OFFSET: [f32; 2] = [-2.9, 8.1];
/// The cloud octaves' weights and the finer one's offset.
const CLOUD_WEIGHTS: [f32; 2] = [0.68, 0.32];
const CLOUD_DETAIL_OFFSET: [f32; 2] = [17.3, -4.1];

/// The map as RGBA8 texels, row by row (row 0 at the most negative z).
pub fn atmosphere_map() -> Vec<u8> {
    let size = ATMOSPHERE_MAP_SIZE as usize;
    let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    let mut texels = Vec::with_capacity(size * size * 4);
    for j in 0..size {
        for i in 0..size {
            let (slope, height) = relief(relief_position(i), relief_position(j));
            let cloud = cloud_noise(
                i as f32 / (size - 1) as f32 * CLOUD_PERIOD,
                j as f32 / (size - 1) as f32 * CLOUD_PERIOD,
            );
            texels.extend([
                byte(0.5 + slope[0] / RELIEF_SLOPE_RANGE),
                byte(0.5 + slope[1] / RELIEF_SLOPE_RANGE),
                byte(height * 0.5 + 0.5),
                byte(cloud),
            ]);
        }
    }
    texels
}

/// The world coordinate of the relief texel `index` along an axis.
fn relief_position(index: usize) -> f32 {
    let texel = 2.0 * RELIEF_HALF_EXTENT / ATMOSPHERE_MAP_SIZE as f32;
    -RELIEF_HALF_EXTENT + (index as f32 + 0.5) * texel
}

/// The ground's macro relief at world (x, z): its slope along x and z, and its
/// height, -1 in the hollows to 1 on the crests, faded out toward the map's edge.
fn relief(x: f32, z: f32) -> ([f32; 2], f32) {
    let span = x.abs().max(z.abs());
    let t = ((span - RELIEF_FADE[0]) / (RELIEF_FADE[1] - RELIEF_FADE[0])).clamp(0.0, 1.0);
    let share = 1.0 - t * t * (3.0 - 2.0 * t);
    let (swell, swell_slope) = value_noise_gradient(
        x * SWELL_SCALE + SWELL_OFFSET[0],
        z * SWELL_SCALE + SWELL_OFFSET[1],
    );
    let (hummock, hummock_slope) = value_noise_gradient(
        x * HUMMOCK_SCALE + HUMMOCK_OFFSET[0],
        z * HUMMOCK_SCALE + HUMMOCK_OFFSET[1],
    );
    let slope = std::array::from_fn(|axis| {
        (swell_slope[axis] * (SWELL_SCALE * SWELL_HEIGHT)
            + hummock_slope[axis] * (HUMMOCK_SCALE * HUMMOCK_HEIGHT))
            * share
    });
    (slope, ((swell - 0.5) * 1.4 + (hummock - 0.5) * 0.6) * share)
}

/// The clouds' shadow noise at (x, y) in cells of its broad octave, tiling every
/// `CLOUD_PERIOD` cells.
fn cloud_noise(x: f32, y: f32) -> f32 {
    let detail_period = CLOUD_PERIOD * CLOUD_DETAIL;
    CLOUD_WEIGHTS[0] * tiled_value_noise(x, y, CLOUD_PERIOD)
        + CLOUD_WEIGHTS[1]
            * tiled_value_noise(
                x * CLOUD_DETAIL + CLOUD_DETAIL_OFFSET[0],
                y * CLOUD_DETAIL + CLOUD_DETAIL_OFFSET[1],
                detail_period,
            )
}

/// The four hashes round a cell, `common.wgsl` `lattice_corners`: the cell's, then its
/// +x, +y and +xy neighbours', each cell first wrapped by `wrap`.
fn corners(cell_x: i32, cell_y: i32, wrap: impl Fn(i32) -> i32) -> [f32; 4] {
    [
        lattice_hash(wrap(cell_x), wrap(cell_y)),
        lattice_hash(wrap(cell_x + 1), wrap(cell_y)),
        lattice_hash(wrap(cell_x), wrap(cell_y + 1)),
        lattice_hash(wrap(cell_x + 1), wrap(cell_y + 1)),
    ]
}

/// Value noise and its gradient per unit, `common.wgsl` `value_noise_gradient`.
fn value_noise_gradient(x: f32, y: f32) -> (f32, [f32; 2]) {
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let [a, b, c, d] = corners(x.floor() as i32, y.floor() as i32, |cell| cell);
    let (ux, uy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (dux, duy) = (6.0 * fx * (1.0 - fx), 6.0 * fy * (1.0 - fy));
    let k = a - b - c + d;
    let value = a + (b - a) * ux + (c - a) * uy + k * ux * uy;
    (value, [dux * (b - a + k * uy), duy * (c - a + k * ux)])
}

/// Value noise whose lattice repeats every `period` cells (a whole number).
fn tiled_value_noise(x: f32, y: f32, period: f32) -> f32 {
    let period = period as i32;
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let [a, b, c, d] = corners(x.floor() as i32, y.floor() as i32, |cell| {
        cell.rem_euclid(period)
    });
    let (ux, uy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let top = a + (b - a) * ux;
    let bottom = c + (d - c) * ux;
    top + (bottom - top) * uy
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shader::COMMON_WGSL;

    #[test]
    fn the_wgsl_reads_the_map_as_it_is_baked() {
        for (name, value) in [
            ("RELIEF_HALF_EXTENT", RELIEF_HALF_EXTENT),
            ("RELIEF_SLOPE_RANGE", RELIEF_SLOPE_RANGE),
            ("CLOUD_PERIOD", CLOUD_PERIOD),
            ("CLOUD_SHADOW_SCALE", CLOUD_SHADOW_SCALE),
        ] {
            let declaration = format!("const {name}: f32 = {value:?};");
            assert!(COMMON_WGSL.contains(&declaration), "{declaration}");
        }
        let size = ATMOSPHERE_MAP_SIZE as f32;
        assert!(COMMON_WGSL.contains(&format!("const ATMOSPHERE_MAP_SIZE: f32 = {size:?};")));
    }

    #[test]
    fn slopes_fit_their_stored_range() {
        let mut steepest: f32 = 0.0;
        for j in 0..ATMOSPHERE_MAP_SIZE as usize {
            for i in 0..ATMOSPHERE_MAP_SIZE as usize {
                let (slope, height) = relief(relief_position(i), relief_position(j));
                steepest = steepest.max(slope[0].abs()).max(slope[1].abs());
                assert!((-1.0..=1.0).contains(&height));
            }
        }
        assert!(steepest < RELIEF_SLOPE_RANGE * 0.5, "{steepest}");
    }

    #[test]
    fn the_cloud_noise_tiles_and_the_last_texel_repeats_the_first() {
        let map = atmosphere_map();
        let size = ATMOSPHERE_MAP_SIZE as usize;
        let cloud = |i: usize, j: usize| map[(j * size + i) * 4 + 3];
        for k in [0, 17, 128, 254] {
            assert_eq!(cloud(size - 1, k), cloud(0, k));
            assert_eq!(cloud(k, size - 1), cloud(k, 0));
        }
        let (x, y) = (3.7, 11.2);
        let shifted = cloud_noise(x + CLOUD_PERIOD, y - CLOUD_PERIOD);
        assert!((cloud_noise(x, y) - shifted).abs() < 1e-4);
    }
}

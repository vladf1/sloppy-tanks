//! The value-noise lattice the frame's shaders fetch (`common.wgsl`
//! `lattice_corners_fetched`): each texel holds the hashes of one lattice cell's four
//! corners, so the cloud shadows, the ground's macro relief and the sky's clouds,
//! which every lit pixel or sky pixel evaluates, read one texel per noise tap instead
//! of hashing four corners with eight integer multiplies.
//!
//! The texels hold `lattice_hash` itself for the cells within `NOISE_LATTICE_SIZE / 2`
//! of the origin, so the noise is the hashed one there (to 8 bits), and wrap at the
//! edges: beyond them the lattice repeats, every 256 cells (1.6 km of hummocks, 14 km
//! of cloud shadow), and stays continuous across the seam.

/// Cells along each side of the lattice texture.
pub const NOISE_LATTICE_SIZE: u32 = 256;

/// `common.wgsl` `lattice_hash`: an integer hash of a lattice cell, 0..1.
pub fn lattice_hash(x: i32, y: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(1_597_334_677) ^ (y as u32).wrapping_mul(3_812_015_801);
    h = (h ^ (h >> 16)).wrapping_mul(2_246_822_519);
    h ^= h >> 13;
    (h >> 8) as f32 * (1.0 / 16_777_216.0)
}

/// The lattice as RGBA8 texels, row by row: texel (i, j) holds the hashes of cell
/// (i - 128, j - 128) and of its +x, +y and +xy neighbours, in that order.
pub fn noise_lattice() -> Vec<u8> {
    let size = NOISE_LATTICE_SIZE as i32;
    let half = size / 2;
    // The cell a texel index stands for, wrapping past the last one.
    let cell = |index: i32| index.rem_euclid(size) - half;
    let byte = |x: i32, y: i32| (lattice_hash(cell(x), cell(y)) * 255.0).round() as u8;
    let mut texels = Vec::with_capacity((size * size * 4) as usize);
    for j in 0..size {
        for i in 0..size {
            texels.extend([
                byte(i, j),
                byte(i + 1, j),
                byte(i, j + 1),
                byte(i + 1, j + 1),
            ]);
        }
    }
    texels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texels_hold_each_cells_corners_and_wrap_continuously() {
        let texels = noise_lattice();
        let size = NOISE_LATTICE_SIZE as usize;
        let texel = |i: usize, j: usize| &texels[(j * size + i) * 4..(j * size + i) * 4 + 4];
        let quantized = |x: i32, y: i32| (lattice_hash(x, y) * 255.0).round() as u8;
        // The origin's cell sits at the middle texel.
        let middle = texel(size / 2, size / 2);
        assert_eq!(
            middle,
            [
                quantized(0, 0),
                quantized(1, 0),
                quantized(0, 1),
                quantized(1, 1)
            ]
        );
        assert_eq!(texel(size / 2 - 3, size / 2 + 5)[0], quantized(-3, 5));
        // A cell's +x corner is the next texel's own corner, across the seam too.
        for (i, j) in [(10, 20), (size - 1, 7), (3, size - 1), (size - 1, size - 1)] {
            let right = texel((i + 1) % size, j);
            let below = texel(i, (j + 1) % size);
            assert_eq!(texel(i, j)[1], right[0]);
            assert_eq!(texel(i, j)[2], below[0]);
            assert_eq!(texel(i, j)[3], below[1]);
        }
    }
}

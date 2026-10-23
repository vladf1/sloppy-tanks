//! The ground effects' noise as a texture: a small tile of smooth value noise and
//! white noise that the village ground, the harbor apron and the quarry's soil and
//! sandstone read instead of hashing lattice corners per pixel. They cover most of a
//! frame's pixels, and the arithmetic of one hashed value noise cost about 3% of a
//! village frame on an M3 Max (`ground_noise.wgsl`); a filtered lookup costs a
//! fraction of that. The renderer binds the tile in the emissive map's slot of every
//! material of these effects (`gpu::resources` `texture_slots`), which none of them
//! uses, so no material or pipeline changes for it. Its pixels are made here on the
//! CPU, once per page (a quarter of a megabyte), and mipmapped like any texture.

use std::sync::LazyLock;

use sloppy_core::models::effects_scenery;
use sloppy_core::scene::{Effect, Material, TextureRef, Wrap};

/// The generated-texture key of the noise tile.
pub const GROUND_NOISE: &str = "ground-noise";
/// The tile's edge in texels.
pub const GROUND_NOISE_SIZE: usize = 256;
/// Texels per value-noise lattice cell: the tile repeats every
/// `GROUND_NOISE_SIZE / GROUND_NOISE_CELL` cells (`ground_noise.wgsl` agrees).
pub const GROUND_NOISE_CELL: usize = 8;

/// The effects whose materials get the noise tile.
const USERS: [&str; 4] = [
    effects_scenery::MEADOW_SWAY,
    effects_scenery::HARBOR_APRON,
    effects_scenery::SANDSTONE,
    effects_scenery::QUARRY_SOIL,
];

/// The tile as materials bind it: linear data, repeating, mipmapped, rows as stored.
pub fn ground_noise_texture() -> &'static TextureRef {
    static TEXTURE: LazyLock<TextureRef> = LazyLock::new(|| TextureRef {
        wrap: Wrap::Repeat,
        srgb: false,
        flip_y: false,
        ..TextureRef::generated(GROUND_NOISE)
    });
    &TEXTURE
}

/// Whether a material's effect reads the noise tile.
pub fn uses_ground_noise(material: &Material) -> bool {
    matches!(&material.effect, Effect::Custom { name, .. } if USERS.contains(name))
}

/// A well-mixed 32-bit hash of a texel or lattice point and a channel.
fn hash(x: u32, y: u32, channel: u32) -> u32 {
    let mut h = x
        .wrapping_mul(0x9E37_79B1)
        .wrapping_add(y.wrapping_mul(0x85EB_CA77))
        .wrapping_add(channel.wrapping_mul(0xC2B2_AE3D));
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^ (h >> 16)
}

fn unit(h: u32) -> f32 {
    (h >> 8) as f32 / 16_777_216.0
}

/// Value noise of one channel at texel (`x`, `y`), tiling with the texture: random
/// lattice values eased between with a smoothstep, sampled at the texel's centre.
fn value(x: usize, y: usize, channel: u32) -> f32 {
    let cells = (GROUND_NOISE_SIZE / GROUND_NOISE_CELL) as u32;
    let fx = (x as f32 + 0.5) / GROUND_NOISE_CELL as f32;
    let fy = (y as f32 + 0.5) / GROUND_NOISE_CELL as f32;
    let (ix, iy) = (fx.floor(), fy.floor());
    let (tx, ty) = (fx - ix, fy - iy);
    let (ux, uy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
    let corner = |dx: u32, dy: u32| {
        unit(hash(
            (ix as u32 + dx) % cells,
            (iy as u32 + dy) % cells,
            channel,
        ))
    };
    let top = corner(0, 0) + (corner(1, 0) - corner(0, 0)) * ux;
    let bottom = corner(0, 1) + (corner(1, 1) - corner(0, 1)) * ux;
    top + (bottom - top) * uy
}

/// The tile's RGBA8 pixels, row by row: red and green two independent value-noise
/// fields, blue and alpha white noise (two random bytes per texel).
pub fn ground_noise_pixels() -> Vec<u8> {
    let mut pixels = Vec::with_capacity(GROUND_NOISE_SIZE * GROUND_NOISE_SIZE * 4);
    for y in 0..GROUND_NOISE_SIZE {
        for x in 0..GROUND_NOISE_SIZE {
            let white = hash(x as u32, y as u32, 7);
            pixels.extend_from_slice(&[
                (value(x, y, 1) * 255.0).round() as u8,
                (value(x, y, 2) * 255.0).round() as u8,
                white as u8,
                (white >> 8) as u8,
            ]);
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The value noise tiles: the texel past the last one would be the first.
    #[test]
    fn value_noise_tiles_and_spans_its_range() {
        let n = GROUND_NOISE_SIZE;
        for channel in [1, 2] {
            for y in [0, 37, n - 1] {
                let wrap = value(n, y, channel);
                assert!((wrap - value(0, y, channel)).abs() < 1e-5);
            }
            let values: Vec<f32> = (0..n).map(|x| value(x, 11, channel)).collect();
            let low = values.iter().copied().fold(1.0, f32::min);
            let high = values.iter().copied().fold(0.0, f32::max);
            assert!(low < 0.25 && high > 0.75, "{low}..{high}");
        }
    }

    /// One RGBA texel per tile texel, the white noise spread evenly.
    #[test]
    fn pixels_fill_the_tile() {
        let pixels = ground_noise_pixels();
        assert_eq!(pixels.len(), GROUND_NOISE_SIZE * GROUND_NOISE_SIZE * 4);
        let mean = pixels
            .iter()
            .skip(2)
            .step_by(4)
            .map(|&b| f64::from(b))
            .sum::<f64>()
            / (GROUND_NOISE_SIZE * GROUND_NOISE_SIZE) as f64;
        assert!((mean - 127.5).abs() < 2.0, "{mean}");
    }

    #[test]
    fn ground_effects_get_the_tile() {
        let material = |name| Material {
            effect: Effect::Custom {
                name,
                params: vec![],
            },
            ..Material::default()
        };
        assert!(uses_ground_noise(&material(effects_scenery::SANDSTONE)));
        assert!(!uses_ground_noise(&material(effects_scenery::FOLIAGE)));
        assert!(!uses_ground_noise(&Material::default()));
    }
}

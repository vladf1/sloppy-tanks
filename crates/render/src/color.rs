//! Color management matching Three.js r185: authored colors are sRGB hex, lighting
//! runs in linear sRGB, and the output pass applies ACES filmic tone mapping and the
//! sRGB transfer function. The WGSL output shader mirrors `aces_filmic` and
//! `srgb_encode`; these CPU versions document the curves and back the unit tests.

use glam::{Mat3, Vec3};

/// Three's `SRGBToLinear` for one 0..1 channel.
pub fn srgb_to_linear(channel: f32) -> f32 {
    if channel < 0.04045 {
        channel * 0.077_399_38
    } else {
        (channel * 0.947_867_3 + 0.052_132_7).powf(2.4)
    }
}

/// An authored `0xRRGGBB` color in linear sRGB, like `new THREE.Color(hex)`.
pub fn hex_to_linear(hex: u32) -> [f32; 3] {
    let channel = |shift: u32| srgb_to_linear(((hex >> shift) & 0xff) as f32 / 255.0);
    [channel(16), channel(8), channel(0)]
}

/// Linear color scaled by an intensity, as Three's lights premultiply it.
pub fn hex_to_linear_scaled(hex: u32, intensity: f32) -> [f32; 3] {
    let [r, g, b] = hex_to_linear(hex);
    [r * intensity, g * intensity, b * intensity]
}

/// Three's `sRGBTransferOETF` (note its 0.41666 exponent, not exactly 1/2.4).
pub fn srgb_encode(channel: f32) -> f32 {
    if channel <= 0.003_130_8 {
        channel * 12.92
    } else {
        channel.powf(0.41666) * 1.055 - 0.055
    }
}

fn rrt_and_odt_fit(v: Vec3) -> Vec3 {
    let a = v * (v + 0.024_578_6) - 0.000_090_537;
    let b = v * (0.983_729 * v + 0.432_951) + 0.238_081;
    a / b
}

/// Three r185 `acesFilmicToneMapping` (the renderer's ACESFilmicToneMapping).
/// The matrices are written row by row, as Three's `mat3(...)` receives them.
pub fn aces_filmic(color: Vec3, exposure: f32) -> Vec3 {
    let input = Mat3::from_cols_array(&[
        0.59719, 0.07600, 0.02840, 0.35458, 0.90834, 0.13383, 0.04823, 0.01566, 0.83777,
    ]);
    let output = Mat3::from_cols_array(&[
        1.60475, -0.10208, -0.00327, -0.53108, 1.10813, -0.07276, -0.07367, -0.00605, 1.07602,
    ]);
    let color = input * (color * exposure / 0.6);
    (output * rrt_and_odt_fit(color)).clamp(Vec3::ZERO, Vec3::ONE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colors_decode_like_three() {
        let [r, g, b] = hex_to_linear(0x3d6b8f);
        // THREE.Color(0x3d6b8f) → r 0.04666508, g 0.14702727, b 0.27467736
        assert!((r - 0.046_665_08).abs() < 1e-6);
        assert!((g - 0.147_027_27).abs() < 1e-6);
        assert!((b - 0.274_677_36).abs() < 1e-6);
        assert_eq!(hex_to_linear(0xffffff), [1.0, 1.0, 1.0]);
        assert_eq!(hex_to_linear(0), [0.0, 0.0, 0.0]);
    }

    #[test]
    fn aces_curve_matches_three_reference_values() {
        // Reference values from an independent double-precision evaluation of
        // Three's formula (row-major matrices applied as M·v).
        let mid = aces_filmic(Vec3::splat(0.18), 1.0);
        assert!((mid.x - 0.213_105).abs() < 1e-4, "{mid:?}");
        let white = aces_filmic(Vec3::splat(1.0), 1.0);
        assert!((white.x - 0.763_397).abs() < 1e-4, "{white:?}");
        let warm = aces_filmic(Vec3::new(0.5, 0.2, 0.1), 1.0);
        assert!(
            warm.distance(Vec3::new(0.574_004, 0.255_061, 0.118_743)) < 1e-4,
            "{warm:?}"
        );
        assert_eq!(aces_filmic(Vec3::ZERO, 1.0), Vec3::ZERO);
        let hot = aces_filmic(Vec3::splat(40.0), 1.0);
        assert!(hot.min_element() > 0.99);
    }

    #[test]
    fn srgb_round_trip_is_close() {
        for step in 0..=20 {
            let value = step as f32 / 20.0;
            let back = srgb_encode(srgb_to_linear(value));
            assert!((back - value).abs() < 2e-3, "{value} → {back}");
        }
    }
}

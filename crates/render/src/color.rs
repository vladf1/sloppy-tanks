//! Color management matching Three.js r185: authored colors are sRGB hex, lighting
//! runs in linear sRGB, and the output pass applies ACES filmic tone mapping and the
//! sRGB transfer function in `shaders/output.wgsl`.

use sloppy_core::geometry::math::srgb_to_linear;

/// An authored `0xRRGGBB` color in linear sRGB, like `new THREE.Color(hex)`.
pub fn hex_to_linear(hex: u32) -> [f32; 3] {
    // Presentation decodes tints and light colors every frame; `powf` is a
    // software routine in Wasm, so each byte value is decoded once.
    static BYTES: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    let bytes =
        BYTES.get_or_init(|| std::array::from_fn(|i| srgb_to_linear(i as f64 / 255.0) as f32));
    let channel = |shift: u32| bytes[((hex >> shift) & 0xff) as usize];
    [channel(16), channel(8), channel(0)]
}

/// Linear color scaled by an intensity, as Three's lights premultiply it.
pub fn hex_to_linear_scaled(hex: u32, intensity: f32) -> [f32; 3] {
    let [r, g, b] = hex_to_linear(hex);
    [r * intensity, g * intensity, b * intensity]
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
}

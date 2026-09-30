//! Color management matching Three.js r185: authored colors are sRGB hex, lighting
//! runs in linear sRGB, and the output pass applies ACES filmic tone mapping and the
//! sRGB transfer function in `shaders/output.wgsl`.

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

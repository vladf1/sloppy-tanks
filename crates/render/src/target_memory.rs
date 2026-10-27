//! What the multisampled HDR render targets hold, for the GPU memory stats.

/// Bytes an HDR target of this size holds: an RGBA16F color and a 32-bit depth
/// renderbuffer of `samples` samples each, and the single-sample RGBA16F texture they
/// resolve into. WebGL's `samples` 0 (no MSAA) still stores one of each.
pub fn target_bytes(width: u32, height: u32, samples: u32) -> u64 {
    let pixels = width as u64 * height as u64;
    let stored = u64::from(samples.max(1));
    pixels * (8 * stored + 4 * stored + 8)
}

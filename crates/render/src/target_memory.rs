//! What the multisampled HDR render targets hold, for the GPU memory stats.

/// Bytes an HDR target of this size holds: an RGBA16F color and a 32-bit depth
/// renderbuffer of `samples` samples each, and the single-sample RGBA16F texture they
/// resolve into. WebGL's `samples` 0 (no MSAA) still stores one of each.
pub fn target_bytes(width: u32, height: u32, samples: u32) -> u64 {
    let pixels = width as u64 * height as u64;
    let stored = u64::from(samples.max(1));
    pixels * (8 * stored + 4 * stored + 8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_count_the_samples_they_store() {
        assert_eq!(target_bytes(1, 1, 4), 56);
        assert_eq!(target_bytes(1, 1, 2), 32);
        assert_eq!(target_bytes(1, 1, 1), 20);
        // Without MSAA the renderbuffers keep one sample.
        assert_eq!(target_bytes(1, 1, 0), 20);
        // A 2560x1440 view without MSAA holds 126.6 MiB less than with four samples.
        assert_eq!(
            target_bytes(2560, 1440, 4) - target_bytes(2560, 1440, 0),
            132_710_400
        );
    }
}

// WebGL2 cannot copy depth textures, so the WebGL build draws the fixed scenery's
// cached shadow depth into the sun shadow map instead (`gpu/depth_copy.rs`).

// Bound as unfilterable float (GLSL has no texel fetch from depth samplers), and read
// through a nearest sampler: without one, GLSL ES finds a linearly filtered depth
// texture incomplete and reads zero.
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

@vertex
fn vs_fullscreen(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
    let uv = vec2f(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4f(uv * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_copy_depth(@builtin(position) position: vec4f) -> @builtin(frag_depth) f32 {
    // Texel centers, so the nearest sample is the texel itself.
    let uv = position.xy / vec2f(textureDimensions(source));
    return textureSampleLevel(source, source_sampler, uv, 0.0).r;
}

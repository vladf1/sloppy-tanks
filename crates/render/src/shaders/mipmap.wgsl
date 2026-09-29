// Mipmap generation: each level is a bilinear 2×2 average of the level above.
// Rendering to an sRGB view averages in linear space, like Three's WebGPU blits.

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

struct Out {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
}

@vertex
fn vs_blit(@builtin(vertex_index) index: u32) -> Out {
    let uv = vec2f(f32((index << 1u) & 2u), f32(index & 2u));
    var out: Out;
    out.position = vec4f(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_blit(input: Out) -> @location(0) vec4f {
    return textureSampleLevel(source, source_sampler, input.uv, 0.0);
}

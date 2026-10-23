// A bake's full-screen pass (`effects::bakes`): one triangle over the target, each
// fragment the `bake` of its texel at the target's UV, 0 at its first row (as the
// mipmap blits read it, on both backends).

struct BakeOut {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
}

@vertex
fn vs_bake(@builtin(vertex_index) index: u32) -> BakeOut {
    let uv = vec2f(f32((index << 1u) & 2u), f32(index & 2u));
    var out: BakeOut;
    out.position = vec4f(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_bake(input: BakeOut) -> @location(0) vec4f {
    return bake(input.uv);
}

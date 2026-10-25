// Merged alpha-tested shadow casters (`shadow_merge.rs` cutout groups): like
// `shadow_merged.wgsl`, plus the cards' UVs for an alpha test against the shared
// material's cutoff. The vertex stage hands `shadow.wgsl`'s `fs_shadow` the varyings
// of its own vertex stage, so on WebGPU these casters share that fragment stage
// with the per-part casters (`gpu/webgpu/pipelines.rs`); merged casters never fade,
// so their tint keeps its dither out. The WebGL build links `fs_shadow_cutout`.

// The leading fields of `MaterialUniform` in material.wgsl.
struct CutoutMaterial {
    color: vec4f,
    emissive: vec4f,
    // x roughness, y metalness, z alpha test, w bump scale.
    surface: vec4f,
    map_transform: vec4f,
}

@group(1) @binding(0) var<uniform> material: CutoutMaterial;
@group(1) @binding(1) var map_texture: texture_2d<f32>;
@group(1) @binding(2) var map_sampler: sampler;

@vertex
fn vs_shadow_cutout(
    @location(0) position: vec3f,
    @location(1) slot: u32,
    @location(2) base: u32,
    @location(3) uv: vec2f,
) -> VertexOut {
    let instance = instance_at(base + slot);
    let world = instance.world * vec4f(position, 1.0);
    var out: VertexOut;
    out.clip = frame.view_projection * world;
    out.world = world.xyz;
    out.uv = uv;
    out.tint = vec4f(1.0);
    out.data = instance.data;
    out.extra0 = vec4f(0.0);
    out.extra1 = vec4f(0.0);
    return out;
}

@fragment
fn fs_shadow_cutout(input: VertexOut) {
    let map_uv = input.uv * material.map_transform.xy + material.map_transform.zw;
    let texel = textureSample(map_texture, map_sampler, map_uv);
    if material.color.a * texel.a <= material.surface.z {
        discard;
    }
}

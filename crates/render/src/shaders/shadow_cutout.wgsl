// Merged alpha-tested shadow casters (`shadow_merge.rs` cutout groups): like
// `shadow_merged.wgsl`, plus the cards' UVs and a fragment stage that discards
// below the shared material's cutoff (the per-part shadow pass's alpha test).

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

struct CutoutOut {
    @builtin(position) clip: vec4f,
    @location(0) uv: vec2f,
}

@vertex
fn vs_shadow_cutout(
    @location(0) position: vec3f,
    @location(1) slot: u32,
    @location(2) base: u32,
    @location(3) uv: vec2f,
) -> CutoutOut {
    let world = instance_at(base + slot).world;
    var out: CutoutOut;
    out.clip = frame.view_projection * (world * vec4f(position, 1.0));
    out.uv = uv;
    return out;
}

@fragment
fn fs_shadow_cutout(input: CutoutOut) {
    let map_uv = input.uv * material.map_transform.xy + material.map_transform.zw;
    let texel = textureSample(map_texture, map_sampler, map_uv);
    if material.color.a * texel.a <= material.surface.z {
        discard;
    }
}

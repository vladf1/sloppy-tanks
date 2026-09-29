// Material bindings and the effect interface for surface and shadow shaders.
// `shader.rs` prepends the variant constants and `VertexIn`, and appends the
// effect hooks and the pass template.

struct MaterialUniform {
    // Linear rgb, a opacity.
    color: vec4f,
    // Linear rgb × intensity.
    emissive: vec4f,
    // x roughness, y metalness, z alpha test, w bump scale.
    surface: vec4f,
    // xy repeat, zw offset (Three texture matrix without rotation).
    map_transform: vec4f,
    bump_transform: vec4f,
    emissive_transform: vec4f,
    // The repeat and offset of `extra_texture0` and `extra_texture1`.
    extra_transforms: array<vec4f, 2>,
    // Effect parameters, 16 floats.
    params: array<vec4f, 4>,
}

@group(1) @binding(0) var<uniform> material: MaterialUniform;
@group(1) @binding(1) var map_texture: texture_2d<f32>;
@group(1) @binding(2) var map_sampler: sampler;
@group(1) @binding(3) var bump_texture: texture_2d<f32>;
@group(1) @binding(4) var bump_sampler: sampler;
@group(1) @binding(5) var emissive_texture: texture_2d<f32>;
@group(1) @binding(6) var emissive_sampler: sampler;
// The material's `extra_textures` in order (white when absent), for effects; also
// visible to the vertex stage (sample with an explicit level there).
@group(1) @binding(7) var extra_texture0: texture_2d<f32>;
@group(1) @binding(8) var extra_sampler0: sampler;
@group(1) @binding(9) var extra_texture1: texture_2d<f32>;
@group(1) @binding(10) var extra_sampler1: sampler;

// What a vertex effect may edit, in the model's local space.
struct EffectVertex {
    position: vec3f,
    normal: vec3f,
    uv: vec2f,
    color: vec3f,
    // Vertex alpha (RGBA vertex colors); 1 for RGB meshes.
    color_alpha: f32,
    extra0: vec4f,
    extra1: vec4f,
    // World translation of the instance, for per-instance variation.
    instance_origin: vec3f,
    // Four floats per drawn instance: set by presentation, or packed from the
    // mesh's per-instance attributes for InstancedMesh parts.
    instance_data: vec4f,
}

// World-space position and normal after the instance transform (billboards).
struct EffectWorld {
    position: vec3f,
    normal: vec3f,
}

// The surface an effect may edit before lighting, alpha test and fog.
struct Surface {
    color: vec3f,
    opacity: f32,
    // World-space unit normal.
    normal: vec3f,
    roughness: f32,
    metalness: f32,
    emissive: vec3f,
}

struct EffectFragment {
    world: vec3f,
    uv: vec2f,
    extra0: vec4f,
    extra1: vec4f,
    instance_data: vec4f,
    view_direction: vec3f,
    front_facing: bool,
    // Interpolated RGBA vertex color (white without vertex colors, and in the
    // shadow pass), and the instance tint (rgb) and opacity (a).
    vertex_color: vec4f,
    tint: vec4f,
}

// Cofactor form of the inverse transpose; the sign keeps mirrored instances lit
// from the correct side.
fn normal_matrix(m: mat4x4f) -> mat3x3f {
    let c0 = m[0].xyz;
    let c1 = m[1].xyz;
    let c2 = m[2].xyz;
    let det = dot(c0, cross(c1, c2));
    return mat3x3f(cross(c1, c2), cross(c2, c0), cross(c0, c1)) * select(1.0, -1.0, det < 0.0);
}

fn effect_input(input: VertexIn, instance: Instance) -> EffectVertex {
    var v: EffectVertex;
    v.position = input.position;
    v.normal = input.normal;
    v.uv = input.uv;
    v.color = input.color.rgb;
    v.color_alpha = input.color.a;
    v.extra0 = vertex_extra0(input);
    v.extra1 = vertex_extra1(input);
    v.instance_origin = instance.world[3].xyz;
    v.instance_data = instance.data;
    return v;
}

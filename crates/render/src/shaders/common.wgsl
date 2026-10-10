// Frame-level declarations shared by the surface, shadow and water shaders.
// `shader.rs` appends `instance_at`, which reads the instance records from where
// the build keeps them (`instances_storage.wgsl` or `instances_texture.wgsl`).

const RECIPROCAL_PI: f32 = 0.3183098861837907;

struct PointLight {
    // xyz position, w cutoff distance (Three `distance`; 0 = infinite)
    position: vec4f,
    // rgb color × intensity, w decay exponent
    color: vec4f,
}

// One per view (main, water reflection, sun shadow). Linear colors throughout.
struct Frame {
    view_projection: mat4x4f,
    view: mat4x4f,
    // World to shadow-map UV/depth (0.5 bias folded in, WebGPU depth 0..1).
    shadow_matrix: mat4x4f,
    // xyz eye position, w time in seconds for animated effects.
    camera_position: vec4f,
    // rgb fog color, w 1 when the scene has fog.
    fog_color: vec4f,
    // x near, y far (Three linear `Fog`).
    fog_range: vec4f,
    // Hemisphere light colors × intensity; sky_color.w is the strength of the
    // sky's specular reflection (0 off).
    sky_color: vec4f,
    ground_color: vec4f,
    // Unit vector toward the sun.
    sun_direction: vec4f,
    // rgb color × intensity.
    sun_color: vec4f,
    // x bias, y normal bias, z map size, w filter radius in texels.
    shadow: vec4f,
    // Render target width, height and their reciprocals.
    viewport: vec4f,
    camera_right: vec4f,
    camera_up: vec4f,
    point_lights: array<PointLight, 4>,
}

// Per drawn instance: world transform, tint (rgb multiplies the base color, a is
// opacity) and four floats that effects may read.
struct Instance {
    world: mat4x4f,
    tint: vec4f,
    data: vec4f,
}

// An instance as stored (`InstanceRecord` in draw_list.rs): the first three rows of
// its affine world transform, whose last row is always 0, 0, 0, 1, then tint and
// data. `instance_at` reads one and expands it.
fn expand_instance(row0: vec4f, row1: vec4f, row2: vec4f, tint: vec4f, data: vec4f) -> Instance {
    var instance: Instance;
    instance.world = transpose(mat4x4f(row0, row1, row2, vec4f(0.0, 0.0, 0.0, 1.0)));
    instance.tint = tint;
    instance.data = data;
    return instance;
}

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var shadow_map: texture_depth_2d;
@group(0) @binding(2) var shadow_sampler: sampler_comparison;
@group(0) @binding(3) var dfg_lut: texture_2d<f32>;
@group(0) @binding(4) var lut_sampler: sampler;

// TSL `hash` (PCG-style integer hash) used by the debris shadow mask.
fn tsl_hash(seed: f32) -> f32 {
    let state = u32(seed) * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return f32((word >> 22u) ^ word) * (1.0 / 4294967296.0);
}

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
    // The atmosphere (`gpu::Atmosphere`); every field is zero while it is off.
    // Clip position to world for the sky's view rays (the plain projection, also
    // for the oblique mirror).
    sky_rays: mat4x4f,
    // rgb sky overhead, w 1 while the atmosphere is on.
    sky_zenith: vec4f,
    // rgb sky at the horizon, w cosine of the sun disc's angular radius.
    sky_horizon: vec4f,
    // rgb sun halo and disc, w rim light strength.
    sky_sun: vec4f,
    // rgb sunlit cloud tops, w cloud coverage 0..1.
    cloud_lit: vec4f,
    // rgb cloud undersides, w strength of cloud shadows on the ground.
    cloud_shade: vec4f,
    // x haze density per metre at height 0, y its height falloff per metre,
    // z distance from the eye where it begins, w most it hides.
    haze: vec4f,
    // x half-size in metres of the square around the arena centre the haze leaves
    // clear, y the width of its fade-in beyond that square.
    haze_shape: vec4f,
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

// ---------------------------------------------------------------- atmosphere

fn atmosphere_on() -> bool {
    return frame.sky_zenith.w > 0.5;
}

// Integer lattice hash for value noise (bit-exact on WebGPU and WebGL).
fn lattice_hash(cell: vec2i) -> f32 {
    var h = (bitcast<u32>(cell.x) * 1597334677u) ^ (bitcast<u32>(cell.y) * 3812015801u);
    h = (h ^ (h >> 16u)) * 2246822519u;
    h = h ^ (h >> 13u);
    return f32(h >> 8u) * (1.0 / 16777216.0);
}

fn value_noise(p: vec2f) -> f32 {
    let cell = vec2i(floor(p));
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = lattice_hash(cell);
    let b = lattice_hash(cell + vec2i(1, 0));
    let c = lattice_hash(cell + vec2i(0, 1));
    let d = lattice_hash(cell + vec2i(1, 1));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Sky brightness rises toward the horizon by this power of (1 - up).
const SKY_HORIZON_EXPONENT: f32 = 3.2;

// The sky without sun disc or clouds: overhead to horizon gradient and the sun's
// forward-scattered halo. Distant surfaces fade into it and glossy ones reflect it,
// so far hills meet the horizon without a seam.
fn sky_gradient(direction: vec3f) -> vec3f {
    let up = clamp(direction.y, 0.0, 1.0);
    let toward_horizon = pow(1.0 - up, SKY_HORIZON_EXPONENT);
    let sky = mix(frame.sky_zenith.rgb, frame.sky_horizon.rgb, toward_horizon);
    let sun = max(dot(direction, frame.sun_direction.xyz), 0.0);
    let halo = pow(sun, 5.0) * (0.15 + 0.5 * toward_horizon) + pow(sun, 40.0) * 0.45;
    return sky + frame.sky_sun.rgb * halo;
}

// Exponential height haze integrated from where it begins to the surface, tinted
// by the sky behind the surface (aerial perspective). Off, Three's linear fog.
fn apply_fog(color: vec3f, world: vec3f) -> vec3f {
    if atmosphere_on() {
        let eye = frame.camera_position.xyz;
        let offset = world - eye;
        let distance = length(offset);
        let ray = offset / max(distance, 1e-4);
        let start = frame.haze.z;
        let travelled = distance - start;
        if travelled <= 0.0 {
            return color;
        }
        let falloff = frame.haze.y;
        let rise = ray.y * travelled * falloff;
        var column = travelled;
        if abs(rise) > 1e-4 {
            column = travelled * (1.0 - exp(-rise)) / rise;
        }
        let begin_height = eye.y + ray.y * start;
        let depth = frame.haze.x * exp(-falloff * begin_height) * column;
        // The arena stays crisp from every camera, the overview included: haze
        // only settles on the backdrop beyond it.
        let outside = length(max(abs(world.xz) - frame.haze_shape.x, vec2f(0.0)));
        let backdrop = smoothstep(0.0, frame.haze_shape.y, outside);
        let amount = min(1.0 - exp(-depth), frame.haze.w) * backdrop;
        let behind = normalize(vec3f(ray.x, max(ray.y, 0.0), ray.z));
        return mix(color, sky_gradient(behind), amount);
    }
    if frame.fog_color.w > 0.5 {
        let depth = -(frame.view * vec4f(world, 1.0)).z;
        return mix(color, frame.fog_color.rgb, smoothstep(frame.fog_range.x, frame.fog_range.y, depth));
    }
    return color;
}

// Ground-scale cloud shadow features per metre, and their drift in metres per second.
const CLOUD_SHADOW_SCALE: f32 = 0.03;
const CLOUD_SHADOW_DRIFT: vec2f = vec2f(1.1, 0.45);

// The share of sunlight that passing clouds let through at `world`: soft patches,
// projected along the sun so tall things and the ground beneath agree.
fn cloud_shadow(world: vec3f) -> f32 {
    let strength = frame.cloud_shade.w;
    if strength <= 0.0 {
        return 1.0;
    }
    let sun = frame.sun_direction.xyz;
    let ground = world.xz - sun.xz / max(sun.y, 0.2) * world.y;
    let p = (ground + CLOUD_SHADOW_DRIFT * frame.camera_position.w) * CLOUD_SHADOW_SCALE;
    let n = value_noise(p) * 0.68 + value_noise(p * 2.13 + vec2f(17.3, -4.1)) * 0.32;
    let coverage = frame.cloud_lit.w;
    let cover = smoothstep(1.02 - coverage, 1.22 - coverage, n);
    return 1.0 - strength * cover;
}

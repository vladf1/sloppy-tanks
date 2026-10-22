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
    // mostly clear, y the width of its fade-in beyond that square, z the clouds'
    // strength in this view (softer in the water's mirror), w the far plane's
    // distance, where every surface has melted into the sky.
    haze_shape: vec4f,
    // rgb the aerial perspective thin haze shifts distant surfaces toward (cooler
    // and darker than the bright horizon, so far silhouettes stay readable), w the
    // share of the haze that still settles inside the arena square.
    aerial: vec4f,
    // Distant ranges drawn into the sky's horizon: rgb their color, w the ray slope
    // of their tallest peaks (0 for none).
    ridge: vec4f,
    // x their features around the horizon, y flat mesa tops 0..1, z the share of the
    // horizon left open between them 0..1.
    ridge_shape: vec4f,
    // x haze per metre that does not thin with height (the long-range aerial
    // perspective that softens far peaks as much as their feet), y the distance from
    // the eye where it begins, z how strongly haze toward the sun glows with its color,
    // w the most the low mist beyond the arena hides at ground level.
    haze_far: vec4f,
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
    let halo = pow(sun, 8.0) * (0.1 + 0.4 * toward_horizon) + pow(sun, 48.0) * 0.5;
    return sky + frame.sky_sun.rgb * halo;
}

// The near range of the horizon's distant ranges stands this much lower than the far
// one, and each takes this share of the ridge color over the sky behind it.
const NEAR_RIDGE_SCALE: f32 = 0.62;
const FAR_RIDGE_SHARE: f32 = 0.42;
const NEAR_RIDGE_SHARE: f32 = 0.7;
// The ridge line's softness, in ray slope, where surfaces fading into the sky draw it.
const FADED_RIDGE_EDGE: f32 = 0.002;

fn ridge_profile(azimuth: vec2f, offset: vec2f) -> f32 {
    let p = azimuth * frame.ridge_shape.x + offset;
    var h = value_noise(p) * 0.6 + value_noise(p * 2.7 + vec2f(5.2, 1.3)) * 0.28
        + value_noise(p * 7.9 + vec2f(-2.1, 8.4)) * 0.12;
    // Mesas: plateaus with steep sides.
    let mesa = smoothstep(0.42, 0.5, h) * 0.75 + smoothstep(0.6, 0.66, h) * 0.25;
    h = mix(h, mesa * 0.9 + h * 0.1, frame.ridge_shape.y);
    // Gaps of open horizon between headlands and islands.
    let gaps = frame.ridge_shape.z;
    return max(h - gaps, 0.0) / (1.0 - gaps);
}

// How much of the haze behind a range settles over its foot, the azimuth step over
// which a range's slope is measured, and how strongly that slope turns its faces
// toward or away from the sun.
const RIDGE_FOOT_HAZE: f32 = 0.7;
const RIDGE_SLOPE_STEP: f32 = 0.004;
const RIDGE_FACE_LIGHT: f32 = 0.22;
// Ray slope over which the faces fade in above the horizon.
const RIDGE_SHADE_FADE: f32 = 0.004;
// Mesa strata: bands per range height and how much their value alternates.
const RIDGE_STRATA: f32 = 4.0;
const RIDGE_STRATA_CONTRAST: f32 = 0.07;

// One range of the horizon backdrop over `color`: a silhouette whose faces turned
// toward the sun catch light and those turned away fall into shade, banded into
// strata where it is a mesa, its crest crisp and its foot melting into the haze.
fn ridge_layer(color: vec3f, direction: vec3f, heading: vec2f, offset: vec2f, scale: f32, share: f32, aa: f32) -> vec3f {
    let height = frame.ridge.w * scale;
    let top = height * ridge_profile(heading, offset);
    // Low headlands fade in with their height, so where a range ends its haze-thick
    // foot leaves no hard edge down to the horizon.
    let cover = (1.0 - smoothstep(top - aa, top + aa, direction.y)) * smoothstep(0.0, height * 0.3, top);
    if cover <= 0.0 {
        return color;
    }
    let rise = clamp(direction.y / max(top, 1e-4), 0.0, 1.0);
    // Faces and strata show only above the horizon, where the range itself is seen.
    // Rays below it (hazed surfaces fading into the sky behind them, most of an
    // overhead frame's pixels) take the plain ridge color at the hazy foot, with no
    // extra noise taps.
    var shade = 1.0;
    let shown = smoothstep(0.0, RIDGE_SHADE_FADE, direction.y);
    if shown > 0.0 {
        // The range's slope along the horizon: a face falling away to one side
        // faces that way, lit when the sun stands on that side.
        let c = cos(RIDGE_SLOPE_STEP);
        let s = sin(RIDGE_SLOPE_STEP);
        let left = vec2f(heading.x * c - heading.y * s, heading.x * s + heading.y * c);
        let right = vec2f(heading.x * c + heading.y * s, -heading.x * s + heading.y * c);
        let slope = (ridge_profile(right, offset) - ridge_profile(left, offset)) * height / (2.0 * RIDGE_SLOPE_STEP);
        let side = vec2f(-heading.y, heading.x);
        let sun = frame.sun_direction.xyz;
        let toward_sun = dot(side, sun.xz) * clamp(slope * 6.0, -1.0, 1.0) - dot(heading, sun.xz) * 0.35;
        let strata = sin(rise * RIDGE_STRATA * 6.2831853 * max(top / max(height, 1e-4), 0.3));
        let lit = (1.0 + RIDGE_FACE_LIGHT * clamp(toward_sun, -1.0, 1.0))
            * (1.0 + RIDGE_STRATA_CONTRAST * frame.ridge_shape.y * smoothstep(-0.2, 0.6, strata));
        shade = mix(1.0, lit, shown);
    }
    // The air thickens toward the foot, while the crest stays crisp.
    let foot = 1.0 - rise;
    let ridge = mix(frame.ridge.rgb * shade, color, RIDGE_FOOT_HAZE * foot * foot);
    return mix(color, mix(color, ridge, share), cover);
}

// Distant ranges along the horizon in two layers, the far one paler: a backdrop that
// recedes into the haze where the scenery's own hills end.
fn horizon_ridges(color: vec3f, direction: vec3f, aa: f32) -> vec3f {
    let height = frame.ridge.w;
    if height <= 0.0 || direction.y > height {
        return color;
    }
    let heading = normalize(direction.xz + vec2f(1e-5));
    let far = ridge_layer(color, direction, heading, vec2f(0.0, 0.0), 1.0, FAR_RIDGE_SHARE, aa);
    return ridge_layer(far, direction, heading, vec2f(31.7, -12.4), NEAR_RIDGE_SCALE, NEAR_RIDGE_SHARE, aa);
}

// The colour haze takes on along `ray`: thin haze shifts surfaces toward the cool
// aerial tint, thick haze becomes the sky behind them (with the sun's warm forward
// scatter from `sky_gradient`), so silhouettes recede in steps rather than bleach.
// Looking toward the sun the haze glows warm, away from it it stays cool.
fn haze_tint(ray: vec3f, amount: f32) -> vec3f {
    let toward_sun = max(dot(ray, frame.sun_direction.xyz), 0.0);
    let glow = frame.sky_sun.rgb * (frame.haze_far.z * toward_sun * toward_sun * amount);
    return mix(frame.aerial.rgb, sky_behind(ray), amount * amount) + glow;
}

// The sky as drawn behind a surface along `ray`, distant ranges included (no clouds
// or sun disc), so surfaces that melt into it leave no lighter patch across them.
fn sky_behind(ray: vec3f) -> vec3f {
    return horizon_ridges(sky_gradient(ray), ray, FADED_RIDGE_EDGE);
}

// The far haze layer thins up a slope at this share of the near layer's falloff, to
// this share of its density on the highest crests.
const FAR_HAZE_THINNING: f32 = 1.0;
const FAR_HAZE_CREST_SHARE: f32 = 0.3;
// Height in metres over which the low mist beyond the arena thins by e.
const MIST_HEIGHT: f32 = 14.0;
// Shares of the far plane's distance over which surfaces melt into the sky.
const FAR_FADE_START: f32 = 0.84;
const FAR_FADE_END: f32 = 0.98;

// Aerial perspective in two layers. Exponential height haze, integrated from where
// it begins to the surface, lies dense in the valleys and thin up high, so a camera
// looking down from above stays clean. Beyond `haze_far.y` a thinner haze that does
// not fall off with height takes over, so distant peaks fade as a whole into the
// horizon instead of standing hard against it while their feet wash out. Inside the
// arena square only a share of the height haze settles and none of the far haze;
// toward the far plane every surface melts into the sky, so nothing the far plane
// clips draws a hard horizon. Off, Three's linear fog.
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
        let depth = frame.haze.x * exp(-falloff * max(begin_height, 0.0)) * column;
        let outside = length(max(abs(world.xz) - frame.haze_shape.x, vec2f(0.0)));
        let beyond = smoothstep(0.0, frame.haze_shape.y, outside);
        let backdrop = mix(frame.aerial.w, 1.0, beyond);
        // The far layer eases in over its first stretch, so it never draws a line.
        let far_travel = max(distance - frame.haze_far.y, 0.0);
        // It still thins a little up the slopes, so a far range keeps hazy feet under
        // clearer crests and reads as a landform rather than one flat card.
        let crest = exp(-max(world.y, 0.0) * frame.haze.y * FAR_HAZE_THINNING);
        let far_depth = frame.haze_far.x * far_travel * far_travel / (far_travel + 40.0)
            * mix(FAR_HAZE_CREST_SHARE, 1.0, crest);
        let near_amount = min(1.0 - exp(-depth), frame.haze.w) * backdrop;
        let far_amount = (1.0 - exp(-far_depth)) * beyond;
        // Low mist settles in the valleys beyond the arena, thick at the ground and
        // gone a few storeys up, so distant hills stand out of it with clear crests.
        let mist = frame.haze_far.w * exp(-max(world.y, 0.0) / MIST_HEIGHT) * beyond
            * smoothstep(frame.haze_far.y * 0.7, frame.haze_far.y * 1.4, distance);
        let clear = (1.0 - near_amount) * (1.0 - far_amount) * (1.0 - mist);
        let amount = min(1.0 - clear, frame.haze.w);
        var hazed = mix(color, haze_tint(ray, amount), amount);
        let far = frame.haze_shape.w;
        if far > 0.0 {
            // Late and short, so the far mesas and ranges keep their own form and
            // only what the far plane is about to clip melts away.
            let fade = smoothstep(far * FAR_FADE_START, far * FAR_FADE_END, distance);
            if fade > 0.0 {
                hazed = mix(hazed, sky_behind(ray), fade);
            }
        }
        return hazed;
    }
    if frame.fog_color.w > 0.5 {
        let depth = -(frame.view * vec4f(world, 1.0)).z;
        return mix(color, frame.fog_color.rgb, smoothstep(frame.fog_range.x, frame.fog_range.y, depth));
    }
    return color;
}

// Ground-scale cloud shadow features per metre, and their drift in metres per second.
const CLOUD_SHADOW_SCALE: f32 = 0.016;
const CLOUD_SHADOW_DRIFT: vec2f = vec2f(1.1, 0.45);
// Where the shadow noise starts and completes a cloud, above the coverage.
const CLOUD_SHADOW_LOW: f32 = 1.0;
const CLOUD_SHADOW_HIGH: f32 = 1.3;

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
    // Broad patches: about the sky's own share of the ground lies under a cloud, so
    // the field reads in large light and shade masses rather than one flat sheet.
    let cover = smoothstep(CLOUD_SHADOW_LOW - coverage, CLOUD_SHADOW_HIGH - coverage, n);
    return 1.0 - strength * cover;
}

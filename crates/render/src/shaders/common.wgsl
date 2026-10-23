// Frame-level declarations shared by the surface, shadow, water, sky and output
// shaders. `shader.rs` appends the frame group's bindings (`frame_bindings.wgsl`) and
// `instance_at`, which reads the instance records from where the build keeps them
// (`instances_storage.wgsl` or `instances_texture.wgsl`); the output pass binds the
// main view's `frame` alone, in its own group.

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
    // Ground macro variation (`ground_macro`): x the slope of its painted-in relief,
    // y how much its albedo varies, z how far its dry crests bleach, w the share of
    // the sun that bounces back into cast shadows from the sunlit ground around them.
    ground: vec4f,
    // The graphics tier's optional detail (`quality.rs`): y 1 for fine procedural
    // detail (`fine_detail`); w the scene's pixels per canvas pixel along each axis.
    quality: vec4f,
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

// ---------------------------------------------------------------- atmosphere

fn atmosphere_on() -> bool {
    return frame.sky_zenith.w > 0.5;
}

// Procedural detail a few pixels across (the sky clouds' finest octave, effects' fine
// noise), which the graphics tier may drop; WebGL's weaker devices always do. The
// ground's hummocks, baked into the atmosphere map, cost every tier the same tap.
fn fine_detail() -> bool {
    return !LITE && frame.quality.y > 0.5;
}

// Integer lattice hash for value noise (bit-exact on WebGPU and WebGL).
fn lattice_hash(cell: vec2i) -> f32 {
    var h = (bitcast<u32>(cell.x) * 1597334677u) ^ (bitcast<u32>(cell.y) * 3812015801u);
    h = (h ^ (h >> 16u)) * 2246822519u;
    h = h ^ (h >> 13u);
    return f32(h >> 8u) * (1.0 / 16777216.0);
}

// `lattice_hash` of `cell` and of its +x, +y and +xy neighbours (in that order), the
// neighbours' products by each axis's constant taken from the cell's (wrapping
// arithmetic, so the same bits): half the integer multiplies of four hashes.
fn lattice_corners(cell: vec2i) -> vec4f {
    let x = bitcast<u32>(cell.x) * 1597334677u;
    let y = bitcast<u32>(cell.y) * 3812015801u;
    let x1 = x + 1597334677u;
    let y1 = y + 3812015801u;
    var h = vec4u(x ^ y, x1 ^ y, x ^ y1, x1 ^ y1);
    h = (h ^ (h >> vec4u(16u))) * 2246822519u;
    h = h ^ (h >> vec4u(13u));
    return vec4f(h >> vec4u(8u)) * (1.0 / 16777216.0);
}

fn value_noise(p: vec2f) -> f32 {
    let cell = vec2i(floor(p));
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let h = lattice_corners(cell);
    return mix(mix(h.x, h.y, u.x), mix(h.z, h.w, u.x), u.y);
}

// The four hashes `lattice_corners` gives `cell`, fetched from the frame's noise
// lattice (`noise_lattice.rs`): the same values, to 8 bits, within 128 cells of the
// origin, repeating every 256 cells beyond. One texel read in place of the hashing,
// for the sky's clouds; the output pass, which has no lattice, never reaches it.
fn lattice_corners_fetched(cell: vec2i) -> vec4f {
    return textureLoad(noise_lattice, (cell + vec2i(128)) & vec2i(255), 0);
}

// `value_noise` from the frame's noise lattice.
fn lattice_noise(p: vec2f) -> f32 {
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let h = lattice_corners_fetched(vec2i(floor(p)));
    return mix(mix(h.x, h.y, u.x), mix(h.z, h.w, u.x), u.y);
}

// The atmosphere map (`atmosphere_map.rs`): its texels per side, the half side in
// metres of the square its relief covers, the range its slopes are stored in, and the
// broad cloud octave's cells in one tile of its cloud noise.
const ATMOSPHERE_MAP_SIZE: f32 = 256.0;
const RELIEF_HALF_EXTENT: f32 = 192.0;
const RELIEF_SLOPE_RANGE: f32 = 0.3;
const CLOUD_PERIOD: f32 = 16.0;
// Distances from the arena centre, metres along either axis, over which the relief
// fades out before the map's edge.
const RELIEF_FADE: vec2f = vec2f(172.0, 188.0);

struct GroundMacro {
    normal: vec3f,
    // -1 in the hollows to 1 on the crests, scaled by `ground.y`.
    tone: f32,
    // How far the crests bleach, 0..`ground.z`.
    dry: f32,
    // 1 on open, rough ground facing up, 0 on walls, paint and steep faces.
    weight: f32,
}

// Large flat ground reads as one ironed sheet from the overhead camera. Rough
// surfaces facing up take a gentle rolling relief of broad swells and finer hummocks
// (baked into the atmosphere map), painted into their normal only (collision,
// navigation and shadows keep the flat plane), and broad albedo patches whose drier
// crests lighten and warm while hollows stay darker and richer, so a field breaks into
// light and shade masses. Its weight is zero on walls, glossy paint and steep faces,
// which skip the map.
fn ground_macro(world: vec3f, normal: vec3f, roughness: f32) -> GroundMacro {
    var result: GroundMacro;
    result.normal = normal;
    result.tone = 0.0;
    result.dry = 0.0;
    let weight = smoothstep(0.8, 0.95, normal.y) * smoothstep(0.55, 0.7, roughness);
    result.weight = weight;
    if weight <= 0.0 || (frame.ground.x <= 0.0 && frame.ground.y <= 0.0) {
        return result;
    }
    let uv = world.xz * (0.5 / RELIEF_HALF_EXTENT) + 0.5;
    let relief = textureSampleLevel(atmosphere_map, lut_sampler, uv, 0.0);
    let span = max(abs(world.x), abs(world.z));
    let share = weight * (1.0 - smoothstep(RELIEF_FADE.x, RELIEF_FADE.y, span));
    let slope = (relief.xy - 0.5) * (RELIEF_SLOPE_RANGE * frame.ground.x * share);
    result.normal = normalize(normal - vec3f(slope.x, 0.0, slope.y));
    // -1 in the hollows to 1 on the crests.
    let height = relief.z * 2.0 - 1.0;
    result.tone = height * frame.ground.y * share;
    result.dry = max(height, 0.0) * frame.ground.z * share;
    return result;
}

// A surface color with its ground variation: the crests' sun-bleached color fades
// toward a pale straw grey, so dry grass yellows no further toward lime and bare
// earth no further toward orange, and the whole patch lightens or darkens.
fn apply_ground_macro(color: vec3f, ground: GroundMacro) -> vec3f {
    let bleached = vec3f(dot(color, vec3f(0.2126, 0.7152, 0.0722))) * BLEACHED_TINT;
    return mix(calm_yellow_green(color, ground.weight), bleached, ground.dry) * (1.0 + ground.tone);
}

// Green over red (linear albedo) where ground turns from earth to the acid
// yellow-green of a lime meadow edge, and where it is a true green again; earth,
// sand and hazard paint stay below the first.
const YELLOW_GREEN_FROM: vec2f = vec2f(0.66, 0.84);
const TRUE_GREEN_FROM: vec2f = vec2f(1.25, 1.7);
// How much red, and how much of its missing blue, a yellow-green surface takes on.
const YELLOW_GREEN_RED: f32 = 0.18;
const YELLOW_GREEN_BLUE: f32 = 0.06;

// Grass that has yellowed to an acid lime reads as a paint fill from the overhead
// camera: on the ground it turns toward a natural sap green, a little less red and a
// touch of blue, before the macro tone and bleach. Muted colours keep their hue.
fn calm_yellow_green(color: vec3f, weight: f32) -> vec3f {
    if weight <= 0.0 {
        return color;
    }
    let ratio = color.g / max(color.r, 1e-4);
    let vivid = 1.0 - color.b / max(color.g, 1e-4);
    let yellow = smoothstep(YELLOW_GREEN_FROM.x, YELLOW_GREEN_FROM.y, ratio)
        * (1.0 - smoothstep(TRUE_GREEN_FROM.x, TRUE_GREEN_FROM.y, ratio))
        * smoothstep(0.45, 0.7, vivid) * weight;
    var c = color;
    c.r *= 1.0 - YELLOW_GREEN_RED * yellow;
    c.b = mix(c.b, c.g * 0.35, YELLOW_GREEN_BLUE * yellow);
    return c;
}

// The tint of bleached crests at unit luminance: a faint straw.
const BLEACHED_TINT: vec3f = vec3f(1.06, 1.0, 0.86);

// Sky brightness rises toward the horizon by this power of (1 - up).
const SKY_HORIZON_EXPONENT: f32 = 3.2;

// The sky's dome alone: overhead to horizon, brighter on the sun's side of the sky
// and deeper opposite it, so every view has a lit and a shaded quarter instead of
// one even band around the horizon. Glossy finishes reflect just this (their lobe
// blurs the halo away), which also keeps the largest surface variants, which inline
// it beside the haze's full sky, small enough for WebGL's Metal translation.
fn sky_dome(direction: vec3f) -> vec3f {
    let up = clamp(direction.y, 0.0, 1.0);
    let toward_horizon = pow(1.0 - up, SKY_HORIZON_EXPONENT);
    let sky = mix(frame.sky_zenith.rgb, frame.sky_horizon.rgb, toward_horizon);
    let sunward = dot(normalize(direction.xz + vec2f(1e-5)), normalize(frame.sun_direction.xz + vec2f(1e-5))) * 0.5 + 0.5;
    return sky * (1.0 + SKY_SUNWARD_GAIN * (sunward * sunward - 0.4) * (0.35 + 0.65 * toward_horizon));
}

// The sky without sun disc or clouds: the dome and the sun's forward-scattered halo,
// with a broad warm glow along the sunward horizon. Distant surfaces fade into it, so
// far hills meet the horizon without a seam.
fn sky_gradient(direction: vec3f) -> vec3f {
    let sky = sky_dome(direction);
    let toward_horizon = pow(1.0 - clamp(direction.y, 0.0, 1.0), SKY_HORIZON_EXPONENT);
    let sun = max(dot(direction, frame.sun_direction.xyz), 0.0);
    let halo = pow(sun, 8.0) * (0.1 + 0.4 * toward_horizon) + pow(sun, 48.0) * 0.5
        + SKY_SUNWARD_WARMTH * pow(sun, 1.5) * toward_horizon;
    // Around the sun the blue gives way to the sun's own hue before its light adds
    // on top: warm light added over blue alone mixes to pink and lavender.
    let sun_hue = frame.sky_sun.rgb / max(dot(frame.sky_sun.rgb, vec3f(0.2126, 0.7152, 0.0722)), 1e-4);
    let warmed = mix(sky, sun_hue * dot(sky, vec3f(0.2126, 0.7152, 0.0722)), min(halo * HALO_WARMING, 0.9));
    return no_mauve(warmed + frame.sky_sun.rgb * halo);
}

// A blue zenith blended into a peach or sand horizon passes through mauve, which read
// as lavender patches between clouds and magenta in-scatter over far hills. Green
// keeps at least a share of the way from the lesser of red and blue to the greater,
// as it does in a clear blue sky, so the blend turns through a soft grey instead.
fn no_mauve(color: vec3f) -> vec3f {
    let low = min(color.r, color.b);
    let least_green = low + (max(color.r, color.b) - low) * NO_MAUVE_GREEN;
    return vec3f(color.r, max(color.g, least_green), color.b);
}

const NO_MAUVE_GREEN: f32 = 0.3;

// How quickly the sky's blue turns to the sun's hue under its halo.
const HALO_WARMING: f32 = 2.5;
// The sun-relative dome (`sky_gradient`): how much brighter the sky's sunward side
// is than the side away from it, and the broad warm glow along the sunward horizon.
const SKY_SUNWARD_GAIN: f32 = 0.35;
const SKY_SUNWARD_WARMTH: f32 = 0.04;

// The near range of the horizon's distant ranges stands this much lower than the far
// one, and each takes this share of the ridge color over the sky behind it.
const NEAR_RIDGE_SCALE: f32 = 0.62;
const FAR_RIDGE_SHARE: f32 = 0.6;
const NEAR_RIDGE_SHARE: f32 = 0.85;
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
const RIDGE_FOOT_HAZE: f32 = 0.55;
const RIDGE_SLOPE_STEP: f32 = 0.004;
const RIDGE_FACE_LIGHT: f32 = 0.1;
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
// aerial tint, thick haze toward the sky behind them, so silhouettes recede in steps
// of value and saturation rather than bleach. Away from the sun even thick haze keeps
// some of the cool tint, so the far layers step from warm to cool across the view.
// Toward the sun it takes on the sun's own hue and brightens a little; the hue
// replaces the cool tint's rather than adding warm light over blue, which turned
// sunward hills lavender and pink.
fn haze_tint(ray: vec3f, amount: f32) -> vec3f {
    return haze_tint_over(sky_gradient(ray), ray, amount);
}

// `haze_tint` over `sky`, the sky's own gradient along `ray`, which a caller that
// also needs it computes once.
fn haze_tint_over(sky: vec3f, ray: vec3f, amount: f32) -> vec3f {
    let sun = frame.sun_direction.xyz;
    let toward_sun = max(dot(ray, sun), 0.0);
    let away = clamp(-dot(normalize(ray.xz + vec2f(1e-5)), normalize(sun.xz + vec2f(1e-5))), 0.0, 1.0);
    // The sky's own gradient, not the ranges drawn into it: those stand behind
    // every surface, and their azimuth-banded faces and crest line painted into the
    // haze over a nearer mountain drew vertical streaks and a horizontal smear.
    let thick = mix(sky, frame.aerial.rgb, HAZE_AWAY_COOL * away);
    // Even the thickest haze keeps a share of the aerial tint, so a far range or
    // headland stays a solid silhouette a step off the sky behind it, never a
    // translucent card.
    let base = mix(frame.aerial.rgb, thick, amount * amount * HAZE_SKY_MOST);
    let glow = frame.haze_far.z * toward_sun * toward_sun * amount;
    let level = dot(base, vec3f(0.2126, 0.7152, 0.0722));
    let sun_hue = frame.sky_sun.rgb / max(dot(frame.sky_sun.rgb, vec3f(0.2126, 0.7152, 0.0722)), 1e-4);
    // The cool aerial tint and the sun's warm hue meet through grey, never lavender.
    return no_mauve(mix(base, sun_hue * level * (1.0 + HAZE_SUN_BRIGHTENING), clamp(glow, 0.0, 1.0)));
}

// How much of its own colour a surface loses per share of the haze over it.
const HAZE_DRAIN: f32 = 0.4;
// How much brighter than the cool haze the sunward glow is, and the most of the sky
// behind thick haze takes on.
const HAZE_SUN_BRIGHTENING: f32 = 0.2;
const HAZE_SKY_MOST: f32 = 0.55;

// The share of the cool aerial tint thick haze keeps looking straight away from the sun.
const HAZE_AWAY_COOL: f32 = 0.35;
// Ray slopes (downward) over which haze thins for a camera looking steeply down: such a
// ray crosses the low haze layer in a few metres, so the overhead and overview cameras
// see the play space and its surroundings clean while near-level views get the depth.
const STEEP_RAY_HAZY: f32 = 0.2;
const STEEP_RAY_CLEAR: f32 = 0.5;
// The share of the haze that still lies over the far ground in an overhead view: a
// hint of air over the sea and far fields, never a veil.
const STEEP_RAY_RESIDUAL: f32 = 0.3;

// The sky as drawn behind a surface along `ray` (`sky`, its gradient there), distant
// ranges included (no clouds or sun disc), so surfaces that melt into it leave no
// lighter patch across them.
fn sky_behind(sky: vec3f, ray: vec3f) -> vec3f {
    return horizon_ridges(sky, ray, FADED_RIDGE_EDGE);
}

// The far haze layer thins up a slope at this share of the near layer's falloff, to
// this share of its density on the highest crests.
const FAR_HAZE_THINNING: f32 = 1.0;
const FAR_HAZE_CREST_SHARE: f32 = 0.6;
// The most the far layer hides: a distant range keeps this much of its own form.
const FAR_HAZE_MOST: f32 = 0.78;
// Height in metres over which the low mist beyond the arena thins by e.
const MIST_HEIGHT: f32 = 10.0;
// Shares of the far plane's distance over which surfaces melt into the sky.
const FAR_FADE_START: f32 = 0.965;
const FAR_FADE_END: f32 = 0.998;

// Aerial perspective in two layers, seen from `eye` (the mirrored eye for the water's
// reflection) at `world`, `view_depth` along that view's axis: x the share of the
// surface's colour the haze takes, y the share the far fade then takes toward the sky
// behind it. Exponential height haze, integrated from where it begins to the surface,
// lies dense in the valleys and thin up high, so a camera looking down from above
// stays clean. Beyond `haze_far.y` a thinner haze that does not fall off with height
// takes over, so distant peaks recede as a whole instead of standing hard against the
// horizon while their feet wash out; it never hides more than `FAR_HAZE_MOST`, so each
// range stays an opaque landform a step paler than the one before it, and the sky's
// own ranges stand behind them. Inside the arena square only a share of the height
// haze settles and none of the far haze; toward the far plane every surface melts
// into the sky, so nothing the far plane clips draws a hard horizon.
fn haze_weights(eye: vec3f, world: vec3f, view_depth: f32) -> vec2f {
    let offset = world - eye;
    let distance = length(offset);
    let ray = offset / max(distance, 1e-4);
    let start = frame.haze.z;
    let travelled = distance - start;
    if travelled <= 0.0 {
        return vec2f(0.0);
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
    let level = mix(1.0, STEEP_RAY_RESIDUAL, smoothstep(STEEP_RAY_HAZY, STEEP_RAY_CLEAR, -ray.y));
    let near_amount = min(1.0 - exp(-depth), frame.haze.w) * backdrop * level;
    let far_amount = (1.0 - exp(-far_depth)) * FAR_HAZE_MOST * beyond * level;
    // Low mist settles in the valleys beyond the arena, thick at the ground and
    // gone a few storeys up, so distant hills stand out of it with clear crests.
    let mist = frame.haze_far.w * exp(-max(world.y, 0.0) / MIST_HEIGHT) * beyond * level
        * smoothstep(frame.haze_far.y * 0.7, frame.haze_far.y * 1.4, distance);
    let clear = (1.0 - near_amount) * (1.0 - far_amount) * (1.0 - mist);
    let amount = max(min(1.0 - clear, frame.haze.w), 0.0);
    // Late and short, so the far mesas and ranges keep their own form and only what
    // the far plane is about to clip melts away. It follows view depth, as the far
    // plane does: a distant face fades evenly across its width instead of smearing at
    // the sides that curve away.
    let far = frame.haze_shape.w;
    let fade = select(0.0, smoothstep(far * FAR_FADE_START, far * FAR_FADE_END, view_depth), far > 0.0);
    return vec2f(amount, fade);
}

// Whether `world` lies further from the eye than the haze begins (`haze.z`). Nearer
// surfaces, most of an overhead frame, take no haze at all (`haze_weights` is zero
// there), so they skip its terms.
fn beyond_haze_start(world: vec3f) -> bool {
    let offset = world - frame.camera_position.xyz;
    return dot(offset, offset) > frame.haze.z * frame.haze.z;
}

// What remains of a surface's own colour under the haze `weights`. Air between eye
// and surface first drains a surface's colour and only then tints it: a warm mesa or
// green slope mixed straight into the cool aerial tint passed through mauve and read
// as a translucent card. Drained first, it steps back as an opaque, cooler and paler
// band.
fn haze_kept(color: vec3f, weights: vec2f) -> vec3f {
    let drained = mix(color, vec3f(dot(color, vec3f(0.2126, 0.7152, 0.0722))), weights.x * HAZE_DRAIN);
    return drained * ((1.0 - weights.x) * (1.0 - weights.y));
}

// The light the haze `weights` add along `ray` in place of the colour they take: the
// haze's tint, then toward the far plane the sky behind (its gradient computed once
// for both).
fn haze_inscatter(ray: vec3f, weights: vec2f) -> vec3f {
    let sky = sky_gradient(ray);
    var light = haze_tint_over(sky, ray, weights.x) * weights.x;
    if weights.y > 0.0 {
        light = mix(light, sky_behind(sky, ray), weights.y);
    }
    return light;
}

fn aerial_haze(color: vec3f, eye: vec3f, world: vec3f, view_depth: f32) -> vec3f {
    let weights = haze_weights(eye, world, view_depth);
    if weights.x <= 0.0 && weights.y <= 0.0 {
        return color;
    }
    return haze_kept(color, weights) + haze_inscatter(normalize(world - eye), weights);
}

// The view's fog at `world`: the atmosphere's haze, or without it Three's linear fog.
fn apply_fog(color: vec3f, world: vec3f) -> vec3f {
    if atmosphere_on() {
        return aerial_haze(color, frame.camera_position.xyz, world, view_depth(world));
    }
    return linear_fog(color, world);
}

fn view_depth(world: vec3f) -> f32 {
    return -(frame.view * vec4f(world, 1.0)).z;
}

fn linear_fog(color: vec3f, world: vec3f) -> vec3f {
    if frame.fog_color.w > 0.5 {
        return mix(color, frame.fog_color.rgb, smoothstep(frame.fog_range.x, frame.fog_range.y, view_depth(world)));
    }
    return color;
}

// With the atmosphere on, opaque surfaces leave the haze's in-scattered light to the
// pass that reads them, which adds it once per pixel rather than once per overdrawn
// layer and keeps the sky and the ranges it looks up out of every surface variant's
// code: the output pass for the main view, the water for its mirror. They write the
// colour the haze leaves them (`haze_kept`) and, as alpha, 1 + the share it takes
// (`haze_alpha`). Both blend and resolve like colour: a blended layer over them,
// drawn with ordinary alpha blending, leaves the share of their haze that shows
// through it, and alpha at or below 1 takes none (the sky, the water and
// alpha-to-coverage cutouts, which haze themselves).
fn haze_alpha(weights: vec2f) -> f32 {
    return 2.0 - (1.0 - weights.x) * (1.0 - weights.y);
}

// The weights back from `haze_alpha`. The two shares travel as their product: the haze
// takes its most before the far fade begins, so the fade is what lies beyond it.
fn haze_weights_of(alpha: f32) -> vec2f {
    let taken = clamp(alpha - 1.0, 0.0, 1.0);
    let amount = min(taken, frame.haze.w);
    return vec2f(amount, (taken - amount) / max(1.0 - amount, 1e-4));
}

// A blended layer's own haze, over what it blends onto: the haze's plain tint, the
// cool aerial colour thickening toward the horizon's, and none of the sky it would
// look up for the opaque surfaces.
fn blended_haze(color: vec3f, weights: vec2f) -> vec3f {
    let tint = mix(frame.aerial.rgb, frame.sky_horizon.rgb, weights.x * weights.x * HAZE_SKY_MOST);
    return mix(haze_kept(color, vec2f(weights.x, 0.0)) + tint * weights.x, frame.sky_horizon.rgb, weights.y);
}

// The mirror's haze for a sample of the water's reflection, drawn like the main view
// (`haze_alpha`) from the eye mirrored under the plane at `height`, seen where the
// water's surface at `surface` reflects it.
fn mirrored_haze(sample: vec4f, surface: vec3f, height: f32) -> vec3f {
    if !atmosphere_on() || sample.a <= 1.0 {
        return sample.rgb;
    }
    let eye = frame.camera_position.xyz;
    let mirrored = vec3f(eye.x, 2.0 * height - eye.y, eye.z);
    return sample.rgb + haze_inscatter(normalize(surface - mirrored), haze_weights_of(sample.a));
}

// Ground-scale cloud shadow features per metre (cells of the noise's broad octave),
// and their drift in metres per second.
const CLOUD_SHADOW_SCALE: f32 = 0.018;
const CLOUD_SHADOW_DRIFT: vec2f = vec2f(1.1, 0.45);
// Where the shadow noise starts and completes a cloud, above the coverage.
const CLOUD_SHADOW_LOW: f32 = 1.0;
const CLOUD_SHADOW_HIGH: f32 = 1.3;
// Distances from the arena centre in metres over which cloud shadows fade out.
const CLOUD_SHADOW_FAR: vec2f = vec2f(85.0, 125.0);

// The share of sunlight that passing clouds let through at `world`: soft patches,
// projected along the sun so tall things and the ground beneath agree.
fn cloud_shadow(world: vec3f) -> f32 {
    let strength = frame.cloud_shade.w;
    if strength <= 0.0 {
        return 1.0;
    }
    let sun = frame.sun_direction.xyz;
    let ground = world.xz - sun.xz / max(sun.y, 0.2) * world.y;
    // Tiles of the atmosphere map's cloud noise, wrapped into its first 255 texels
    // (the last repeats the first, so the filter crosses the seam).
    let tiles = (ground + CLOUD_SHADOW_DRIFT * frame.camera_position.w) * (CLOUD_SHADOW_SCALE / CLOUD_PERIOD);
    let uv = (fract(tiles) * (ATMOSPHERE_MAP_SIZE - 1.0) + 0.5) / ATMOSPHERE_MAP_SIZE;
    let n = textureSampleLevel(atmosphere_map, lut_sampler, uv, 0.0).a;
    let coverage = frame.cloud_lit.w;
    // Broad patches: about the sky's own share of the ground lies under a cloud, so
    // the field reads in large light and shade masses rather than one flat sheet.
    let cover = smoothstep(CLOUD_SHADOW_LOW - coverage, CLOUD_SHADOW_HIGH - coverage, n);
    // On the far backdrop the patches shrink to blots that smudge mountain faces and
    // snowfields like a painted card: they fade out beyond the arena's surroundings.
    let far = smoothstep(CLOUD_SHADOW_FAR.x, CLOUD_SHADOW_FAR.y, length(world.xz));
    return 1.0 - strength * cover * (1.0 - far);
}

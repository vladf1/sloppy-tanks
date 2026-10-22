// Planar-reflection water, ported from water-surface.ts (a MeshBasicNodeMaterial
// colorNode): four scrolling normal-map samples for the swell (one turned off the
// others' grid so the tile never lines up) and a fast fifth for glitter, a Fresnel
// mix of the mirrored scene with a depth-tinted scatter color, a sun glint, foam
// that laps against everything standing in the water and, in the harbor, the
// wakes of the tugs working the side channels. Group 0 is the main view's frame;
// the reflection is rendered first with the mirror camera.

struct Water {
    // Unit vector toward the sun.
    sun_direction: vec4f,
    sun_color: vec4f,
    deep_color: vec4f,
    shallow_color: vec4f,
    // x world-to-ripple scale, y distortion scale, z normal strength, w ripple time.
    settings: vec4f,
    // x shore mode (0 square harbor basin, 1 creek across uv.x), y basin half size,
    // z surface height.
    shore: vec4f,
}

@group(1) @binding(0) var<uniform> water: Water;
@group(1) @binding(1) var water_normals: texture_2d<f32>;
@group(1) @binding(2) var water_sampler: sampler;
@group(1) @binding(3) var reflection: texture_2d<f32>;
@group(1) @binding(4) var reflection_sampler: sampler;

struct WaterOut {
    @builtin(position) clip: vec4f,
    @location(0) world: vec3f,
    @location(1) uv: vec2f,
}

// What stands in the harbor basin besides the quay, as (centre x, centre z, half
// x, half z) in metres, axis-aligned: the moored hulls (`harbor_vessels::SHIPS`,
// an outline from x -34 to 36 and z ±7 before each ship's scale and quarter-turn
// yaw; the sheer boot-top band meets the water at the full outline), then the
// crane plinths (`CRANES`, 14 × 9 m), the forklift berths and the
// warehouse quays (`harbor_scenery`). Foam and hull shade hug these footprints: a
// footprint wider than the hull left a band of water inside it, which foamed into
// an even white outline round every ship.
const HULLS = array<vec4f, 3>(
    vec4f(-7.0, -80.0, 35.0, 7.0),
    vec4f(-77.0, -8.85, 5.95, 29.75),
    vec4f(77.0, 15.72, 5.04, 25.2),
);
// Which way along its long axis each hull's bow points (`harbor_vessels::SHIPS`
// yaw), and its waterline taper (`harbor_vessels::OUTLINE`) as fractions of the
// half length from the middle: the bow narrows from 0.66 to the shoulder at 0.86
// (at 0.69 of the beam) and to the stem at 1, the stern from 0.89 to 0.69 of the
// beam at the transom.
const HULL_BOWS = array<f32, 3>(1.0, -1.0, 1.0);
const HULL_ROUNDING = 1.0;
// Height above the water of the hulls' shadow-casting bulk (freeboard and the
// lower tiers of cargo), metres.
const HULL_SHADOW_HEIGHT = 7.0;
const PIERS = array<vec4f, 8>(
    vec4f(-40.0, -65.0, 7.0, 4.5),
    vec4f(40.0, -65.0, 7.0, 4.5),
    vec4f(-65.5, 16.0, 3.375, 5.25),
    vec4f(65.5, -20.0, 3.375, 5.25),
    vec4f(-65.0, 43.0, 3.5, 6.5),
    vec4f(65.0, 43.0, 3.5, 6.5),
    vec4f(-115.0, 0.0, 11.0, 66.0),
    vec4f(115.0, 0.0, 11.0, 66.0),
);
// The tugs' loops (`harbor_vessels::TUG_LANES`): centre x, half the straight
// along z, turning radius and metres along the loop at time 0; their speed
// (`TUG_SPEED`, m/s) and the hull's half length. Wakes are drawn from where each
// tug was along its loop, so they follow it round the turns.
const TUGS = array<vec4f, 2>(
    vec4f(-93.5, 46.0, 5.0, 0.0),
    vec4f(93.0, 38.0, 4.5, 120.0),
);
const TUG_SPEED = 3.2;
const TUG_HALF_LENGTH = 6.0;
const TUG_HALF_BEAM = 2.3;
// Seconds a wake lasts, and the half angle of its Kelvin arms (tan 19.5°).
const WAKE_SECONDS = 24.0;
const KELVIN_SLOPE = 0.354;
const WATER_PI = 3.14159265;

// Linear colors of the harbor's own grade: churned white foam, the bright teal a
// sunlit swell shows through its crest, and the water body from the silty green
// against the quays through the basin's teal to the dredged channel's blue.
const FOAM_COLOR = vec3f(0.8, 0.85, 0.85);
const CREST_COLOR = vec3f(0.035, 0.19, 0.17);
const HARBOR_SHALLOW = vec3f(0.034, 0.105, 0.078);
const HARBOR_BASIN = vec3f(0.012, 0.062, 0.078);
const HARBOR_DEEP = vec3f(0.004, 0.022, 0.052);
// The dark wet shade the water takes in the lee of a quay face or a hull: the
// sky half hidden and the swell slapping back, so a wall stands in its water
// instead of floating on a lit sheet.
const HARBOR_CONTACT = vec3f(0.006, 0.026, 0.026);
// The rubble toe the quays and piers stand on, seen through the water: its lit
// stone, the metres of water that hide half of it, how deep it lies at a wall's
// foot and how fast it shelves away, and the most of it that ever shows.
const TOE_COLOR = vec3f(0.11, 0.2, 0.15);
const TOE_CLARITY = 1.1;
const TOE_FOOT = 0.25;
const TOE_SHELF = 0.3;
const TOE_SHOW = 0.5;
// Fresnel reflectance of the harbor water seen straight down: real water's 2%
// lifted a little so the sky still tints it from above, without hiding the
// water's own depth colour.
const HARBOR_F0 = 0.07;
// How far above the sky's own gradient a reflected highlight may rise: brighter
// cloud tops broken up by the swell read as a tiled white pattern from above, so
// they are held down to it, while the darker cranes, hulls and shores mirror whole.
const HARBOR_CLOUD_CEILING = 1.12;
const WATER_LUMA = vec3f(0.2126, 0.7152, 0.0722);
// Metres from the nearest face where the basin turns to channel.
const HARBOR_CHANNEL = 34.0;
// Metres from the eye over which the mirror turns glossy, the screen fraction its
// streaks reach down the view at most, the share of sky the far facets show and
// the most the water ever mirrors at a grazing angle.
const HARBOR_GLOSSY_NEAR = 35.0;
const HARBOR_GLOSSY_FAR = 190.0;
const HARBOR_SMEAR = 0.011;
const HARBOR_SKY_SHARE = 0.3;
const HARBOR_MIRROR_MAX = 0.9;
// Shadow-map texels between the water shadow's taps, and how dark the water body
// goes in the shade of a hull or crane (the sky still lights it).
const WATER_SHADOW_SPREAD = 1.5;
const WATER_SHADE = 0.3;
// The share of the scene's haze the harbor's own water takes.
const HARBOR_FOG_SHARE = 0.72;

fn rounded_box_distance(p: vec2f, footprint: vec4f, radius: f32) -> f32 {
    let q = abs(p - footprint.xy) - footprint.zw + radius;
    return length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

fn water_hash(p: vec2f) -> f32 {
    var q = fract(p * vec2f(0.1031, 0.1030));
    q = q + dot(q, q.yx + 33.33);
    return fract((q.x + q.y) * q.x);
}

fn water_noise(p: vec2f) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(water_hash(i), water_hash(i + vec2f(1.0, 0.0)), u.x),
        mix(water_hash(i + vec2f(0.0, 1.0)), water_hash(i + vec2f(1.0, 1.0)), u.x),
        u.y,
    );
}

// Metres from `p` to the waterline of hull `i`, negative inside: its half beam
// where the point lies along the hull, so the water hugs the pointed bow and the
// tapered stern, not a box round them.
fn hull_distance(p: vec2f, i: i32) -> f32 {
    let hull = HULLS[i];
    let along_x = hull.z > hull.w;
    let q = p - hull.xy;
    let along = select(q.y, q.x, along_x);
    let across = select(q.x, q.y, along_x);
    let reach = select(hull.w, hull.z, along_x);
    let breadth = select(hull.z, hull.w, along_x);
    let u = along * HULL_BOWS[i] / reach;
    let bow = select(mix(1.0, 0.69, (u - 0.66) / 0.2), mix(0.69, 0.0, (u - 0.86) / 0.14), u > 0.86);
    let taper = select(select(1.0, mix(1.0, 0.69, (-u - 0.89) / 0.11), u < -0.89), bow, u > 0.66);
    return rounded_box_distance(vec2f(along, across), vec4f(0.0, 0.0, reach, breadth * clamp(taper, 0.0, 1.0)), HULL_ROUNDING);
}

// How deep in the moored hulls' shadow `p` lies, 0..1: the sun-lit shadow map ends
// at the arena's margin, so each hull's freeboard and cargo are swept away from the
// sun as its outline raised step by step, with a soft edge for the rippled water.
fn hull_shade(p: vec2f) -> f32 {
    let sun = frame.sun_direction.xyz;
    let reach = -sun.xz / max(sun.y, 0.2);
    var shade = 0.0;
    for (var i = 0; i < 3; i++) {
        // Only hulls whose shadow could reach this far are swept.
        if hull_distance(p, i) > HULL_SHADOW_HEIGHT * length(reach) + 2.0 {
            continue;
        }
        for (var k = 1; k <= 4; k++) {
            let d = hull_distance(p - reach * (HULL_SHADOW_HEIGHT * f32(k) * 0.25), i);
            shade = max(shade, 1.0 - smoothstep(-1.2, 1.4, d));
        }
    }
    return shade;
}

// What the water meets near a harbor point: x the metres to the nearest quay
// face, hull or pier; y how much of the churn that face throws up (a hull's bow
// and stern most, its flat sides least); z unused (a moored hull trails no wash);
// w 1 where that face stands on a rubble toe (a quay or pier), 0 at a hull.
fn harbor_shore(p: vec2f) -> vec4f {
    var out = vec4f(max(abs(p.x), abs(p.y)) - water.shore.y, 0.55, 0.0, 1.0);
    for (var i = 0; i < 3; i++) {
        let hull = HULLS[i];
        let along_x = hull.z > hull.w;
        let q = p - hull.xy;
        let along = select(q.y, q.x, along_x);
        let reach = select(hull.w, hull.z, along_x);
        let d = hull_distance(p, i);
        if d < out.x {
            out.x = d;
            // Moored, so no bow wave: the swell slaps white round the bow and the
            // transom and only laps along the flat sides, where an even white
            // outline would read as a sticker round the hull.
            out.y = 0.06 + 0.84 * smoothstep(0.55, 1.0, abs(along) / reach);
            out.w = 0.0;
        }
    }
    for (var i = 0; i < 8; i++) {
        let d = rounded_box_distance(p, PIERS[i], 0.3);
        if d < out.x {
            out.x = d;
            out.y = 0.7;
            out.w = 1.0;
        }
    }
    return out;
}

// Where `p` lies against a tug's stadium loop: x the metres along the loop of
// its nearest point (as `harbor_vessels::tug_pose` counts them), y the signed
// metres off the loop, positive outside it.
fn tug_loop_place(p: vec2f, lane: vec4f) -> vec2f {
    let half = lane.y;
    let radius = lane.z;
    let straight = 2.0 * half;
    let turn = WATER_PI * radius;
    if abs(p.y) <= half {
        if p.x >= lane.x {
            return vec2f(p.y + half, p.x - lane.x - radius);
        }
        return vec2f(straight + turn + half - p.y, lane.x - p.x - radius);
    }
    if p.y > half {
        let q = p - vec2f(lane.x, half);
        return vec2f(straight + atan2(q.y, q.x) * radius, length(q) - radius);
    }
    let q = p - vec2f(lane.x, -half);
    return vec2f(2.0 * straight + turn + (atan2(q.y, q.x) + WATER_PI) * radius, length(q) - radius);
}

// The tugs' wakes at `p`, `time` seconds into the presentation clock: x foam,
// y the ridges of the Kelvin arms and the transverse waves between them (they
// roughen the surface), z the propeller wash churned up behind the stern.
fn tug_wakes(p: vec2f, time: f32) -> vec3f {
    var out = vec3f(0.0);
    for (var i = 0; i < 2; i++) {
        add_tug_wake(&out, p, time, i);
    }
    return out;
}

// The sun's shadow on the water: what the moored hulls, the cranes and the quay
// throw across the basin, from four hardware-filtered taps spread wide, since a
// shadow on rippled water has a soft edge. Outside the shadow map the sun is clear.
fn water_shadow(world: vec3f) -> f32 {
    let size = frame.shadow.z;
    if size <= 0.0 {
        return 1.0;
    }
    let position = frame.shadow_matrix * vec4f(world, 1.0);
    let projected = position.xyz / position.w;
    let coord = vec3f(projected.x, 1.0 - projected.y, projected.z + frame.shadow.x);
    let spread = WATER_SHADOW_SPREAD * max(frame.shadow.w, 1.0) / size;
    var sum = 0.0;
    for (var i = 0; i < 4; i++) {
        let corner = vec2f(f32(i & 1), f32(i >> 1u)) * 2.0 - 1.0;
        sum += textureSampleCompareLevel(shadow_map, shadow_sampler, coord.xy + corner * spread, coord.z);
    }
    let inside = coord.x >= 0.0 && coord.x <= 1.0 && coord.y >= 0.0 && coord.y <= 1.0 && coord.z <= 1.0;
    return select(1.0, sum * 0.25, inside);
}

// Tug `i`'s wake added to `tug_wakes`'s `out` (a function of its own, so the loop
// over the tugs needs no `continue` and unrolls).
fn add_tug_wake(wakes: ptr<function, vec3f>, p: vec2f, time: f32, i: i32) {
    var out = *wakes;
    let lane = TUGS[i];
    let perimeter = 2.0 * (2.0 * lane.y + WATER_PI * lane.z);
    let boat = (lane.w + time * TUG_SPEED) % perimeter;
    let place = tug_loop_place(p, lane);
    let across = abs(place.y);
    if across > 26.0 {
        return;
    }
    // Metres behind the tug's middle along its loop, and ahead of it.
    let behind = ((boat - place.x) % perimeter + perimeter) % perimeter;
    let ahead = perimeter - behind;
    if ahead < TUG_HALF_LENGTH + 1.0 {
        // Bow wave: a collar of white water pushed off the stem and shoulders.
        let shoulder = TUG_HALF_BEAM * (1.0 - smoothstep(TUG_HALF_LENGTH - 2.5, TUG_HALF_LENGTH + 0.3, ahead));
        let collar = exp(-pow((across - shoulder - 0.35) / 0.4, 2.0));
        out.x = max(out.x, collar * 0.9);
        out.y = out.y + collar;
    }
    let age = behind / TUG_SPEED;
    if age < WAKE_SECONDS {
        let fade = exp(-age / 7.0);
        // From the bow, the arms spread at the Kelvin angle.
        let from_bow = behind + TUG_HALF_LENGTH;
        let arm = TUG_HALF_BEAM + from_bow * KELVIN_SLOPE;
        let arm_width = 0.35 + age * 0.06;
        let arm_line = exp(-pow((across - arm) / arm_width, 2.0)) * fade;
        let inside = 1.0 - smoothstep(arm * 0.55, arm, across);
        let transverse = (0.5 + 0.5 * cos(from_bow * 1.4)) * inside * fade;
        // Propeller wash: white and churned off the stern, widening and
        // settling into a glassy scar whose edges the swell breaks up and
        // bends, so it never reads as a band with straight sides.
        let stern = smoothstep(TUG_HALF_LENGTH - 1.0, TUG_HALF_LENGTH + 1.0, behind);
        let wander = (water_noise(vec2f(behind * 0.09, f32(i) * 7.0)) - 0.5) * (0.6 + age * 0.12);
        let ragged = water_noise(vec2f(behind * 0.45, across * 0.6 + f32(i) * 3.0));
        let wash_width = (1.2 + age * 0.22) * (0.7 + 0.6 * ragged);
        let off_axis = abs(place.y + wander);
        let wash = (1.0 - smoothstep(wash_width * 0.3, wash_width, off_axis)) * exp(-age / 4.5) * stern;
        let scar = (1.0 - smoothstep(wash_width * 0.4, wash_width * 1.2, off_axis)) * exp(-age / 9.0) * stern
            * smoothstep(0.25, 0.6, ragged + 0.2);
        // The arms break white only close behind the boat; further back they
        // are ridges that roughen the surface.
        let arm_foam = arm_line * 0.6 * smoothstep(1.0, 4.0, from_bow) * exp(-age / 2.5);
        out.x = max(out.x, max(wash, arm_foam));
        out.y = out.y + arm_line + transverse * 0.45;
        out.z = max(out.z, max(wash, scar * 0.5));
    }
    *wakes = out;
}

@vertex
fn vs_water(@location(0) position: vec3f, @location(2) uv: vec2f) -> WaterOut {
    let world = position + vec3f(0.0, water.shore.z, 0.0);
    var out: WaterOut;
    out.clip = frame.view_projection * vec4f(world, 1.0);
    out.world = world;
    out.uv = uv;
    return out;
}

@fragment
fn fs_water(input: WaterOut) -> @location(0) vec4f {
    let t = water.settings.w;
    let p = input.world.xz * water.settings.x;
    // One of the swell samples runs on a grid turned 37° from the others', so
    // their tiles never line up into a lattice.
    let turned = vec2f(p.x * 0.8 - p.y * 0.6, p.x * 0.6 + p.y * 0.8);
    let noise = (textureSample(water_normals, water_sampler, p / 103.0 + vec2f(t / 17.0, t / 29.0))
        + textureSample(water_normals, water_sampler, turned / 107.0 - vec2f(t / -19.0, t / 31.0))
        + textureSample(water_normals, water_sampler, p / vec2f(8907.0, 9803.0) + vec2f(t / 101.0, t / 97.0))
        + textureSample(water_normals, water_sampler, p / vec2f(1091.0, 1027.0) - vec2f(t / 109.0, t / -113.0)))
        * 0.5 - 1.0;
    let ripple = textureSample(water_normals, water_sampler, input.world.xz / 3.1 + vec2f(t * 0.031, -t * 0.023)).rgb * 2.0 - 1.0;
    let breakup = textureSample(water_normals, water_sampler, input.world.xz * 0.11 + t * 0.025).r;
    let lace = textureSample(water_normals, water_sampler, input.world.xz * 0.37 - vec2f(t * 0.04, t * 0.017)).g;
    // Gusts and slicks: broad, slowly drifting patches where the wind roughens the
    // water or leaves it glassy, so the ripple tile never reads as a grid.
    let gust_a = textureSample(water_normals, water_sampler, input.world.xz / 173.0 + vec2f(t * 0.0021, t * 0.0013)).r;
    let gust_b = textureSample(water_normals, water_sampler, vec2f(input.world.z, -input.world.x) / 91.0 - vec2f(t * 0.0017, 0.0)).g;
    // How much water one pixel spans: fine foam lace and glitter fade out where
    // they would only shimmer.
    let footprint = length(fwidth(input.world));
    let harbor = water.shore.x < 0.5;
    let gust = select(0.5, smoothstep(0.38, 0.62, gust_a * 0.6 + gust_b * 0.4), harbor);
    var wake = vec3f(0.0);
    if harbor {
        wake = tug_wakes(input.world.xz, frame.camera_position.w);
    }
    let strength = water.settings.z * select(1.0, mix(0.45, 1.25, gust) * (1.0 + wake.y * 0.8 + wake.z * 0.6), harbor);
    let normal = normalize(noise.xzy * vec3f(strength, 1.0, strength));
    let glitter_normal = normalize(normal + ripple.xzy * vec3f(0.35, 0.0, 0.35));
    let world_to_eye = frame.camera_position.xyz - input.world;
    let eye = normalize(world_to_eye);
    let sun = water.sun_direction.xyz;
    let sun_color = water.sun_color.rgb;

    // Where a pixel spans many ripples, the sun's glints and the bright horizon
    // caught by their tilted facets would break into streaks of the ripple tile:
    // the Fresnel mirror and the glints take the calmer mean surface instead, so
    // far water shades as one soft sheen.
    let blurred = smoothstep(0.06, 0.45, footprint) * select(0.0, 1.0, water.shore.x < 0.5);
    let glint_normal = normalize(mix(normal, vec3f(0.0, 1.0, 0.0), 0.7 * blurred));
    // Far off the facets a pixel holds tilt every way, so the sun's highlight
    // spreads into a broad glitter path down the water toward it.
    let specular = pow(max(0.0, dot(eye, normalize(reflect(-sun, glint_normal)))), mix(100.0, 16.0, blurred))
        * sun_color * mix(2.0, 0.42, blurred);
    // Glitter: tiny facets of the fast ripples catch the sun one by one.
    let facet = max(0.0, dot(eye, normalize(reflect(-sun, glitter_normal))));
    // Where one pixel spans many ripples their sparkle would only shimmer in a
    // pattern of the ripple tile, so it fades out with distance.
    let glitter = pow(facet, 1400.0) * 14.0 * sun_color * select(0.0, 0.5 + gust, harbor)
        * (1.0 - smoothstep(0.12, 0.45, footprint));
    let diffuse = max(dot(sun, normal), 0.0) * sun_color * 0.5;
    let distortion = glint_normal.xz * (0.001 + 1.0 / length(world_to_eye)) * water.settings.y;
    let screen = input.clip.xy * frame.viewport.zw;
    let mirror_uv = vec2f(1.0 - screen.x, screen.y) + distortion;
    let mirror = textureSampleLevel(reflection, reflection_sampler, mirror_uv, 0.0).rgb;
    let theta = max(dot(eye, glint_normal), 0.0);

    var color: vec3f;
    if harbor {
        let near = harbor_shore(input.world.xz);
        let shore = near.x;
        let detail = 1.0 - smoothstep(0.08, 0.35, footprint);
        // Silty green against the quays and hulls, the basin's teal, then the
        // dredged channel's blue, with the gusts roughening it a little paler.
        // A hull floats in dredged water: no silty shallows beside it.
        var body = mix(mix(HARBOR_BASIN, HARBOR_SHALLOW, near.w), HARBOR_BASIN, smoothstep(0.5, 10.0, shore));
        body = mix(body, HARBOR_DEEP, smoothstep(10.0, HARBOR_CHANNEL, shore));
        // Broad, slowly drifting bands where the tide mixes silt into the water:
        // they read from the air where the ripple tile cannot, so the basin is
        // never one clean gradient.
        let tide_band = water_noise(input.world.xz / 47.0 + vec2f(t * 0.004, -t * 0.003)) * 0.6
            + water_noise(input.world.xz / 19.0 - vec2f(t * 0.006, 0.0)) * 0.4;
        body = body * mix(0.72, 1.06, smoothstep(0.2, 0.8, tide_band)) * mix(0.92, 1.12, gust);
        // The swell the hulls and walls throw back: faint rings of ruffled water
        // standing off every face, moving out and dying away within a few metres.
        let rebound = sin(shore * 1.35 - t * 1.1 + breakup * 1.6) * exp(-max(shore, 0.0) / 5.5) * smoothstep(0.4, 1.4, shore);
        body = body * (1.0 + rebound * 0.15);
        // The quays' rubble toe shelving away under the water: blocks of stone
        // bent by the swell's refraction, paling the water at a wall's foot and
        // gone into the basin's depth a few metres out.
        let refracted = input.world.xz + normal.xz * 0.6;
        let blocks = mix(0.5, water_noise(refracted * 0.85) * 0.6 + water_noise(refracted * 2.2 + 7.0) * 0.4, detail);
        let toe_depth = TOE_FOOT + max(shore, 0.0) * TOE_SHELF + (blocks - 0.5) * 0.5;
        let toe = near.w * exp2(-toe_depth / TOE_CLARITY);
        body = mix(body, TOE_COLOR * mix(0.6, 1.3, blocks), toe * TOE_SHOW);
        // Shade along hulls and quay faces, where the sky is half hidden; a hull's
        // flare and the water it displaces darken it close in. The band's edge
        // wanders with the swell so it never rules a line round the wall.
        let lee = 1.0 - smoothstep(0.0, mix(6.0, 3.2, near.w) + breakup * 1.6, shore);
        body = mix(body, HARBOR_CONTACT, lee * mix(0.85, 0.7, near.w));
        // Swell crests lit through from the sun side.
        let crest = clamp(noise.y * 0.9 + 0.15, 0.0, 1.0) * max(dot(-sun.xz, normal.xz) * 4.0 + 0.4, 0.0);
        // Churned wash is aerated: paler and greener under the foam.
        let aerated = clamp(wake.z * 1.4, 0.0, 1.0);
        var scatter = body * (vec3f(0.8) + diffuse * 0.35) + CREST_COLOR * (crest * 0.35 + wake.y * 0.12);
        scatter = mix(scatter, CREST_COLOR * 1.6 + vec3f(0.02), aerated * 0.45);
        // Far off, one pixel holds many facets of the swell: the mirror smears
        // into streaks down the view, and the facets tilted toward the eye show
        // the sky above whatever stands on the shore, so a far hillside never
        // mirrors one to one.
        let far = smoothstep(HARBOR_GLOSSY_NEAR, HARBOR_GLOSSY_FAR, length(world_to_eye));
        let smear = vec2f(normal.x * 0.004, HARBOR_SMEAR) * far;
        let streaked = mirror * 0.3
            + textureSampleLevel(reflection, reflection_sampler, mirror_uv + smear, 0.0).rgb * 0.22
            + textureSampleLevel(reflection, reflection_sampler, mirror_uv - smear, 0.0).rgb * 0.22
            + textureSampleLevel(reflection, reflection_sampler, mirror_uv + smear * 2.3, 0.0).rgb * 0.13
            + textureSampleLevel(reflection, reflection_sampler, mirror_uv - smear * 2.3, 0.0).rgb * 0.13;
        var reflected = streaked;
        if atmosphere_on() {
            let ray = reflect(-eye, glint_normal);
            let sky = sky_gradient(normalize(vec3f(ray.x, max(ray.y, 0.0) + 0.12, ray.z)));
            // Held by brightness, not per channel, so a red hull or crane keeps its
            // colour in the mirror instead of being clipped to the sky's.
            let ceiling = dot(sky, WATER_LUMA) * HARBOR_CLOUD_CEILING;
            let held = streaked * min(1.0, ceiling / max(dot(streaked, WATER_LUMA), 1e-4));
            reflected = mix(held, sky, far * HARBOR_SKY_SHARE * (0.6 + 0.4 * gust));
        }
        // The long swell rolling in from the open sea, crests running along x a
        // few metres apart: far off it breaks the mirror into rows down to the
        // horizon, so the open water never reads as one pale sheet.
        let swell_reach = smoothstep(20.0, 110.0, length(world_to_eye));
        if swell_reach > 0.0 {
            let swell_rows = water_noise(vec2f(input.world.x * 0.035, input.world.z * 0.28 + t * 0.35)) * 0.65
                + water_noise(vec2f(input.world.x * 0.08 + 5.0, input.world.z * 0.6 - t * 0.2)) * 0.35;
            reflected = reflected * mix(1.0, mix(0.62, 1.1, swell_rows), swell_reach);
        }
        // Glassy slicks mirror the cranes and hulls more clearly than choppy water;
        // even at grazing angles some of the water body shows through the swell.
        let reflectance = HARBOR_F0 + (1.0 - HARBOR_F0) * pow(1.0 - theta, 5.0);
        let mirror_weight = clamp(reflectance * mix(1.3, 0.9, gust) * (1.0 - aerated * 0.5), 0.0, HARBOR_MIRROR_MAX);
        // In the shade of a hull or crane the sun's glints and its light through the
        // swell go out; the mirrored sky stays.
        let sunlit = min(water_shadow(input.world), 1.0 - hull_shade(input.world.xz) * 0.9);
        let albedo = mix(scatter * mix(WATER_SHADE, 1.0, sunlit), reflected * mix(0.75, 1.0, sunlit) + specular * sunlit, mirror_weight)
            + glitter * sunlit;
        // Foam: a crisp line where the water slaps every face, a broken collar
        // beside it that is heaviest at a hull's bow and stern, wash lapping
        // outwards in rings, streaks trailing off the hulls' ends and the tugs'
        // wakes. The lace breaks it up where the pixels can resolve it.
        let churn = near.y;
        // Where the swell slaps the face: broken along it into slaps and lulls,
        // continuous only where the churn is heavy.
        let slaps = smoothstep(0.45, 0.8, water_noise(input.world.xz * 0.55 + vec2f(t * 0.09, -t * 0.07)) + lace * 0.25 - 0.1);
        // Against a quay the slap throws a broad, broken skirt of white up the
        // wall's foot, wide enough to read from the air; along a hull's flat sides
        // only a thin lap.
        let line_reach = mix(0.12 + breakup * 0.3, 0.3 + breakup * 0.55 + lace * 0.25, near.w) + footprint * 1.4;
        let line = (1.0 - smoothstep(0.04, line_reach, shore)) * mix(0.5, 1.0, churn)
            * mix(slaps, 1.0, 0.35 + 0.65 * churn) * mix(1.0, smoothstep(0.25, 0.55, lace + breakup * 0.5), near.w * 0.6);
        // The collar frays into lace; from high up, where the lace cannot resolve,
        // it breaks into the swell's patches rather than a solid white band.
        let collar_lace = mix(smoothstep(0.42, 0.7, breakup + gust * 0.25), smoothstep(0.3, 0.55, breakup + lace * 0.45 - churn * 0.15), detail);
        let collar = (1.0 - smoothstep(0.1, 0.3 + churn * (0.4 + breakup * 0.8), shore)) * collar_lace;
        let ring = sin(shore * 2.1 - t * 2.4 + breakup * 2.5) * 0.5 + 0.5;
        let wash = smoothstep(0.6, 0.92, ring) * (1.0 - smoothstep(0.2, 1.0 + churn * 3.0, shore))
            * smoothstep(0.4, 0.64, lace) * detail;
        let flecks = smoothstep(0.72, 0.82, lace * breakup * 1.6) * (1.0 - smoothstep(1.5, 6.0 + churn * 6.0, shore)) * detail;
        // Seen from high up the wake still breaks into patches of foam, never a
        // flat white band.
        let wake_lace = mix(smoothstep(0.3, 0.75, breakup * 0.8 + gust * 0.4), smoothstep(0.25, 0.6, lace * 0.6 + breakup * 0.6), detail);
        // From high up the wake softens rather than breaking into white flecks of
        // the ripple tile.
        let wake_foam = wake.x * wake_lace * mix(0.55, 1.0, detail);
        // Whitecaps out on the open water: wind-streaked patches that form where
        // two drifting swell fields crest together and dissolve as they part,
        // more of them in the gusts, none in the lee of the quays, faded where a
        // pixel would only see them shimmer. Only where a squall is passing over
        // the basin: whole stretches of it lie calm, so the caps never spread
        // evenly over the water. The noise runs only where caps can show.
        let open_water = smoothstep(8.0, 26.0, shore) * (1.0 - smoothstep(0.4, 1.4, footprint));
        var whitecap = 0.0;
        if open_water > 0.0 {
            let squall = smoothstep(0.4, 0.72, water_noise(input.world.xz / 55.0 + vec2f(t * 0.012, -t * 0.007)));
            if squall > 0.0 {
                let cap_drift = vec2f(t * 0.16, t * 0.06);
                let cap_a = water_noise(input.world.xz * vec2f(0.3, 1.1) + cap_drift);
                let cap_b = water_noise(input.world.xz * vec2f(0.8, 1.4) - cap_drift * 1.6 + 13.0);
                let cap_field = cap_a * 0.6 + cap_b * 0.4 + (gust - 0.5) * 0.45;
                // A bright breaking crest fading into the lace of its trailing foam.
                whitecap = smoothstep(0.8, 0.97, cap_field) * open_water * squall
                    * smoothstep(0.3, 0.75, lace + breakup * 0.3);
            }
        }
        let foam = clamp(line * 0.85 + collar * 0.75 + wash * 0.5 + flecks * 0.35 + wake_foam
            + whitecap * 0.65, 0.0, 1.0);
        let lit_foam = FOAM_COLOR * (0.7 + 0.3 * max(dot(sun, normal), 0.0)) * mix(0.65, 1.0, sunlit);
        color = mix(albedo, lit_foam, foam * 0.92);
    } else {
        let reflectance = pow(1.0 - theta, 5.0) * 0.82 + 0.18;
        let shore = (1.0 - abs(input.uv.x * 2.0 - 1.0)) * 6.5;
        let shallow = exp(max(shore, 0.0) * -0.55);
        let bank = mix(water.deep_color.rgb, water.shallow_color.rgb, shallow * 0.65);
        let scatter = max(0.0, dot(normal, eye)) * bank;
        let albedo = mix(scatter * (vec3f(0.85) + diffuse * 0.25), mirror + specular, reflectance);
        let wash = shore + sin(t * 1.8 + input.world.x * 0.35 + input.world.z * 0.3) * 0.18;
        let foam = (1.0 - smoothstep(0.12, 0.85, wash)) * smoothstep(0.46, 0.64, breakup);
        color = mix(albedo, vec3f(0.54, 0.67, 0.61), foam * 0.5);
    }
    // The harbor's water takes less of the sea mist than the air over it would
    // give: the basin keeps its colour and mirrors the far terminal instead of
    // fading to one pale band under the coast.
    let fogged = apply_fog(color, input.world);
    return vec4f(select(fogged, mix(color, fogged, HARBOR_FOG_SHARE), water.shore.x < 0.5), 1.0);
}

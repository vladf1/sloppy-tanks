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
// yaw), then the crane plinths (`CRANES`, 14 × 9 m), the forklift berths and the
// warehouse quays (`harbor_scenery`). Foam and hull shade hug these footprints.
const HULLS = array<vec4f, 3>(
    vec4f(-7.0, -80.0, 35.0, 7.0),
    vec4f(-77.0, -8.85, 5.95, 29.75),
    vec4f(77.0, 15.72, 5.04, 25.2),
);
const HULL_ROUNDING = 4.0;
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
// Metres the harbor's sea mist is thinned by (`harbor_apron.wgsl` has the same).
const SEA_MIST_LIFT = 8.0;

// Linear colors of the harbor's own grade: churned white foam, the bright teal a
// sunlit swell shows through its crest, and the water body from a cool teal
// against the quays through the basin's blue-teal to the dredged channel's blue.
const FOAM_COLOR = vec3f(0.8, 0.85, 0.85);
const CREST_COLOR = vec3f(0.03, 0.21, 0.22);
const HARBOR_SHALLOW = vec3f(0.016, 0.092, 0.108);
const HARBOR_BASIN = vec3f(0.008, 0.066, 0.094);
const HARBOR_DEEP = vec3f(0.004, 0.03, 0.08);
// The rubble toe the quays stand on, seen through the water: its lit stone, the
// metres of water that hide half of it, how deep it lies at a wall's foot and
// how fast it shelves away, and the most of it that ever shows.
const TOE_COLOR = vec3f(0.1, 0.19, 0.16);
const TOE_CLARITY = 1.0;
const TOE_FOOT = 0.3;
const TOE_SHELF = 0.32;
const TOE_SHOW = 0.8;
// Fresnel reflectance of the harbor water seen straight down: real water's 2%
// lifted so the sky and whatever stands over the basin still show from above.
const HARBOR_F0 = 0.085;
// Metres from the nearest face where the basin turns to channel, and where the
// wash of a moored hull's ends trails off.
const HARBOR_CHANNEL = 40.0;
const HULL_TAIL = 8.0;
// Metres from the eye over which the mirror turns glossy, the screen fraction its
// streaks reach down the view at most, the share of sky the far facets show and
// the most the water ever mirrors at a grazing angle.
const HARBOR_GLOSSY_NEAR = 35.0;
const HARBOR_GLOSSY_FAR = 190.0;
const HARBOR_SMEAR = 0.011;
const HARBOR_SKY_SHARE = 0.25;
const HARBOR_MIRROR_MAX = 0.86;

// Texels between the taps of the water's sun shadow lookup: wide enough that the
// swell never shows the map's texels where hulls and cranes shade the basin.
const WATER_SHADOW_SPREAD = 1.6;
// Share of the water body's light that is sunlight scattered up through it, which
// a shadow takes away; the sky's fill keeps the rest.
const WATER_SUN_SCATTER = 0.42;

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

// What the water meets near a harbor point: x the metres to the nearest quay
// face, hull or pier; y how much of the churn that face throws up (a hull's bow
// and stern most, its flat sides least); z the foam trailing off a hull's ends,
// streaked along its axis by the tide; w 1 where that face stands on a rubble
// toe (a quay or pier), 0 at a hull.
fn harbor_shore(p: vec2f, t: f32) -> vec4f {
    var out = vec4f(max(abs(p.x), abs(p.y)) - water.shore.y, 0.55, 0.0, 1.0);
    for (var i = 0; i < 3; i++) {
        let hull = HULLS[i];
        let d = rounded_box_distance(p, hull, HULL_ROUNDING);
        let along_x = hull.z > hull.w;
        let q = p - hull.xy;
        let along = select(q.y, q.x, along_x);
        let across = select(q.x, q.y, along_x);
        let reach = select(hull.w, hull.z, along_x);
        let breadth = select(hull.z, hull.w, along_x);
        if d < out.x {
            out.x = d;
            // Moored, so the bow and stern only lap a little harder than the
            // flat sides: no bow wave.
            out.y = 0.2 + 0.55 * smoothstep(0.6, 1.0, abs(along) / reach);
            out.w = 0.0;
        }
        let past = abs(along) - reach;
        let bend = (water_noise(vec2f(along * 0.08, 7.0)) - 0.5) * 3.0;
        let streaks = water_noise(vec2f((across + bend) * 0.8, (abs(along) - t * 0.6) * 0.35))
            * water_noise(vec2f(across * 2.3 + 11.0, abs(along) * 0.6));
        let narrowing = 1.0 - smoothstep(breadth * 0.4, breadth * 0.9 + past * 0.12, abs(across));
        out.z = max(out.z, exp(-max(past, 0.0) / HULL_TAIL) * step(0.0, past) * narrowing * smoothstep(0.22, 0.5, streaks));
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
        let lane = TUGS[i];
        let perimeter = 2.0 * (2.0 * lane.y + WATER_PI * lane.z);
        let boat = (lane.w + time * TUG_SPEED) % perimeter;
        let place = tug_loop_place(p, lane);
        let across = abs(place.y);
        if across > 26.0 {
            continue;
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
    }
    return out;
}

// The sun's shadow on the water, from four hardware-filtered taps of the shadow map
// (the surface is flat, so no normal offset), lit outside the map.
fn water_sun_shadow(world: vec3f) -> f32 {
    let position = frame.shadow_matrix * vec4f(world, 1.0);
    let projected = position.xyz / position.w;
    let coord = vec3f(projected.x, 1.0 - projected.y, projected.z + frame.shadow.x);
    let texel = WATER_SHADOW_SPREAD / frame.shadow.z;
    let sum = textureSampleCompareLevel(shadow_map, shadow_sampler, coord.xy + vec2f(-texel, -texel * 0.5), coord.z)
        + textureSampleCompareLevel(shadow_map, shadow_sampler, coord.xy + vec2f(texel, -texel * 0.5), coord.z)
        + textureSampleCompareLevel(shadow_map, shadow_sampler, coord.xy + vec2f(-texel * 0.5, texel), coord.z)
        + textureSampleCompareLevel(shadow_map, shadow_sampler, coord.xy + vec2f(texel * 0.5, texel), coord.z);
    let inside = coord.x >= 0.0 && coord.x <= 1.0 && coord.y >= 0.0 && coord.y <= 1.0 && coord.z <= 1.0;
    return select(1.0, sum * 0.25, inside);
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
    // How much water one pixel spans: fine foam lace fades out where it would
    // only shimmer.
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

    // Hulls, cranes and passing clouds shade the harbor's water: no glint and less
    // of the sunlight scattered up through it.
    var sunlit = 1.0;
    if harbor {
        sunlit = water_sun_shadow(input.world) * cloud_shadow(input.world);
    }
    let specular = pow(max(0.0, dot(eye, normalize(reflect(-sun, normal)))), 100.0) * sun_color * 2.0 * sunlit;
    // Glitter: tiny facets of the fast ripples catch the sun one by one.
    let facet = max(0.0, dot(eye, normalize(reflect(-sun, glitter_normal))));
    let glitter = pow(facet, 1400.0) * 14.0 * sun_color * select(0.0, 0.5 + gust, harbor) * sunlit;
    let diffuse = max(dot(sun, normal), 0.0) * sun_color * 0.5 * sunlit;
    let distortion = normal.xz * (0.001 + 1.0 / length(world_to_eye)) * water.settings.y;
    let screen = input.clip.xy * frame.viewport.zw;
    let mirror_uv = vec2f(1.0 - screen.x, screen.y) + distortion;
    let mirror = textureSampleLevel(reflection, reflection_sampler, mirror_uv, 0.0).rgb;
    let theta = max(dot(eye, normal), 0.0);

    var color: vec3f;
    if harbor {
        let near = harbor_shore(input.world.xz, t);
        let shore = near.x;
        let detail = 1.0 - smoothstep(0.08, 0.35, footprint);
        // Silty green against the quays and hulls, the basin's teal, then the
        // dredged channel's blue, with the gusts roughening it a little paler.
        var body = mix(HARBOR_SHALLOW, HARBOR_BASIN, smoothstep(0.5, 10.0, shore));
        body = mix(body, HARBOR_DEEP, smoothstep(10.0, HARBOR_CHANNEL, shore));
        // Wind lanes: roughened water a little paler, glassy slicks deeper.
        body = body * mix(0.84, 1.16, gust);
        // The quays' rubble toe shelving away under the water: blocks of stone
        // bent by the swell's refraction, paling the water at a wall's foot and
        // gone into the basin's depth a few metres out.
        let refracted = input.world.xz + normal.xz * 0.6;
        let blocks = mix(0.5, water_noise(refracted * 0.85) * 0.6 + water_noise(refracted * 2.2 + 7.0) * 0.4, detail);
        let toe_depth = TOE_FOOT + max(shore, 0.0) * TOE_SHELF + (blocks - 0.5) * 0.5;
        let toe = near.w * exp2(-toe_depth / TOE_CLARITY) * mix(0.55, 1.0, sunlit);
        body = mix(body, TOE_COLOR * mix(0.55, 1.3, blocks), toe * TOE_SHOW);
        // Shade along hulls and quay faces, where the sky is half hidden.
        body = body * mix(0.5, 1.0, smoothstep(0.0, 2.2, shore));
        // Swell crests lit through from the sun side.
        let crest = clamp(noise.y * 0.9 + 0.15, 0.0, 1.0) * max(dot(-sun.xz, normal.xz) * 4.0 + 0.4, 0.0);
        // Churned wash is aerated: paler and greener under the foam.
        let aerated = clamp(wake.z * 1.4, 0.0, 1.0);
        var scatter = body * (vec3f(0.8) + diffuse * 0.35) * mix(1.0 - WATER_SUN_SCATTER, 1.0, sunlit)
            + CREST_COLOR * (crest * 0.35 * sunlit + wake.y * 0.12);
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
            let ray = reflect(-eye, normal);
            let sky = sky_gradient(normalize(vec3f(ray.x, max(ray.y, 0.0) + 0.12, ray.z)));
            reflected = mix(streaked, sky, far * HARBOR_SKY_SHARE * (0.6 + 0.4 * gust));
        }
        // Glassy slicks mirror the cranes and hulls more clearly than choppy water;
        // even at grazing angles some of the water body shows through the swell.
        let reflectance = HARBOR_F0 + (1.0 - HARBOR_F0) * pow(1.0 - theta, 5.0);
        let mirror_weight = clamp(reflectance * mix(1.55, 0.85, gust) * (1.0 - aerated * 0.5), 0.0, HARBOR_MIRROR_MAX);
        let albedo = mix(scatter, reflected + specular, mirror_weight) + glitter;
        // Foam: a crisp line where the water slaps every face, a broken collar
        // beside it that is heaviest at a hull's bow and stern, wash lapping
        // outwards in rings, streaks trailing off the hulls' ends and the tugs'
        // wakes. The lace breaks it up where the pixels can resolve it.
        let churn = near.y;
        let line = 1.0 - smoothstep(0.03, 0.12 + breakup * 0.1 + footprint, shore);
        // The collar frays into soft lace rather than a serrated white band.
        let collar = (1.0 - smoothstep(0.08, 0.3 + churn * (0.4 + breakup * 0.7), shore))
            * smoothstep(0.35, 0.75, breakup + lace * 0.45 - churn * 0.15);
        let ring = sin(shore * 2.1 - t * 2.4 + breakup * 2.5) * 0.5 + 0.5;
        let wash = smoothstep(0.6, 0.92, ring) * (1.0 - smoothstep(0.2, 1.0 + churn * 3.0, shore))
            * smoothstep(0.4, 0.64, lace) * detail;
        let flecks = smoothstep(0.72, 0.82, lace * breakup * 1.6) * (1.0 - smoothstep(1.5, 6.0 + churn * 6.0, shore)) * detail;
        let tail = near.z * smoothstep(0.35, 0.6, lace) * 0.8;
        // Seen from high up the wake still breaks into patches of foam, never a
        // flat white band.
        let wake_lace = mix(smoothstep(0.3, 0.75, breakup * 0.8 + gust * 0.4), smoothstep(0.25, 0.6, lace * 0.6 + breakup * 0.6), detail);
        // Softer from high up, where the lace cannot resolve and white streaks read
        // as brush strokes.
        let wake_foam = wake.x * wake_lace * mix(0.6, 1.0, detail);
        let foam = clamp(line * 0.75 + collar * 0.55 + wash * 0.32 + flecks * 0.22 + tail * 0.7 + wake_foam, 0.0, 1.0);
        let lit_foam = FOAM_COLOR * (0.7 + 0.3 * max(dot(sun, normal), 0.0));
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
    // The harbor's sea mist is thinned as the far shore's is (`harbor_apron.wgsl`
    // SEA_MIST_LIFT), so the water and the bank behind it fade alike.
    let fogged_at = input.world + vec3f(0.0, select(0.0, SEA_MIST_LIFT, harbor), 0.0);
    return vec4f(apply_fog(color, fogged_at), 1.0);
}

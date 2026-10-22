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

// Linear colors of the harbor's own grade: churned white foam, the bright teal a
// sunlit swell shows through its crest, and the water body from the silty green
// against the quays through the basin's teal to the dredged channel's blue.
const FOAM_COLOR = vec3f(0.8, 0.85, 0.85);
const CREST_COLOR = vec3f(0.04, 0.24, 0.21);
const HARBOR_SHALLOW = vec3f(0.035, 0.135, 0.105);
const HARBOR_BASIN = vec3f(0.011, 0.082, 0.1);
const HARBOR_DEEP = vec3f(0.004, 0.03, 0.068);
// Fresnel reflectance of the harbor water seen straight down: real water's 2%
// lifted a little so the sky still tints it from above.
const HARBOR_F0 = 0.06;
// Metres from the nearest face where the basin turns to channel, and where the
// wash of a moored hull's ends trails off.
const HARBOR_CHANNEL = 40.0;
const HULL_TAIL = 8.0;

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
// streaked along its axis by the tide.
fn harbor_shore(p: vec2f, t: f32) -> vec3f {
    var out = vec3f(max(abs(p.x), abs(p.y)) - water.shore.y, 0.55, 0.0);
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
            out.y = 0.25 + 0.95 * smoothstep(0.55, 1.0, abs(along) / reach);
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
            // settling into a glassy scar.
            let stern = smoothstep(TUG_HALF_LENGTH - 1.0, TUG_HALF_LENGTH + 1.0, behind);
            let wash_width = 1.3 + age * 0.3;
            let wash = (1.0 - smoothstep(wash_width * 0.35, wash_width, across)) * exp(-age / 6.5) * stern;
            let scar = (1.0 - smoothstep(wash_width * 0.5, wash_width * 1.3, across)) * exp(-age / 16.0) * stern;
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

    let specular = pow(max(0.0, dot(eye, normalize(reflect(-sun, normal)))), 100.0) * sun_color * 2.0;
    // Glitter: tiny facets of the fast ripples catch the sun one by one.
    let facet = max(0.0, dot(eye, normalize(reflect(-sun, glitter_normal))));
    let glitter = pow(facet, 1400.0) * 14.0 * sun_color * select(0.0, 0.5 + gust, harbor);
    let diffuse = max(dot(sun, normal), 0.0) * sun_color * 0.5;
    let distortion = normal.xz * (0.001 + 1.0 / length(world_to_eye)) * water.settings.y;
    let screen = input.clip.xy * frame.viewport.zw;
    let mirror = textureSampleLevel(reflection, reflection_sampler, vec2f(1.0 - screen.x, screen.y) + distortion, 0.0).rgb;
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
        body = body * mix(0.92, 1.12, gust);
        // Shade along hulls and quay faces, where the sky is half hidden.
        body = body * mix(0.55, 1.0, smoothstep(0.0, 3.5, shore));
        // Swell crests lit through from the sun side.
        let crest = clamp(noise.y * 0.9 + 0.15, 0.0, 1.0) * max(dot(-sun.xz, normal.xz) * 4.0 + 0.4, 0.0);
        // Churned wash is aerated: paler and greener under the foam.
        let aerated = clamp(wake.z * 1.4, 0.0, 1.0);
        var scatter = body * (vec3f(0.8) + diffuse * 0.35) + CREST_COLOR * (crest * 0.35 + wake.y * 0.12);
        scatter = mix(scatter, CREST_COLOR * 1.6 + vec3f(0.02), aerated * 0.6);
        // Glassy slicks mirror the cranes and hulls more clearly than choppy water.
        let reflectance = HARBOR_F0 + (1.0 - HARBOR_F0) * pow(1.0 - theta, 5.0);
        let mirror_weight = clamp(reflectance * mix(1.3, 0.9, gust) * (1.0 - aerated * 0.5), 0.0, 1.0);
        let albedo = mix(scatter, mirror + specular, mirror_weight) + glitter;
        // Foam: a crisp line where the water slaps every face, a broken collar
        // beside it that is heaviest at a hull's bow and stern, wash lapping
        // outwards in rings, streaks trailing off the hulls' ends and the tugs'
        // wakes. The lace breaks it up where the pixels can resolve it.
        let churn = near.y;
        let line = 1.0 - smoothstep(0.04, 0.16 + breakup * 0.12 + footprint, shore);
        let collar = (1.0 - smoothstep(0.1, 0.35 + churn * (0.5 + breakup * 0.9), shore))
            * smoothstep(0.3, 0.55, breakup + lace * 0.45 - churn * 0.15);
        let ring = sin(shore * 2.1 - t * 2.4 + breakup * 2.5) * 0.5 + 0.5;
        let wash = smoothstep(0.6, 0.92, ring) * (1.0 - smoothstep(0.2, 1.0 + churn * 3.0, shore))
            * smoothstep(0.4, 0.64, lace) * detail;
        let flecks = smoothstep(0.72, 0.82, lace * breakup * 1.6) * (1.0 - smoothstep(1.5, 6.0 + churn * 6.0, shore)) * detail;
        let tail = near.z * smoothstep(0.35, 0.6, lace) * 0.8;
        let wake_lace = mix(0.75, smoothstep(0.25, 0.6, lace * 0.6 + breakup * 0.6), detail);
        let wake_foam = wake.x * wake_lace;
        let foam = clamp(line * 0.85 + collar * 0.8 + wash * 0.5 + flecks * 0.35 + tail * 0.7 + wake_foam, 0.0, 1.0);
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
    return vec4f(apply_fog(color, input.world), 1.0);
}

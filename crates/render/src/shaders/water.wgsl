// Planar-reflection water, ported from water-surface.ts (a MeshBasicNodeMaterial
// colorNode): four scrolling normal-map samples for the swell and a fast fifth for
// glitter, a Fresnel mix of the mirrored scene with a depth-tinted scatter color, a
// sun glint, and foam that laps against everything standing in the water. Group 0
// is the main view's frame; the reflection is rendered first with the mirror camera.

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
    vec4f(-115.0, 0.0, 11.0, 39.0),
    vec4f(115.0, 0.0, 11.0, 39.0),
);

// Linear colors of the harbor's own grade: churned white foam, the bright teal a
// sunlit swell shows through its crest, and the shade under a hull or quay.
const FOAM_COLOR = vec3f(0.78, 0.84, 0.84);
const CREST_COLOR = vec3f(0.04, 0.22, 0.2);
const HARBOR_DEEP = vec3f(0.008, 0.045, 0.075);
const HARBOR_SHALLOW = vec3f(0.025, 0.14, 0.15);

fn rounded_box_distance(p: vec2f, footprint: vec4f, radius: f32) -> f32 {
    let q = abs(p - footprint.xy) - footprint.zw + radius;
    return length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

// Metres from a harbor point to the nearest quay face, hull or pier.
fn harbor_shore_distance(p: vec2f) -> f32 {
    var d = max(abs(p.x), abs(p.y)) - water.shore.y;
    for (var i = 0; i < 3; i++) {
        d = min(d, rounded_box_distance(p, HULLS[i], HULL_ROUNDING));
    }
    for (var i = 0; i < 8; i++) {
        d = min(d, rounded_box_distance(p, PIERS[i], 0.3));
    }
    return d;
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
    let noise = (textureSample(water_normals, water_sampler, p / 103.0 + vec2f(t / 17.0, t / 29.0))
        + textureSample(water_normals, water_sampler, p / 107.0 - vec2f(t / -19.0, t / 31.0))
        + textureSample(water_normals, water_sampler, p / vec2f(8907.0, 9803.0) + vec2f(t / 101.0, t / 97.0))
        + textureSample(water_normals, water_sampler, p / vec2f(1091.0, 1027.0) - vec2f(t / 109.0, t / -113.0)))
        * 0.5 - 1.0;
    let ripple = textureSample(water_normals, water_sampler, input.world.xz / 3.1 + vec2f(t * 0.031, -t * 0.023)).rgb * 2.0 - 1.0;
    let breakup = textureSample(water_normals, water_sampler, input.world.xz * 0.11 + t * 0.025).r;
    let lace = textureSample(water_normals, water_sampler, input.world.xz * 0.37 - vec2f(t * 0.04, t * 0.017)).g;
    let strength = water.settings.z;
    let normal = normalize(noise.xzy * vec3f(strength, 1.0, strength));
    let glitter_normal = normalize(normal + ripple.xzy * vec3f(0.35, 0.0, 0.35));
    let world_to_eye = frame.camera_position.xyz - input.world;
    let eye = normalize(world_to_eye);
    let sun = water.sun_direction.xyz;
    let sun_color = water.sun_color.rgb;
    let harbor = water.shore.x < 0.5;

    let specular = pow(max(0.0, dot(eye, normalize(reflect(-sun, normal)))), 100.0) * sun_color * 2.0;
    // Glitter: tiny facets of the fast ripples catch the sun one by one.
    let facet = max(0.0, dot(eye, normalize(reflect(-sun, glitter_normal))));
    let glitter = pow(facet, 1400.0) * 14.0 * sun_color * select(0.0, 1.0, harbor);
    let diffuse = max(dot(sun, normal), 0.0) * sun_color * 0.5;
    let distortion = normal.xz * (0.001 + 1.0 / length(world_to_eye)) * water.settings.y;
    let screen = input.clip.xy * frame.viewport.zw;
    let mirror = textureSampleLevel(reflection, reflection_sampler, vec2f(1.0 - screen.x, screen.y) + distortion, 0.0).rgb;
    let theta = max(dot(eye, normal), 0.0);
    let reflectance = pow(1.0 - theta, 5.0) * 0.82 + 0.18;

    var color: vec3f;
    if harbor {
        let shore = harbor_shore_distance(input.world.xz);
        // Teal near the quays and hulls, deepening to blue-green in open water.
        let open = smoothstep(0.5, 9.0, shore);
        var body = mix(HARBOR_SHALLOW, HARBOR_DEEP, open);
        // Shade along hulls and quay faces, where the sky is half hidden.
        body = body * mix(0.55, 1.0, smoothstep(0.0, 3.0, shore));
        // Swell crests lit through from the sun side.
        let crest = clamp(noise.y * 0.9 + 0.15, 0.0, 1.0) * max(dot(-sun.xz, normal.xz) * 4.0 + 0.4, 0.0);
        let scatter = body * (vec3f(0.8) + diffuse * 0.35) + CREST_COLOR * crest * 0.35;
        let albedo = mix(scatter, mirror + specular, reflectance) + glitter;
        // Foam: a broken white collar against every face, and wash that laps
        // outwards in rings and frays into lace.
        let collar = 1.0 - smoothstep(0.05, 0.55 + breakup * 0.6, shore);
        let ring = sin(shore * 2.1 - t * 2.4 + breakup * 2.5) * 0.5 + 0.5;
        let wash = smoothstep(0.55, 0.9, ring) * (1.0 - smoothstep(0.2, 4.0, shore)) * smoothstep(0.38, 0.62, lace);
        let flecks = smoothstep(0.7, 0.8, lace * breakup * 1.6) * (1.0 - smoothstep(2.0, 9.0, shore));
        let foam = clamp(collar * smoothstep(0.25, 0.5, breakup + lace * 0.4) + wash * 0.75 + flecks * 0.45, 0.0, 1.0);
        let lit_foam = FOAM_COLOR * (0.7 + 0.3 * max(dot(sun, normal), 0.0));
        color = mix(albedo, lit_foam, foam * 0.92);
    } else {
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

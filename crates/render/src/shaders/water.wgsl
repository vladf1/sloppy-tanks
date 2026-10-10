// Planar-reflection water, ported from water-surface.ts (a MeshBasicNodeMaterial
// colorNode): four scrolling normal-map samples, Fresnel mix of the mirrored scene
// with a depth-tinted scatter color, a sun glint, and shoreline foam. Group 0 is
// the main view's frame; the reflection is rendered first with the mirror camera.

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
    let breakup = textureSample(water_normals, water_sampler, input.world.xz * 0.11 + t * 0.025).r;
    let strength = water.settings.z;
    let normal = normalize(noise.xzy * vec3f(strength, 1.0, strength));
    let world_to_eye = frame.camera_position.xyz - input.world;
    let eye = normalize(world_to_eye);
    let sun = water.sun_direction.xyz;
    let specular = pow(max(0.0, dot(eye, normalize(reflect(-sun, normal)))), 100.0) * water.sun_color.rgb * 2.0;
    let diffuse = max(dot(sun, normal), 0.0) * water.sun_color.rgb * 0.5;
    let distortion = normal.xz * (0.001 + 1.0 / length(world_to_eye)) * water.settings.y;
    let screen = input.clip.xy * frame.viewport.zw;
    let mirror = textureSampleLevel(reflection, reflection_sampler, vec2f(1.0 - screen.x, screen.y) + distortion, 0.0).rgb;
    let theta = max(dot(eye, normal), 0.0);
    let reflectance = pow(1.0 - theta, 5.0) * 0.82 + 0.18;
    var shore = (1.0 - abs(input.uv.x * 2.0 - 1.0)) * 6.5;
    if water.shore.x < 0.5 {
        shore = max(abs(input.world.x), abs(input.world.z)) - water.shore.y;
    }
    let shallow = exp(max(shore, 0.0) * -0.55);
    let bank = mix(water.deep_color.rgb, water.shallow_color.rgb, shallow * 0.65);
    let scatter = max(0.0, dot(normal, eye)) * bank;
    let albedo = mix(scatter * (vec3f(0.85) + diffuse * 0.25), mirror + specular, reflectance);
    let wash = shore + sin(t * 1.8 + input.world.x * 0.35 + input.world.z * 0.3) * 0.18;
    let foam = (1.0 - smoothstep(0.12, 0.85, wash)) * smoothstep(0.46, 0.64, breakup);
    var color = mix(albedo, vec3f(0.54, 0.67, 0.61), foam * 0.5);
    if frame.fog_color.w > 0.5 {
        let depth = -(frame.view * vec4f(input.world, 1.0)).z;
        color = mix(color, frame.fog_color.rgb, smoothstep(frame.fog_range.x, frame.fog_range.y, depth));
    }
    return vec4f(color, 1.0);
}

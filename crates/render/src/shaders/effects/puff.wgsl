// Billowing smoke, fire and dust puffs: a camera-facing quad shaded as a lumpy,
// sun-lit ball whose rim boils with value noise. A hot puff glows through a
// blackbody ramp and cools into the smoke it carries, so one puff turns from
// fireball to smoke over its life. The instance transform is translation × scale
// only, so the offset from its origin is the scaled corner.
// Tint = linear smoke albedo and opacity; instance_data = (seed 0..1, heat 0..1.5,
// erosion 0..1 that eats the puff from its rim into wisps, softness 0 billow ..
// 1 haze).
fn effect_world(w: ptr<function, EffectWorld>, v: EffectVertex) {
    let corner = (*w).position - v.instance_origin;
    (*w).position = v.instance_origin + frame.camera_right.xyz * corner.x + frame.camera_up.xyz * corner.y;
}

fn puff_hash(p: vec2f) -> f32 {
    var q = fract(vec3f(p.x, p.y, p.x) * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn puff_noise(p: vec2f) -> f32 {
    let cell = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = puff_hash(cell);
    let b = puff_hash(cell + vec2f(1.0, 0.0));
    let c = puff_hash(cell + vec2f(0.0, 1.0));
    let d = puff_hash(cell + vec2f(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Linear HDR blackbody-ish ramp: soot-red embers, red, orange, yellow, white
// core. The output pass's tone curve maps these to saturated fire, not cream.
fn puff_fire(t: f32) -> vec3f {
    var c = mix(vec3f(0.18, 0.025, 0.01), vec3f(1.2, 0.16, 0.02), smoothstep(0.06, 0.3, t));
    c = mix(c, vec3f(2.6, 0.62, 0.05), smoothstep(0.3, 0.55, t));
    c = mix(c, vec3f(3.9, 1.75, 0.28), smoothstep(0.55, 0.9, t));
    c = mix(c, vec3f(4.8, 3.3, 1.3), smoothstep(1.05, 1.6, t));
    return c;
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let seed = f.instance_data.x;
    let heat = f.instance_data.y;
    let erosion = f.instance_data.z;
    let softness = f.instance_data.w;
    let point = f.uv * 2.0 - 1.0;
    let radius = length(point);
    // Each puff turns its noise its own way, and the noise boils slowly upward.
    let turn = seed * 6.2831853;
    let c = cos(turn);
    let sn = sin(turn);
    let p = vec2f(c * point.x - sn * point.y, sn * point.x + c * point.y);
    let offset = vec2f(seed * 37.1, seed * 91.7);
    let boil = vec2f(0.0, frame.camera_position.w * 0.35);
    let n1 = puff_noise(p * 2.1 + offset - boil);
    let n2 = puff_noise(p * 4.6 + offset * 1.7 - boil * 1.8);
    let n3 = puff_noise(p * 9.3 + offset * 2.3);
    let noise = n1 * 0.55 + n2 * 0.3 + n3 * 0.15;

    // Cauliflower silhouette: the noise pushes the rim in and out; haze is softer.
    // Erosion pulls the rim in, most where the noise is thin, so an old puff frays
    // into wisps from the outside instead of going full of holes.
    let lumpiness = mix(0.75, 0.42, softness);
    let edge = radius + (noise - 0.5) * lumpiness + erosion * (0.25 + 0.5 * (1.0 - noise));
    let rim = mix(0.84, 0.98, softness);
    let feather = mix(0.07, 0.5, softness) + erosion * 0.12;
    let density = 1.0 - smoothstep(rim - feather, rim, edge);

    // A lumpy ball: noise tilts the sphere normal into billows, lit like the scene.
    let bulge = sqrt(max(1.0 - radius * radius, 0.0));
    let lumps = vec2f(n2 - 0.5, n1 - 0.5) * 1.2;
    let normal = normalize(
        frame.camera_right.xyz * (point.x + lumps.x) + frame.camera_up.xyz * (point.y + lumps.y)
            + f.view_direction * (bulge + 0.3),
    );
    let sun = dot(normal, frame.sun_direction.xyz);
    let direct = smoothstep(-0.3, 0.85, sun) * (0.6 + 0.4 * noise);
    let hemisphere = mix(frame.ground_color.rgb, frame.sky_color.rgb, normal.y * 0.5 + 0.5);
    // Deep folds between billows take less fill; the sunlit rim of a billow
    // catches a little extra light, so the shapes stay readable from above.
    let occlusion = 0.55 + 0.45 * smoothstep(0.2, 0.75, noise);
    let albedo = (*s).color;
    var color = albedo * (frame.sun_color.rgb * direct * 1.15 + hemisphere * occlusion) * RECIPROCAL_PI;
    let rim_light = smoothstep(0.45, 0.95, radius) * smoothstep(0.1, 0.7, sun) * (1.0 - softness);
    color += albedo * frame.sun_color.rgb * rim_light * 0.12;

    // Fire: hottest at the core and in the dense noise, cooling to a sooty rim.
    let core = clamp(1.0 - radius * 0.85, 0.0, 1.0);
    let temperature = heat * (0.35 + 0.7 * core) * (0.4 + 0.95 * noise);
    let glow = smoothstep(0.05, 0.32, temperature);
    color = mix(color, puff_fire(temperature), glow);
    // Smoke over a fire glows from below with its light.
    let underglow = clamp((heat - 0.12) * 2.5, 0.0, 1.0) * (1.0 - glow) * clamp(0.6 - normal.y, 0.0, 1.0);
    color += vec3f(1.6, 0.42, 0.07) * underglow * 0.55;

    (*s).color = color;
    (*s).opacity *= density;
}

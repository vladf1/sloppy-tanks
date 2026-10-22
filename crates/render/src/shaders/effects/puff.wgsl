// Billowing smoke, fire and dust puffs: a camera-facing quad shaded as a lumpy,
// sun-lit ball whose rim boils with value noise. A hot puff glows through a
// blackbody ramp and burns out from its cooling rim; smoke over a fire glows from
// below. The instance transform is translation × scale only, so the offset from
// its origin is the scaled corner.
// Tint = linear smoke albedo and opacity; instance_data = (seed 0..1 plus the
// underglow level 0..15 above it, heat 0..1.5, erosion 0..1 that eats the puff
// from its rim into wisps, softness 0 billow .. 1 haze).
fn effect_world(w: ptr<function, EffectWorld>, v: EffectVertex) {
    let corner = (*w).position - v.instance_origin;
    // The card stands at the front of the ball it draws, pulled toward the eye
    // by most of its radius, so a hull or the ground the puff envelops cuts its
    // far side rather than slicing through its middle. A low puff is pulled
    // further, until its card's lowest edge clears the ground: a card that dips
    // into the ground would be cut along every step of the terrain.
    let radius = abs(corner.x);
    let height = radius * 0.9;
    let to_eye_far = frame.camera_position.xyz - v.instance_origin;
    let to_eye = normalize(to_eye_far);
    // The card keeps the world's up rather than the camera's, so a rolling or
    // shaking camera does not turn every billow with it.
    var up = vec3f(0.0, 1.0, 0.0) - to_eye * to_eye.y;
    if dot(up, up) < 1e-4 {
        up = frame.camera_up.xyz;
    }
    up = normalize(up);
    let right = normalize(cross(up, to_eye));
    let lowest = v.instance_origin.y - abs(up.y) * height;
    let lift = max(0.06 - lowest, 0.0) / max(to_eye.y, 0.2);
    let pull = min(max(radius * 0.6, lift), length(to_eye_far) * 0.4);
    (*w).position = v.instance_origin + to_eye * pull + right * corner.x + up * corner.y;
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

// Linear HDR blackbody-ish ramp: deep red embers, red, orange, yellow, white
// core. The output pass's tone curve maps these to saturated fire, not cream.
fn puff_fire(t: f32) -> vec3f {
    var c = mix(vec3f(0.32, 0.04, 0.01), vec3f(1.05, 0.11, 0.015), smoothstep(0.03, 0.18, t));
    c = mix(c, vec3f(2.0, 0.42, 0.035), smoothstep(0.18, 0.4, t));
    c = mix(c, vec3f(2.8, 0.95, 0.1), smoothstep(0.4, 0.7, t));
    c = mix(c, vec3f(3.6, 1.85, 0.4), smoothstep(0.75, 1.1, t));
    c = mix(c, vec3f(4.6, 3.3, 1.6), smoothstep(1.15, 1.6, t));
    return c;
}

// A soft staircase over the fire ramp: the temperature settles into a few broad
// bands with soft steps between them, the stylised look of a toon fireball.
fn puff_bands(t: f32) -> f32 {
    let f = t * 4.0;
    let step = floor(f) + smoothstep(0.3, 0.7, fract(f));
    return mix(t, step / 4.0, 0.6);
}

// Below this heat a puff's fire never reaches the ramp's first glow (temperature
// 0.03 after the bands' 0.4 floor and the core and noise gains of at most 1.3).
const PUFF_FIRE_MIN_HEAT: f32 = 0.05;

// Smoke puffs pile up deep over a burning wreck and a blast, so most of a frame's
// puff pixels are hidden or empty: the noise runs only inside the card's circle,
// and the lighting and fire only where the puff has density. The skipped pixels
// get exactly the zero opacity they would have computed.
fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let point = f.uv * 2.0 - 1.0;
    let radius = length(point);
    var density = 0.0;
    if radius < 1.0 {
        let packed = f.instance_data.x;
        let seed = fract(packed);
        let heat = f.instance_data.y;
        let erosion = f.instance_data.z;
        let softness = f.instance_data.w;
        let fiery = smoothstep(0.04, 0.25, heat);
        // Each puff turns its noise its own way, and the noise boils slowly upward.
        let turn = seed * 6.2831853;
        let c = cos(turn);
        let sn = sin(turn);
        let p = vec2f(c * point.x - sn * point.y, sn * point.x + c * point.y);
        let offset = vec2f(seed * 37.1, seed * 91.7);
        // Slow enough that a billow's outline drifts rather than trembles. A hot
        // puff churns on as it cools, driven by its falling heat: a boil rate that
        // changed with heat would scale the whole running time, so a cooling puff's
        // noise would leap to a new pattern every frame and its density flicker,
        // the more the longer the game has run.
        let boil = vec2f(0.0, frame.camera_position.w * 0.16 - heat * 0.25);
        // Cauliflower silhouette: the noise pushes the rim in and out; haze is softer.
        // Erosion pulls the rim in, most where the noise is thin, so an old puff frays
        // into wisps from the outside instead of going full of holes. Fine noise
        // breaks the rim's outline so no billow reads as a clean disc.
        let lumpiness = mix(0.72, 0.42, softness);
        let rim = mix(0.8, 0.94, softness);
        let n1 = puff_noise(p * 2.1 + offset - boil);
        // The edge is linear in the finer octaves, each in 0..1: where even their
        // most favourable values leave it past the rim, the puff is empty.
        let slope = lumpiness - 0.55 * erosion;
        let least_edge = radius - 0.5 * lumpiness - 0.03 + 0.8 * erosion + slope * 0.55 * n1
            + min(0.3 * slope, 0.0) + min(0.15 * slope + 0.06, 0.0);
        if least_edge < rim {
            let n2 = puff_noise(p * 4.6 + offset * 1.7 - boil * 1.5);
            let n3 = puff_noise(p * 9.3 + offset * 2.3 - boil * 1.8);
            let noise = n1 * 0.55 + n2 * 0.3 + n3 * 0.15;
            let edge = radius + (noise - 0.5) * lumpiness + (n3 - 0.5) * 0.06
                + erosion * (0.25 + 0.55 * (1.0 - noise));
            let feather = mix(0.14, 0.5, softness * softness) + erosion * 0.16 + fiery * 0.04;
            // The card's own edge is always clear, however far the noise pushes the
            // rim, so no puff shows the straight side of its quad.
            let window = 1.0 - smoothstep(0.8, 1.0, radius);
            density = (1.0 - smoothstep(rim - feather, rim, edge)) * window;
            if density > 0.0 {
                (*s).color = puff_color((*s).color, point, radius, floor(packed) / 15.0, heat, softness, fiery, n1, n2, noise, f.view_direction);
            }
        }
    }
    (*s).opacity *= density;
}

// A visible puff pixel's color: the lit lumpy ball, its fire and its underglow.
fn puff_color(
    albedo: vec3f,
    point: vec2f,
    radius: f32,
    underglow: f32,
    heat: f32,
    softness: f32,
    fiery: f32,
    n1: f32,
    n2: f32,
    noise: f32,
    view_direction: vec3f,
) -> vec3f {
    // A lumpy ball: noise tilts the sphere normal into billows, lit like the scene.
    let bulge = sqrt(max(1.0 - radius * radius, 0.0));
    let lumps = vec2f(n2 - 0.5, n1 - 0.5) * 1.2;
    let normal = normalize(
        frame.camera_right.xyz * (point.x + lumps.x) + frame.camera_up.xyz * (point.y + lumps.y)
            + view_direction * (bulge + 0.3),
    );
    let sun = dot(normal, frame.sun_direction.xyz);
    let direct = smoothstep(-0.25, 0.85, sun) * (0.55 + 0.45 * noise);
    let hemisphere = mix(frame.ground_color.rgb, frame.sky_color.rgb, normal.y * 0.5 + 0.5);
    // Lit tops and shaded bellies: billows facing down sit in the cloud's own
    // shadow, deep folds between billows take less fill, and the ball's lower
    // half sits in the shade of its upper half.
    let belly = mix(0.35, 1.0, smoothstep(-0.55, 0.65, normal.y));
    let self_shadow = mix(0.62, 1.08, smoothstep(-0.9, 0.7, point.y + (n1 - 0.5) * 0.6));
    let occlusion = (0.5 + 0.5 * smoothstep(0.2, 0.75, noise)) * belly * self_shadow;
    var color = albedo * (frame.sun_color.rgb * direct * 1.6 * mix(0.7, 1.0, self_shadow) + hemisphere * occlusion * 0.85)
        * RECIPROCAL_PI;
    // The shade side cools toward the sky's blue; the sunlit rim of a billow
    // catches a silver lining, so the shapes stay readable from above.
    color *= mix(vec3f(0.84, 0.9, 1.06), vec3f(1.0), smoothstep(-0.2, 0.5, sun));
    let rim_light = smoothstep(0.45, 0.95, radius) * smoothstep(0.1, 0.7, sun) * (1.0 - softness);
    color += albedo * frame.sun_color.rgb * rim_light * 0.22;

    var glow = 0.0;
    if heat > PUFF_FIRE_MIN_HEAT {
        // Fire: hottest at the core and in the dense noise, settling into toon bands.
        // As a fire puff cools its rim goes first, through deep red into the soot it
        // carries, so a fireball keeps a dark smoky rim and then turns to smoke.
        let core = clamp(1.0 - radius * 0.85, 0.0, 1.0);
        // Broad noise only, so the bands cool in large lobes rather than speckles.
        let broad = n1 * 0.85 + n2 * 0.15;
        let temperature = puff_bands(heat * (0.3 + 0.7 * core) * (0.35 + 0.95 * broad));
        glow = smoothstep(0.03, 0.28, temperature);
        // Each lobe of a fireball is brighter on its upper face and deepens into
        // its folds, so the fire keeps its rolling volume.
        let lobe = 0.5 + 0.7 * smoothstep(-0.35, 0.85, dot(normal, frame.camera_up.xyz) * 0.7 + (n2 - 0.5) * 0.9)
            * (0.75 + 0.25 * smoothstep(0.3, 0.7, noise));
        color = mix(color, puff_fire(temperature) * mix(1.0, lobe, fiery), glow);
    }
    if underglow > 0.0 {
        // Smoke over a fire glows from below with its light.
        let lit_from_below = underglow * (1.0 - glow) * clamp(0.75 - normal.y, 0.15, 1.0)
            * smoothstep(0.6, -0.6, point.y);
        color += vec3f(1.7, 0.45, 0.07) * lit_from_below * (0.4 + 0.6 * noise);
    }
    return color;
}

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
    // into the ground would be cut along every step of the terrain. Fire stands
    // where it burns: a hull in front of a lobe hides it, so a vehicle's blast
    // shows the hull it erupts from.
    let radius = abs(corner.x);
    let height = abs(corner.y);
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
    let far = length(to_eye_far);
    // However near the camera a billow drifts, its card never spans more than
    // this share of its distance, so smoke can veil part of the view but never
    // blank it out.
    let pull = min(max(radius * 0.6, lift), far * 0.4);
    let cap = min((far - pull) * PUFF_SCREEN_SHARE / max(radius, 1e-3), 1.0);
    (*w).position = v.instance_origin + to_eye * pull + (right * corner.x + up * corner.y) * cap;
}

// The widest a puff's card may be, as a share of its distance from the eye.
const PUFF_SCREEN_SHARE: f32 = 0.2;

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
// bands with soft steps between them, a hint of a toon fireball that still falls
// off smoothly from its white heart to its red rim.
fn puff_bands(t: f32) -> f32 {
    let f = t * 4.0;
    let step = floor(f) + smoothstep(0.2, 0.8, fract(f));
    return mix(t, step / 4.0, 0.12);
}

// Smoke thins as the wall behind it comes within this distance (m) along the
// view, like a soft particle against the depth buffer, so no wall cuts a billow
// along a hard line; and as the ground does, within a shorter one.
const PUFF_WALL_SOFTNESS: f32 = 1.2;
const PUFF_GROUND_SOFTNESS: f32 = 0.3;
// Smoke this close to the camera (m) thins away, so a billow drifting up past a
// low camera never becomes a flat curtain over the view.
const PUFF_NEAR_FADE: vec2f = vec2f(1.5, 6.0);

// How much of a puff survives at world point `p`: the ground's and the walls'
// soft-particle fade along the view. The walls are material.params: up to three
// boxes as (centre x, centre z, half width, half depth) standing on the ground,
// their heights in params[3].xyz and their count in params[3].w (`walls.rs`).
fn puff_wall_fade(p: vec3f) -> f32 {
    let ray = normalize(p - frame.camera_position.xyz);
    // Every component away from zero, so the slabs' divisions stay finite.
    let dir = select(ray, vec3f(1e-4), abs(ray) < vec3f(1e-4));
    var fade = smoothstep(PUFF_NEAR_FADE.x, PUFF_NEAR_FADE.y, length(p - frame.camera_position.xyz));
    if dir.y < 0.0 {
        fade = min(fade, smoothstep(0.0, PUFF_GROUND_SOFTNESS, p.y / -dir.y));
    }
    let count = material.params[3].w;
    if count > 0.5 {
        fade = min(fade, puff_box_fade(p, dir, material.params[0], material.params[3].x));
    }
    if count > 1.5 {
        fade = min(fade, puff_box_fade(p, dir, material.params[1], material.params[3].y));
    }
    if count > 2.5 {
        fade = min(fade, puff_box_fade(p, dir, material.params[2], material.params[3].z));
    }
    return fade;
}

// The fade at `p` for the wall box the view ray from it may strike: how far along
// the ray the box's surface lies, through the slabs of its faces.
fn puff_box_fade(p: vec3f, dir: vec3f, wall: vec4f, height: f32) -> f32 {
    let low = vec3f(wall.x - wall.z, 0.0, wall.y - wall.w);
    let high = vec3f(wall.x + wall.z, height, wall.y + wall.w);
    let t0 = (low - p) / dir;
    let t1 = (high - p) / dir;
    let enter = max(max(min(t0.x, t1.x), min(t0.y, t1.y)), min(t0.z, t1.z));
    let leave = min(min(max(t0.x, t1.x), max(t0.y, t1.y)), max(t0.z, t1.z));
    if leave < max(enter, 0.0) {
        return 1.0;
    }
    return smoothstep(0.0, PUFF_WALL_SOFTNESS, max(enter, 0.0));
}

// Below this heat a puff's fire never reaches the ramp's first glow (temperature
// 0.03 after the bands' 0.6 floor and the core and noise gains of at most 1.3).
const PUFF_FIRE_MIN_HEAT: f32 = 0.038;

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
        let lumpiness = mix(0.56, 0.32, softness);
        let rim = mix(0.8, 0.94, softness);
        let n1 = puff_noise(p * 2.1 + offset - boil);
        // The edge is linear in the finer octaves, each in 0..1: where even their
        // most favourable values leave it past the rim, the puff is empty.
        let slope = lumpiness - 0.55 * erosion;
        let least_edge = radius - 0.5 * lumpiness - 0.02 + 0.8 * erosion + slope * 0.55 * n1
            + min(0.3 * slope, 0.0) + min(0.15 * slope + 0.04, 0.0);
        if least_edge < rim {
            let n2 = puff_noise(p * 4.6 + offset * 1.7 - boil * 1.5);
            let n3 = puff_noise(p * 9.3 + offset * 2.3 - boil * 1.8);
            let noise = n1 * 0.55 + n2 * 0.3 + n3 * 0.15;
            let edge = radius + (noise - 0.5) * lumpiness + (n3 - 0.5) * 0.04
                + erosion * (0.25 + 0.55 * (1.0 - noise));
            // Fire keeps a crisper edge than smoke, so its billows read as
            // shapes rather than a blur.
            let feather = mix(0.2, 0.56, softness) * (1.0 - 0.4 * fiery) + erosion * 0.16;
            // The card's own edge is always clear, however far the noise pushes the
            // rim, so no puff shows the straight side of its quad.
            let window = 1.0 - smoothstep(0.8, 1.0, radius);
            density = (1.0 - smoothstep(rim - feather, rim, edge)) * window * puff_wall_fade(f.world);
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
        // Broad noise only, so the bands cool in large lobes rather than speckles,
        // sharpened so the fire splits into bright billows with dark creases of
        // soot laced between them rather than one flat orange blob.
        let broad = mix(n1 * 0.85 + n2 * 0.15, noise, 0.35);
        let lobes = smoothstep(0.28, 0.74, broad);
        let temperature = puff_bands(heat * (0.14 + 0.86 * core * core) * (0.1 + 1.45 * lobes));
        glow = smoothstep(0.05, 0.32, temperature);
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
        color += vec3f(1.5, 0.56, 0.12) * lit_from_below * (0.4 + 0.6 * noise);
    }
    return color;
}

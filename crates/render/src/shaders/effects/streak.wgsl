// Additive glowing streaks: sparks stretched along their flight, round glows
// (flash cores, tracer heads), muzzle and wreck flame tongues and shell tracers.
// The mesh is a unit XY quad; the instance transform is translation × scale with
// x the width and y the length (a glow's diameter on both). instance_data =
// (direction xyz unit world, kind): kind 0 a streak centred on its origin with
// its head along +direction, 1 a camera-facing glow, 2 a flame tongue whose base
// sits on the origin, 3 a tracer centred like a streak, white-hot at its head and
// fading down a tapered tail, 4 (plus a seed fraction) a licking flame based like
// a tongue, burning from yellow through orange to red licks, 5 firelight lying
// flat on the ground. Tint = linear HDR color and opacity.
fn streak_hash(p: vec2f) -> f32 {
    var q = fract(vec3f(p.x, p.y, p.x) * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn streak_noise(p: vec2f) -> f32 {
    let cell = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = streak_hash(cell);
    let b = streak_hash(cell + vec2f(1.0, 0.0));
    let c = streak_hash(cell + vec2f(0.0, 1.0));
    let d = streak_hash(cell + vec2f(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn effect_world(w: ptr<function, EffectWorld>, v: EffectVertex) {
    let corner = (*w).position - v.instance_origin;
    let kind = v.instance_data.w;
    if kind > 0.5 && kind < 1.5 {
        (*w).position = v.instance_origin + frame.camera_right.xyz * corner.x + frame.camera_up.xyz * corner.y;
        return;
    }
    if kind > 4.5 {
        (*w).position = v.instance_origin + vec3f(corner.x, 0.0, corner.y);
        return;
    }
    let width = abs(corner.x) * 2.0;
    let extent = abs(corner.y) * 2.0;
    let direction = normalize(v.instance_data.xyz + vec3f(0.0, 1e-5, 0.0));
    let to_camera = normalize(frame.camera_position.xyz - v.instance_origin);
    var side = cross(direction, to_camera);
    if dot(side, side) < 1e-6 {
        side = frame.camera_right.xyz;
    }
    side = normalize(side);
    // Seen end on, a streak keeps a width-sized dot instead of collapsing.
    let screen_up = normalize(cross(to_camera, side));
    var along = v.position.y;
    if (kind > 1.5 && kind < 2.5) || (kind > 3.5 && kind < 4.5) {
        along += 0.5;
    }
    (*w).position = v.instance_origin + side * v.position.x * width + direction * along * extent
        + screen_up * v.position.y * width * 0.6;
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let kind = f.instance_data.w;
    let across = abs(f.uv.x * 2.0 - 1.0);
    let along = f.uv.y;
    var shape = 0.0;
    var core = 0.0;
    if kind < 0.5 {
        let body = 1.0 - smoothstep(0.0, 1.0, across);
        let taper = smoothstep(0.0, 0.75, along) * (1.0 - smoothstep(0.88, 1.0, along));
        shape = body * body * taper;
        core = pow(body, 6.0) * smoothstep(0.55, 0.92, along) * 0.35;
    } else if kind < 1.5 {
        let radius = length(f.uv * 2.0 - 1.0);
        let fall = clamp(1.0 - radius, 0.0, 1.0);
        shape = fall * fall;
        core = pow(fall, 6.0);
    } else if kind < 2.5 {
        // A tongue of flame: wide and white at its base, ragged and licking to
        // its tip.
        let seed = f.instance_data.x * 40.0 + f.instance_data.z * 17.0;
        let lick = frame.camera_position.w * 9.0 + seed;
        let ragged = 0.72 + 0.28 * sin(along * 19.0 - lick) * sin(along * 7.0 + seed);
        let half_width = (1.0 - pow(along, 0.75)) * ragged + 0.03;
        let body = 1.0 - smoothstep(half_width * 0.35, half_width, across);
        shape = body * smoothstep(0.0, 0.08, along) * (1.0 - smoothstep(0.7, 1.0, along));
        core = body * (1.0 - smoothstep(0.0, 0.45, along)) * (1.0 - smoothstep(0.0, half_width * 0.5, across)) * 0.6;
    } else if kind > 4.5 {
        // Firelight on the ground: soft, brightest under the fire, its edge
        // trembling with the flames.
        let point = f.uv * 2.0 - 1.0;
        let wobble = streak_noise(point * 3.0 + vec2f(frame.camera_position.w * 1.7, 0.0)) - 0.5;
        let r = length(point) + wobble * 0.18;
        let fall = clamp(1.0 - r, 0.0, 1.0);
        (*s).opacity *= fall * fall * (1.0 - smoothstep(0.85, 1.0, length(point)));
        return;
    } else if kind > 3.5 {
        // A licking flame: a soft body of fire whose edge is eaten by noise
        // scrolling upward at one rate, so its top tears into separate licks
        // that dissolve as they climb; white-yellow where it is thickest and
        // lowest, burning out through orange to deep red at its fringes. Its
        // outline comes from the noise, never from a tapered card.
        let seed = (kind - 4.0) * 2.0 * 97.0;
        let time = frame.camera_position.w;
        let x = f.uv.x * 2.0 - 1.0;
        let rise = vec2f(0.0, time * 1.9);
        let broad = streak_noise(vec2f(x * 1.3 + seed, along * 1.6) - rise * 0.8);
        let mid = streak_noise(vec2f(x * 2.9 - seed * 0.7, along * 3.4) - rise * 1.3);
        let fine = streak_noise(vec2f(x * 6.1 + seed * 1.3, along * 6.8) - rise * 2.1);
        let n = broad * 0.55 + mid * 0.3 + fine * 0.15;
        // The licks sway with the broad noise, more the higher they reach.
        let lx = x + (broad - 0.5) * 0.7 * along;
        // A rounded body, widest a little above its root and narrowing slowly.
        let half_width = 0.82 * (1.0 - along * along * 0.8) * mix(0.75, 1.0, smoothstep(0.0, 0.3, along));
        let field = 1.0 - abs(lx) / max(half_width, 0.05) - (n - 0.42) * (0.5 + 1.5 * along) - along * 0.45;
        // The tips thin out into the air rather than ending in points.
        let dissolve = 1.0 - smoothstep(0.4, 0.95, along + (n - 0.5) * 0.45);
        let body = smoothstep(0.0, 0.5, field) * smoothstep(0.0, 0.14, along)
            * (1.0 - smoothstep(0.84, 1.0, abs(x))) * dissolve;
        let temperature = clamp(field * 1.3 * (1.05 - 0.75 * along) + (n - 0.5) * 0.3, 0.0, 1.0);
        let red = vec3f(0.55, 0.07, 0.012);
        let orange = vec3f(1.15, 0.36, 0.05);
        let white = vec3f(1.7, 1.1, 0.42);
        let fire = mix(mix(red, orange, smoothstep(0.05, 0.45, temperature)), white, smoothstep(0.55, 0.95, temperature));
        (*s).color = fire * (*s).color;
        (*s).opacity *= body * (0.3 + 0.7 * smoothstep(0.0, 0.6, temperature));
        return;
    } else {
        // A tracer: a slim white-hot core at the head, the team's glow widening
        // a little and dying away exponentially down the tail.
        let tail = along;
        let falloff = exp((tail - 1.0) * 3.2);
        let half_width = 0.35 + 0.65 * tail;
        let body = 1.0 - smoothstep(0.0, half_width, across);
        shape = body * body * falloff * smoothstep(0.0, 0.08, tail) * (1.0 - smoothstep(0.97, 1.0, tail));
        core = pow(body, 5.0) * smoothstep(0.6, 0.97, tail) * 0.8;
    }
    (*s).color = (*s).color * (0.75 + 0.5 * shape) + vec3f(2.0, 1.8, 1.4) * core;
    (*s).opacity *= max(shape, core);
}

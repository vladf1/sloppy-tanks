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
        // A licking flame: a broad yellow root, licks that sway and split as they
        // rise, burning out through orange to deep red at their ragged tips.
        let seed = (kind - 4.0) * 2.0 * 97.0;
        let time = frame.camera_position.w;
        let x = f.uv.x * 2.0 - 1.0;
        let sway = sin(along * 5.0 - time * 7.0 + seed) * 0.22 + sin(along * 11.0 - time * 13.0 + seed * 1.7) * 0.08;
        let lx = x + sway * along;
        // Soft-edged, it dissolves upward instead of ending in cut-out teeth.
        let half_width = (1.0 - pow(along, 1.1)) * (0.45 + 0.55 * smoothstep(0.0, 0.3, along)) * 0.7 + 0.03;
        let licks = streak_noise(vec2f(lx * 2.4 + seed, along * 2.6 - time * 5.0 + seed * 0.3));
        let fine = streak_noise(vec2f(lx * 5.0 - seed, along * 6.0 - time * 9.0));
        let ragged = half_width - abs(lx) + (licks - 0.5) * 0.36 * along + (fine - 0.5) * 0.08;
        let body = smoothstep(-0.04, 0.32, ragged) * smoothstep(0.0, 0.1, along);
        let dissolve = 1.0 - smoothstep(0.35, 1.0, along + (licks - 0.5) * 0.35);
        let inner = smoothstep(0.05, 0.55, ragged / max(half_width, 0.05));
        let hot = 1.0 - smoothstep(0.0, 0.7, along + (licks - 0.5) * 0.4);
        let root = vec3f(1.25, 0.46, 0.07);
        let lick = vec3f(0.8, 0.13, 0.02);
        (*s).color = mix(lick, root, hot) * (0.5 + 0.7 * inner) * (*s).color;
        (*s).opacity *= body * dissolve * (0.3 + 0.7 * hot) * (0.45 + 0.55 * inner);
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

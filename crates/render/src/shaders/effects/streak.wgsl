// Additive glowing streaks: sparks stretched along their flight, round glows
// (flash cores, tracer heads), muzzle and wreck flame tongues and shell tracers.
// The mesh is a unit XY quad; the instance transform is translation × scale with
// x the width and y the length (a glow's diameter on both). instance_data =
// (direction xyz unit world, kind): kind 0 a streak centred on its origin with
// its head along +direction, 1 a camera-facing glow, 2 a flame tongue whose base
// sits on the origin, 3 a tracer centred like a streak, white-hot at its head and
// fading down a tapered tail, 4 (plus a seed fraction) a licking flame based like
// a tongue, rising out of its opening in tongues that burn from yellow through
// orange to red, 5 firelight lying flat on the ground. Tint = linear HDR color
// and opacity.
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

// A flame's pattern climbs at one rate (noise cells a second) and pockets of
// flame break off its top at one rate (Hz): the clock is never scaled per pixel
// or per flame, only phases differ.
const FLAME_RISE: f32 = 1.45;
const FLAME_PULSE: f32 = 1.35;

// Fire's emission from its coolest fringe to its hottest core: deep red, red,
// orange, yellow. Saturated rather than white, so stacked flames stay fire
// coloured instead of washing out to cream.
fn flame_emission(t: f32) -> vec3f {
    var c = mix(vec3f(0.36, 0.035, 0.005), vec3f(1.0, 0.19, 0.018), smoothstep(0.0, 0.3, t));
    c = mix(c, vec3f(1.65, 0.58, 0.07), smoothstep(0.3, 0.62, t));
    return mix(c, vec3f(2.1, 1.15, 0.26), smoothstep(0.62, 0.95, t));
}

// A licking flame (kind 4 plus a seed fraction) standing in the opening it burns
// out of: as wide as the opening at its root, tearing into tongues as it climbs.
// Hot gas speeds up as it rises, so the pattern climbs at one rate through a
// height coordinate that grows ever more slowly: its features stretch out and
// race off the top, though never into needles. The flame snakes from side to
// side more the higher it reaches, and pockets of flame pinch off its top and
// rise on their own, so its outline never stands still like a card with a
// texture scrolling through it. White-yellow deep in its body near the root,
// burning out through orange to deep red at its edges and tips.
fn streak_flame(s: ptr<function, Surface>, f: EffectFragment, kind: f32) {
    let seed = (kind - 4.0) * 2.0 * 97.0;
    let time = frame.camera_position.w;
    let x = f.uv.x * 2.0 - 1.0;
    let along = f.uv.y;
    let lift = sqrt(along) * 1.7 + along * 1.1;
    let flow = lift - time * FLAME_RISE;
    let sway = (streak_noise(vec2f(seed * 0.37, flow * 0.6)) - 0.5) * 1.1 * along;
    let wx = x - sway;
    // As wide as its opening at the root and only a little narrower up top,
    // thinning with height so the noise splits its upper half into tongues.
    let envelope = 1.0 - abs(wx) / (0.86 - 0.4 * along) - 0.65 * along;
    let pulse = sin((lift * 0.6 - time * FLAME_PULSE) * 6.2831853 + seed);
    let pinch = smoothstep(0.2, 0.8, along) * (0.5 - 0.5 * pulse) * 0.8;
    let ragged = 1.0 + 1.3 * along;
    var body = 0.0;
    var temperature = 0.0;
    // The noise only moves the field by half its ragged amplitude either way:
    // where even that leaves it empty, the noise is skipped.
    if envelope + 0.5 * ragged - pinch > 0.0 {
        let n1 = streak_noise(vec2f(wx * 3.6 + seed, flow));
        let n2 = streak_noise(vec2f(wx * 8.0 - seed * 0.7 + n1 * 1.3, flow * 2.4 - n1 * 0.8));
        let n = n1 * 0.55 + n2 * 0.45;
        let field = envelope + (n - 0.5) * ragged - pinch;
        // The root curls up out of the opening or off the ground: lowest in the
        // middle, rising toward the flame's sides, and ragged, never a line.
        let lip = 0.2 * wx * wx + (n1 - 0.5) * 0.08;
        let root = smoothstep(lip, lip + 0.05 + 0.14 * n2, along);
        // Crisp low down, softening into wisps toward the tips.
        body = smoothstep(0.0, 0.14 + 0.42 * along, field) * root
            * (1.0 - smoothstep(0.86, 1.0, along));
        temperature = clamp(field * 1.5 * (1.0 - 0.4 * along) + (n2 - 0.5) * 0.3, 0.0, 1.0);
    }
    (*s).color = flame_emission(temperature) * (*s).color;
    (*s).opacity *= body * (0.25 + 0.75 * smoothstep(0.0, 0.55, temperature));
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
        streak_flame(s, f, kind);
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

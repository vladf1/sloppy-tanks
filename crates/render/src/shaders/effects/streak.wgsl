// Additive glowing streaks: sparks stretched along their flight, round glows
// (flash cores, tracer heads), muzzle and wreck flame tongues and shell tracers.
// The mesh is a unit XY quad; the instance transform is translation × scale with
// x the width and y the length (a glow's diameter on both). instance_data =
// (direction xyz unit world, kind): kind 0 a streak centred on its origin with
// its head along +direction, 1 a camera-facing glow, 2 a flame tongue whose base
// sits on the origin, 3 a tracer centred like a streak, white-hot at its head and
// fading down a tapered tail. Tint = linear HDR color and opacity.
fn effect_world(w: ptr<function, EffectWorld>, v: EffectVertex) {
    let corner = (*w).position - v.instance_origin;
    let kind = v.instance_data.w;
    if kind > 0.5 && kind < 1.5 {
        (*w).position = v.instance_origin + frame.camera_right.xyz * corner.x + frame.camera_up.xyz * corner.y;
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
    if kind > 1.5 && kind < 2.5 {
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

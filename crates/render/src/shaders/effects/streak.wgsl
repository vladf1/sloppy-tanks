// Additive glowing streaks: sparks and shell tracers stretched along their
// flight, round glows (flash cores, tracer heads) and muzzle flash tongues.
// The mesh is a unit XY quad; the instance transform is translation × scale with
// x the width and y the length (a glow's diameter on both). instance_data =
// (direction xyz unit world, kind): kind 0 a streak centred on its origin with
// its head along +direction, 1 a camera-facing glow, 2 a flash tongue whose base
// sits on the origin. Tint = linear HDR color and opacity.
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
    if kind > 1.5 {
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
        core = pow(body, 4.0) * smoothstep(0.4, 0.9, along);
    } else if kind < 1.5 {
        let radius = length(f.uv * 2.0 - 1.0);
        let fall = clamp(1.0 - radius, 0.0, 1.0);
        shape = fall * fall;
        core = pow(fall, 5.0);
    } else {
        // A tongue of flame: wide and white at the muzzle, ragged to its tip.
        let ragged = 0.75 + 0.25 * sin(along * 23.0 + f.instance_data.x * 40.0) * sin(along * 9.0);
        let half_width = (1.0 - pow(along, 0.8)) * ragged + 0.04;
        let body = 1.0 - smoothstep(half_width * 0.4, half_width, across);
        shape = body * smoothstep(0.0, 0.06, along);
        core = body * (1.0 - smoothstep(0.0, 0.55, along)) * (1.0 - smoothstep(0.0, half_width * 0.6, across));
    }
    (*s).color = (*s).color * (0.7 + 0.6 * shape) + vec3f(2.2, 2.0, 1.7) * core;
    (*s).opacity *= shape;
}

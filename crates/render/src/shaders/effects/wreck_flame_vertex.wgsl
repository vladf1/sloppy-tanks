// The visible outline of a rectangle consists of the two edges meeting at the
// nearest corner. Sample them in screen order so the texture stays continuous
// as a grille turns end-on.
fn grille_edge(x: vec3f, z: vec3f, side: vec3f, front: vec3f, u: f32) -> vec3f {
    let near_x = select(-x, x, dot(x, front) >= 0.0);
    let near_z = select(-z, z, dot(z, front) >= 0.0);
    let near = near_x + near_z;
    let end = near_x - near_z;
    let right = select(-end, end, dot(end, side) >= 0.0);
    let span = dot(right, side);
    let offset = clamp(u, -1.0, 1.0) * span - dot(near, side);
    let edge = select(-right, right, offset >= 0.0) - near;
    return near + edge * clamp(abs(offset) / max(abs(dot(edge, side)), 0.00001), 0.0, 1.0)
        + side * (u - clamp(u, -1.0, 1.0)) * span;
}

fn effect_world(w: ptr<function, EffectWorld>, v: EffectVertex) {
    let axis = normalize(v.instance_basis[1]);
    let eye = normalize(frame.camera_position.xyz - v.instance_origin);
    var side = cross(axis, eye);
    if dot(side, side) < 1e-5 {
        side = frame.camera_right.xyz - axis * dot(axis, frame.camera_right.xyz);
    }
    side = normalize(side);
    let front = cross(side, axis);
    let rise = v.instance_basis[1] * v.position.y;
    let above = max(dot(rise, axis) - v.instance_data.x, 0.0);
    var edge = (side * v.position.x + front * v.position.z) * length(v.instance_basis[0]);
    if v.instance_data.z > 0.5 {
        edge = grille_edge(v.instance_basis[0], v.instance_basis[2], side, front, v.position.x);
    }
    let plume = side * (v.uv.x - 0.5) * v.instance_data.y;
    let bend = smoothstep(0.25, 0.95, v.uv.y) * above;
    let updraft = vec3f(0.43, 1.0, -0.17);
    (*w).position = v.instance_origin + rise + mix(edge, plume, smoothstep(0.0, 1.3, above))
        + bend * ((updraft - axis) * 0.70 + normalize(cross(eye, side)) * 0.16);
}

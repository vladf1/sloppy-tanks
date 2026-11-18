// A rounded rectangle keeps the measured width and depth of a grille, while
// the gas loses density towards its corners. The mesh profile is a semicircle;
// scale each radial direction to the fourth-power contour of the opening.
fn vent_face(x: vec3f, z: vec3f, side: vec3f, front: vec3f, profile: vec2f) -> vec3f {
    let tangent = normalize(vec2f(dot(x, side), dot(z, side)));
    let depth = vec2f(-tangent.y, tangent.x);
    let facing = sign(dot(x * depth.x + z * depth.y, front));
    let uv = tangent * profile.x + depth * (facing * profile.y);
    let square = uv * uv;
    let scale = sqrt(dot(uv, uv) * inverseSqrt(max(dot(square, square), 1e-6)));
    return (x * uv.x + z * uv.y) * scale;
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
        edge = vent_face(v.instance_basis[0], v.instance_basis[2], side, front, v.position.xz);
    }
    let plume = side * (v.uv.x - 0.5) * v.instance_data.y;
    // Only the foot follows the surface. Above the lip project the flow into
    // the camera plane: a tall, upright strip can fold back across itself or
    // pass through the near plane when viewed down the plume.
    let screen_up = normalize(cross(eye, side));
    let at_lip = rise - axis * above;
    let upright = min(above, 0.35);
    let projected = max(dot(axis, screen_up), 0.23);
    let lean = dot(vec3f(0.30, 0.0, -0.12), side) * smoothstep(0.10, 0.85, v.uv.y);
    (*w).position = v.instance_origin + at_lip + axis * upright
        + screen_up * projected * (above - upright) + side * lean * above
        + mix(edge, plume, smoothstep(0.0, 1.3, above));
}

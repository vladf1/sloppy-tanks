// Keep the mouth of the plume in the plane of the ring for any camera angle.
fn effect_world(w: ptr<function, EffectWorld>, v: EffectVertex) {
    let corner = (*w).position - v.instance_origin;
    let mouth = v.instance_data.xyz;
    let axis = normalize(mouth);
    let eye = normalize(frame.camera_position.xyz - v.instance_origin);
    var side = cross(axis, eye);
    if dot(side, side) < 1e-5 {
        side = frame.camera_right.xyz;
    }
    side = normalize(side);
    let above = max(corner.y - length(mouth), 0.0);
    let bend = smoothstep(0.25, 0.95, v.uv.y) * above;
    let updraft = vec3f(0.43, 1.0, -0.17);
    (*w).position = v.instance_origin + axis * corner.y + side * corner.x
        + bend * ((updraft - axis) * 0.70 + normalize(cross(eye, side)) * 0.16);
}

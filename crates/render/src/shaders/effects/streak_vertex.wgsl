// A streak's quad (`streak.wgsl`), a shared vertex hook (`EffectDefinition::shared_vertex`).
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

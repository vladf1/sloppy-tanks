// A dust billboard's quad (`dust.wgsl`), a shared vertex hook
// (`EffectDefinition::shared_vertex`).
fn effect_world(w: ptr<function, EffectWorld>, v: EffectVertex) {
    let corner = (*w).position - v.instance_origin;
    (*w).position = v.instance_origin + frame.camera_right.xyz * corner.x + frame.camera_up.xyz * corner.y;
}

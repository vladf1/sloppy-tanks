// Wind sway of the village meadow tufts (`village-vegetation.ts`), after the
// instance transform (tufts are instanced at world placements, so this is world
// space). instance_data.xy is the tuft's wind origin (world x/z). Normals stay.
fn effect_world(w: ptr<function, EffectWorld>, v: EffectVertex) {
    let t = frame.camera_position.w;
    let o = v.instance_data.xy;
    let p = (*w).position;
    (*w).position = p + vec3f(
        sin(t * 1.2 + o.x * 0.7 + o.y * 0.4) * p.y * 0.2,
        0.0,
        cos(t * 0.8 + o.y * 0.5) * p.y * 0.12,
    );
}

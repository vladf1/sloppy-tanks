// Explosion puffs (explosion-effects.ts): a camera-facing quad and a soft,
// slightly top-lit disc. The instance transform is translation × scale only, so
// the offset from its origin is the scaled corner; Three's billboardVertex read
// the same scale from the matrix columns. Tint = linear rgb and opacity.
fn effect_world(w: ptr<function, EffectWorld>, v: EffectVertex) {
    let corner = (*w).position - v.instance_origin;
    (*w).position = v.instance_origin + frame.camera_right.xyz * corner.x + frame.camera_up.xyz * corner.y;
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let point = f.uv * 2.0 - 1.0;
    let radius = length(point);
    (*s).color *= clamp(1.0 - radius + point.y * 0.3, 0.0, 1.0) * 0.24 + 0.76;
    (*s).opacity *= 1.0 - smoothstep(0.35, 1.0, radius);
}

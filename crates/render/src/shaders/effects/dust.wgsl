// Dust billboards (effect-materials.ts dustMaterial): a camera-facing quad with a
// soft round falloff. The instance transform is translation × scale only.
// Tint = linear rgb and opacity.
fn effect_world(w: ptr<function, EffectWorld>, v: EffectVertex) {
    let corner = (*w).position - v.instance_origin;
    (*w).position = v.instance_origin + frame.camera_right.xyz * corner.x + frame.camera_up.xyz * corner.y;
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let radius = length(f.uv * 2.0 - 1.0);
    (*s).opacity *= 1.0 - smoothstep(0.1, 1.0, radius);
}

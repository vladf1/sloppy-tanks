// Dust billboards (effect-materials.ts dustMaterial): a camera-facing quad with a
// soft, slightly ragged falloff, sunnier on its upper side. The instance
// transform is translation × scale only. Tint = linear rgb and opacity;
// instance_data.x is an optional seed that turns each puff's breakup its own way.
fn effect_world(w: ptr<function, EffectWorld>, v: EffectVertex) {
    let corner = (*w).position - v.instance_origin;
    (*w).position = v.instance_origin + frame.camera_right.xyz * corner.x + frame.camera_up.xyz * corner.y;
}

fn dust_hash(p: vec2f) -> f32 {
    var q = fract(vec3f(p.x, p.y, p.x) * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn dust_noise(p: vec2f) -> f32 {
    let cell = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = dust_hash(cell);
    let b = dust_hash(cell + vec2f(1.0, 0.0));
    let c = dust_hash(cell + vec2f(0.0, 1.0));
    let d = dust_hash(cell + vec2f(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let point = f.uv * 2.0 - 1.0;
    let seed = f.instance_data.x * 57.3;
    let n = dust_noise(point * 2.6 + vec2f(seed, seed * 0.7)) * 0.65 + dust_noise(point * 5.9 - vec2f(seed * 1.3, seed)) * 0.35;
    let radius = length(point) + (n - 0.5) * 0.38;
    (*s).opacity *= 1.0 - smoothstep(0.1, 1.0, radius);
    (*s).color *= 0.86 + 0.18 * clamp(point.y * 0.5 + 0.5, 0.0, 1.0) + 0.1 * (n - 0.5);
}

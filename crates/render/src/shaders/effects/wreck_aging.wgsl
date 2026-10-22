// Burnt wreck darkening (`wreck-aging.ts`): the base color (after vertex colors
// and map) and the emissive glow are both scaled by the brightness. Presentation
// feeds it per instance as instance_data = (brightness, 1, -, -), so one shared
// wreck model serves every age; otherwise params[0].x is the brightness.
// The burnt steel keeps broad, soft soot blotches, sootiest low on the hull.
fn wreck_hash(p: vec2f) -> f32 {
    var q = fract(vec3f(p.x, p.y, p.x) * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn wreck_noise(p: vec2f) -> f32 {
    let cell = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = wreck_hash(cell);
    let b = wreck_hash(cell + vec2f(1.0, 0.0));
    let c = wreck_hash(cell + vec2f(0.0, 1.0));
    let d = wreck_hash(cell + vec2f(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let brightness = select(material.params[0].x, f.instance_data.x, f.instance_data.y > 0.5);
    let p = f.world.xz * 0.9 + vec2f(f.world.y * 0.7, -f.world.y * 0.5);
    let soot = wreck_noise(p) * 0.65 + wreck_noise(p * 2.3) * 0.35;
    let low = 1.0 - smoothstep(0.3, 1.6, f.world.y);
    (*s).color *= brightness * (0.86 + 0.2 * soot - 0.12 * low);
    (*s).emissive *= brightness;
}

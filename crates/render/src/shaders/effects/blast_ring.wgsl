// A blast's ground ring (explosion-effects.ts): a ragged dusty band that flashes
// in and fades. instance_data = (age s, phase rad).
fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let point = f.uv * 2.0 - 1.0;
    let radius = length(point);
    let age = f.instance_data.xy;
    let angle = atan2(point.y, point.x);
    let breakup = sin(angle * 7.0 + age.y) * 0.12 + sin(angle * 13.0 - age.y) * 0.08 + 0.8;
    let band = smoothstep(0.43, 0.65, radius) * (1.0 - smoothstep(0.72, 0.98, radius));
    let fade = smoothstep(0.0, 0.07, age.x) * (1.0 - smoothstep(0.12, 0.55, age.x));
    (*s).color = vec3f(0.48, 0.35, 0.21);
    (*s).opacity = band * breakup * fade * 0.34;
}

// Burnt wreck charring (`wreck-aging.ts`): the base color (after vertex colors
// and map) chars toward soot-black steel and the emissive glow dims with the
// brightness. Presentation feeds it per instance as instance_data = (brightness,
// 1, upright, -), so one shared wreck model serves every age; otherwise params[0].x is
// the brightness. Brightness falls from 0.8 at death to 0.2 a few seconds later.
// The burnt steel keeps blistered patches of its paint, sootiest low on the
// hull, and its seams smoulder with flickering embers.
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
    // Most of the paint is gone the moment the ammunition goes, so a fresh wreck
    // reads as burnt steel with fire on it, not a glowing painted hull; the rest
    // chars as the fire does its work.
    let charred = mix(0.72, 1.0, smoothstep(0.8, 0.45, brightness));
    let p = f.world.xz * 1.1 + vec2f(f.world.y * 0.9, -f.world.y * 0.6);
    let soot = wreck_noise(p) * 0.6 + wreck_noise(p * 2.7) * 0.4;
    let low = 1.0 - smoothstep(0.3, 1.6, f.world.y);
    let up = smoothstep(0.2, 0.9, (*s).normal.y);
    // Charred steel: near-black soot, grey-white ash settled on the faces turned
    // up to the fire, a little blistered rust-brown low on the sides, and only
    // rare scraps of the paint. Dark values so the fire on it is the only warm
    // thing about it.
    let paint = smoothstep(0.78, 0.9, soot + 0.06 * (1.0 - low));
    let rusted = smoothstep(0.5, 0.66, soot) * (1.0 - paint) * (1.0 - up) * 0.7;
    let steel = vec3f(0.028, 0.026, 0.025) * (0.7 + 0.6 * soot);
    let ash = vec3f(0.16, 0.155, 0.15) * (0.6 + 0.6 * wreck_noise(p * 4.3 + vec2f(2.9, 5.1)));
    let ashen = up * smoothstep(0.35, 0.65, wreck_noise(p * 1.7 + vec2f(9.1, 4.4)));
    var burnt = mix(steel, vec3f(0.075, 0.042, 0.026), rusted);
    burnt = mix(burnt, ash, ashen * 0.85);
    let kept = (*s).color * (0.3 + 0.15 * soot);
    let char_color = mix(burnt, kept, paint * 0.6);
    (*s).color = mix((*s).color * (0.86 + 0.2 * soot - 0.12 * low), char_color, charred * 0.97);
    (*s).color *= mix(1.0, 0.85 + 0.15 * brightness / 0.8, 1.0 - charred);
    (*s).roughness = mix((*s).roughness, 0.95, charred);
    // Embers glow only from faces turned up toward the fire, deep in a hatch or
    // the turret well, never as a disc on an upturned belly.
    // A piece fed per instance says how upright it lies: a hull on its side or
    // its roof shows no glowing turret well.
    let upright = select(1.0, smoothstep(0.6, 0.9, f.instance_data.z), f.instance_data.y > 0.5);
    (*s).emissive *= brightness * 0.55 * up * upright;
}

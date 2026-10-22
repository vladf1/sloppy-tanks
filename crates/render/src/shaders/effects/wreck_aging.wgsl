// Burnt wreck charring (`wreck-aging.ts`): the base color (after vertex colors
// and map) chars toward soot-black steel and the emissive glow dims with the
// brightness. Presentation feeds it per instance as instance_data = (brightness,
// 1, -, -), so one shared wreck model serves every age; otherwise params[0].x is
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
    // 0 at the moment of death, 1 once the fire has done its work.
    let charred = smoothstep(0.78, 0.3, brightness);
    let p = f.world.xz * 1.1 + vec2f(f.world.y * 0.9, -f.world.y * 0.6);
    let soot = wreck_noise(p) * 0.6 + wreck_noise(p * 2.7) * 0.4;
    let low = 1.0 - smoothstep(0.3, 1.6, f.world.y);
    // Patches where the paint survived, scorched brown at their edges.
    let paint = smoothstep(0.66, 0.8, soot + 0.08 * (1.0 - low));
    let scorched = smoothstep(0.42, 0.6, soot) * (1.0 - paint);
    let steel = vec3f(0.035, 0.032, 0.03) * (0.75 + 0.5 * soot);
    let burnt_paint = mix(steel, vec3f(0.11, 0.06, 0.035), scorched);
    let kept = (*s).color * (0.32 + 0.2 * soot);
    let char_color = mix(burnt_paint, kept, paint * 0.8);
    (*s).color = mix((*s).color * (0.86 + 0.2 * soot - 0.12 * low), char_color, charred * 0.92);
    (*s).color *= mix(1.0, 0.85 + 0.15 * brightness / 0.8, 1.0 - charred);
    (*s).emissive *= brightness;
    // Seams and low panels smoulder: thin bright cracks that breathe.
    let crack = wreck_noise(p * 3.3 + vec2f(7.1, 2.3));
    let sparse = smoothstep(0.55, 0.75, wreck_noise(p * 0.8 + vec2f(3.7, 1.9)));
    let seam = smoothstep(0.035, 0.0, abs(crack - 0.5)) * sparse;
    let breathe = 0.6 + 0.4 * sin(frame.camera_position.w * 3.1 + soot * 11.0);
    let heat = mix(1.0, 0.45, charred);
    (*s).emissive += vec3f(1.8, 0.4, 0.05) * seam * breathe * heat * charred * (0.3 + 0.7 * low);
}

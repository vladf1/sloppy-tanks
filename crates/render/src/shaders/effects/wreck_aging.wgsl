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
    // Half the paint goes the moment the ammunition does, so even a hull still
    // in the air reads as scorched steel; the rest chars as the fire works.
    let charred = mix(0.55, 0.92, smoothstep(0.8, 0.4, brightness));
    let p = f.world.xz * 1.1 + vec2f(f.world.y * 0.9, -f.world.y * 0.6);
    let soot = wreck_noise(p) * 0.6 + wreck_noise(p * 2.7) * 0.4;
    let low = 1.0 - smoothstep(0.3, 1.6, f.world.y);
    let up = smoothstep(0.2, 0.9, (*s).normal.y);
    // Soot streaks run up the sides from the fire, so the char has a grain the
    // eye reads as burnt metal at any distance, never a flat dark slab.
    let streak = wreck_noise(vec2f(dot(f.world.xz, vec2f(2.3, 1.7)), f.world.y * 0.45));
    // Charred steel: dark gunmetal under a soot bloom, blistered rust-brown
    // where the paint burnt off, grey ash on the faces turned up to the fire,
    // and ragged patches of the scorched paint left on the panels.
    let paint = smoothstep(0.62, 0.8, soot + 0.08 * (1.0 - low) - 0.12 * streak);
    let rusted = smoothstep(0.42, 0.6, soot) * (1.0 - paint) * (1.0 - 0.6 * up);
    let steel = vec3f(0.062, 0.058, 0.055) * (0.65 + 0.7 * streak);
    let ash = vec3f(0.2, 0.19, 0.18) * (0.6 + 0.6 * wreck_noise(p * 4.3 + vec2f(2.9, 5.1)));
    let ashen = up * smoothstep(0.4, 0.7, wreck_noise(p * 1.7 + vec2f(9.1, 4.4)));
    var burnt = mix(steel, vec3f(0.13, 0.07, 0.04) * (0.7 + 0.5 * streak), rusted * 0.85);
    burnt = mix(burnt, ash, ashen * 0.75);
    // The paint left is scorched: darker, browner, sootier low down.
    let scorched = (*s).color * vec3f(0.5, 0.44, 0.38) * (0.6 + 0.3 * soot - 0.25 * low);
    let char_color = mix(burnt, scorched, paint * 0.85);
    (*s).color = mix((*s).color * (0.86 + 0.2 * soot - 0.12 * low), char_color, charred);
    (*s).color *= mix(1.0, 0.85 + 0.15 * brightness / 0.8, 1.0 - charred);
    (*s).roughness = mix((*s).roughness, 0.9, charred);
    // Embers glow only from faces turned up toward the fire, deep in a hatch or
    // the turret well, never as a disc on an upturned belly.
    // A piece fed per instance says how upright it lies: a hull on its side or
    // its roof shows no glowing turret well.
    let upright = select(1.0, smoothstep(0.6, 0.9, f.instance_data.z), f.instance_data.y > 0.5);
    (*s).emissive *= brightness * 0.55 * up * upright;
    // Torn seams glow hot from the blast and cool within seconds: thin bright
    // cracks that breathe at one rate, each with its own phase.
    let crack = wreck_noise(p * 3.3 + vec2f(7.1, 2.3));
    let sparse = smoothstep(0.6, 0.8, wreck_noise(p * 0.8 + vec2f(3.7, 1.9)));
    let seam = smoothstep(0.022, 0.0, abs(crack - 0.5)) * sparse * (1.0 - 0.6 * up);
    let breathe = 0.65 + 0.35 * sin(frame.camera_position.w * 3.1 + soot * 11.0);
    let heat = smoothstep(0.35, 0.8, brightness);
    (*s).emissive += mix(vec3f(0.9, 0.12, 0.02), vec3f(1.8, 0.5, 0.08), heat) * seam * breathe * heat;
}

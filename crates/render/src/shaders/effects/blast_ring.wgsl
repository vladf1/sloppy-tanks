// A blast's ground decals, one look per instance: instance_data = (age s, phase
// rad, kind, strength). Kind 0 is the dust a blast throws out low (tint = the
// ground's dust color), kind 1 the shock front racing ahead of it: one continuous
// band of lifted dust, frayed by noise as it spreads (tint the same dust), kind 2
// the scorch it leaves behind: a charred burn with a noise-broken edge, soot
// rays, clumps of thrown dirt (tint the dust) and embers that flicker and cool,
// faded out by `strength` as it ages. Kinds 3 and 4 stand on a wall's face where a
// shell struck it: a chipped crater in stone (tint = the wall's color) and a sooty
// dent in steel with a hot heart that cools within a second or two.
fn ring_hash(p: vec2f) -> f32 {
    var q = fract(vec3f(p.x, p.y, p.x) * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn ring_noise(p: vec2f) -> f32 {
    let cell = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = ring_hash(cell);
    let b = ring_hash(cell + vec2f(1.0, 0.0));
    let c = ring_hash(cell + vec2f(0.0, 1.0));
    let d = ring_hash(cell + vec2f(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let point = f.uv * 2.0 - 1.0;
    let radius = length(point);
    let age = f.instance_data.x;
    let phase = f.instance_data.y;
    let kind = f.instance_data.z;
    let strength = f.instance_data.w;
    let seed = vec2f(cos(phase), sin(phase)) * 13.7;
    // A domain warp breaks every ring into irregular lobes with no repeating
    // pattern around it.
    let warp = vec2f(ring_noise(point * 2.3 + seed), ring_noise(point * 2.3 - seed + 5.1)) - 0.5;
    let n = ring_noise((point + warp * 0.5) * 3.1 + seed) * 0.6 + ring_noise(point * 7.3 - seed) * 0.4;
    // Nothing reaches the quad's edge, however far the warp pushes a shape.
    let inside = 1.0 - smoothstep(0.82, 0.98, radius);
    if kind < 0.5 {
        // Dust thrown out low: a broad, clumpy wave thickest just behind its front.
        let ragged = length(point + warp * 0.35) + (n - 0.5) * 0.18;
        let band = smoothstep(0.3, 0.84, ragged) * (1.0 - smoothstep(0.84, 1.0, ragged));
        let clumps = smoothstep(0.3, 0.75, ring_noise((point + warp) * 4.3 + seed * 0.7));
        let fade = smoothstep(0.0, 0.05, age) * (1.0 - smoothstep(0.1, 0.7, age));
        (*s).color *= 0.85 + 0.25 * n;
        (*s).opacity *= band * (0.3 + 0.7 * clumps) * fade * 0.48 * inside;
    } else if kind < 1.5 {
        // The shock front: one unbroken band of lifted dust racing out, a pale
        // crest leading a darker trough, fraying into gaps as it weakens.
        let r = length(point + warp * 0.12);
        let crest = smoothstep(0.7, 0.92, r) * (1.0 - smoothstep(0.92, 0.985, r));
        let trough = smoothstep(0.5, 0.8, r) * (1.0 - smoothstep(0.8, 0.9, r));
        let fray = smoothstep(0.75 - 0.55 * strength, 1.0 - 0.45 * strength, ring_noise(point * 5.5 + seed) * 0.6 + n * 0.5);
        let hot = 1.0 - smoothstep(0.0, 0.07, age);
        let pale = (*s).color * 1.5 + vec3f(0.1, 0.09, 0.07);
        let dark = (*s).color * 0.55;
        let mixed_color = mix(dark, pale, crest / max(crest + trough * 0.6, 1e-4));
        (*s).color = mix(mixed_color, vec3f(1.6, 0.75, 0.28), hot * 0.6);
        (*s).opacity = clamp(crest * 0.85 + trough * 0.45, 0.0, 1.0) * fray * strength * inside;
    } else if kind > 2.5 {
        wall_mark(s, point, radius, warp, n, seed, age, kind > 3.5, strength * inside);
    } else {
        // Char: a burn darkest at its heart whose edge the noise breaks into
        // lobes and soot rays, ringed by clumps of thrown dirt and embers that
        // flicker as they cool.
        let fine = ring_noise(point * 9.0 - seed * 1.3);
        let r = length(point + warp * 0.45) + (fine - 0.5) * 0.12;
        let around = point / max(radius, 1e-4);
        let rays = smoothstep(0.45, 0.85, ring_noise(around * 2.2 + seed) * 0.7 + ring_noise(around * 4.5 - seed) * 0.3);
        let heart = 1.0 - smoothstep(0.06, 0.55, r);
        let reach = rays * (1.0 - smoothstep(0.25, 0.86, r + (n - 0.5) * 0.2)) * smoothstep(0.05, 0.3, r);
        let burn = clamp(heart * (0.8 + 0.3 * n) + reach * 0.38, 0.0, 1.0) * (0.85 + 0.15 * fine);
        let soot = mix(vec3f(0.075, 0.055, 0.04), vec3f(0.012, 0.011, 0.01), smoothstep(0.15, 0.85, heart));
        let clumps = smoothstep(0.55, 0.8, ring_noise(point * 6.0 + seed * 0.4) * 0.65 + fine * 0.45);
        let thrown = smoothstep(0.42, 0.6, r) * (1.0 - smoothstep(0.62, 0.92, r)) * clumps;
        let dirt_alpha = thrown * 0.32 * (1.0 - burn);
        let alpha = burn * 0.9 + dirt_alpha;
        let dirt = (*s).color * 0.78;
        var color = (soot * burn * 0.9 + dirt * dirt_alpha) / max(alpha, 1e-4);
        // Embers ring the burn's edge and fleck its heart; each flickers on its
        // own, the hottest die within seconds and a few smoulder on.
        let ring = smoothstep(0.24, 0.38, r) * (1.0 - smoothstep(0.38, 0.54, r));
        let speck = ring_noise(point * 15.0 + seed * 2.0);
        let flecks = smoothstep(0.72, 0.92, speck) * heart;
        // One rate for every ember, each with its own phase: a rate that varied
        // across the decal would scale the running time into per-pixel sparkle.
        let flicker = 0.55 + 0.45 * sin(frame.camera_position.w * 6.0 + speck * 40.0 + fine * 9.0);
        let cool = exp(-age / 1.8);
        let smoulder = 0.18 * (1.0 - smoothstep(4.0, 14.0, age)) * smoothstep(0.8, 0.95, speck);
        let embers = ((ring * smoothstep(0.45, 0.8, n) + flecks * 0.7) * cool + smoulder * heart) * flicker;
        let ember_color = mix(vec3f(1.1, 0.1, 0.02), vec3f(3.2, 0.95, 0.15), cool);
        color = color + ember_color * embers;
        // The whole decal fades out toward its rim, never at a hard ellipse.
        let edge = 1.0 - smoothstep(0.7, 0.95, r);
        (*s).color = color;
        (*s).opacity = clamp(max(alpha, embers), 0.0, 1.0) * strength * inside * edge;
    }
}

// A shell's mark on a wall. Stone: a dark pit inside a ring of freshly broken,
// paler stone, cracks running out of it and a faint scorch. Steel: a punched dent,
// a ring of scraped bare metal and a halo of soot, its heart glowing as it cools.
fn wall_mark(
    s: ptr<function, Surface>,
    point: vec2f,
    radius: f32,
    warp: vec2f,
    n: f32,
    seed: vec2f,
    age: f32,
    steel: bool,
    strength: f32,
) {
    let fine = ring_noise(point * 11.0 - seed * 1.3);
    let r = length(point + warp * 0.3) + (fine - 0.5) * 0.1 + (n - 0.5) * 0.12;
    let edge = 1.0 - smoothstep(0.78, 0.97, r);
    if steel {
        // Streaks of soot thrown out from the hole, a scraped ring of bare steel
        // round the dent, and the hole itself, glowing as it cools.
        let around = point / max(radius, 1e-4);
        let streaks = ring_noise(around * 3.5 + seed) * 0.6 + ring_noise(around * 7.0 - seed) * 0.4;
        let hole = 1.0 - smoothstep(0.08, 0.18, r);
        let bare = smoothstep(0.1, 0.2, r) * (1.0 - smoothstep(0.34, 0.5, r + (fine - 0.5) * 0.1));
        let soot = (1.0 - smoothstep(0.25, 0.65 + 0.3 * streaks, r)) * (0.55 + 0.45 * n);
        let hot = exp(-age / 0.55) * (1.0 - smoothstep(0.0, 0.38, r));
        var color = mix(vec3f(0.03, 0.026, 0.022), vec3f(0.62, 0.62, 0.64) * (0.8 + 0.3 * fine), bare / max(bare + soot * 0.5, 1e-4));
        color = mix(color, vec3f(0.005), hole);
        color += mix(vec3f(1.4, 0.18, 0.03), vec3f(4.0, 1.7, 0.4), hot) * hot;
        let alpha = max(max(soot * 0.9, bare * 0.95), max(hole, hot));
        (*s).color = color;
        (*s).opacity = clamp(alpha, 0.0, 1.0) * edge * strength;
        return;
    }
    let skin = (*s).color;
    let pit = 1.0 - smoothstep(0.14, 0.38, r);
    let chipped = smoothstep(0.12, 0.34, r) * (1.0 - smoothstep(0.5, 0.72, r)) * (0.65 + 0.45 * fine);
    // Ridged noise draws thin cracks running out from the pit.
    let ridge = 1.0 - abs(ring_noise((point + warp * 0.4) * 4.2 + seed) * 2.0 - 1.0);
    let cracks = smoothstep(0.9, 0.975, ridge) * (1.0 - smoothstep(0.45, 0.92, r));
    let scorch = (1.0 - smoothstep(0.2, 0.85, r)) * 0.38 * exp(-age / 20.0);
    let broken = min(skin * 1.3 + vec3f(0.06), vec3f(1.0));
    var color = mix(skin * 0.3, broken, chipped / max(chipped + scorch, 1e-4));
    color = mix(color, skin * 0.16, pit);
    color = mix(color, skin * 0.22, cracks);
    let alpha = max(max(pit * 0.92, chipped * 0.85), max(cracks * 0.8, scorch));
    (*s).color = color;
    (*s).opacity = clamp(alpha, 0.0, 1.0) * edge * strength;
}

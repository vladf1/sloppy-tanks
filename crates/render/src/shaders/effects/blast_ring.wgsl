// A blast's ground decals, one look per instance: instance_data = (age s, phase
// rad, kind, strength). Kind 0 is the dust a blast throws out low (tint = the
// ground's dust color), kind 1 the shock front racing ahead of it: one continuous
// band of lifted dust, frayed by noise as it spreads (tint the same dust), kind 2
// the scorch it leaves behind: a charred burn with a noise-broken edge, soot
// rays, clumps of thrown dirt (tint the dust) and embers that flicker and cool,
// faded out by `strength` as it ages. Kinds 3 and 4 stand on a wall's face where a
// shell struck it: a chipped crater in stone (tint = the wall's color) and a sooty
// dent in steel with a hot heart that cools within a second or two. Kind 5 is the
// pale powder a concrete hit spills on the ground at the wall's foot.
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
        // The crest is lifted dust, a shade paler than the ground, never a
        // bright drawn circle.
        let pale = (*s).color * 1.12 + vec3f(0.03, 0.028, 0.024);
        let dark = (*s).color * 0.55;
        let mixed_color = mix(dark, pale, crest / max(crest + trough * 0.6, 1e-4));
        (*s).color = mix(mixed_color, vec3f(1.6, 0.75, 0.28), hot * 0.45);
        (*s).opacity = clamp(crest * 0.55 + trough * 0.4, 0.0, 1.0) * fray * strength * inside;
    } else if kind > 4.5 {
        // Powder a concrete hit spilled at the wall's foot: a pale, patchy
        // stain, feathered into the ground, grainy where the grit lies thick.
        let r = length(point + warp * 0.55) + (n - 0.5) * 0.25;
        let stain = 1.0 - smoothstep(0.15, 0.85, r);
        let grain = smoothstep(0.35, 0.8, ring_noise(point * 11.0 + seed) * 0.6 + n * 0.4);
        let settle_in = smoothstep(0.1, 0.6, age);
        (*s).color *= 0.9 + 0.2 * grain;
        (*s).opacity = stain * (0.32 + 0.28 * grain) * settle_in * strength * inside;
    } else if kind > 2.5 {
        wall_mark(s, point, radius, warp, n, seed, age, kind > 3.5, strength * inside);
    } else {
        // Char: a radial burn, sooty black at its heart and feathering out
        // through brown soot into the ground, streaked by soot rays blown out
        // from the blast, a soft rim of thrown dirt and embers that flicker and
        // cool. No hard flecks: from overhead they read as noise, not burn.
        let fine = ring_noise(point * 9.0 - seed * 1.3);
        let r = length(point + warp * 0.3) + (fine - 0.5) * 0.06;
        let around = point / max(radius, 1e-4);
        let rays = smoothstep(0.35, 0.8, ring_noise(around * 2.6 + seed) * 0.65 + ring_noise(around * 6.0 - seed) * 0.35);
        let heart = 1.0 - smoothstep(0.0, 0.62, r);
        let reach = rays * (1.0 - smoothstep(0.3, 0.92, r + (n - 0.5) * 0.15)) * smoothstep(0.1, 0.35, r);
        let burn = clamp(pow(heart, 0.7) * (0.9 + 0.15 * n) + reach * 0.45, 0.0, 1.0);
        let soot = mix(vec3f(0.06, 0.047, 0.036), vec3f(0.01, 0.009, 0.008), smoothstep(0.1, 0.8, heart));
        let clumps = smoothstep(0.45, 0.85, ring_noise(point * 4.5 + seed * 0.4) * 0.7 + fine * 0.3);
        let thrown = smoothstep(0.45, 0.65, r) * (1.0 - smoothstep(0.65, 0.95, r)) * clumps;
        let dirt_alpha = thrown * 0.16 * (1.0 - burn);
        let alpha = burn * 0.92 + dirt_alpha;
        let dirt = (*s).color * 0.8;
        var color = (soot * burn * 0.92 + dirt * dirt_alpha) / max(alpha, 1e-4);
        // Embers ring the burn's edge and glow through its heart; each flickers
        // on its own, the hottest die within seconds and a few smoulder on.
        let ring = smoothstep(0.2, 0.36, r) * (1.0 - smoothstep(0.36, 0.56, r));
        let speck = ring_noise(point * 7.0 + seed * 2.0);
        let flecks = smoothstep(0.6, 0.9, speck) * heart;
        // One rate for every ember, each with its own phase: a rate that varied
        // across the decal would scale the running time into per-pixel sparkle.
        // The phase drifts slowly across the scorch: a phase that ran steeply
        // with fine noise would draw its gradient as bands of light.
        let ember_phase = ring_noise(point * 4.0 + seed * 3.0) * 6.2831853;
        let flicker = 0.6 + 0.4 * sin(frame.camera_position.w * 6.0 + ember_phase);
        let cool = exp(-age / 1.8);
        let smoulder = 0.14 * (1.0 - smoothstep(4.0, 14.0, age)) * smoothstep(0.7, 0.95, speck);
        // Embers lie in patches, never a continuous glowing ring.
        let embers = ((ring * smoothstep(0.58, 0.85, n) * 0.8 + flecks * 0.7) * cool + smoulder * heart) * flicker;
        let ember_color = mix(vec3f(1.1, 0.1, 0.02), vec3f(3.2, 0.95, 0.15), cool);
        color = color + ember_color * embers;
        // The whole decal feathers out toward its rim, never at a hard ellipse.
        let edge = 1.0 - smoothstep(0.5, 0.97, r);
        (*s).color = color;
        (*s).opacity = clamp(max(alpha, embers), 0.0, 1.0) * strength * inside * edge;
    }
}

// A shell's mark on a wall. Stone: a crater knocked out of the face, its far
// slope in shadow and its lip catching the light so it reads as a hole, not a
// stain; bent rebar across its floor, cracks running out to the face's edges and
// a pale bloom of crushed dust round it. Steel: the sheet punched in and torn
// open: a black breach with jagged petals of bare steel bent back round it, a
// dent ring of shaded and lit metal, blistered paint and a soot fan, the torn
// edges glowing as they cool.
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
    let edge = 1.0 - smoothstep(0.8, 0.98, r);
    let around = point / max(radius, 1e-4);
    // One side of every hole is in the shade of its lip: a fixed direction in
    // the mark's own frame, turned by its phase so no two marks match.
    let lit_side = dot(around, normalize(seed));
    if steel {
        let paint = (*s).color;
        let angle = atan2(point.y, point.x);
        // Petals: the breach's edge pushed in and out by angle, so the hole is a
        // torn star rather than a round punch.
        let petals = ring_noise(vec2f(angle * 2.2, 0.0) + seed) * 0.6 + ring_noise(vec2f(angle * 5.1, 1.7) - seed) * 0.4;
        let breach = 0.17 + 0.24 * petals * petals;
        let hole = 1.0 - smoothstep(breach - 0.015, breach + 0.015, r);
        let lip = smoothstep(breach - 0.01, breach + 0.02, r) * (1.0 - smoothstep(breach + 0.05, breach + 0.15 + 0.08 * petals, r));
        // The dent: the sheet sags in toward the breach, darker on its shaded
        // side and bright where it faces the light.
        let dent = smoothstep(0.85, 0.3, r) * (1.0 - hole);
        let slope = dent * smoothstep(0.15, 0.6, r);
        let shade = 1.0 - 0.55 * slope * smoothstep(-0.2, 0.8, -lit_side);
        let shine = slope * smoothstep(0.3, 0.9, lit_side);
        let streaks = ring_noise(around * 3.5 + seed) * 0.6 + ring_noise(around * 7.0 - seed) * 0.4;
        let soot = (1.0 - smoothstep(0.3, 0.7 + 0.3 * streaks, r)) * (0.6 + 0.4 * n);
        let blister = smoothstep(0.45, 0.58, r) * (1.0 - smoothstep(0.66, 0.82, r + (fine - 0.5) * 0.2)) * smoothstep(0.35, 0.65, fine);
        let bare = vec3f(0.55, 0.56, 0.58) * (0.75 + 0.35 * fine);
        var color = paint * shade;
        color = mix(color, vec3f(0.035, 0.03, 0.026), soot * 0.85);
        color = mix(color, vec3f(0.16, 0.08, 0.04), blister * 0.7);
        color = mix(color, bare, lip);
        color += bare * shine * 0.6;
        color = mix(color, vec3f(0.004), hole);
        // The torn edges stay hot for a few seconds after the hit.
        let hot = exp(-age / 1.8) * (lip + 0.6 * hole * smoothstep(0.0, breach, r));
        color += mix(vec3f(1.4, 0.18, 0.03), vec3f(4.0, 1.7, 0.4), exp(-age / 0.35)) * hot;
        let alpha = max(max(hole, lip), max(soot * 0.95, max(slope * 0.8, blister * 0.75)));
        (*s).color = color;
        (*s).opacity = clamp(alpha, 0.0, 1.0) * edge * strength;
        return;
    }
    let skin = (*s).color;
    // The crater: a ragged hole whose shaded far slope and lit lip give it depth.
    let crater_r = r + (n - 0.5) * 0.2;
    let pit = 1.0 - smoothstep(0.34, 0.46, crater_r);
    let depth = smoothstep(0.46, 0.0, crater_r);
    let lip = smoothstep(0.38, 0.46, crater_r) * (1.0 - smoothstep(0.48, 0.6, crater_r));
    let wall_shade = pit * (0.35 + 0.65 * smoothstep(-0.6, 0.6, lit_side));
    // Bent rebar: two dark bars across the crater's floor.
    let bar_dir = normalize(seed);
    let bars = max(
        smoothstep(0.035, 0.01, abs(dot(point, vec2f(-bar_dir.y, bar_dir.x)) - 0.06)),
        smoothstep(0.03, 0.008, abs(dot(point, bar_dir) + 0.05))
    ) * pit * smoothstep(0.44, 0.28, crater_r);
    // Chipped stone round the lip, in clumps.
    let clumps = smoothstep(0.42, 0.72, ring_noise(point * 6.5 + seed * 0.8));
    let chipped = smoothstep(0.4, 0.52, r) * (1.0 - smoothstep(0.6, 0.8, r)) * clumps * (0.6 + 0.4 * fine);
    // Cracks: ridged noise drawn thin, running out to the mark's edge.
    let ridge = 1.0 - abs(ring_noise((point + warp * 0.4) * 3.6 + seed) * 2.0 - 1.0);
    let ridge2 = 1.0 - abs(ring_noise((point - warp * 0.3) * 6.8 - seed) * 2.0 - 1.0);
    let cracks = max(smoothstep(0.92, 0.985, ridge), smoothstep(0.94, 0.99, ridge2) * 0.8) * smoothstep(0.35, 0.5, r);
    // Crushed dust settles pale round the hole; a little soot at its heart.
    let dust = (1.0 - smoothstep(0.4, 0.95, r + (n - 0.5) * 0.3)) * 0.5;
    let scorch = (1.0 - smoothstep(0.3, 0.7, r + (n - 0.5) * 0.3)) * 0.45 * exp(-age / 25.0);
    var color = mix(skin, min(skin * 1.18 + vec3f(0.05), vec3f(1.0)), dust);
    color = mix(color, skin * 0.35, scorch);
    color = mix(color, min(skin * 1.25 + vec3f(0.06), vec3f(1.0)), chipped + lip * 0.8);
    color = mix(color, skin * mix(0.08, 0.4, wall_shade) * (0.8 + 0.4 * fine), pit);
    color = mix(color, skin * 0.12 * (1.0 - 0.5 * depth), cracks);
    color = mix(color, vec3f(0.05, 0.035, 0.03), bars);
    let alpha = max(max(pit, chipped * 0.9), max(cracks * 0.9, max(dust, lip)));
    (*s).color = color;
    (*s).opacity = clamp(alpha, 0.0, 1.0) * edge * strength;
}

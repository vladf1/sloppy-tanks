// A blast's ground decals, one look per instance: instance_data = (age s, phase
// rad, kind, strength). Kind 0 is the dust a blast throws out low (tint = the
// ground's dust color), kind 1 the shock front's pale flash racing ahead of it
// (tint the same dust), kind 2 the scorch it leaves behind: a soft charred
// burn with a rim of thrown dirt (tint the dust) and an ember ring that cools,
// faded out by `strength` as it ages; kind 3 the fire's light on the ground (tint
// = HDR glow color, strength its brightness), drawn under the smoke.
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
        (*s).color *= 0.92 + 0.22 * n;
        (*s).opacity *= band * (0.3 + 0.7 * clumps) * fade * 0.42 * inside;
    } else if kind < 1.5 {
        // The shock front: a thin pale flash of lifted dust racing out.
        let ragged = length(point + warp * 0.25);
        let band = smoothstep(0.72, 0.95, ragged) * (1.0 - smoothstep(0.95, 1.0, ragged));
        (*s).color = (*s).color * 1.45 + vec3f(0.08, 0.07, 0.05);
        (*s).opacity = band * (0.5 + 0.5 * n) * strength * 0.35 * inside;
    } else if kind < 2.5 {
        // Char: a soft radial burn, darkest at its heart, reaching out in a few
        // soft streaks, ringed by the dirt it threw and embers that cool.
        let r = length(point + warp * 0.3);
        let around = point / max(radius, 1e-4);
        let streaks = smoothstep(0.45, 0.85, ring_noise(around * 3.0 + seed) * 0.6 + ring_noise(around * 6.5 - seed) * 0.4);
        let heart = 1.0 - smoothstep(0.08, 0.58, r);
        let reach = streaks * (1.0 - smoothstep(0.3, 0.82, r)) * smoothstep(0.05, 0.3, r);
        let burn = clamp(heart * (0.85 + 0.25 * n) + reach * 0.45, 0.0, 1.0);
        let soot = mix(vec3f(0.07, 0.052, 0.04), vec3f(0.012, 0.011, 0.01), smoothstep(0.15, 0.85, heart));
        let thrown = smoothstep(0.48, 0.64, r) * (1.0 - smoothstep(0.66, 0.95, r)) * smoothstep(0.35, 0.7, n);
        let dirt_alpha = thrown * 0.22 * (1.0 - burn);
        let alpha = burn * 0.88 + dirt_alpha;
        let dirt = (*s).color * 0.95;
        var color = (soot * burn * 0.88 + dirt * dirt_alpha) / max(alpha, 1e-4);
        // Embers ring the burn's edge and fleck its heart, cooling over seconds.
        let ring = smoothstep(0.26, 0.4, r) * (1.0 - smoothstep(0.4, 0.56, r));
        let flecks = smoothstep(0.7, 0.92, ring_noise(point * 13.0 + seed * 2.0)) * heart;
        let cool = exp(-age / 1.6);
        let embers = (ring * smoothstep(0.45, 0.8, n) + flecks * 0.7) * cool;
        let ember_color = mix(vec3f(1.2, 0.12, 0.02), vec3f(3.4, 1.0, 0.16), cool);
        color = color + ember_color * embers;
        (*s).color = color;
        (*s).opacity = clamp(max(alpha, embers), 0.0, 1.0) * strength * inside;
    } else {
        // Firelight on the ground: soft, brightest under the fire, its edge
        // trembling with the flames.
        let r = length(point + warp * 0.25);
        let fall = clamp(1.0 - r, 0.0, 1.0);
        let flicker = 0.85 + 0.15 * sin(frame.camera_position.w * 17.0 + phase * 3.0 + n * 4.0);
        (*s).opacity = fall * fall * (0.75 + 0.25 * n) * flicker * strength * 0.5 * inside;
    }
}

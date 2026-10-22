// A blast's ground decals, one look per instance: instance_data = (age s, phase
// rad, kind, strength). Kind 0 is the dusty ring a blast throws out (tint = the
// ground's dust color), kind 1 the shockwave's crisp dust front racing ahead of
// it (tint the same dust), kind 2 the scorch it leaves behind: a charred blotch
// with blast rays and embers that cool, faded out by `strength` as it ages.
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
    // Seamless angular noise: sample around a circle rather than by angle.
    let around = point / max(radius, 1e-4);
    let n = ring_noise(point * 3.1 + seed) * 0.6 + ring_noise(point * 7.3 - seed) * 0.4;
    if kind < 0.5 {
        // Dust thrown out low: a broad, clumpy wave thickest just behind its front.
        let ragged = radius + (n - 0.5) * 0.2;
        let band = smoothstep(0.2, 0.86, ragged) * (1.0 - smoothstep(0.86, 1.0, ragged));
        let clumps = smoothstep(0.25, 0.75, ring_noise(around * 6.0 + seed * 0.5) * 0.5 + n * 0.6);
        let fade = smoothstep(0.0, 0.05, age) * (1.0 - smoothstep(0.12, 0.75, age));
        (*s).color *= 0.92 + 0.2 * n;
        (*s).opacity *= band * band * (0.35 + 0.65 * clumps) * fade * 0.55;
    } else if kind < 1.5 {
        // The shock front: dust blasted outward in ragged rays along a thin
        // racing band, glowing warm for its first instant.
        let ragged = radius + (n - 0.5) * 0.06;
        let band = smoothstep(0.6, 0.94, ragged) * (1.0 - smoothstep(0.94, 1.0, ragged));
        let rays = smoothstep(0.3, 0.8, ring_noise(around * 11.0 + seed) * 0.7 + n * 0.4);
        let hot = 1.0 - smoothstep(0.0, 0.08, age);
        (*s).color = mix((*s).color * 1.3 + vec3f(0.05), vec3f(1.6, 0.8, 0.3), hot * 0.6);
        (*s).opacity = band * band * (0.15 + 0.85 * rays) * strength * 0.75;
    } else {
        // Char: a lumpy blotch, rays thrown outward, embers that die over seconds.
        let blot = 1.0 - smoothstep(0.32, 0.72, radius + (n - 0.5) * 0.5);
        let ray_noise = ring_noise(around * 4.0 + seed) * 0.7 + ring_noise(around * 9.0 - seed) * 0.3;
        let rays = smoothstep(0.5, 0.8, ray_noise) * (1.0 - smoothstep(0.35, 1.0, radius)) * smoothstep(0.1, 0.4, radius);
        let fine = ring_noise(point * 16.0 + seed * 2.0);
        let char_amount = clamp(max(blot, rays * 0.75) * (0.75 + 0.35 * fine), 0.0, 1.0);
        let embers = smoothstep(0.62, 0.86, fine * 0.6 + n * 0.5) * blot * (1.0 - smoothstep(0.4, 3.5, age));
        let soot = vec3f(0.018, 0.016, 0.014);
        (*s).color = soot + vec3f(3.2, 0.8, 0.12) * embers;
        (*s).opacity = clamp(char_amount * 0.82 + embers, 0.0, 1.0) * strength;
    }
}

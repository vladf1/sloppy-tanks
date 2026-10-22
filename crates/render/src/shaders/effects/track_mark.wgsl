// Tread marks (tracks.ts): a mark keeps its strength for four seconds, then fades
// out by TRACK_LIFETIME (24 s). instance_data = (birth s, strength); params[0].x
// is the trail clock in simulation seconds, written by the pool each frame, so
// marks freeze with the simulation while paused. Each mark is one grouser's
// imprint: pressed deepest across its middle, its edges soft and broken by the
// ground's grain, so a trail reads as a churned tread rather than stencilled
// dashes.
fn track_hash(p: vec2f) -> f32 {
    var q = fract(vec3f(p.x, p.y, p.x) * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn track_noise(p: vec2f) -> f32 {
    let cell = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = track_hash(cell);
    let b = track_hash(cell + vec2f(1.0, 0.0));
    let c = track_hash(cell + vec2f(0.0, 1.0));
    let d = track_hash(cell + vec2f(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let age = material.params[0].x - f.instance_data.x;
    let across = abs(f.uv.x * 2.0 - 1.0);
    let along = abs(f.uv.y * 2.0 - 1.0);
    let grain = track_noise(f.world.xz * 4.5) * 0.6 + track_noise(f.world.xz * 11.0) * 0.4;
    let side = 1.0 - smoothstep(0.55, 1.0, across + (grain - 0.5) * 0.35);
    let press = 1.0 - smoothstep(0.35, 1.0, along + (grain - 0.5) * 0.3);
    let imprint = side * (0.35 + 0.65 * press);
    // The pressed middle is darkest; the churned rims sit a little lighter.
    (*s).color *= mix(1.25, 0.9, press);
    (*s).opacity *= f.instance_data.y * imprint * (0.6 + 0.5 * grain) * 0.9
        * (1.0 - smoothstep(4.0, 24.0, age));
}

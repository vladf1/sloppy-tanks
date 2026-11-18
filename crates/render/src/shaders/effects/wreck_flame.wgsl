// Two advected temperature slices and the front slice's soot from
// generate-wreck-fire.mjs. Blend time in the fields before shading them.
fn plume_uv(index: f32, uv: vec2f) -> vec2f {
    let row = floor(index / 12.0);
    let tile = vec2f(index - row * 12.0, 11.0 - row);
    let inset = vec2f(0.5 / 96.0, 0.5 / 144.0);
    return (tile + mix(inset, vec2f(1.0) - inset, uv)) / 12.0;
}

fn plume_sample(t: f32, uv: vec2f) -> vec3f {
    let index = floor(t) % 144.0;
    let a = textureSample(extra_texture0, extra_sampler0, plume_uv(index, uv)).rgb;
    let b = textureSample(extra_texture0, extra_sampler0, plume_uv((index + 1.0) % 144.0, uv)).rgb;
    return mix(a, b, fract(t));
}

fn plume_emission(heat: f32) -> vec3f {
    // Cooler gas loses its blue and then green emission first.
    let h = max(heat - 0.07, 0.0);
    let h2 = h * h;
    return vec3f(8.64 * h, 0.408 * h + 5.76 * h2, 0.042 * h + 3.72 * h2 * h2);
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let t = f.instance_data.w;
    let uv = vec2f(select(f.uv.x, 1.0 - f.uv.x, f.tint.z > 0.5), f.uv.y);
    // The two slices travel independently. Their combined silhouette does
    // not repeat during a wreck's life, even when one slice wraps.
    let near = plume_sample(t, uv);
    let far = plume_sample(t * 0.83 + f.tint.z * 47.0, uv);
    let gas = vec3f(far.r, near.gb);
    let back = smoothstep(0.09, 0.47, gas.r);
    let front = smoothstep(0.09, 0.47, gas.g);
    let soot = smoothstep(0.055, 0.36, gas.b) * (1.0 - smoothstep(0.10, 0.40, gas.g)) * 0.57;
    let opacity = front + (1.0 - front) * (soot + (1.0 - soot) * back);
    let light = plume_emission(gas.g) * front + (1.0 - front)
        * (soot * vec3f(0.035, 0.030, 0.027) + (1.0 - soot) * plume_emission(gas.r) * back);
    let above = smoothstep(f.tint.y, f.tint.y + 0.40, uv.y);
    let width = mix(f.tint.x, 0.5, above);
    let opening = 1.0 - smoothstep(width * 0.83, width, abs(uv.x - 0.5));
    (*s).color = light / max(opacity, 0.001);
    (*s).opacity = f.tint.a * opacity * opening * (1.0 - smoothstep(0.82, 1.0, uv.y));
}

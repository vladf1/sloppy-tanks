// Helpers shared by the quarry surfaces (`effects_scenery.rs`): an explicit
// smoothstep (also for e0 > e1), sRGB hex colors, the derivative bump and the
// world-space grit. Normals are world space here; the bump's dot and cross
// products are invariant under the view rotation Three evaluated them in.

const LUMA = vec3f(0.2126, 0.7152, 0.0722);

fn quarry_smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = clamp((x - e0) / (e1 - e0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn srgb_hex(hex: u32) -> vec3f {
    let c = vec3f(f32((hex >> 16u) & 255u), f32((hex >> 8u) & 255u), f32(hex & 255u)) / 255.0;
    return select(pow((c + 0.055) / 1.055, vec3f(2.4)), c / 12.92, c <= vec3f(0.04045));
}

// Mikkelsen's surface-gradient bump from a height already sampled for color.
// Height and position use the same derivative sign convention (Three's
// dFdy = -dpdy), so it cancels.
fn derivative_bump(height: f32, scale: f32, world: vec3f, normal: vec3f, front: bool) -> vec3f {
    let slope = vec2f(dpdx(height), -dpdy(height)) * scale;
    let sigma_x = normalize(dpdx(world));
    let sigma_y = normalize(-dpdy(world));
    let r1 = cross(sigma_y, normal);
    let r2 = cross(normal, sigma_x);
    let det = dot(sigma_x, r1) * select(-1.0, 1.0, front);
    let gradient = sign(det) * (slope.x * r1 + slope.y * r2);
    return normalize(abs(det) * normal - gradient);
}

struct QuarryGrit {
    color: f32,
    normal: vec3f,
}

// Centimetre grit and pebbles from the packed-dirt tile (extra texture 0).
fn quarry_grit(amount: f32, relief: f32, f: EffectFragment, normal: vec3f) -> QuarryGrit {
    let p = f.world.xz;
    let near = dot(textureSample(extra_texture0, extra_sampler0, p / 3.3).rgb, LUMA);
    let turned = vec2f(p.x * 0.6 + p.y * 0.8, p.y * 0.6 - p.x * 0.8) / 11.7;
    let far = textureSample(extra_texture0, extra_sampler0, turned);
    let detail = near * (dot(far.rgb, LUMA) + 0.327) / (0.327 * 0.327 * 2.0);
    var grit: QuarryGrit;
    grit.color = mix(1.0, detail, amount);
    grit.normal = derivative_bump(near, relief, f.world, normal, f.front_facing);
    return grit;
}

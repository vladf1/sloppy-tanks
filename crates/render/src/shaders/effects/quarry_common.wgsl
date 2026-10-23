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

// The hashes and value noise below are lookups of the ground noise tile
// (`ground_noise.wgsl`), bound with the soil's and the sandstone's materials:
// hashing lattice corners per pixel cost the rock and the floor more than any of
// their layers.

// Four random numbers in [0, 1] for a lattice cell (whole coordinates; a fractional
// one stands for the cell holding it).
fn quarry_hash(cell: vec2f) -> vec4f {
    return vec4f(ground_hash(cell), ground_hash(cell + vec2f(113.0, 57.0)));
}

// One random number in [0, 1] for a lattice cell, the value noise's corner.
fn quarry_hash1(cell: vec2f) -> f32 {
    return ground_hash(cell).x;
}

// Smooth value noise in [0, 1] on a unit lattice.
fn quarry_value(p: vec2f) -> f32 {
    return ground_noise(p).x;
}

// Smooth value noise with its gradient: (value, d/dx, d/dy) on a unit lattice,
// from four corners of white noise so the gradient is exact.
fn quarry_value_grad(p: vec2f) -> vec3f {
    let i = floor(p);
    let f = p - i;
    let u = f * f * (3.0 - 2.0 * f);
    let du = 6.0 * f * (1.0 - f);
    let a = quarry_hash1(i);
    let b = quarry_hash1(i + vec2f(1.0, 0.0));
    let c = quarry_hash1(i + vec2f(0.0, 1.0));
    let d = quarry_hash1(i + vec2f(1.0, 1.0));
    let k = a - b - c + d;
    return vec3f(
        a + (b - a) * u.x + (c - a) * u.y + k * u.x * u.y,
        du.x * ((b - a) + k * u.y),
        du.y * ((c - a) + k * u.x),
    );
}

// The world-space metres one pixel spans on a surface: computed in uniform
// control flow, before any branch, and passed to the detail layers that fade
// out before they shrink below a pixel and shimmer.
fn quarry_footprint(world: vec3f) -> f32 {
    return max(length(dpdx(world)), length(dpdy(world)));
}



// Plates cracked apart on a `cell` metre jittered lattice (dried mud, jointed
// caprock): x is the crack line (1 inside a crack about `width` metres wide), y
// the curled or rounded, paler plate rim beside it, z a hash in [0, 1) of the
// plate the point lies on.
fn quarry_cracks(p: vec2f, cell: f32, width_scale: f32, footprint: f32) -> vec3f {
    // A gentle warp curves the plate edges so the network never reads as tiles.
    let g0 = p / cell;
    let g = g0 + 0.22 * sin(g0.yx * vec2f(2.3, 1.9) + vec2f(1.7, 0.4));
    let id = floor(g);
    var f1 = 8.0;
    var f2 = 8.0;
    var nearest = id;
    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            let o = vec2f(f32(i), f32(j));
            let h = ground_hash(id + o + 71.0);
            let d = length(id + o + 0.05 + h.xy * 0.9 - g);
            if d < f1 {
                f2 = f1;
                f1 = d;
                nearest = id + o;
            } else if d < f2 {
                f2 = d;
            }
        }
    }
    let edge = (f2 - f1) * cell;
    let width = (0.012 + 0.01 * quarry_hash1(id + 29.0)) * width_scale;
    let soft = max(footprint, 0.004);
    // Thin cracks fade to a faint darkening once a pixel spans them.
    let visible = width / max(width, footprint);
    let crack = (1.0 - quarry_smoothstep(width - soft, width + soft, edge)) * visible;
    let rim = quarry_smoothstep(width * 2.0 + 0.06, width, edge) * (1.0 - crack);
    return vec3f(crack, rim, ground_hash(nearest + 171.0).x);
}

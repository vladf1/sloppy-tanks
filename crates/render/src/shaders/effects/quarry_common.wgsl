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

// Four hashes in [0, 1) of a lattice cell (Hoskins' hash without sine).
fn quarry_hash(cell: vec2f) -> vec4f {
    var p = fract(cell.xyxy * vec4f(0.1031, 0.1030, 0.0973, 0.1099));
    p = p + dot(p, p.wzxy + 33.33);
    return fract((p.xxyz + p.yzzw) * p.zywx);
}

// Smooth value noise in [0, 1) on a unit lattice, from `quarry_hash`.
fn quarry_value(p: vec2f) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = quarry_hash(i).x;
    let b = quarry_hash(i + vec2f(1.0, 0.0)).x;
    let c = quarry_hash(i + vec2f(0.0, 1.0)).x;
    let d = quarry_hash(i + vec2f(1.0, 1.0)).x;
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Smooth value noise with its gradient: (value, d/dx, d/dy) on a unit lattice.
fn quarry_value_grad(p: vec2f) -> vec3f {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let du = 6.0 * f * (1.0 - f);
    let a = quarry_hash(i).x;
    let b = quarry_hash(i + vec2f(1.0, 0.0)).x;
    let c = quarry_hash(i + vec2f(0.0, 1.0)).x;
    let d = quarry_hash(i + vec2f(1.0, 1.0)).x;
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

struct QuarryPebbles {
    // Coverage of a stone (0..1), the contact shade around it, the shadow it
    // casts away from the sun, the horizontal lean of its domed surface's normal
    // (world x/z) and its tint over the soil.
    cover: f32,
    contact: f32,
    shadow: f32,
    lean: vec2f,
    tint: vec3f,
    // How far toward its buried rim a covered pixel lies (0 crown, 1 rim).
    rim: f32,
}

// One layer of loose stones on a `cell` metre lattice: at most one stone per
// cell, wholly inside it, so a single lookup finds it. `density` is the chance a
// cell holds one. Stones are tinted relative to the soil so they sit in it, and
// fade into the ground before they shrink to a pixel and shimmer.
fn quarry_pebbles(p: vec2f, cell: f32, density: f32, footprint: f32, salt: f32) -> QuarryPebbles {
    let g = p / cell;
    let id = floor(g);
    let h = quarry_hash(id + salt);
    let k = quarry_hash(id - salt * 1.7);
    let radius = mix(0.12, 0.3, h.y * h.y);
    let stretch = 1.0 + 0.5 * k.w;
    let center = id + 0.5 + (h.zw - 0.5) * (1.0 - 2.0 * radius * stretch);
    // Flattened, turned ovals rather than coins, each a little lumpy.
    let axis = normalize(k.xw - 0.5 + vec2f(0.0001, 0.0));
    let side = vec2f(-axis.y, axis.x);
    let d0 = g - center;
    let local = vec2f(dot(d0, axis) / stretch, dot(d0, side)) / radius;
    let lump = 1.0 + 0.1 * sin(local.x * 2.7 + local.y * 1.3 + k.z * 6.0);
    let r = length(local) / lump;
    let pixel = footprint / (radius * cell);
    let present = step(h.x, density) * (1.0 - quarry_smoothstep(0.25, 0.7, pixel));
    let edge = max(pixel, 0.12);
    var out: QuarryPebbles;
    out.cover = (1.0 - quarry_smoothstep(1.0 - edge, 1.0, r)) * present;
    out.contact = quarry_smoothstep(1.55, 1.0, r) * (1.0 - out.cover) * present;
    // The ground a stone's height shades: its outline seen from the sun, a low
    // sun stretching it out across the soil.
    let sun = frame.sun_direction.xyz;
    let reach = radius * 0.55 * min(length(sun.xz) / max(sun.y, 0.15), 3.0);
    let lit = d0 + normalize(sun.xz + vec2f(0.0001, 0.0)) * reach;
    let lit_local = vec2f(dot(lit, axis) / stretch, dot(lit, side)) / radius;
    let blocked = 1.0 - quarry_smoothstep(0.75, 1.05, length(lit_local) / lump);
    out.shadow = blocked * (1.0 - out.cover) * present;
    // A low dome: the normal leans out from the centre, steepest at the rim.
    let dome = local / max(length(local), 0.001) * quarry_smoothstep(0.1, 1.0, r) * 0.8;
    out.lean = (dome.x * axis + dome.y * side) * out.cover;
    // The rim sinks into shade on the side turned from the sun.
    let from_sun = dot(d0 / max(length(d0), 0.0001), -normalize(sun.xz + vec2f(0.0001, 0.0)));
    out.rim = quarry_smoothstep(0.35, 1.0, r) * out.cover * (0.3 + 0.7 * clamp(from_sun * 0.6 + 0.5, 0.0, 1.0));
    // Pale limestone, the soil's own stone, cool grey grit, iron-stained, dark chert.
    var t = vec3f(1.18, 1.14, 1.08);
    t = select(t, vec3f(0.98, 0.93, 0.86), k.y > 0.3);
    t = select(t, vec3f(0.78, 0.8, 0.84), k.y > 0.55);
    t = select(t, vec3f(1.02, 0.72, 0.55), k.y > 0.75);
    t = select(t, vec3f(0.52, 0.48, 0.45), k.y > 0.9);
    out.tint = t * (0.85 + 0.3 * k.z);
    return out;
}

// Plates cracked apart on a `cell` metre jittered lattice (dried mud, jointed
// caprock): x is the crack line (1 inside a crack about `width` metres wide), y
// the curled or rounded, paler plate rim beside it.
fn quarry_cracks(p: vec2f, cell: f32, width_scale: f32, footprint: f32) -> vec2f {
    // A gentle warp curves the plate edges so the network never reads as tiles.
    let g0 = p / cell;
    let g = g0 + 0.22 * sin(g0.yx * vec2f(2.3, 1.9) + vec2f(1.7, 0.4));
    let id = floor(g);
    var f1 = 8.0;
    var f2 = 8.0;
    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            let o = vec2f(f32(i), f32(j));
            let h = quarry_hash(id + o + 71.0);
            let d = length(id + o + 0.05 + h.xy * 0.9 - g);
            if d < f1 {
                f2 = f1;
                f1 = d;
            } else if d < f2 {
                f2 = d;
            }
        }
    }
    let edge = (f2 - f1) * cell;
    let width = (0.012 + 0.01 * quarry_hash(id).z) * width_scale;
    let soft = max(footprint, 0.004);
    // Thin cracks fade to a faint darkening once a pixel spans them.
    let visible = width / max(width, footprint);
    let crack = (1.0 - quarry_smoothstep(width - soft, width + soft, edge)) * visible;
    let rim = quarry_smoothstep(width * 2.0 + 0.06, width, edge) * (1.0 - crack);
    return vec2f(crack, rim);
}

// Pine Village meadow (`effects_scenery::MEADOW_SWAY`). One effect serves three
// village materials, told apart by params[0].x (a uniform, so the branches stay
// in uniform control flow):
//
// 0: the meadow tufts (`village_vegetation.rs`) sway in the wind after the
//    instance transform (tufts are instanced at world placements, so this is
//    world space), by their height above their own base. instance_data.xy is the
//    tuft's wind origin (world x/z).
// 1: the village ground (`ground_surfaces.rs`): a meadow of lush and dry
//    patches worn into dirt roads with tank treads, packed lanes, puddles,
//    pebbles and wildflowers, its edges breaking into clumps over a hand's
//    width in places and a few metres in others, cottage gardens and the
//    farmland outside the wall, all in world space. The map is
//    the grass tile at the mesh UVs (world / 8 m), extra texture 0 the
//    packed-dirt tile, extra texture 1 the baked ground plan (`village_ground.rs`,
//    linear, unflipped). params[0] = (1, arena half size, unused,
//    unused); params[1].xyz the north-south roads' centre x; params[2].xyz the
//    east-west roads' centre z; params[3] = half widths (north-south outer,
//    north-south centre, east-west outer, east-west centre).
// 2: the mountain ranges (`village_landscape.rs`): ridged crags break the
//    slopes into buttresses, gullies and ledges, a canopy of single conifer
//    crowns thins into separate trees at a ragged treeline, and crisp snow
//    fills the gentler faces and gullies above the snowline, in world space;
//    the low meadows keep their vertex colours.
fn effect_world(w: ptr<function, EffectWorld>, v: EffectVertex) {
    if material.params[0].x > 0.5 {
        return;
    }
    let t = frame.camera_position.w;
    let o = v.instance_data.xy;
    var p = (*w).position;
    // Every blade of every tuft its own length: a hash of the tuft and the
    // blade's tip (its mesh position), so no two clumps share a silhouette.
    // Roots stay put; only the height above the tuft's base stretches.
    let blade = vg_hash(floor(v.position.xz * 37.0) + o * 1.618);
    let reach = mix(0.55, 1.4, blade * blade) * mix(0.85, 1.15, vg_hash(o + 3.7));
    let lift = p.y - v.instance_origin.y;
    p.y = v.instance_origin.y + lift * reach;
    let h = max(lift * reach, 0.0);
    (*w).position = p + vec3f(
        sin(t * 1.2 + o.x * 0.7 + o.y * 0.4) * h * 0.2,
        0.0,
        cos(t * 0.8 + o.y * 0.5) * h * 0.12,
    );
}

const VG_LUMA = vec3f(0.2126, 0.7152, 0.0722);

fn vg_linear(hex: u32) -> vec3f {
    let c = vec3f(f32((hex >> 16u) & 255u), f32((hex >> 8u) & 255u), f32(hex & 255u)) / 255.0;
    return select(pow((c + 0.055) / 1.055, vec3f(2.4)), c / 12.92, c <= vec3f(0.04045));
}

// Dave Hoskins' sine-free hashes: stable across GPUs at world-sized inputs.
fn vg_hash(p: vec2f) -> f32 {
    var p3 = fract(vec3f(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn vg_hash2(p: vec2f) -> vec2f {
    var p3 = fract(vec3f(p.x, p.y, p.x) * vec3f(0.1031, 0.1030, 0.0973));
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.xx + p3.yz) * p3.zy);
}

fn vg_noise(p: vec2f) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = vg_hash(i);
    let b = vg_hash(i + vec2f(1.0, 0.0));
    let c = vg_hash(i + vec2f(0.0, 1.0));
    let d = vg_hash(i + vec2f(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn vg_fbm(p: vec2f) -> f32 {
    return vg_noise(p) * 0.55 + vg_noise(p * 2.07 + 17.3) * 0.3 + vg_noise(p * 4.13 - 9.1) * 0.15;
}

// Surface-gradient bump of a world-space height field (metres), like the
// standard shader's bump but from a procedural height.
fn vg_bump(height: f32, world: vec3f, normal: vec3f, front: bool) -> vec3f {
    let slope = vec2f(dpdx(height), -dpdy(height));
    let sigma_x = normalize(dpdx(world));
    let sigma_y = normalize(-dpdy(world));
    let r1 = cross(sigma_y, normal);
    let r2 = cross(normal, sigma_x);
    let det = dot(sigma_x, r1) * select(-1.0, 1.0, front);
    let gradient = sign(det) * (slope.x * r1 + slope.y * r2);
    return normalize(abs(det) * normal - gradient);
}

// One road family's nearest strip: (signed metres inside it, offset across it,
// half width). `centres` are the strips' centres across, `along` the coordinate
// along them; strips stop a metre short of the arena edge `extent`.
fn vg_strip(across: f32, along: f32, centres: vec3f, outer: f32, centre: f32, extent: f32) -> vec3f {
    var best = vec3f(-1000.0, 0.0, 1.0);
    for (var i = 0; i < 3; i++) {
        let half = select(outer, centre, i == 1);
        let offset = across - centres[i];
        let inside = min(half - abs(offset), extent - 1.0 - abs(along));
        best = select(best, vec3f(inside, offset, half), inside > best.x);
    }
    return best;
}

// A disc on a jittered cell grid: x the coverage of this cell's disc (0 when the
// cell has none), y its dome height in metres, zw two random numbers for its look.
// Only a cell that holds a disc works out its shape.
fn vg_scatter(p: vec2f, density: f32, cells: f32, size: vec2f, aa: f32, seed: f32) -> vec4f {
    // No cell has a disc: callers weight everything else by the coverage.
    if density <= 0.0 {
        return vec4f(0.0);
    }
    let q = p * cells;
    let cell = floor(q);
    let jitter = vg_hash2(cell + seed);
    let pick = vg_hash(cell + seed * 1.37 + 5.1);
    var cover = 0.0;
    var dome = 0.0;
    if fract(pick * 13.7) < density {
        let centre = (cell + 0.2 + jitter * 0.6) / cells;
        let stretch = mix(0.65, 1.0, jitter.y);
        let angle = pick * 6.2832;
        let local = p - centre;
        let turned = vec2f(
            local.x * cos(angle) - local.y * sin(angle),
            (local.x * sin(angle) + local.y * cos(angle)) / stretch,
        );
        let base_radius = mix(size.x, size.y, fract(pick * 7.31));
        // A stone, a clod or a bloom is never a true ellipse: a few lobes
        // break its outline, each its own.
        let theta = atan2(turned.y, turned.x);
        let radius = base_radius * (1.0 + 0.15 * sin(theta * 3.0 + pick * 19.0) + 0.08 * sin(theta * 5.0 + jitter.x * 11.0));
        let distance = length(turned);
        cover = 1.0 - smoothstep(radius - aa, radius + aa, distance);
        dome = sqrt(max(1.0 - distance * distance / (radius * radius), 0.0)) * base_radius;
    }
    return vec4f(cover, dome, jitter.x, fract(pick * 3.71));
}

// `vg_scatter` working out the disc of every cell, empty or not, for the cottage
// beds: skipping their empty cells changes how the beds draw (measured; the full
// computation evidently leaves those cells non-zero there), so they keep it.
fn vg_dense_scatter(p: vec2f, density: f32, cells: f32, size: vec2f, aa: f32, seed: f32) -> vec4f {
    if density <= 0.0 {
        return vec4f(0.0);
    }
    let q = p * cells;
    let cell = floor(q);
    let jitter = vg_hash2(cell + seed);
    let pick = vg_hash(cell + seed * 1.37 + 5.1);
    let centre = (cell + 0.2 + jitter * 0.6) / cells;
    let stretch = mix(0.65, 1.0, jitter.y);
    let angle = pick * 6.2832;
    let local = p - centre;
    let turned = vec2f(
        local.x * cos(angle) - local.y * sin(angle),
        (local.x * sin(angle) + local.y * cos(angle)) / stretch,
    );
    let radius = mix(size.x, size.y, fract(pick * 7.31));
    let distance = length(turned);
    let present = select(0.0, 1.0, fract(pick * 13.7) < density);
    let cover = (1.0 - smoothstep(radius - aa, radius + aa, distance)) * present;
    let dome = sqrt(max(1.0 - distance * distance / (radius * radius), 0.0)) * radius * present;
    return vec4f(cover, dome, jitter.x, fract(pick * 3.71));
}

// A low rosette of leaves on a jittered cell grid: x its coverage, y how far
// out along its leaves (0 at the heart), z a random number for its look.
fn vg_rosette(p: vec2f, density: f32, cells: f32, size: vec2f, aa: f32, seed: f32) -> vec3f {
    if density <= 0.0 {
        return vec3f(0.0);
    }
    let cell = floor(p * cells);
    let jitter = vg_hash2(cell + seed);
    let pick = vg_hash(cell + seed * 1.37 + 5.1);
    let local = p - (cell + 0.25 + jitter * 0.5) / cells;
    let distance = length(local);
    let radius = mix(size.x, size.y, fract(pick * 7.31));
    // Five or six leaves, each tapering to a point.
    let leaves = select(5.0, 6.0, pick > 0.5);
    let angle = atan2(local.y, local.x) + pick * 6.2832;
    let reach = radius * mix(0.35, 1.0, pow(abs(cos(angle * leaves * 0.5)), 0.7));
    let present = select(0.0, 1.0, fract(pick * 13.7) < density);
    let cover = (1.0 - smoothstep(reach - aa, reach + aa, distance)) * present;
    return vec3f(cover, distance / radius, jitter.x);
}

// A tussock of blades seen from above on a jittered cell grid: x its coverage,
// y how far out along its blades (0 at the heart), z a random number for its
// look. Nine to twelve thin points, so it reads as a clump of grass, not a disc.
fn vg_tussock(p: vec2f, density: f32, cells: f32, size: vec2f, aa: f32, seed: f32) -> vec3f {
    if density <= 0.0 {
        return vec3f(0.0);
    }
    let cell = floor(p * cells);
    let jitter = vg_hash2(cell + seed);
    let pick = vg_hash(cell + seed * 1.37 + 5.1);
    let local = p - (cell + 0.25 + jitter * 0.5) / cells;
    let distance = length(local);
    let radius = mix(size.x, size.y, fract(pick * 7.31));
    let blades = floor(mix(9.0, 13.0, fract(pick * 3.3)));
    let angle = atan2(local.y, local.x) + pick * 6.2832;
    let reach = radius * mix(0.3, 1.0, pow(abs(cos(angle * blades * 0.5)), 0.45));
    let present = select(0.0, 1.0, fract(pick * 13.7) < density);
    let cover = (1.0 - smoothstep(reach - aa, reach + aa, distance)) * present;
    return vec3f(cover, distance / radius, jitter.x);
}

// The baked ground plan (`village_ground.rs`): three squares side by side, the
// arena, the valley and the arena's tracks, sampled at a world position.
// Coordinates stay half a texel inside each square.
const VG_ARENA_PLAN = 64.0;
const VG_VALLEY_PLAN = 170.0;
const VG_PLAN_WIDTH = 1536.0;
const VG_PLAN_HEIGHT = 512.0;
// The tracks' proximity ramp (`village_tracks::TREAD_REACH`) and a tread's half
// width, in metres.
const VG_TREAD_REACH = 1.0;
const VG_TREAD_HALF = 0.27;
// The wind over the meadow (unit x/z) and its gusts' speed in noise cells per second.
const VG_WIND = vec2f(0.8, 0.6);
const VG_GUST_SPEED = 0.55;
// How far the wind's lines bend across the meadow, in metres of sideways warp.
const VG_WIND_BEND = 7.0;
// How deep a fresh rut reads (m) for the shade its sunward wall casts.
const VG_RUT_DEPTH = 0.1;
// Spacing of the turf's tussocks in metres (`vg_turf`).
const VG_TURF_CELL = 0.32;
// How high the turf stands over the worn earth (m), for the shade its rim casts.
const VG_TURF_LIP = 0.12;
// The mean luminance of the tufts' instance tints (`village_vegetation::TUFT_COLORS`).
const VG_TUFT_LUMA = 0.27;
// Earth seen between thinning blades at the turf's edge.
const VG_ROOT_EARTH = vec3f(0.135, 0.095, 0.06);
// Size of the hedged fields parcelling the valley, in metres.
const VG_ENCLOSURE = 42.0;
// How tall the valley's hedgerows stand (m), for the shade they cast.
const VG_HEDGE_HEIGHT = 1.5;
// Cloud shadows drifting over the valley and the ranges: noise cells per
// metre and their drift in cells per second (one rate for every point).
const VG_CLOUD_SCALE = 0.0055;
const VG_CLOUD_DRIFT = vec2f(0.012, 0.008);

// How much sunlight reaches `p` through the drifting clouds: 1 in the open,
// lower under a cloud's shadow, with soft edges a few tens of metres wide.
fn vg_cloud_light(p: vec2f) -> f32 {
    let q = p * VG_CLOUD_SCALE + VG_CLOUD_DRIFT * frame.camera_position.w;
    let cover = vg_noise(q) * 0.65 + vg_noise(q * 2.3 + 5.0) * 0.35;
    return 1.0 - smoothstep(0.5, 0.7, cover) * 0.3;
}

// Where long grass stands, positive in the stands; `village_vegetation::long_grass`
// is the same field.
fn vg_long_grass(p: vec2f) -> f32 {
    return sin(p.x * 0.083 + p.y * 0.041) * cos(p.y * 0.067 - p.x * 0.029) * 0.65
        + sin(p.x * 0.19 - p.y * 0.16 + 1.3) * 0.35;
}

// The lie of the land: low swells and hollows a few metres across, positive on
// the crests. The floor is flat for the simulation, so it lives only in the
// shading and in the colour of what grows there.
fn vg_swell(p: vec2f) -> f32 {
    return (vg_noise(p * 0.075 + 90.0) - 0.5) * 1.9
        + (vg_noise(vec2f(p.x * 0.17 - p.y * 0.12, p.y * 0.17 + p.x * 0.12) + 4.0) - 0.5) * 0.55;
}

// The meadow's broad colour at `p` (rgb) and how much of its turf has dried
// (w, 0 to 1). The land decides it: deep, dark grass where the water gathers in
// the hollows, fresh yellow-green on the open flats, sun-dried clumps on the
// crests, in drifts warped so neither a direction nor a grid shows. The turf's
// clumps and the tufts take their colour from it, so they read as part of the
// meadow they grow in rather than as confetti.
fn vg_lawn_tone(p: vec2f) -> vec4f {
    let warp = vec2f(vg_noise(p * 0.021 + 1.7), vg_noise(p * 0.021 + 8.3)) - 0.5;
    let q = p + warp * 26.0;
    let broad = vg_noise(q * 0.034);
    let turned = vec2f(q.x * 0.6 - q.y * 0.8, q.x * 0.8 + q.y * 0.6);
    let patches = vg_noise(turned * 0.13 + 5.0);
    let swell = vg_swell(p);
    let lush = clamp(smoothstep(0.36, 0.72, broad * 0.7 + patches * 0.3) * 0.6 + smoothstep(-0.05, -0.8, swell) * 0.45, 0.0, 1.0);
    let fresh = smoothstep(0.45, 0.78, patches) * (1.0 - lush);
    let dry = clamp(smoothstep(0.3, 0.95, swell) * 0.8 + smoothstep(0.62, 0.84, vg_noise(vec2f(q.y, -q.x) * 0.052 + 20.0)) * 0.3, 0.0, 1.0)
        * (1.0 - lush * 0.85);
    var tone = mix(vg_linear(0x5b8838), vg_linear(0x34612f), lush);
    tone = mix(tone, vg_linear(0x84a444), fresh * 0.5);
    // Sun-dried crests go olive and tawny, never lemon.
    tone = mix(tone, vg_linear(0x8e8f52), dry * 0.45);
    tone = mix(tone, tone * vec3f(0.9, 0.99, 1.08), smoothstep(0.5, 0.8, patches) * 0.3 * (1.0 - dry));
    return vec4f(tone * mix(0.86, 1.08, patches), dry);
}

// The turf seen from above: a mosaic of tussocks, each a fan of blades from its
// heart, crowding edge to edge where the turf is full and shrinking and dropping
// out one by one as `fullness` falls (each clump has its own threshold), so
// thinning turf breaks into separate tussocks over the earth rather than along a
// frayed line. `cell` is the clump spacing in metres, `aa` the pixel footprint,
// `sun_flat` the sun's compass direction.
struct VgTurf {
    // Coverage of the nearest clump.
    cover: f32,
    // 0 at the clump's heart, 1 at its blade tips.
    reach: f32,
    // Which side of the clump this is: up to 1 toward the sun, -1 away.
    sun: f32,
    // The clump's own random number.
    pick: f32,
    // 1 in the crease where two clumps meet.
    crease: f32,
    // The clump's shade on the ground just past its blades, away from the sun.
    shadow: f32,
}

fn vg_turf(p: vec2f, cell: f32, fullness: f32, aa: f32, sun_flat: vec2f) -> VgTurf {
    let q = p / cell;
    let base = floor(q);
    var first = 9.0;
    var second = 9.0;
    var nearest = vec2f(0.0);
    var id = vec2f(0.0);
    for (var j = -1; j <= 1; j++) {
        for (var k = -1; k <= 1; k++) {
            let c = base + vec2f(f32(k), f32(j));
            let centre = c + 0.15 + vg_hash2(c + 57.0) * 0.7;
            let d = length(q - centre);
            let nearer = d < first;
            second = select(min(second, d), first, nearer);
            nearest = select(nearest, centre, nearer);
            id = select(id, c, nearer);
            first = min(first, d);
        }
    }
    let pick = vg_hash(id + 91.0);
    let local = q - nearest;
    let towards = dot(local / max(first, 1e-4), sun_flat);
    // Eleven to sixteen blades, some longer than others.
    let blades = floor(mix(11.0, 16.0, fract(pick * 5.3)));
    let angle = atan2(local.y, local.x);
    let blade = floor((angle + 3.1416) * blades / 6.2832);
    let fan = pow(abs(cos(angle * blades * 0.5 + pick * 6.2832)), 2.6) * mix(0.7, 1.0, vg_hash(vec2f(blade, pick * 17.0)));
    let present = select(0.0, 1.0, fract(pick * 7.7) < fullness * fullness * 1.1 - 0.04);
    let radius = mix(0.6, 1.0, fullness) * mix(0.82, 1.14, fract(pick * 3.1));
    let reach_out = radius * mix(0.42, 1.0, fan);
    let pixel = aa / cell;
    let cover = (1.0 - smoothstep(reach_out - pixel, reach_out + pixel, first)) * present;
    let past = smoothstep(reach_out - pixel, reach_out + 0.3, first);
    let shadow = present * (1.0 - cover) * smoothstep(0.1, -0.55, towards) * (1.0 - past);
    let crease = 1.0 - smoothstep(0.0, 0.16, second - first);
    return VgTurf(
        cover,
        clamp(first / max(reach_out, 0.05), 0.0, 1.0),
        towards * clamp(first / max(radius, 0.05), 0.0, 1.0),
        pick,
        crease,
        shadow,
    );
}

// The hedgerows parcelling the valley into fields: coverage of a hedge at `p`,
// on gently bending lines a field apart, broken by gaps where gates and tracks
// pass and fading out toward the ranges. Their width swells and narrows bush by
// bush.
fn vg_enclosure_hedge(p: vec2f, aa: f32) -> f32 {
    let lean = vec2f(p.x * 0.96 - p.y * 0.28, p.x * 0.28 + p.y * 0.96);
    let bend = (vec2f(vg_noise(p * 0.011 + 3.0), vg_noise(p * 0.011 + 9.0)) - 0.5) * 22.0;
    let inner = fract((lean + bend) / VG_ENCLOSURE);
    let to_edge = min(min(inner.x, 1.0 - inner.x), min(inner.y, 1.0 - inner.y)) * VG_ENCLOSURE;
    let gap = smoothstep(0.24, 0.32, vg_noise(p * 0.07 + 13.0));
    let hedge_half = 1.1 + (vg_noise(p * 0.45 + 8.0) - 0.5) * 0.7;
    return (1.0 - smoothstep(hedge_half - aa, hedge_half + aa * 2.0 + 0.15, to_edge)) * gap
        * (1.0 - smoothstep(VG_VALLEY_PLAN - 22.0, VG_VALLEY_PLAN - 8.0, max(abs(p.x), abs(p.y))));
}

// Dried-mud cracks: distance in cells to the nearest border between jittered
// Voronoi cells (F2 - F1 of the two nearest centres, halved), so 0 on a crack.
fn vg_cracks(p: vec2f) -> f32 {
    let cell = floor(p);
    var first = 8.0;
    var second = 8.0;
    for (var j = -1; j <= 1; j++) {
        for (var k = -1; k <= 1; k++) {
            let c = cell + vec2f(f32(k), f32(j));
            let d = length(p - c - 0.15 - vg_hash2(c + 31.0) * 0.7);
            second = select(second, d, d < second);
            let nearer = d < first;
            second = select(second, first, nearer);
            first = select(first, d, nearer);
        }
    }
    return (second - first) * 0.5;
}

fn vg_plan_uv(p: vec2f, half: f32, square: f32) -> vec2f {
    let uv = clamp(p / half * 0.5 + 0.5, vec2f(0.5 / VG_PLAN_HEIGHT), vec2f(1.0 - 0.5 / VG_PLAN_HEIGHT));
    return vec2f((square + uv.x) / 3.0, uv.y);
}

// A crop field's look at `p`: rgb colour and a furrow height in metres. `code`
// is the plan's field byte: crop in the top two bits, row direction in eighths
// of a half turn below.
fn vg_crop(p: vec2f, code: f32, footprint: f32) -> vec4f {
    let crop = floor(code / 64.0);
    let bin = floor((code - crop * 64.0) / 8.0);
    let angle = bin * 0.39269908;
    let across = dot(p, vec2f(-sin(angle), cos(angle)));
    let along = dot(p, vec2f(cos(angle), sin(angle)));
    // Rows fade to their average once they shrink under a few pixels.
    let rows_visible = 1.0 - smoothstep(0.08, 0.22, footprint);
    let sway = vg_fbm(p * 0.07 + crop * 13.0);
    if (crop < 0.5) {
        // Ripe wheat: gold with wind-combed swirls and tractor tramlines.
        let row = sin(across * 9.0) * 0.5 + 0.5;
        var color = mix(vg_linear(0xa88a43), vg_linear(0xcdb46a), smoothstep(0.25, 0.8, sway));
        color = mix(color, vg_linear(0x8f8a46), smoothstep(0.6, 0.9, vg_noise(p * 0.3 + 4.0)) * 0.5);
        // Wind-combed ripples running with the rows.
        let ripple = vg_noise(vec2f(along * 0.12, across * 0.7)) * 0.6 + vg_noise(vec2f(along * 0.4, across * 2.1)) * 0.4;
        color = color * mix(0.86, 1.1, ripple);
        color = color * mix(1.0, mix(0.86, 1.06, row), rows_visible);
        let tram = 1.0 - smoothstep(0.25, 0.45, abs(fract(across / 12.0) - 0.5) * 12.0 - 0.0);
        color = mix(color, vg_linear(0x8f7a3e), tram * 0.55);
        return vec4f(color, row * 0.05 * rows_visible);
    } else if (crop < 1.5) {
        // Young crop in rows over brown soil.
        let row = sin(across * 8.4) * 0.5 + 0.5;
        let leaf = smoothstep(0.35, 0.7, row + (vg_noise(vec2f(across * 8.0, along * 3.0)) - 0.5) * 0.4);
        let soil = vg_linear(0x6e543b);
        let green = mix(vg_linear(0x4f7d2c), vg_linear(0x77a03c), sway);
        let average = mix(soil, green, 0.58);
        return vec4f(mix(average, mix(soil, green, leaf), rows_visible), leaf * 0.08 * rows_visible);
    } else if (crop < 2.5) {
        // Ploughed: dark furrows and sunlit ridges.
        let row = sin(across * 10.5) * 0.5 + 0.5;
        let clod = vg_noise(p * 6.0);
        var color = mix(vg_linear(0x504232), vg_linear(0x88724f), row * 0.7 + clod * 0.3);
        color = mix(mix(vg_linear(0x5b4a37), vg_linear(0x78644a), 0.5), color, rows_visible);
        return vec4f(color * mix(0.92, 1.05, sway), (row * 0.09 + clod * 0.02) * rows_visible);
    }
    // Mown hay: pale stubble with raked windrows.
    let windrow = 1.0 - smoothstep(0.35, 0.8, abs(fract(across / 4.5) - 0.5) * 4.5);
    var color = mix(vg_linear(0x9da35b), vg_linear(0xc2bb72), sway);
    color = mix(color, vg_linear(0xb79a4e), windrow * 0.8);
    let stubble = sin(across * 14.0) * 0.5 + 0.5;
    color = color * mix(1.0, mix(0.94, 1.04, stubble), rows_visible);
    return vec4f(color, windrow * 0.12);
}

// The ground is most of every frame's pixels, so each feature runs only where it
// shows: a skipped feature's weight is exactly zero there, so the pixel comes out
// the same (and derivatives of the relief stay continuous). Texture samples and
// derivatives stay ahead of the branches, in uniform control flow.
fn village_ground(s: ptr<function, Surface>, f: EffectFragment) {
    let p = f.world.xz;
    let extent = material.params[0].y;
    let widths = material.params[3];
    // Pixel footprint in metres: sub-pixel detail fades instead of shimmering.
    // Mostly the pixel's area rather than its longer side, so the ground at a
    // low, grazing view keeps its pebbles and grain instead of smearing.
    let footprint_x = length(dpdx(f.world));
    let footprint_y = length(dpdy(f.world));
    let footprint = sqrt(footprint_x * footprint_y) * 0.7 + max(footprint_x, footprint_y) * 0.3;
    let aa = footprint * 0.75;
    let fine = 1.0 - smoothstep(0.02, 0.06, footprint);
    // Detail a few centimetres across still reads from the gameplay camera.
    let mid = 1.0 - smoothstep(0.07, 0.16, footprint);
    // Detail a few decimetres across: the clumps of the turf's edge.
    let coarse = 1.0 - smoothstep(0.1, 0.24, footprint);
    let inside = 1.0 - smoothstep(extent + 2.0, extent + 3.0, max(abs(p.x), abs(p.y)));
    let sun_flat = normalize(frame.sun_direction.xz + vec2f(1e-4, 0.0));
    let sun_run = min(length(frame.sun_direction.xz) / max(frame.sun_direction.y, 0.2), 2.6);

    // ---- The baked plan: inside the wall the arena and its tracks, outside the
    // valley.
    let arena_plan = textureSample(extra_texture1, extra_sampler1, vg_plan_uv(p, VG_ARENA_PLAN, 0.0));
    let valley_plan = textureSample(extra_texture1, extra_sampler1, vg_plan_uv(p, VG_VALLEY_PLAN, 1.0));
    let tracks = textureSample(extra_texture1, extra_sampler1, vg_plan_uv(p, VG_ARENA_PLAN, 2.0));
    // Two neighbours a texel away give the tread's direction for its cleats.
    let tracks_x = textureSample(extra_texture1, extra_sampler1, vg_plan_uv(p + vec2f(0.25, 0.0), VG_ARENA_PLAN, 2.0)).r;
    let tracks_z = textureSample(extra_texture1, extra_sampler1, vg_plan_uv(p + vec2f(0.0, 0.25), VG_ARENA_PLAN, 2.0)).r;
    let valley_texel = vec2i(clamp(vg_plan_uv(p, VG_VALLEY_PLAN, 1.0) * vec2f(VG_PLAN_WIDTH, VG_PLAN_HEIGHT), vec2f(512.0, 0.0), vec2f(1023.0, 511.0)));
    let field_code = textureLoad(extra_texture1, valley_texel, 0).b * 255.0;
    let worn = mix(valley_plan.r, arena_plan.r, inside);
    let trampled = arena_plan.g * inside;
    let garden = arena_plan.b * inside;
    // The contact shade stays at the foot of what stands on the floor.
    let open = mix(1.0, arena_plan.a, inside);
    let field = valley_plan.g * (1.0 - inside);
    // The fields blurred over a few metres by a coarse mip: hedgerows grow
    // just outside their edges.
    // An explicit level needs no derivatives, so only the valley fetches it.
    var field_near = 0.0;
    // The same where the sun's ray over this point meets a hedge's top.
    var field_near_sun = 0.0;
    var field_sun = 0.0;
    if inside < 1.0 {
        field_near = textureSampleLevel(extra_texture1, extra_sampler1, vg_plan_uv(p, VG_VALLEY_PLAN, 1.0), 2.0).g * (1.0 - inside);
        let toward = p + sun_flat * sun_run * VG_HEDGE_HEIGHT;
        field_near_sun = textureSampleLevel(extra_texture1, extra_sampler1, vg_plan_uv(toward, VG_VALLEY_PLAN, 1.0), 2.0).g * (1.0 - inside);
        field_sun = textureSampleLevel(extra_texture1, extra_sampler1, vg_plan_uv(toward, VG_VALLEY_PLAN, 1.0), 0.0).g * (1.0 - inside);
    }
    let wet_bank = (1.0 - valley_plan.a) * (1.0 - inside);
    // The plan where the sun's ray over this point meets the turf's rim.
    let worn_sun = mix(worn, textureSample(extra_texture1, extra_sampler1, vg_plan_uv(p + sun_flat * sun_run * VG_TURF_LIP, VG_ARENA_PLAN, 0.0)).r, inside);
    // The tracks where the sun's ray over a rut's floor crosses its rim.
    let tracks_sun = textureSample(extra_texture1, extra_sampler1, vg_plan_uv(p + sun_flat * sun_run * VG_RUT_DEPTH, VG_ARENA_PLAN, 2.0));

    // ---- Treads (`village_tracks.rs`): metres to the nearest tread's centre,
    // how fresh it is, the traffic that packed the ground and standing water.
    // The walls crumble and the line wanders a few centimetres, so no tread
    // runs as a ruled curve.
    let tread_aa = max(fwidth((1.0 - tracks.r) * VG_TREAD_REACH), aa);
    var tread_distance = (1.0 - tracks.r) * VG_TREAD_REACH;
    // Only near a tread (none reaches past 0.61 m), out of uniform control.
    var tread_break = 1.0;
    if tracks.r > 0.3 {
        let crumble = vg_noise(p * 2.7 + 5.0);
        tread_distance += (crumble - 0.5) * 0.08;
        tread_break = smoothstep(0.32, 0.6, vg_noise(p * 0.42 + 19.0) * 0.8 + crumble * 0.2 + tracks.g * 0.5);
    }
    let fresh = tracks.g * inside;
    let traffic = tracks.b * inside;
    let standing = clamp((1.0 - tracks.a) / 0.6, 0.0, 1.0) * inside;
    // Old treads survive in broken stretches, rained and driven out between;
    // fresh ones run unbroken.
    let tread_bed = (1.0 - smoothstep(VG_TREAD_HALF - tread_aa, VG_TREAD_HALF + tread_aa, tread_distance)) * tread_break;
    let tread_berm = smoothstep(VG_TREAD_HALF - 0.02, VG_TREAD_HALF + 0.08, tread_distance)
        * (1.0 - smoothstep(VG_TREAD_HALF + 0.1, VG_TREAD_HALF + 0.34, tread_distance)) * tread_break;
    // Old treads blur into the packed dirt; fresh ones keep sharp walls.
    let rut = tread_bed * smoothstep(0.05, 0.6, fresh);
    // The same rut where the sun's ray over this point enters it: the rut's
    // sunward wall shades its floor.
    let tread_sun = 1.0 - smoothstep(VG_TREAD_HALF - tread_aa, VG_TREAD_HALF + tread_aa, (1.0 - tracks_sun.r) * VG_TREAD_REACH);
    let rut_sun = tread_sun * smoothstep(0.05, 0.6, tracks_sun.g * inside);
    let berm = tread_berm * smoothstep(0.35, 0.9, fresh);
    // Cleat prints across a fresh tread: bars square to its run, even about
    // its centreline so the two halves meet without a seam.
    let tread_across = normalize(vec2f(tracks_x - tracks.r, tracks_z - tracks.r) + vec2f(1e-5, 0.0));
    let tread_along = dot(p, vec2f(-tread_across.y, tread_across.x));
    let cleat_fade = (1.0 - smoothstep(0.025, 0.07, footprint)) * smoothstep(0.2, 0.6, fresh);
    let cleat = smoothstep(0.1, 0.6, cos(tread_along * 28.56)) * rut * cleat_fade;

    // ---- Textures: two scales of each tile, turned against each other, and a
    // close one of the dirt that only shows near the camera.
    let turned = vec2f(p.x * 0.6 + p.y * 0.8, p.y * 0.6 - p.x * 0.8);
    let grass_near = textureSample(map_texture, map_sampler, f.uv);
    let grass_far = textureSample(map_texture, map_sampler, turned / 21.0);
    let dirt_near = textureSample(extra_texture0, extra_sampler0, p / 3.1);
    let dirt_far = textureSample(extra_texture0, extra_sampler0, turned / 13.0);
    let dirt_close = textureSample(extra_texture0, extra_sampler0, vec2f(p.y, -p.x) / 0.83 + 0.37);
    let grass_luma = dot(grass_near.rgb, VG_LUMA);
    // The tile's blade speckle flattens with distance, so it never sparkles.
    let grass_detail = mix(1.0, grass_luma / 0.3 * mix(1.0, dot(grass_far.rgb, VG_LUMA) / 0.3, 0.45), mix(0.55, 1.0, mid));
    let dirt_luma = dot(dirt_near.rgb, VG_LUMA);
    let dirt_detail = dirt_luma / 0.36 * mix(1.0, dot(dirt_far.rgb, VG_LUMA) / 0.36, 0.5);
    let close_detail = mix(1.0, dot(dirt_close.rgb, VG_LUMA) / 0.36, fine);

    // ---- Where the grass wears through. The plan ramps the dirt over metres
    // (`village_ground::EDGE_RAMP`, a plan unit is 4.8 m); the edge wanders off
    // it at several scales, so no stretch runs as one even line, and spreads
    // over a hand's width in some stretches and over two or three metres of
    // thinning clumps in others, always where the grass is trampled.
    // Positive on the lawn's side of the edge, in plan units. Away from an
    // edge the plan is flat (0 on a lawn, 1 on a road) and the wander could
    // not reach the edge, so the noise runs only where the plan ramps or the
    // grass is trampled; elsewhere the values are the edge's own averages.
    var lawn_side = 0.5 - worn;
    var spread = 0.035;
    var density = select(0.0, 1.0, worn < 0.5);
    var clump_field = 0.5;
    var root_damp = 0.0;
    var straw = 0.0;
    var bald = 1.0;
    if (worn > 0.004 && worn < 0.996) || trampled > 0.4 {
        let edge_warp = vec2f(vg_noise(p * 0.13 + 3.0), vg_noise(p * 0.13 + 17.0)) - 0.5;
        // Bites a metre or two across keep the edge crisp and ragged even
        // from the overview, where the clumps below are too small to show.
        let wander = (vg_noise(p * 0.19 + edge_warp * 1.6) - 0.5) * 0.34 + (vg_noise(p * 0.62 + 31.0) - 0.5) * 0.17
            + (vg_noise(p * 1.45 + 9.0) - 0.5) * 0.1;
        // Every edge breaks into clumps and bites a little; long stretches
        // and the trampled ground crumble over a metre or more.
        let ragged = max(max(smoothstep(0.35, 0.75, vg_noise(p * 0.045 + 5.0)), 0.3), smoothstep(0.3, 0.8, trampled));
        lawn_side = 0.5 - worn - wander * mix(0.6, 1.0, ragged);
        spread = mix(0.035, 0.24, ragged);
        density = smoothstep(-spread, spread, lawn_side);
        // Trampled ground goes bald in scuffed patches; the noise lifts it at
        // most 0.35, so ground trampled less than 0.4 never does.
        if trampled > 0.4 {
            bald = 1.0 - smoothstep(0.75, 1.05, trampled + (vg_fbm(p * 0.5 + 9.0) - 0.5) * 0.7) * 0.6;
            density = density * bald;
        }
        // Tufts hold out over the dirt and the dirt shows first between them:
        // a clump-sized field, with the grass tile's own blades close up,
        // against the density, so the edge breaks into clumps and bald holes
        // rather than a band. Each scale fades to its average before it could
        // shimmer, and it never reaches 0 or 1, so no speck of turf stands in
        // the middle of a road.
        clump_field = clamp(0.5 + (vg_noise(p * 1.7 + 7.0) - 0.5) * 0.56 * coarse
            + (vg_noise(p * 4.3 + 2.0) - 0.5) * 0.3 * mid
            + clamp(1.0 - grass_detail, -0.5, 0.5) * 0.22 * fine, 0.03, 0.97);
        // The earth along the edge stays dark and damp where the roots hold
        // the water; the turf's tips dry to straw in other stretches.
        // A darker, trodden band runs all along the earth's side of the edge,
        // wider and wetter in some stretches.
        root_damp = smoothstep(-0.3, 0.0, lawn_side) * mix(0.5, 1.0, smoothstep(0.3, 0.7, vg_noise(p * 0.08 + 61.0)));
        straw = smoothstep(0.45, 0.8, vg_noise(p * 0.06 + 50.0));
    }
    let cover_drive = density - clump_field;
    let cover_aa = max(fwidth(cover_drive), 0.025);
    // The same edge where the sun's ray over this point meets the turf's
    // rim: the wander barely changes over the lip's run, so only the plan
    // moves. The turf stands a hand high and shades the earth below its rim.
    let lip_drive = smoothstep(-spread, spread, lawn_side + worn - worn_sun) * bald - clump_field;
    // Close up the turf is tussocks (`vg_turf`): its edge breaks into separate
    // clumps over the earth, each lit on its sunward side with its own shade
    // beside it; far off the clump field above stands in for them.
    let turf_view = 1.0 - smoothstep(0.06, 0.14, footprint);
    var turf = VgTurf(0.0, 1.0, 0.0, 0.5, 0.0, 0.0);
    if turf_view > 0.0 && density > 0.0 {
        turf = vg_turf(p, VG_TURF_CELL, density, aa, sun_flat);
    }
    let turf_full = smoothstep(0.82, 0.97, density);
    let far_cover = smoothstep(-cover_aa, cover_aa, cover_drive);
    let turf_cover = mix(far_cover, max(turf.cover, turf_full), turf_view);
    // Mud rings every puddle, in the lawns too: the water lies in a soaked
    // hollow, not on the turf.
    var mud_rim = 0.0;
    if standing > 0.0 {
        mud_rim = smoothstep(0.02, 0.24, standing + (vg_noise(p * 1.9 + 8.0) - 0.5) * 0.1);
    }
    // Treads cut the turf wherever they cross it.
    let dirt = max(max(1.0 - turf_cover, rut), mud_rim);
    // The dirt just outside each clump lies in its shade: the tussock's own
    // close up, the turf's rim toward the sun far off.
    let rim_shade = smoothstep(-cover_aa, cover_aa, lip_drive) * (1.0 - far_cover);
    let clump_shade = max(turf.shadow * turf_view, rim_shade * (1.0 - turf_view * 0.6)) * dirt * (1.0 - rut);
    // Thinning turf near its edge: the earth shows between the blades in some
    // stretches, the tips dry to straw in others.
    let thin = (1.0 - density) * (1.0 - dirt);
    // Clumps of turf torn loose or spilled onto the dirt, thinning out over a
    // metre or two of the road's shoulder.
    let spill = smoothstep(-spread - 0.14, -spread * 0.5, lawn_side) * (1.0 - smoothstep(0.3, 0.8, traffic)) * dirt * (1.0 - rut);
    let tussocks = vg_tussock(p, spill * 0.4 * fine, 2.6, vec2f(0.05, 0.12), aa, 47.0);
    let wear = smoothstep(0.0, 0.9, max(worn, trampled * 0.85));
    // A gravel drip line along the foot of the perimeter wall (none past 1.35 m,
    // where even the noise's 0.25 m reach leaves it at full distance).
    let wall_gap = extent - max(abs(p.x), abs(p.y));
    var verge = 0.0;
    if wall_gap >= -0.5 && wall_gap < 1.35 {
        verge = 1.0 - smoothstep(0.35, 1.1, wall_gap + (vg_noise(p * 1.7) - 0.5) * 0.5);
    }

    let packed = smoothstep(0.2, 0.85, traffic);
    // A berm of loose earth along each road's shoulder, a metre or two inside
    // the turf, where the lanes' traffic pushed it.
    var road_berm = 0.0;
    if worn > 0.42 && worn < 0.9 && inside > 0.0 {
        road_berm = smoothstep(0.42, 0.6, worn) * (1.0 - smoothstep(0.68, 0.9, worn)) * inside * (1.0 - packed * 0.7)
            * smoothstep(0.25, 0.55, vg_noise(p * 0.3 + 61.0));
    }
    // Water stands in the deepest ruts and the junctions' potholes.
    let puddle = smoothstep(0.35, 0.65, standing);
    let wet = max(max(standing * 0.85, rut * fresh * 0.35), wet_bank * dirt);

    // ---- The lie of the land. The floor is flat for the simulation, so the
    // swells are light and colour only: low mounds and hollows a few metres
    // across, drier on the crests and lusher where the water collects.
    let swell = vg_swell(p);
    let bank = (1.0 - smoothstep(0.08, 0.95, worn)) * 0.3;
    // The lawns swell gently toward their middles: the plan blurred over a few
    // metres by a coarse mip.
    let lawn_swell = (1.0 - textureSampleLevel(extra_texture1, extra_sampler1, vg_plan_uv(p, VG_ARENA_PLAN, 0.0), 4.0).r) * inside;
    // Long grass grows in broad stands the mowers skip (the tall tufts of
    // `village_vegetation::long_grass` follow the same field) and never where
    // the ground is worn or walked.
    let stand_field = vg_long_grass(p) + (vg_noise(p * 0.31 + 23.0) - 0.5) * 0.5;
    let long_grass = smoothstep(0.08, 0.4, stand_field) * (1.0 - smoothstep(0.05, 0.45, wear)) * (1.0 - field);
    // Gusts roll over the long grass: soft bright swathes where the blades bow
    // and show their pale sides, downwind at one speed (the phase varies with
    // place, never the rate). The wind's lines bend over the meadow, so
    // neither the gusts nor the combed grass run one way across a whole lawn.
    let wind_bend = (vg_noise(p * 0.035 + 70.0) - 0.5) * VG_WIND_BEND + (vg_noise(p * 0.09 + 12.0) - 0.5) * VG_WIND_BEND * 0.3;
    let wind_along = dot(p, VG_WIND);
    let wind_across = dot(p, vec2f(-VG_WIND.y, VG_WIND.x)) + wind_bend;
    let gust = vg_noise(vec2f(wind_along * 0.085 - frame.camera_position.w * VG_GUST_SPEED, wind_across * 0.06))
        * 0.7 + vg_noise(vec2f(wind_along * 0.21 - frame.camera_position.w * VG_GUST_SPEED * 2.45, wind_across * 0.15 + 7.0)) * 0.3;
    // Close up the turf heaves into soft tussock lumps, its blades combed along
    // the bending wind and across it, lit where they lie toward the light;
    // both blend to their average once they shrink under a few pixels.
    let clump_view = 1.0 - smoothstep(0.05, 0.13, footprint);
    var comb = 0.5;
    if clump_view * (1.0 - dirt) > 0.0 {
        let wave = vg_noise(p * 0.45 + 13.0) * 2.4;
        comb = vg_noise(vec2f(wind_along * 2.2 + wave, wind_across * 5.0)) * 0.55
            + vg_noise(vec2f(wind_across * 2.6 - wave, wind_along * 6.0 + 3.0)) * 0.45;
    }
    // 0 in the hollows between tussocks, 1 over their crowns.
    let tussock_crown = mix(0.5, turf.cover * (1.0 - turf.reach * turf.reach), turf_view);
    let clump_hue = turf.pick;
    // Kept low: fine, steep relief darkens the turf on average, and the long
    // grass would read as a stain.
    let clump_height = (tussock_crown * 0.025 + (comb - 0.5) * 0.01) * clump_view * (1.0 - dirt);

    // ---- Grass: a living meadow, not a lawn. Broad lush and sun-dried
    // patches, darker long stands, per-tussock hue, straw on the crests and the
    // walked lines, clover and wildflower drifts. Neither grass nor its
    // wildflowers show on bare dirt.
    var grass = vec3f(0.0);
    var enclosure_hedge = 0.0;
    var hedge_sunward = 0.0;
    var hedge_shade = 0.0;
    var blooms = vec4f(0.0);
    var bloom_color = vec3f(0.0);
    if dirt < 1.0 || tussocks.x > 0.0 {
        // Two octaves: a third under the grass's own grain never showed.
        let meso = vg_noise(p * 0.15 - 11.0) * 0.62 + vg_noise(p * 0.31 + 6.0) * 0.38;
        // The lie of the land sets the meadow's colour (`vg_lawn_tone`): dark
        // lush hollows, fresh flats, olive crests.
        let lawn = vg_lawn_tone(p);
        let tone = lawn.rgb;
        grass = mix(tone, tone * vec3f(0.8, 0.92, 0.86), long_grass * 0.75);
        // Walked grass: flattened blades, paler and greyer, in scuffs a
        // stride across where the traffic spills onto the turf. It keeps the
        // turf's value and turns khaki rather than yellow, so it never glows
        // as a lime halo, and its scuffs are fine enough to read as texture,
        // not as stains.
        let walk_drive = max(trampled * 0.9, smoothstep(0.15, 0.7, worn) * 0.35);
        var walked = 0.0;
        if walk_drive > 0.0 {
            let scuff = vg_noise(p * 1.1 + 31.0) * 0.6 + mix(0.2, clamp(grass_detail - 0.5, 0.0, 1.0) * 0.4, mid);
            walked = smoothstep(0.35, 0.7, walk_drive + (scuff - 0.5) * 0.8);
            let flat_luma = dot(grass, VG_LUMA);
            let khaki = vec3f(flat_luma) * vec3f(1.12, 1.02, 0.7) * mix(0.95, 1.1, scuff);
            grass = mix(grass, khaki, walked * 0.5);
        }
        let blades = mix(0.84, 1.16, clamp((grass_detail - 0.55) * 0.9, 0.0, 1.0));
        grass = grass * blades;
        // Clumps a couple of metres across, a shade lighter or darker, so the
        // turf keeps a grain even from the overview.
        grass = grass * mix(1.0, mix(0.9, 1.08, vg_noise(p * 0.55 + 77.0)), mix(0.6, 1.0, coarse));
        // Tussocks: drifts of yellower and bluer clumps, their crowns lighter
        // than the hollows between; blades combed with and across the wind,
        // the long grass most, bright where they lie toward the light.
        // Close up every tussock is its own: yellower or bluer, its sunward
        // blades lit and its far side in its own shade, the creases between
        // tussocks dark where the turf is full. Rough stretches show their
        // clumps most, grazed ones least.
        let rough_turf = mix(0.25, 1.0, smoothstep(0.35, 0.7, vg_noise(p * 0.11 + 37.0)));
        let tussock_hue = mix(vec3f(1.06, 1.03, 0.88), vec3f(0.92, 0.99, 0.98), smoothstep(0.3, 0.7, clump_hue));
        let tussock_lit = mix(0.93, 1.07, smoothstep(-0.55, 0.65, turf.sun));
        // Over the earth each tussock is dark at its heart, where the blades
        // crowd and shade each other, and bright at its sunlit tips.
        let tuft_ramp = mix(0.84, 1.12, smoothstep(0.1, 0.95, turf.reach));
        let tussock_tone = tussock_hue * tussock_lit * mix(tuft_ramp, mix(0.95, 1.04, tussock_crown), turf_full);
        grass = grass * mix(vec3f(1.0), tussock_tone, turf_view * turf.cover * mix(1.0, rough_turf, turf_full));
        // Dry tussocks bleach at their tips on the crests.
        grass = mix(grass, vec3f(dot(grass, VG_LUMA)) * vec3f(1.2, 1.08, 0.72), lawn.w * turf.reach * turf.cover * turf_view * 0.35);
        // The combing comes and goes in drifts, so the strokes never print
        // one even texture across the lawn.
        let stroke = mix(0.94, 1.06, smoothstep(0.2, 0.8, comb));
        let combed = mix(0.3, 1.0, long_grass) * smoothstep(0.2, 0.7, meso + long_grass * 0.4);
        grass = grass * mix(1.0, stroke, combed * clump_view);
        // Bowed blades in a gust.
        let sheen = smoothstep(0.55, 0.9, gust) * mix(0.3, 1.0, long_grass);
        grass = mix(grass, grass * vec3f(1.14, 1.14, 1.02) + vg_linear(0x2a2a12) * 0.15, sheen * 0.22);
        // From any height the turf keeps a grain of clumps a stride or two
        // across: denser, darker tussocks and sparser, sunnier grazed spots.
        let tussock_field = vg_noise(p * 0.62 + 17.0 + (grass_detail - 0.5) * 0.6);
        grass = mix(grass, grass * vec3f(0.84, 0.9, 0.82), smoothstep(0.58, 0.78, tussock_field) * 0.55 * (1.0 - walked));
        grass = mix(grass, grass * vec3f(1.08, 1.06, 0.92), smoothstep(0.4, 0.22, tussock_field) * 0.4 * (1.0 - long_grass));
        // Clover and dark clumps break the carpet.
        let clover = smoothstep(0.66, 0.82, vg_noise(p * 1.3 + 2.0)) * (1.0 - wear) * (1.0 - long_grass);
        grass = mix(grass, vg_linear(0x3d6f2e) * blades, clover * 0.1);
        // The turf's last metre: the earth between thinning blades in some
        // stretches, straw-dry tips in others, never one band all round.
        // Close up the tussocks themselves show the thinning, so the earth
        // tint stays faint there.
        let thinning = thin * (1.0 - turf_view * 0.75);
        grass = mix(grass, mix(grass, VG_ROOT_EARTH, 0.45), thinning * (1.0 - straw * 0.5) * 0.55);
        grass = mix(grass, vec3f(dot(grass, VG_LUMA)) * vec3f(1.15, 1.0, 0.68), thin * straw * 0.3);
        // At the foot of what stands on the turf (cottages, walls, timber,
        // trunks) the grass thins, darkens and browns over litter and bare
        // earth, so nothing looks set down on a mown lawn.
        let foot = smoothstep(0.0, 0.45, 1.0 - open) * inside;
        grass = mix(grass, mix(grass * vec3f(0.8, 0.74, 0.6), VG_ROOT_EARTH * 1.7, 0.3), foot * 0.75);
        // Out in the valley the pasture is grazed short in some stretches
        // and left rough in others, dark tussocks dotting the rough and
        // bracken browning the crests, so the hills read as land, not felt.
        let valley = 1.0 - inside;
        if valley > 0.0 {
            let rough = smoothstep(0.42, 0.68, vg_noise(p * 0.045 + 13.0) * 0.8 + meso * 0.2);
            let dots_view = 1.0 - smoothstep(0.12, 0.35, footprint);
            let dots = vg_scatter(p, rough * 0.85 * dots_view, 0.8, vec2f(0.3, 0.6), aa, 61.0);
            grass = mix(grass, grass * vec3f(0.84, 0.88, 0.8), rough * 0.55 * valley);
            grass = mix(grass, grass * vec3f(0.62, 0.7, 0.58), (dots.x * 0.65 + rough * 0.12 * (1.0 - dots_view)) * valley);
            let bracken = smoothstep(0.6, 0.78, vg_noise(p * 0.026 + 70.0) + (meso - 0.5) * 0.25) * valley;
            grass = mix(grass, vg_linear(0x7d6a3e) * mix(0.8, 1.1, meso), bracken * 0.4);
            // Enclosures: the valley's pasture is parcelled into fields a few
            // dozen metres across by hedgerows on gently bending lines, each
            // field grazed, mown or left long in its own tone, so the valley
            // reads as farmed land round the village rather than open felt.
            let lean = vec2f(p.x * 0.96 - p.y * 0.28, p.x * 0.28 + p.y * 0.96);
            let bend = (vec2f(vg_noise(p * 0.011 + 3.0), vg_noise(p * 0.011 + 9.0)) - 0.5) * 22.0;
            let q = (lean + bend) / VG_ENCLOSURE;
            let parcel = floor(q);
            let inner = fract(q);
            let kind = vg_hash(parcel + 41.0);
            let parcel_tone = select(select(vec3f(1.0), vec3f(1.09, 1.06, 0.84), kind > 0.55), vec3f(0.86, 0.92, 0.84), kind > 0.82);
            grass = grass * mix(vec3f(1.0), parcel_tone, valley);
            // Mown fields keep the mower's broad stripes.
            let mown = select(0.0, 1.0, kind > 0.55 && kind <= 0.82) * (1.0 - smoothstep(0.15, 0.4, footprint));
            grass = grass * mix(1.0, mix(0.95, 1.05, step(0.5, fract(inner.x * 7.0))), mown * valley);
            // The hedges stand a man's height: lit along their sunward
            // side, shaded beyond, and casting their shade across the grass
            // away from the sun, so they read as rows of bushes, not lines.
            enclosure_hedge = vg_enclosure_hedge(p, aa) * valley;
            hedge_sunward = 1.0 - vg_enclosure_hedge(p + sun_flat * 0.8, aa);
            hedge_shade = vg_enclosure_hedge(p + sun_flat * sun_run * VG_HEDGE_HEIGHT, aa * 2.0) * valley * (1.0 - enclosure_hedge);
        }
        // Flower drifts tint the lawn even where single blooms are too small to draw.
        let flowers = vg_noise(p * 0.08 + 61.0) * 0.65 + vg_noise(p * 0.17 + 3.0) * 0.35;
        let drift = smoothstep(0.5, 0.78, flowers) * (1.0 - wear) * (1.0 - fine);
        if drift > 0.0 {
            let drift_tint = mix(vg_linear(0xe9e6c8), vg_linear(0xe8c64a), smoothstep(0.4, 0.6, vg_noise(p * 0.05 + 7.0)));
            grass = mix(grass, drift_tint, drift * 0.12);
        }
        // Keep the bank tint and the far hills' vertex shading.
        grass = grass * f.vertex_color.rgb;

        // ---- Wildflowers in meadow drifts: daisies with gold hearts, buttercups,
        // violets and campion.
        let meadow = smoothstep(0.45, 0.72, flowers) * (1.0 - smoothstep(0.1, 0.4, wear));
        blooms = vg_scatter(p, (0.03 + meadow * 0.22) * fine, 1.9, vec2f(0.035, 0.06), aa, 27.0);
        let bloom_pick = blooms.z;
        var petals = vg_linear(0xf3f0e4);
        petals = select(petals, vg_linear(0xf2c94a), bloom_pick > 0.55);
        petals = select(petals, vg_linear(0xb08fe0), bloom_pick > 0.78);
        petals = select(petals, vg_linear(0xec9fb8), bloom_pick > 0.92);
        let heart = smoothstep(0.55, 0.3, blooms.y / 0.05) * select(0.0, 1.0, bloom_pick <= 0.55);
        bloom_color = mix(petals, vg_linear(0xe7b52d), heart);
    }

    // ---- Pebbles: thick along the road shoulders and the verge, kicked out of
    // the ruts onto the loose ground, a scatter everywhere else.
    let shoulder = smoothstep(0.15, 0.45, wear) * (1.0 - smoothstep(0.7, 0.95, wear));
    // Stones gather: shed in drifts along the shoulders and the ruts' berms,
    // washed out in patches, with bare earth between, never an even sprinkle.
    var stone_cluster = 0.0;
    if dirt > 0.0 || verge > 0.0 || wet_bank > 0.0 {
        stone_cluster = smoothstep(0.45, 0.8, vg_noise(p * 0.3 + 40.0) * 0.65 + vg_noise(p * 1.1 + 3.0) * 0.35 + berm * 0.25);
    }
    let pebble_density = ((0.1 + shoulder * 0.25 + (1.0 - packed) * 0.08) * dirt + verge * 0.6 + wet_bank * 0.4) * mix(0.15, 2.2, stone_cluster) * fine * (1.0 - rut);
    let pebbles = vg_scatter(p, pebble_density, 2.9, vec2f(0.025, 0.075), aa, 3.0);
    // Each pebble's shade on the earth, away from the sun.
    let pebble_shadow = vg_scatter(p + sun_flat * 0.04 * min(sun_run, 1.6), pebble_density, 2.9, vec2f(0.025, 0.075), aa * 1.6, 3.0).x * (1.0 - pebbles.x);
    let grit = vg_scatter(p, dirt * 0.62 * fine, 6.5, vec2f(0.014, 0.032), aa, 9.0);
    // Gravel: drifts of small stones in the clusters, each with its shade.
    let gravel_density = dirt * fine * smoothstep(0.15, 0.7, stone_cluster) * 0.8 * (1.0 - rut);
    let gravel = vg_scatter(p, gravel_density, 7.5, vec2f(0.014, 0.034), aa, 13.0);
    let gravel_shadow = vg_scatter(p + sun_flat * 0.022 * min(sun_run, 1.6), gravel_density, 7.5, vec2f(0.014, 0.034), aa * 1.5, 13.0).x * (1.0 - gravel.x);
    // Pale, dry crumbs of earth catching the light, only close up.
    let crumbs = vg_scatter(p, dirt * 0.55 * fine * (1.0 - smoothstep(0.004, 0.02, footprint)) * (1.0 - rut), 11.0, vec2f(0.012, 0.024), aa, 19.0);

    // Damp hollows in the loose ground: darker, cooler earth with a crisp
    // drying rim and a sheen, so they read as wet rather than as smudges. Its
    // derivative stays out of the branch below.
    // One field of how the water runs: it soaks the hollows and washes the
    // topsoil off the crests down to gravel.
    let runoff = vg_noise(p * 0.12 + 33.0);
    let damp_field = runoff * 0.6 - swell * 0.45 + (1.0 - packed) * 0.08 + standing * 0.3;
    let damp_aa = max(fwidth(damp_field), 0.004);
    let damp = smoothstep(0.58 - damp_aa, 0.58 + damp_aa, damp_field) * dirt;

    // ---- Dirt: loose, darker earth where little drove, packed pale and combed
    // along the busy lines, dark damp ruts with crumbled berms. Pebbles and the
    // spilled tussocks take their warmth from it, so it is shaded wherever any
    // shows.
    var soil = vec3f(0.0);
    var clods = 0.0;
    var churn = 0.0;
    if dirt > 0.0 || pebbles.x > 0.0 {
        // Which way the road runs, for streaks combed along it.
        let along = vg_strip(p.x, p.y, material.params[1].xyz, widths.x, widths.y, extent);
        let across = vg_strip(p.y, p.x, material.params[2].xyz, widths.z, widths.w, extent);
        let lane = select(vec2f(across.y, p.x), vec2f(along.y, p.y), along.x > across.x);
        let streaks = vg_noise(vec2f(lane.x * 6.0, lane.y * 0.09)) * 0.6 + vg_noise(vec2f(lane.x * 17.0, lane.y * 0.25)) * 0.4;
        let dirt_tex = mix(vec3f(dirt_luma), dirt_near.rgb, 0.45);
        // Loose, darker and redder where little drives; packed pale and dusty
        // along the busy lines.
        let loose = mix(vg_linear(0x6a5442), vg_linear(0x846b53), smoothstep(0.25, 0.8, vg_fbm(p * 0.07 + 21.0)));
        let hard = mix(vg_linear(0x9c8c72), vg_linear(0xb0a185), streaks);
        soil = mix(loose, hard, smoothstep(0.0, 1.0, packed) * 0.9);
        // Broad patches tens of metres across: redder loam, greyer dust and
        // sandier ground, so the roads are not one colour.
        let region = vg_noise(p * 0.035 + 7.0);
        soil = mix(soil, soil * vec3f(1.1, 0.95, 0.82), smoothstep(0.58, 0.85, region) * 0.75);
        soil = mix(soil, soil * vec3f(0.92, 0.95, 1.0), smoothstep(0.42, 0.15, region) * 0.8);
        // The shoulders: loose earth the treads pushed off the lanes, heaped
        // in a low berm along the turf, darker and lumpier than the lanes.
        soil = mix(soil, soil * vec3f(0.88, 0.84, 0.8), road_berm * 0.55);
        // Earth mottled at a stride and at hand scale: drier crumbs and darker
        // loam, and close up the tile's own crumbs and fine scratches dragged
        // along the lane.
        let lump = vg_noise(p * 3.7 + 19.0);
        let churn_light = smoothstep(0.25, 0.75, lump + (vg_noise(p * 3.7 + 19.0 + sun_flat * 0.06) - lump) * -4.0);
        let loam = vg_noise(p * 0.85 + 41.0);
        let stride = vg_noise(vec2f(p.x * 0.6 + p.y * 0.8, p.y * 0.6 - p.x * 0.8) * 0.33 + 3.0);
        soil = soil * mix(0.9, 1.07, smoothstep(0.25, 0.75, loam)) * mix(0.93, 1.06, stride) * mix(1.0, mix(0.92, 1.06, vg_noise(p * 3.1 + 5.0)), mid);
        // The tile's grain at full strength: earlier it was averaged nearly
        // flat and the roads read as a smear.
        soil = soil * clamp(1.0 + (dirt_detail / 0.8 - 1.0) * mix(1.5, 1.1, packed), 0.6, 1.4)
            * (dirt_tex / max(dirt_luma, 0.05) * 0.3 + 0.7);
        // Lumps a hand across lit on their sunward side, hollows between.
        soil = soil * mix(1.0, mix(0.86, 1.1, churn_light), mid);
        soil = soil * clamp(1.0 + (close_detail - 1.0) * 1.6, 0.55, 1.45);
        let scratch = vg_noise(vec2f(lane.x * 34.0, lane.y * 0.8)) * 0.6 + vg_noise(vec2f(lane.x * 71.0, lane.y * 1.7)) * 0.4;
        soil = soil * mix(1.0, mix(0.9, 1.06, scratch), fine * packed);
        // The dark, damp earth along the turf's edge, in the stretches that
        // keep it.
        soil = mix(soil, soil * vec3f(0.8, 0.77, 0.74), root_damp * 0.7);
        let damp_rim = (1.0 - smoothstep(0.0, 0.025, abs(damp_field - 0.58))) * dirt * (1.0 - damp);
        soil = mix(soil, soil * vec3f(0.8, 0.79, 0.8), damp);
        // The soaked mud round each puddle.
        soil = mix(soil, soil * vec3f(0.62, 0.6, 0.58), mud_rim * (1.0 - puddle) * 0.85);
        soil = mix(soil, soil * vec3f(0.9, 0.88, 0.86), damp_rim * 0.5);
        // Sun-baked mud on the dry, packed lines crazes into plates; the cracks
        // fade out before they could shimmer.
        let baked = smoothstep(0.55, 0.72, vg_noise(p * 0.09 + 71.0)) * packed * (1.0 - damp) * dirt;
        let crack_fade = 1.0 - smoothstep(0.012, 0.035, footprint);
        if baked * crack_fade > 0.0 {
            let crack_distance = vg_cracks(p * 2.7) / 2.7;
            let crack_aa = max(footprint * 0.6, 0.003);
            // Not every plate has split from its neighbours yet.
            let split = smoothstep(0.3, 0.55, vg_noise(p * 2.3 + 8.0));
            let crack = (1.0 - smoothstep(0.004, 0.004 + crack_aa * 2.0, crack_distance)) * baked * crack_fade * split;
            soil = mix(soil, soil * vec3f(0.7, 0.67, 0.64), crack * 0.8);
        }
        soil = mix(soil, soil * vec3f(1.06, 1.05, 1.02), baked * 0.5);
        // Dark clods of turned earth in the loose ground.
        clods = smoothstep(0.72, 0.8, vg_noise(p * 5.3 + 1.7)) * fine * (1.0 - packed * 0.7);
        // Churned soil: the loose ground lies in lumps and hollows a hand
        // across, which the low sun rakes; packed lanes are smoother.
        churn = (lump * 0.6 + vg_noise(p * 8.3 - 4.0) * 0.4) * (1.0 - packed * 0.6) * mid;
        soil = mix(soil, soil * 0.68, clods * 0.6);
        soil = mix(soil, soil * vec3f(1.14, 1.11, 1.05), berm * 0.75);
        // Ruts: a worn-out tread is only pressed a little paler into the dirt;
        // fresh ones cut deeper and darker, with cleat prints across them.
        soil = mix(soil, soil * vec3f(1.04, 1.035, 1.02), tread_bed * (1.0 - smoothstep(0.2, 0.6, fresh)) * 0.5);
        soil = mix(soil, soil * vec3f(0.66, 0.62, 0.6), rut * smoothstep(0.25, 0.85, fresh) * 0.8);
        soil = mix(soil, soil * vec3f(0.8, 0.78, 0.76), cleat * 0.5);
        soil = mix(soil, soil * vec3f(0.6, 0.6, 0.66), clump_shade * 0.65);
        // Sandy creek banks outside the wall, and damp mud by the water.
        soil = mix(soil, vg_linear(0x9d917a) * mix(0.85, 1.12, dirt_detail * 0.5), (1.0 - inside) * 0.35);
        soil = mix(soil, soil * vec3f(0.55, 0.53, 0.5), wet);
    }
    let gravel_tone = vg_linear(0x8e8676) * mix(0.75, 1.2, dirt_detail * 0.5);

    // A few fist-sized stones along the shoulders, each with its own shadow
    // cast away from the sun.
    // Gravelly patches in the loose ground, where the topsoil washed off.
    let gravel_patch = smoothstep(0.5, 0.7, (1.0 - runoff) * 0.75 + swell * 0.3) * dirt * (1.0 - packed * 0.6);
    let stone_density = ((0.02 + shoulder * 0.14 + (1.0 - packed) * 0.04 + road_berm * 0.15) * dirt + verge * 0.3 + gravel_patch * 0.3) * mix(0.1, 2.4, stone_cluster) * mid * (1.0 - rut);
    let sun_shift = sun_flat * min(sun_run, 1.6);
    let stones = vg_scatter(p, stone_density, 0.9, vec2f(0.06, 0.13), aa, 57.0);
    let stone_shadow = vg_scatter(p + sun_shift * 0.1, stone_density, 0.9, vec2f(0.06, 0.13), aa * 2.0, 57.0).x * (1.0 - stones.x);
    // The stone where the sun's ray leaves it: its sunward face is lit, the
    // rest of it in its own shade.
    let stone_lit = 1.0 - vg_scatter(p + sun_flat * 0.05, stone_density, 0.9, vec2f(0.06, 0.13), aa, 57.0).x;
    // Pebbles half sunk in the earth: its colour with a little grey, lit on
    // their sunward crown.
    let pebble_crown = smoothstep(0.0, 0.85, clamp(pebbles.y / 0.06, 0.0, 1.0));
    let pebble_lit = 1.0 - vg_scatter(p + sun_flat * 0.025, pebble_density, 2.9, vec2f(0.025, 0.075), aa, 3.0).x;
    let stone = mix(vg_linear(0x5e5850), vg_linear(0x8e877a), pebbles.z) * mix(0.62, 1.0, pebble_crown) * mix(0.85, 1.25, pebble_lit);
    let stone_warm = mix(stone, soil * mix(0.62, 1.12, pebble_crown), 0.4 + pebbles.w * 0.3);

    // ---- Cottage gardens: flower beds on dark mulch, vegetable rows, and a
    // plank edging where a bed meets the lawn. Rows and blooms settle into
    // their average colour before they could shimmer.
    let flower_bed = smoothstep(0.72, 0.9, garden);
    let veg_plot = smoothstep(0.3, 0.42, garden) * (1.0 - smoothstep(0.6, 0.72, garden));
    let edging = smoothstep(0.08, 0.16, garden) * (1.0 - smoothstep(0.24, 0.32, garden)) * fine;
    let rows_visible = 1.0 - smoothstep(0.04, 0.1, footprint);
    var bed = vec3f(0.0);
    var bed_relief = 0.0;
    var veg = vec3f(0.0);
    var veg_relief = 0.0;
    var plank = vec3f(0.0);
    // None below a garden level of 0.08.
    if garden > 0.08 {
        let mulch = vg_linear(0x4a3a2b) * mix(0.8, 1.2, vg_noise(p * 9.0) * rows_visible + 0.5 * (1.0 - rows_visible));
        let bed_leaves = vg_dense_scatter(p, 0.95 * rows_visible, 5.5, vec2f(0.07, 0.12), aa, 71.0);
        let bed_blooms = vg_dense_scatter(p, 0.8 * rows_visible, 8.0, vec2f(0.035, 0.06), aa, 73.0);
        var bed_flower = vg_linear(0xe0574e);
        bed_flower = select(bed_flower, vg_linear(0xf2d04f), bed_blooms.z > 0.4);
        bed_flower = select(bed_flower, vg_linear(0xf4f0ec), bed_blooms.z > 0.65);
        bed_flower = select(bed_flower, vg_linear(0xc36fd0), bed_blooms.z > 0.85);
        bed = mix(mulch, vg_linear(0x3f6e2c) * mix(0.8, 1.15, bed_leaves.w), bed_leaves.x);
        bed = mix(bed, bed_flower, bed_blooms.x);
        // Far off a bed is leaves speckled with colour.
        bed = mix(mix(vg_linear(0x46653a), vg_linear(0xc0806a), 0.25), bed, rows_visible);
        bed_relief = bed_leaves.y * 0.5 + bed_blooms.y * 0.4;
        // Tilled soil, raked between the rows, and the rows' leaves.
        let veg_row = sin(p.x * 11.4) * 0.5 + 0.5;
        let tilth = vg_linear(0x6b5440) * mix(0.85, 1.1, vg_noise(p * 7.0));
        let heads = vg_dense_scatter(vec2f(p.x, p.y), 0.9, 4.0, vec2f(0.1, 0.16), aa, 77.0);
        let leaves = vg_linear(0x6a9a3a) * mix(0.85, 1.15, heads.w);
        let near_rows = mix(tilth, leaves, smoothstep(0.5, 0.8, veg_row) * max(heads.x, 0.6));
        veg = mix(mix(tilth, leaves, 0.4), near_rows, rows_visible);
        veg_relief = (veg_row * 0.05 + heads.y * 0.3) * rows_visible;
        plank = vg_linear(0x7a5a3a) * mix(0.85, 1.1, vg_noise(p * 3.0));
    }

    // ---- Combine colour, height and finish.
    var color = mix(grass, soil, dirt);
    // Tussocks on the dirt: the turf's colour, lit at their tips and dark at
    // their heart, where the blades bunch.
    let spilled_tone = mix(soil, grass, 0.7) * mix(0.62, 1.05, smoothstep(0.1, 0.9, tussocks.y)) * mix(0.9, 1.08, tussocks.z);
    color = mix(color, spilled_tone, tussocks.x);
    color = mix(color, gravel_tone, verge * 0.85);
    color = mix(color, color * 0.66, grit.x * 0.55);
    color = mix(color, color * vec3f(0.55, 0.55, 0.62), gravel_shadow * 0.75);
    let chip_tone = mix(mix(vg_linear(0x5f594f), vg_linear(0x887f71), gravel.z), color, 0.45) * mix(0.7, 1.12, smoothstep(0.0, 0.8, gravel.y / 0.025));
    color = mix(color, chip_tone, gravel.x);
    color = mix(color, color * mix(1.0, 1.32, smoothstep(0.0, 0.8, crumbs.y / 0.02)), crumbs.x);
    color = mix(color, color * vec3f(0.58, 0.58, 0.64), pebble_shadow * 0.7 * (1.0 - flower_bed));
    color = mix(color, stone_warm, pebbles.x * (1.0 - flower_bed));
    color = mix(color, color * vec3f(0.5, 0.5, 0.57), stone_shadow * 0.8 * (1.0 - flower_bed));
    // Lit from the dome's height: a dark foot, a pale crown.
    let crown = clamp(stones.y / 0.11, 0.0, 1.0);
    // Half buried, so they take the earth's warmth and dust; lit from the
    // dome's height with a dark foot.
    let boulder = mix(soil * 0.8, mix(vg_linear(0x67625a), vg_linear(0x8f887a), stones.z), 0.45) * mix(0.6, 1.0, smoothstep(0.0, 0.85, crown)) * mix(0.72, 1.3, stone_lit);
    color = mix(color, boulder, stones.x * (1.0 - flower_bed));
    // Weeds where the loose ground meets the turf and along the wall's foot,
    // never out on the open road: small rosettes, dark at the heart.
    let weed_edge = min(smoothstep(-0.2, -0.04, lawn_side) + verge, 1.0);
    let weed_density = dirt * (1.0 - packed) * (1.0 - smoothstep(0.0, 0.3, rut + fresh * 0.5)) * weed_edge * 0.25 * fine;
    let weeds = vg_rosette(p, weed_density, 1.25, vec2f(0.07, 0.13), aa, 91.0);
    let weed_leaf = mix(vg_linear(0x4f6230), vg_linear(0x6f7a42), weeds.z) * mix(0.5, 0.95, smoothstep(0.05, 0.8, weeds.y));
    color = mix(color, weed_leaf, weeds.x * (1.0 - flower_bed) * (1.0 - puddle));
    color = mix(color, bloom_color, blooms.x * (1.0 - dirt) * (1.0 - flower_bed));
    color = mix(color, veg, veg_plot);
    color = mix(color, bed, flower_bed);
    color = mix(color, plank, edging * 0.85);
    // Crop fields outside the wall.
    var crop = vec4f(0.0);
    if field > 0.0 {
        crop = vg_crop(p, field_code, footprint);
        color = mix(color, crop.rgb * mix(0.9, 1.0, f.vertex_color.g), field);
    }
    // Hedgerows of dark, lumpy shrubs along the field edges, broken by gaps
    // and kept off the farm lanes.
    var hedge = enclosure_hedge * (1.0 - field) * (1.0 - worn);
    if field_near > 0.06 && field < 0.35 {
        hedge = max(hedge, smoothstep(0.06, 0.26, field_near) * (1.0 - smoothstep(0.02, 0.35, field))
            * smoothstep(0.28, 0.42, vg_noise(p * 0.11 + 5.0)) * (1.0 - worn));
    }
    // The field hedges' shade, from the plan where the sun's ray meets them.
    var shade_cast = hedge_shade;
    if field_near_sun > 0.06 && field_sun < 0.35 {
        let toward = p + sun_flat * sun_run * VG_HEDGE_HEIGHT;
        let field_hedge = smoothstep(0.06, 0.26, field_near_sun) * (1.0 - smoothstep(0.02, 0.35, field_sun))
            * smoothstep(0.28, 0.42, vg_noise(toward * 0.11 + 5.0));
        shade_cast = max(shade_cast, field_hedge * (1.0 - hedge) * (1.0 - field));
    }
    color = mix(color, color * vec3f(0.5, 0.54, 0.64), shade_cast * 0.7);
    var shrubs = 0.0;
    if hedge > 0.0 {
        shrubs = vg_noise(p * 1.4 + 3.0) * 0.6 + vg_noise(p * 3.7 + 9.0) * 0.4 * coarse;
        // Bushes lit along the hedge's sunward side, dark beyond and between.
        let lit = max(hedge_sunward * enclosure_hedge, smoothstep(0.5, 0.8, shrubs) * 0.6);
        let leaves = mix(vg_linear(0x203b1d), vg_linear(0x56783a), smoothstep(0.25, 0.85, shrubs * 0.6 + lit * 0.55));
        color = mix(color, leaves * f.vertex_color.rgb, smoothstep(0.0, 0.5, hedge) * smoothstep(0.15, 0.35, shrubs + hedge * 0.3));
    }
    // Puddles: muddy water, dark where it is deep, mirroring what stands
    // round it: the hills and trees at the horizon when the view grazes it,
    // the pale sky overhead, never a flat silver card.
    var mirror = vec3f(0.0);
    if puddle > 0.0 {
        let view = normalize(frame.camera_position.xyz - f.world);
        let depth = smoothstep(0.35, 1.0, standing);
        let water = mix(color * vec3f(0.36, 0.33, 0.29), vg_linear(0x221d16), depth * 0.75);
        let horizon = mix(vg_linear(0x4f5f4c), vg_linear(0x8a9a9c), smoothstep(0.3, 0.7, vg_noise(p * 0.5 + 3.0)));
        let mirrored = mix(horizon, vg_linear(0xaebfcb), smoothstep(0.1, 0.55, view.y));
        let fresnel = 0.05 + 0.4 * pow(1.0 - clamp(view.y, 0.0, 1.0), 4.0);
        color = mix(color, water, puddle);
        // Its own light, so the reflection holds in shade too; the surface
        // stays matte to the lighting, which would mirror the whole bright
        // sky and turn it to foil.
        mirror = mirrored * fresnel * 0.7 * puddle;
    }
    // The rut walls' shade on their floors: cool, since only the sky lights them.
    let rut_shadow = rut * (1.0 - rut_sun) * dirt;
    color = mix(color, color * vec3f(0.55, 0.57, 0.66), rut_shadow * 0.45 * (1.0 - puddle));
    // Contact shade at the foot of walls, cottages, timber and trunks.
    color = color * mix(0.52, 1.0, open);

    // The earth undulates in hollows and humps a metre or two across, which
    // the low sun rakes: the floor is flat for the simulation, so this lives
    // only in the shading.
    var hummocks = 0.0;
    if dirt > 0.0 {
        hummocks = (vg_noise(p * 0.55 + 23.0) * 0.75 + churn * 0.25) * 0.09 * (1.0 - packed * 0.5);
    }
    let relief = mix(grass_detail * 0.02 + 0.012, dirt_detail * 0.02 + close_detail * 0.012 - rut * 0.1 + berm * 0.025 + clods * 0.012 + churn * 0.035 + hummocks, dirt)
        + pebbles.y * 0.6 + stones.y * 0.7 + weeds.x * 0.015 + grit.y * 0.4 + gravel.y * 0.5 + crumbs.y * 0.5 + cleat * 0.012 * dirt + blooms.y * 0.3 * (1.0 - dirt)
        + tussocks.x * 0.02 + bed_relief * flower_bed + veg_relief * veg_plot
        + edging * 0.03 + crop.a * field + hedge * shrubs * 0.5 + road_berm * 0.05
        + clump_height + swell * 0.35 + bank * inside + lawn_swell * 0.6;
    let normal = vg_bump(mix(relief, -0.08, puddle), f.world, (*s).normal, f.front_facing);

    // Cloud shadows drift over the valley; the arena keeps the steady sun its
    // shadows are read against.
    if inside < 1.0 {
        color = color * mix(vg_cloud_light(p), 1.0, inside);
    }

    (*s).color = color * material.color.rgb * f.tint.rgb;
    (*s).normal = normalize(mix(normal, (*s).normal, puddle));
    (*s).emissive = (*s).emissive + mirror;
    (*s).roughness = mix(mix(mix(mix(0.95, mix(0.92, 0.82, packed), dirt), 0.68, wet * 0.6), 0.55, damp * 0.8), 0.62, puddle);
    (*s).opacity = material.color.a * f.tint.a;
}

// Ridged relief on one plane: 0 in the creases, 1 on the ribs.
fn vg_ridged(q: vec2f) -> f32 {
    let r = 1.0 - abs(vg_noise(q) * 2.0 - 1.0);
    return r * r;
}

// Conifers stand on the faces in rows seen from the side, like the valley's
// own pines: the face is projected onto a vertical plane (`u` along it, `v` the
// world height), trees root on staggered rows of `row` metres and the lowest
// tree covering a point wins, since from the valley the lower trees stand in
// front. x the coverage, y how far up its tree the point lies (1 at the tip),
// z where across its crown the point lies (-1 to 1), w a random tone. `cap` is
// the height above which trees thin out; `aa` the pixel footprint in metres.
// Trees stand tallest and thickest low on the slopes and shrink and scatter
// toward the treeline, and open into meadows where [`vg_forest_clearing`] says.
fn vg_conifer_rows(u: f32, v: f32, seed: f32, cap: f32, aa: f32) -> vec4f {
    let width = 2.2;
    let row = 1.9;
    let tallest = 5.6;
    let top_row = floor(v / row);
    var hit = vec4f(0.0, 0.0, 1.0, 0.5);
    for (var k = 3; k >= 0; k--) {
        let r = top_row - f32(k);
        let base = r * row;
        let shift = vg_hash(vec2f(r, seed)) * width;
        let cell = floor((u - shift) / width);
        for (var j = -1; j <= 1; j++) {
            let c = cell + f32(j);
            let pick = vg_hash2(vec2f(c, r) + seed);
            let tone = vg_hash(vec2f(c * 1.7, r * 2.3) + seed + 5.0);
            // Thinning toward the cap: each tree stands or not as a whole, so
            // the wood breaks into stands and single trees well below it.
            let room = smoothstep(cap + 3.0, cap - 14.0, base);
            if (pick.y > room * 0.97 + 0.03 * (1.0 - step(cap + 6.0, base))) {
                continue;
            }
            let axis = shift + (c + 0.25 + pick.x * 0.5) * width;
            // Meadows open where a slow field over the face says so; a few
            // single gaps let the forest floor through elsewhere.
            if (vg_forest_clearing(axis, base, seed) > 0.5 || vg_hash(vec2f(c * 2.9, r * 1.3) + seed + 9.0) < 0.04) {
                continue;
            }
            let altitude = smoothstep(0.0, cap, base);
            // Stands of one age: a slow field sets how tall the trees round
            // here grow, each tree its own height within that.
            let age = mix(0.72, 1.2, vg_noise(vec2f(axis * 0.045, base * 0.06) + seed * 2.3));
            let h = tallest * mix(0.42, 1.05, fract(pick.y * 7.13)) * age * mix(0.75, 1.0, room) * mix(1.2, 0.7, altitude);
            let foot = base + (tone - 0.5) * 1.8;
            let t = (v - foot) / h;
            if (t < 0.0 || t > 1.0) {
                continue;
            }
            let offset = u - axis;
            // Stylised tiers: each whorl of branches flares out and tucks in;
            // some trees are slim spires, some broad and full.
            let girth = mix(0.8, 1.4, fract(pick.x * 5.31));
            let tiers = fract(t * mix(3.0, 5.5, fract(pick.x * 11.7)) + tone);
            let half = h * 0.3 * girth * (1.0 - t) * mix(mix(0.6, 0.82, tone), 1.0, tiers) + 0.05;
            let cover = 1.0 - smoothstep(half - aa, half + aa, abs(offset));
            if (cover > hit.x) {
                hit = vec4f(cover, t, clamp(offset / max(half, 0.05), -1.0, 1.0), tone);
            }
        }
        if (hit.x > 0.99) {
            break;
        }
    }
    return hit;
}

// Where the wood on a face opens into a meadow, above 0.5: a slow field over
// the face's rows (`u` along them, `v` the height), the same for each tree's
// foot and for the ground between.
fn vg_forest_clearing(u: f32, v: f32, seed: f32) -> f32 {
    let field = vg_noise(vec2f(u * 0.026, v * 0.045) + seed * 3.1) * 0.75 + vg_noise(vec2f(u * 0.09, v * 0.12) + seed * 1.7) * 0.25;
    // The wood opens up a little with altitude, so it thins toward the
    // treeline in stands, not along a line.
    let thin = smoothstep(14.0, 40.0, v) * 0.07;
    return smoothstep(0.31 + thin, 0.24 + thin, field);
}

fn village_ranges(s: ptr<function, Surface>, f: EffectFragment) {
    let p = f.world;
    let base = (*s).color;
    let footprint = max(length(dpdx(p)), length(dpdy(p)));
    let smooth_normal = (*s).normal;
    let up = smooth_normal.y;
    // Rock relief: ribs and gullies running down the fall line of the steep
    // faces (the noise is stretched along y), broken outcrops on gentle ground.
    var weights = pow(abs(smooth_normal), vec3f(4.0));
    weights = weights / (weights.x + weights.y + weights.z);
    let fine = 1.0 - smoothstep(0.35, 1.2, footprint);
    let ribs_x = vg_ridged(vec2f(p.z * 0.085, p.y * 0.05)) * 0.75 + vg_ridged(vec2f(p.z * 0.23, p.y * 0.14) + 3.7) * 0.25 * fine;
    let ribs_z = vg_ridged(vec2f(p.x * 0.085, p.y * 0.05) + 9.1) * 0.75 + vg_ridged(vec2f(p.x * 0.23, p.y * 0.14) + 5.3) * 0.25 * fine;
    let outcrop = vg_ridged(p.xz * 0.06 + 1.3) * 0.75 + vg_ridged(p.xz * 0.17) * 0.25 * fine;
    let crag = ribs_x * weights.x + outcrop * weights.y + ribs_z * weights.z;
    let relief_fade = 1.0 - smoothstep(1.5, 4.0, footprint);
    // A touch of the mesh's own planes keeps the rock crisp without showing
    // the grid as facets.
    var flat_normal = normalize(cross(dpdx(p), dpdy(p)));
    flat_normal = flat_normal * sign(dot(flat_normal, smooth_normal));
    let planar = normalize(mix(smooth_normal, flat_normal, 0.08));
    let crag_normal = vg_bump(crag * 1.6 * relief_fade, p, planar, f.front_facing);

    // Bands, broken by the relief and broad noise so no contour line shows.
    let band_noise = vg_fbm(p.xz * 0.016 + 30.0);
    let treeline = 22.0 + band_noise * 15.0;
    // The snow reaches lower down the shaded northern ranges and the big
    // massifs than on the sunny shoulders, so no one contour runs across them.
    let snowline = 41.0 + band_noise * 12.0 + (vg_noise(p.xz * 0.0055 + 12.0) - 0.5) * 18.0;
    let meadow = 1.0 - smoothstep(5.0, 11.0, p.y + (band_noise - 0.5) * 6.0);

    // Rock: light, warm ribs over cool, dark gullies, faint bedding across the
    // faces, mossy toward its foot.
    let tone = vg_fbm(p.xz * 0.04 + p.y * 0.015);
    let bedding = sin(p.y * 0.9 + vg_noise(p.xz * 0.05) * 6.0) * 0.5 + 0.5;
    // Painted value groups: the relief splits into crisp lit ribs and shaded
    // gullies, with a soft step between, rather than a smooth airbrush.
    let rock_value = crag * 0.85 + tone * 0.3;
    let rock_aa = max(fwidth(rock_value), 0.02);
    let rock_step = smoothstep(0.45 - rock_aa, 0.45 + rock_aa, rock_value) * 0.6 + smoothstep(0.7 - rock_aa, 0.7 + rock_aa, rock_value) * 0.4;
    var rock = mix(vg_linear(0x5a5852), vg_linear(0xb0a792), mix(smoothstep(0.15, 0.75, rock_value), rock_step, 0.55));
    // Ledges: strata run across the faces every few metres, a pale lip over
    // a shaded overhang.
    let strata = fract(p.y / 6.5 + vg_noise(p.xz * 0.03) * 1.6);
    let ledge = smoothstep(0.82, 0.88, strata) * (1.0 - smoothstep(0.93, 0.99, strata)) * (1.0 - weights.y) * relief_fade;
    let overhang = smoothstep(0.0, 0.05, strata) * (1.0 - smoothstep(0.08, 0.2, strata)) * (1.0 - weights.y) * relief_fade;
    rock = mix(rock, rock * 1.12, ledge * 0.2);
    rock = mix(rock, rock * vec3f(0.7, 0.73, 0.8), overhang * 0.2);
    rock = rock * mix(0.94, 1.04, bedding * fine);
    rock = mix(rock, rock * vec3f(0.7, 0.76, 0.86), (1.0 - smoothstep(0.04, 0.4, crag)) * 0.35 * (1.0 - weights.y * 0.6));
    rock = mix(rock, vg_linear(0x5b684f), (1.0 - smoothstep(14.0, 30.0, p.y + crag * 6.0)) * 0.55);

    // Derivatives first: the trees below loop with data-dependent exits.
    let snow_drive = (p.y - snowline) / 4.0 + (up - 0.55) * 5.0 + (0.45 - crag) * 2.2 + (tone - 0.5) * 1.2;
    let snow_aa = max(fwidth(snow_drive), 0.04);

    // Forest: rows of conifers seen from the side, the same pines as the
    // valley's, two-toned by the sun and dark at their feet, thinning into
    // single trees at a ragged treeline and keeping off the cliffs. Grassy
    // avalanche chutes run down some of the gullies.
    let slope_ok = smoothstep(0.5, 0.62, up + crag * 0.06);
    let chute = smoothstep(0.62, 0.75, vg_noise(vec2f(p.x * 0.02 + p.z * 0.02, 3.0)))
        * (1.0 - smoothstep(0.1, 0.22, crag)) * (1.0 - weights.y);
    // The rows' plane faces the slope: the face's compass direction snaps to
    // eighths of a turn, so a trunk stays upright on screen wherever the face
    // looks, and neighbouring eighths overlap where the face turns between them.
    let heading = atan2(smooth_normal.z, smooth_normal.x) / 0.7853982 + 8.0;
    let sector = floor(heading);
    let turn = heading - sector;
    let cap = treeline;
    let pixel = max(footprint, 0.02);
    let angle0 = sector * 0.7853982;
    let angle1 = angle0 + 0.7853982;
    let gate_a = 1.0 - smoothstep(0.55, 0.8, turn);
    let gate_b = smoothstep(0.2, 0.45, turn);
    // Each eighth's rows only where its gate lets them show, and none where
    // no tree can stand or far enough for the canopy's average.
    let wanted = slope_ok * (1.0 - smoothstep(0.16, 0.42, footprint)) > 0.0;
    var rows_a = vec4f(0.0, 0.0, 1.0, 0.5);
    var rows_b = vec4f(0.0, 0.0, 1.0, 0.5);
    if wanted && gate_a > 0.0 {
        rows_a = vg_conifer_rows(dot(p.xz, vec2f(-sin(angle0), cos(angle0))), p.y, 11.0 + (sector % 4.0) * 7.0, cap, pixel);
    }
    if wanted && gate_b > 0.0 {
        rows_b = vg_conifer_rows(dot(p.xz, vec2f(-sin(angle1), cos(angle1))), p.y, 11.0 + ((sector + 1.0) % 4.0) * 7.0, cap, pixel);
    }
    let b_wins = rows_b.x * gate_b > rows_a.x * gate_a;
    var trees = select(rows_a, rows_b, b_wins);
    trees.x = max(rows_a.x * gate_a, rows_b.x * gate_b);
    let along_angle = select(angle0, angle1, b_wins);
    let along = vec3f(-sin(along_angle), 0.0, cos(along_angle));
    // The meadows between the stands, on the nearer eighth's rows.
    let near_sector = select(sector, sector + 1.0, turn > 0.5);
    let near_angle = near_sector * 0.7853982;
    let glade = vg_forest_clearing(dot(p.xz, vec2f(-sin(near_angle), cos(near_angle))), p.y, 11.0 + (near_sector % 4.0) * 7.0);
    // Far away the single trees blur into the canopy's average.
    let distant = smoothstep(0.16, 0.42, footprint);
    let below = smoothstep(-2.0, 4.0, treeline - p.y);
    let stand = slope_ok * (1.0 - chute * 0.9);
    let stands = vg_fbm(p.xz * 0.012 + 50.0);
    // Crowns a tree across and a little taller, for the far wood's texture:
    // each lit on its upper side, so the mass reads as treetops, not felt.
    let crown_u = p.x + p.z * 0.7;
    let crown_a = vg_noise(vec2f(crown_u * 0.42, p.y * 0.21) + 7.0);
    let crown_b = vg_noise(vec2f(crown_u * 0.42, (p.y + 1.1) * 0.21) + 7.0);
    let crown_lit = smoothstep(-0.1, 0.12, crown_b - crown_a);
    let needles_dark = mix(vg_linear(0x173322), vg_linear(0x22422a), smoothstep(0.3, 0.7, stands));
    let needles_lit = mix(vg_linear(0x2f5a33), vg_linear(0x4a7238), smoothstep(0.35, 0.75, stands));
    let floor_shade = vg_linear(0x0f1d14);
    let tree_tone = mix(0.84, 1.1, trees.w) * mix(0.94, 1.04, stands);
    // Low on the slopes the woods are warmer and lighter, near the treeline
    // dark and blue; one tree in eight is a larch, yellow-green among the
    // spruces.
    let height_tint = mix(vec3f(1.1, 1.08, 0.88), vec3f(0.84, 0.92, 1.06), smoothstep(4.0, treeline, p.y));
    let larch = smoothstep(0.86, 0.9, fract(trees.w * 7.7));
    let needles = mix(mix(needles_dark, needles_lit, 0.35 + trees.y * 0.4), mix(vg_linear(0x3e5a26), vg_linear(0x7a8c3a), 0.3 + trees.y * 0.5), larch);
    let tree_color = needles * height_tint * tree_tone;
    // The ground between the trees: grass in the meadows, the forest floor
    // under the stands.
    let floor_tone = mix(floor_shade, vg_linear(0x587a3e) * mix(0.85, 1.1, vg_fbm(p.xz * 0.05)), glade);
    let near_forest = mix(floor_tone, tree_color, trees.x);
    // Far off the wood is one mass of crowns: lit and shaded clumps, darker
    // stands and yellower drifts of larch, never a printed pattern of single
    // trees. The finer clumps fade before they could shimmer.
    var mass = mix(needles_dark, needles_lit, 0.5);
    if distant * stand > 0.0 {
        let crown_fade = 1.0 - smoothstep(0.6, 1.4, footprint);
        let crowns = mix(0.45, crown_lit * 0.75 + crown_a * 0.25, crown_fade) * 0.7 + vg_noise(p.xz * 0.07 + p.y * 0.05) * 0.3;
        let larch_drift = smoothstep(0.7, 0.86, vg_noise(p.xz * 0.06 + 3.0));
        mass = mix(needles_dark, needles_lit, smoothstep(0.2, 0.85, crowns) * 0.85) * mix(0.92, 1.05, stands) * height_tint;
        mass = mix(mass, vg_linear(0x5f7a34) * height_tint, larch_drift * 0.3);
    }
    let far_forest = mix(floor_tone, mass, 0.92 * (1.0 - glade));
    let canopy = mix(near_forest, far_forest, distant);
    // Rock breaks through the wood on the sharpest ribs.
    let rock_through = smoothstep(0.66, 0.78, crag + (tone - 0.5) * 0.3) * (1.0 - weights.y * 0.5);
    let forest = stand * mix(trees.x, below * 0.85, distant) * (1.0 - rock_through);
    // Below the treeline the ground between trees is forest floor, above it
    // bare rock or alpine grass shows between them.
    let floor_amount = stand * below * (1.0 - distant);
    var color = mix(rock, floor_tone * mix(0.9, 1.2, tone), floor_amount * 0.9);
    color = mix(color, canopy, forest);
    // Alpine grass above the wood and down the chutes.
    let pasture = max(max(smoothstep(-6.0, 0.0, treeline - p.y) * slope_ok, chute * slope_ok * below), glade * stand * below)
        * (1.0 - forest);
    // Muted and tawny, so the high pastures never read as bright green
    // blotches on the rock.
    color = mix(color, vg_linear(0x6c7a4c) * mix(0.88, 1.06, tone), pasture * 0.65);
    // Each tree is lit as a cone: the half toward the sun bright, the far half
    // in shade, so the rows read by their light like the near pines.
    let outward = normalize(vec3f(smooth_normal.x, 0.0, smooth_normal.z) + 1e-4);
    let cone = normalize(outward * 0.55 + vec3f(0.0, 0.45, 0.0) + along * sign(trees.z) * 0.4);
    var normal = normalize(mix(crag_normal, cone, forest * (1.0 - distant)));
    normal = normalize(mix(normal, normalize(smooth_normal + vec3f(0.0, 0.3, 0.0)), forest * distant));

    // Snow: crisp fields above the snowline on the gentler faces, fingers of it
    // down the gullies, the steep ribs blown bare.
    let snow = smoothstep(-snow_aa, snow_aa, snow_drive) * smoothstep(snowline - 16.0, snowline - 6.0, p.y);
    let snow_light = mix(vg_linear(0xbfcde0), vg_linear(0xf5f7f5), smoothstep(0.3, 0.85, up));
    color = mix(color, snow_light, snow);
    normal = normalize(mix(normal, normalize(mix(crag_normal, smooth_normal, 0.5)), snow));
    // Low meadows keep their vertex colour, mottled.
    color = mix(color, base * mix(0.9, 1.08, vg_fbm(p.xz * 0.08)), meadow * 0.8);
    // Cloud shadows drift over the ranges as they do over the valley, kept
    // faint so they never read as stains on the rock.
    color = color * mix(1.0, vg_cloud_light(p.xz), 0.45);
    // Aerial perspective: the ranges lose their colour and pale into the air
    // with distance, so the farther slopes stand behind the nearer ones and
    // the backdrop never competes with the village for value. Part of the
    // haze is light of its own, so the shaded faces fade into it too.
    // Nearer ridges keep their colour and contrast, the farther ones layer
    // back into the air, so each range reads as its own silhouette.
    // The ramp shifts toward the sky's blue rather than greying, so the far
    // ranges stay clean and cool instead of muddy.
    let air = smoothstep(90.0, 520.0, length(p - frame.camera_position.xyz));
    let haze = vg_linear(0x9db5c9);
    color = mix(color, vec3f(dot(color, VG_LUMA)), air * 0.15);
    color = mix(color, haze * mix(0.82, 1.0, up), air * (0.35 + air * 0.25));
    (*s).emissive = (*s).emissive + haze * air * 0.1;

    (*s).color = color;
    (*s).normal = normalize(mix(normal, smooth_normal, meadow * 0.7));
    (*s).roughness = mix(0.94, 0.6, snow);
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    if material.params[0].x < 0.5 {
        // Tufts light like the turf they grow from from either side of a blade:
        // a back face keeps its upward normal instead of turning black.
        (*s).normal = (*s).normal * select(1.0, -1.0, (*s).normal.y < 0.0);
        // Each tuft takes most of its colour from the meadow it stands in,
        // keeping its own shade and the root-to-tip ramp of its blades.
        let shade = dot(f.tint.rgb, VG_LUMA) / VG_TUFT_LUMA;
        let lawn = vg_lawn_tone(f.instance_data.xy);
        let turf = lawn.rgb * f.vertex_color.rgb * shade;
        var tuft = mix((*s).color, turf * material.color.rgb, 0.65);
        // Some clumps have gone to seed or dried at their tips, a few are a
        // fresh, bluer green: the tips (bright vertex colour) take it most.
        let pick = vg_hash(f.instance_data.xy * 0.731 + 11.0);
        let tip = smoothstep(0.7, 1.1, f.vertex_color.g);
        // Tufts on the dry crests have gone to seed more often.
        let seeded = smoothstep(0.72 - lawn.w * 0.35, 0.9 - lawn.w * 0.3, pick) * tip;
        tuft = mix(tuft, vg_linear(0x9c9160) * shade * f.vertex_color.rgb, seeded * 0.55);
        tuft = mix(tuft, tuft * vec3f(0.86, 0.98, 1.02), smoothstep(0.25, 0.08, pick) * 0.6);
        (*s).color = tuft;
    } else if material.params[0].x > 1.5 {
        village_ranges(s, f);
    } else if material.params[0].x > 0.5 {
        village_ground(s, f);
    }
}

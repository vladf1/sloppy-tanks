// Pine Village meadow (`effects_scenery::MEADOW_SWAY`). One effect serves three
// village materials, told apart by params[0].x (a uniform, so the branches stay
// in uniform control flow):
//
// 0: the meadow tufts (`village_vegetation.rs`) sway in the wind after the
//    instance transform (tufts are instanced at world placements, so this is
//    world space), by their height above their own base. instance_data.xy is the
//    tuft's wind origin (world x/z).
// 1: the village ground (`ground_surfaces.rs`): lush grass worn into dirt roads
//    with wheel ruts, pebbles, wildflowers and ragged grass edges, all in world
//    space. The map is the grass tile at the mesh UVs (world / 8 m), extra
//    texture 0 the packed-dirt tile. params[0] = (1, arena half size, unused,
//    unused); params[1].xyz the north-south roads' centre x; params[2].xyz the
//    east-west roads' centre z; params[3] = half widths (north-south outer,
//    north-south centre, east-west outer, east-west centre).
// 2: the mountain ranges (`village_landscape.rs`): the vertex colours carry the
//    forest, rock and snow bands; this adds conifer stands, rock strata and
//    crags, and snow that settles on the gentler slopes, in world space.
fn effect_world(w: ptr<function, EffectWorld>, v: EffectVertex) {
    if material.params[0].x > 0.5 {
        return;
    }
    let t = frame.camera_position.w;
    let o = v.instance_data.xy;
    let p = (*w).position;
    let h = max(p.y - v.instance_origin.y, 0.0);
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

// Two lanes of paired wheel ruts meandering along a strip: x is 1 in a rut's
// bed, y on the loose berm beside it. `seed` varies the meander per road.
fn vg_ruts(offset: f32, along: f32, half: f32, seed: f32) -> vec2f {
    let wander = (vg_noise(vec2f(along * 0.045, seed)) - 0.5) * 1.1
        + (vg_noise(vec2f(along * 0.19, seed + 7.0)) - 0.5) * 0.25;
    let u = offset + wander;
    let lane = half * 0.42;
    let gauge = 0.82;
    let nearest = min(
        min(abs(u - lane - gauge), abs(u - lane + gauge)),
        min(abs(u + lane - gauge), abs(u + lane + gauge)),
    );
    let bed = 1.0 - smoothstep(0.05, 0.3, nearest);
    let berm = smoothstep(0.24, 0.36, nearest) * (1.0 - smoothstep(0.36, 0.62, nearest));
    // Ruts come and go where wheels wandered.
    let worn = mix(0.15, 1.0, smoothstep(0.3, 0.7, vg_noise(vec2f(along * 0.09, seed + 3.0))));
    return vec2f(bed, berm) * worn;
}

// A disc on a jittered cell grid: x the coverage of this cell's disc (0 when the
// cell has none), y its dome height in metres, zw two random numbers for its look.
fn vg_scatter(p: vec2f, density: f32, cells: f32, size: vec2f, aa: f32, seed: f32) -> vec4f {
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

fn village_ground(s: ptr<function, Surface>, f: EffectFragment) {
    let p = f.world.xz;
    let extent = material.params[0].y;
    let widths = material.params[3];
    // Pixel footprint in metres: sub-pixel detail fades instead of shimmering.
    let footprint = max(length(dpdx(f.world)), length(dpdy(f.world)));
    let aa = footprint * 0.75;
    let fine = 1.0 - smoothstep(0.02, 0.06, footprint);

    // ---- Where the roads are, with ragged, grass-eaten edges.
    let along = vg_strip(p.x, p.y, material.params[1].xyz, widths.x, widths.y, extent);
    let across = vg_strip(p.y, p.x, material.params[2].xyz, widths.z, widths.w, extent);
    let road = max(along.x, across.x);
    let ragged = (vg_fbm(p * 0.42) - 0.5) * 1.5 + (vg_noise(p * 2.6) - 0.5) * 0.45;
    let wear = smoothstep(-2.2, 0.9, road + ragged);
    let clumps = vg_noise(p * 3.3 + 4.0) * 0.65 + vg_noise(p * 7.9) * 0.35;
    let reveal = wear + (clumps - 0.5) * 0.6;
    let reveal_aa = max(fwidth(reveal), 0.02);
    let dirt = smoothstep(0.5 - reveal_aa, 0.5 + reveal_aa, reveal);
    // A gravel drip line along the foot of the perimeter wall.
    let wall_gap = extent - max(abs(p.x), abs(p.y));
    let verge = (1.0 - smoothstep(0.35, 1.1, wall_gap + (vg_noise(p * 1.7) - 0.5) * 0.5))
        * step(-0.5, wall_gap);

    // ---- Wheel ruts along both road families, softened where they cross.
    let in_along = smoothstep(-0.4, 0.6, along.x);
    let in_across = smoothstep(-0.4, 0.6, across.x);
    let crossing = in_along * in_across;
    let ruts_along = vg_ruts(along.y, p.y, along.z, 1.0 + along.z) * in_along;
    let ruts_across = vg_ruts(across.y, p.x, across.z, 11.0 + across.z) * in_across;
    let ruts = max(ruts_along, ruts_across) * (1.0 - crossing * 0.65);
    let rut = ruts.x * dirt;
    let berm = ruts.y * dirt;
    // Rain lingers in the deepest stretches of rut.
    let wet_patch = smoothstep(0.7, 0.82, vg_fbm(p * 0.21 + 40.0)) * smoothstep(0.5, 0.7, vg_noise(p * 0.9 + 3.0));
    let wet = smoothstep(0.35, 0.7, rut) * wet_patch;
    let puddle = smoothstep(0.6, 0.9, rut) * smoothstep(0.25, 0.6, wet_patch);

    // ---- Textures: two scales of each tile, turned against each other.
    let turned = vec2f(p.x * 0.6 + p.y * 0.8, p.y * 0.6 - p.x * 0.8);
    let grass_near = textureSample(map_texture, map_sampler, f.uv);
    let grass_far = textureSample(map_texture, map_sampler, turned / 21.0);
    let dirt_near = textureSample(extra_texture0, extra_sampler0, p / 3.1);
    let dirt_far = textureSample(extra_texture0, extra_sampler0, turned / 13.0);
    let grass_luma = dot(grass_near.rgb, VG_LUMA);
    let grass_detail = grass_luma / 0.3 * mix(1.0, dot(grass_far.rgb, VG_LUMA) / 0.3, 0.45);
    let dirt_luma = dot(dirt_near.rgb, VG_LUMA);
    let dirt_detail = dirt_luma / 0.36 * mix(1.0, dot(dirt_far.rgb, VG_LUMA) / 0.36, 0.5);

    // ---- Grass: lush greens, sunlit and shaded drifts, dry wear by the road.
    let macro_tone = vg_fbm(p * 0.045 + 3.0);
    let meso = vg_fbm(p * 0.21 - 11.0);
    var grass = mix(vg_linear(0x4d8030), vg_linear(0x84a044), smoothstep(0.3, 0.75, macro_tone));
    grass = mix(grass, vg_linear(0x3f7231), smoothstep(0.5, 0.8, meso) * 0.4);
    grass = mix(grass, vg_linear(0xa4a35e), smoothstep(0.3, 0.8, wear) * 0.5);
    let blades = mix(0.74, 1.26, clamp((grass_detail - 0.55) * 0.9, 0.0, 1.0));
    grass = grass * blades;
    // Clover and dark clumps break the carpet.
    let clover = smoothstep(0.66, 0.82, vg_noise(p * 1.3 + 2.0)) * (1.0 - wear);
    grass = mix(grass, vg_linear(0x3a6d2c) * blades, clover * 0.35);
    // Keep the bank tint and the far hills' vertex shading.
    grass = grass * f.vertex_color.rgb;

    // ---- Dirt: dusty brown with a compacted crown, damp ruts and loose berms.
    let dirt_tex = mix(vec3f(dirt_luma), dirt_near.rgb, 0.55);
    var soil = mix(vg_linear(0x8a6b4a), vg_linear(0xb79a71), smoothstep(0.35, 0.75, vg_fbm(p * 0.09 + 21.0)));
    soil = soil * mix(0.62, 1.38, clamp((dirt_detail - 0.55) * 1.1, 0.0, 1.0))
        * (dirt_tex / max(dirt_luma, 0.05) * 0.35 + 0.65);
    // Damp hollows and dry, sun-bleached crusts.
    let damp = vg_fbm(p * 0.6 + 7.0);
    soil = soil * mix(vec3f(0.84, 0.82, 0.8), vec3f(1.1, 1.08, 1.04), smoothstep(0.3, 0.72, damp));
    // Dark clods of turned earth.
    let clods = smoothstep(0.72, 0.8, vg_noise(p * 5.3 + 1.7)) * fine;
    soil = mix(soil, soil * 0.68, clods * 0.6);
    soil = mix(soil, soil * vec3f(1.16, 1.12, 1.05), berm * 0.6);
    soil = mix(soil, soil * vec3f(0.7, 0.67, 0.64), rut * 0.7);
    // Damp mud darkens around standing water.
    soil = mix(soil, soil * vec3f(0.58, 0.55, 0.52), wet);
    let gravel_tone = vg_linear(0x8e8676) * mix(0.75, 1.2, dirt_detail * 0.5);

    // ---- Pebbles: thick along the road shoulders and the verge, sparse elsewhere.
    let shoulder = smoothstep(0.15, 0.45, wear) * (1.0 - smoothstep(0.7, 0.95, wear));
    let pebble_density = (0.02 + shoulder * 0.28 + dirt * 0.05 + verge * 0.6) * fine;
    let pebbles = vg_scatter(p, pebble_density, 2.4, vec2f(0.04, 0.11), aa, 3.0);
    let grit = vg_scatter(p, (0.12 + dirt * 0.25) * fine, 6.5, vec2f(0.014, 0.03), aa, 9.0);
    let stone = mix(vg_linear(0x6f6a62), vg_linear(0xa29d90), pebbles.z);
    let stone_warm = mix(stone, soil * 1.15, 0.25 + pebbles.w * 0.4);

    // ---- Wildflowers in meadow drifts: daisies with gold hearts, buttercups,
    // violets and campion.
    let meadow = smoothstep(0.45, 0.72, vg_fbm(p * 0.08 + 61.0)) * (1.0 - smoothstep(0.1, 0.4, wear));
    let blooms = vg_scatter(p, (0.03 + meadow * 0.22) * fine, 1.9, vec2f(0.035, 0.06), aa, 27.0);
    let bloom_pick = blooms.z;
    var petals = vg_linear(0xf3f0e4);
    petals = select(petals, vg_linear(0xf2c94a), bloom_pick > 0.55);
    petals = select(petals, vg_linear(0xb08fe0), bloom_pick > 0.78);
    petals = select(petals, vg_linear(0xec9fb8), bloom_pick > 0.92);
    let heart = smoothstep(0.55, 0.3, blooms.y / 0.05) * select(0.0, 1.0, bloom_pick <= 0.55);
    let bloom_color = mix(petals, vg_linear(0xe7b52d), heart);

    // ---- Combine colour, height and finish.
    var color = mix(grass, soil, dirt);
    color = mix(color, gravel_tone, verge * 0.85);
    color = mix(color, color * 0.72, grit.x * 0.5);
    color = mix(color, stone_warm, pebbles.x);
    color = mix(color, bloom_color, blooms.x * (1.0 - dirt));
    // Puddles: dark, glassy water over the rut bed.
    // Puddles: still water mirroring the pale sky, with a dark muddy floor.
    let sky = mix(vg_linear(0x5d6f78), vg_linear(0xa9bcc4), vg_noise(p * 0.4));
    color = mix(color, mix(color * 0.4, sky, 0.55), puddle);

    let relief = mix(grass_detail * 0.02, dirt_detail * 0.02 - rut * 0.05 + berm * 0.012 + clods * 0.012, dirt)
        + pebbles.y * 0.6 + grit.y * 0.4 + blooms.y * 0.3 * (1.0 - dirt);
    let normal = vg_bump(mix(relief, -0.07, puddle), f.world, (*s).normal, f.front_facing);

    (*s).color = color * material.color.rgb * f.tint.rgb;
    (*s).normal = normalize(mix(normal, (*s).normal, puddle));
    (*s).roughness = mix(mix(0.95, 0.9, dirt), 0.06, puddle);
    (*s).opacity = material.color.a * f.tint.a;
}

// Ridged noise for crags: sharp creases where the value noise crosses 0.5.
fn vg_ridged(p: vec2f) -> f32 {
    let a = 1.0 - abs(vg_noise(p) * 2.0 - 1.0);
    let b = 1.0 - abs(vg_noise(p * 2.13 + 5.7) * 2.0 - 1.0);
    return a * a * 0.65 + b * b * 0.35;
}

fn village_ranges(s: ptr<function, Surface>, f: EffectFragment) {
    let p = f.world;
    let base = (*s).color;
    let footprint = max(length(dpdx(p)), length(dpdy(p)));
    // Crags and gullies: a procedural height bumped into the coarse normal. The
    // derivatives are taken before any branch.
    let crag = vg_ridged(p.xz * 0.05) * 1.5 + vg_ridged(p.xz * 0.17 + 9.0) * 0.45
        + vg_noise(p.xz * 0.7) * 0.18 * (1.0 - smoothstep(0.4, 1.2, footprint));
    let normal = vg_bump(crag, p, (*s).normal, f.front_facing);
    let up = normal.y;

    // Bands, broken by noise so no contour line shows.
    let band_noise = vg_fbm(p.xz * 0.02 + 30.0);
    let treeline = 26.0 + band_noise * 14.0;
    let forest = 1.0 - smoothstep(treeline - 5.0, treeline + 3.0, p.y);
    let snowline = 47.0 + band_noise * 10.0 - crag * 2.0;
    let high = smoothstep(snowline - 2.0, snowline + 4.0, p.y);
    let meadow = 1.0 - smoothstep(5.0, 13.0, p.y + (band_noise - 0.5) * 6.0);

    // Rock: warm and cool greys, faint bedding on the faces, dark in the creases.
    let bedding = sin(p.y * 0.55 + vg_noise(p.xz * 0.025) * 9.0) * 0.5 + 0.5;
    let rock_tone = mix(vg_linear(0x75766d), vg_linear(0xa09c8c), vg_fbm(p.xz * 0.06 + 3.0));
    let crease = smoothstep(0.32, 0.05, vg_ridged(p.xz * 0.17 + 9.0));
    var rock = rock_tone * mix(0.92, 1.06, bedding) * mix(1.0, 0.66, crease);
    rock = mix(rock, vg_linear(0x6b7a4c), (1.0 - smoothstep(18.0, 40.0, p.y)) * 0.4);

    // Forest: a mottled canopy of dark conifer crowns and lighter clearings.
    let crowns = vg_fbm(p.xz * 0.32 + 12.0);
    let clearings = smoothstep(0.62, 0.75, vg_fbm(p.xz * 0.04 + 50.0));
    let canopy = mix(vg_linear(0x264730), vg_linear(0x4a7438), smoothstep(0.3, 0.75, crowns));
    var color = mix(base, canopy, forest * (1.0 - clearings) * (1.0 - meadow) * 0.75);
    // Steep ground shows rock through the trees, and above the treeline.
    let steep = 1.0 - smoothstep(0.45, 0.72, up);
    color = mix(color, rock, max(steep * 0.55, (1.0 - forest) * 0.7) * (1.0 - meadow));
    // Snow settles on the gentler slopes above the snowline and lingers in gullies.
    let settles = smoothstep(0.5, 0.75, up) * high;
    let gullies = high * crease * 0.7;
    let snow = clamp(max(settles, gullies), 0.0, 1.0);
    color = mix(color, vg_linear(0xf2f4f0) * mix(0.84, 1.0, up), snow);
    // Low meadows keep their vertex colour, mottled.
    color = mix(color, base * mix(0.88, 1.1, vg_fbm(p.xz * 0.08)), meadow * 0.7);

    (*s).color = color;
    (*s).normal = normalize(mix(normal, (*s).normal, meadow * 0.6));
    (*s).roughness = mix(0.94, 0.62, snow);
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    if material.params[0].x > 1.5 {
        village_ranges(s, f);
    } else if material.params[0].x > 0.5 {
        village_ground(s, f);
    }
}

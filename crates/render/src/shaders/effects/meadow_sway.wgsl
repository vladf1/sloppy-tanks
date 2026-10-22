// Pine Village meadow (`effects_scenery::MEADOW_SWAY`). One effect serves three
// village materials, told apart by params[0].x (a uniform, so the branches stay
// in uniform control flow):
//
// 0: the meadow tufts (`village_vegetation.rs`) sway in the wind after the
//    instance transform (tufts are instanced at world placements, so this is
//    world space), by their height above their own base. instance_data.xy is the
//    tuft's wind origin (world x/z).
// 1: the village ground (`ground_surfaces.rs`): lush grass worn into dirt roads
//    with tank treads, packed lanes, puddles, pebbles, wildflowers and grass
//    edges that are crisp turf in places and crumble in others, cottage
//    gardens and the farmland outside the wall, all in world space. The map is
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

// A disc on a jittered cell grid: x the coverage of this cell's disc (0 when the
// cell has none), y its dome height in metres, zw two random numbers for its look.
fn vg_scatter(p: vec2f, density: f32, cells: f32, size: vec2f, aa: f32, seed: f32) -> vec4f {
    // No cell has a disc: callers weight everything else by the coverage.
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
        var color = mix(vg_linear(0x4e3a2a), vg_linear(0x8a6c4f), row * 0.7 + clod * 0.3);
        color = mix(mix(vg_linear(0x5d4633), vg_linear(0x7a5f45), 0.5), color, rows_visible);
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
    let footprint = max(length(dpdx(f.world)), length(dpdy(f.world)));
    let aa = footprint * 0.75;
    let fine = 1.0 - smoothstep(0.02, 0.06, footprint);

    // ---- The baked plan: inside the wall the arena and its tracks, outside the
    // valley.
    let arena_plan = textureSample(extra_texture1, extra_sampler1, vg_plan_uv(p, VG_ARENA_PLAN, 0.0));
    let valley_plan = textureSample(extra_texture1, extra_sampler1, vg_plan_uv(p, VG_VALLEY_PLAN, 1.0));
    let tracks = textureSample(extra_texture1, extra_sampler1, vg_plan_uv(p, VG_ARENA_PLAN, 2.0));
    let valley_texel = vec2i(clamp(vg_plan_uv(p, VG_VALLEY_PLAN, 1.0) * vec2f(VG_PLAN_WIDTH, VG_PLAN_HEIGHT), vec2f(512.0, 0.0), vec2f(1023.0, 511.0)));
    let field_code = textureLoad(extra_texture1, valley_texel, 0).b * 255.0;
    let inside = 1.0 - smoothstep(extent + 2.0, extent + 3.0, max(abs(p.x), abs(p.y)));
    let worn = mix(valley_plan.r, arena_plan.r, inside);
    let trampled = arena_plan.g * inside;
    let garden = arena_plan.b * inside;
    let open = mix(1.0, arena_plan.a, inside);
    let field = valley_plan.g * (1.0 - inside);
    let wet_bank = (1.0 - valley_plan.a) * (1.0 - inside);

    // ---- Treads (`village_tracks.rs`): metres to the nearest tread's centre,
    // how fresh it is, the traffic that packed the ground and standing water.
    let tread_distance = (1.0 - tracks.r) * VG_TREAD_REACH;
    let tread_aa = max(fwidth(tread_distance), aa);
    let fresh = tracks.g * inside;
    let traffic = tracks.b * inside;
    let standing = clamp((1.0 - tracks.a) / 0.6, 0.0, 1.0) * inside;
    let tread_bed = 1.0 - smoothstep(VG_TREAD_HALF - tread_aa, VG_TREAD_HALF + tread_aa, tread_distance);
    let tread_berm = smoothstep(VG_TREAD_HALF - 0.02, VG_TREAD_HALF + 0.08, tread_distance)
        * (1.0 - smoothstep(VG_TREAD_HALF + 0.1, VG_TREAD_HALF + 0.34, tread_distance));
    // Old treads blur into the packed dirt; fresh ones keep sharp walls.
    let rut = tread_bed * smoothstep(0.05, 0.6, fresh);
    let berm = tread_berm * smoothstep(0.35, 0.9, fresh);

    // ---- Where the grass wears through. The edge is crisp turf with a lip in
    // some stretches and breaks into clumps and bald scuffs over a metre or two
    // in others; treads cut the turf wherever they cross it.
    // Trampled grass, and what survives in the quiet middle of a road, always
    // breaks up.
    let crumbly = max(smoothstep(0.3, 0.78, vg_noise(p * 0.045 + 5.0)), smoothstep(0.45, 0.9, trampled));
    // The plan ramps the dirt over metres (`village_ground::EDGE_RAMP`); `band`
    // is how much of that ramp the edge spreads over.
    let band = mix(0.05, 0.4, crumbly);
    let edge_noise = vg_fbm(p * 0.62) - 0.5;
    // Clumps of turf left on the dirt and scuffs bitten into the grass, in two
    // sizes; each disc stays well inside its scatter cell, so none is clipped.
    let tufts = vg_scatter(p, 0.8, 1.0, vec2f(0.09, 0.19), aa, 41.0);
    let sods = vg_scatter(p, 0.7, 0.45, vec2f(0.2, 0.42), aa, 43.0);
    let clumps = select(sods, tufts, tufts.x > sods.x);
    let clump_sign = select(-1.0, 1.0, clumps.w > 0.55);
    var reveal = worn + (edge_noise * 0.8 + clump_sign * clumps.x * 0.55) * band;
    reveal = reveal + (vg_noise(p * 6.5) - 0.5) * 0.03;
    // Trampled ground goes bald in scuffed patches; the noise lifts it at most
    // 0.35, so ground trampled less than 0.27 never does.
    var bald = 0.0;
    if trampled > 0.27 {
        bald = smoothstep(0.62, 1.0, trampled + (vg_fbm(p * 0.5 + 9.0) - 0.5) * 0.7) * 0.6;
    }
    reveal = max(reveal, bald + (clumps.x * clumps.z - 0.3) * 0.3);
    reveal = max(reveal, 0.5 + (rut - 0.5) * 2.0);
    let reveal_aa = max(fwidth(reveal), 0.02);
    let dirt = smoothstep(0.5 - reveal_aa, 0.5 + reveal_aa, reveal);
    // The turf lip: a raised, sunlit grass edge over a shaded undercut, only
    // where the edge is crisp.
    let crisp = 1.0 - crumbly;
    let lip = (1.0 - smoothstep(0.0, 0.05, 0.5 - reveal)) * (1.0 - dirt) * crisp;
    let undercut = (1.0 - smoothstep(0.0, 0.03, reveal - 0.5)) * dirt * crisp * (1.0 - rut);
    let wear = smoothstep(0.0, 0.9, max(worn, trampled * 0.85));
    // A gravel drip line along the foot of the perimeter wall (none past 1.35 m,
    // where even the noise's 0.25 m reach leaves it at full distance).
    let wall_gap = extent - max(abs(p.x), abs(p.y));
    var verge = 0.0;
    if wall_gap >= -0.5 && wall_gap < 1.35 {
        verge = 1.0 - smoothstep(0.35, 1.1, wall_gap + (vg_noise(p * 1.7) - 0.5) * 0.5);
    }

    let packed = smoothstep(0.2, 0.85, traffic);
    // Water stands in the deepest ruts and the junctions' potholes.
    let puddle = smoothstep(0.35, 0.65, standing);
    let wet = max(max(standing * 0.85, rut * fresh * 0.25), wet_bank * dirt);

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

    // ---- Grass: lush greens in broad sunlit and shaded drifts, cooler under
    // the clover, dry and flattened where it is walked, bright at the lip.
    // Neither grass nor its wildflowers show on bare dirt.
    var grass = vec3f(0.0);
    var blooms = vec4f(0.0);
    var bloom_color = vec3f(0.0);
    if dirt < 1.0 {
        let macro_tone = vg_fbm(p * 0.032 + 3.0);
        let meso = vg_fbm(p * 0.15 - 11.0);
        grass = mix(vg_linear(0x4c8030), vg_linear(0x82a043), smoothstep(0.3, 0.78, macro_tone));
        grass = mix(grass, vg_linear(0x3a6b33), smoothstep(0.52, 0.82, meso) * 0.45);
        // Walked grass dries toward straw and olive.
        grass = mix(grass, mix(vg_linear(0x8d8a52), vg_linear(0xa39d5e), meso), wear * 0.6);
        let blades = mix(0.8, 1.2, clamp((grass_detail - 0.55) * 0.9, 0.0, 1.0));
        grass = grass * blades;
        // Clover and dark clumps break the carpet.
        let clover = smoothstep(0.66, 0.82, vg_noise(p * 1.3 + 2.0)) * (1.0 - wear);
        grass = mix(grass, vg_linear(0x386b2b) * blades, clover * 0.35);
        grass = mix(grass, grass * vec3f(1.12, 1.12, 0.9), lip * 0.6);
        // Flower drifts tint the lawn even where single blooms are too small to draw.
        let flowers = vg_fbm(p * 0.08 + 61.0);
        let drift = smoothstep(0.5, 0.78, flowers) * (1.0 - wear) * (1.0 - fine);
        if drift > 0.0 {
            let drift_tint = mix(vg_linear(0xe9e6c8), vg_linear(0xe8c64a), smoothstep(0.4, 0.6, vg_noise(p * 0.05 + 7.0)));
            grass = mix(grass, drift_tint, drift * 0.16);
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
    // the ruts onto the loose ground, sparse elsewhere.
    let shoulder = smoothstep(0.15, 0.45, wear) * (1.0 - smoothstep(0.7, 0.95, wear));
    let pebble_density = (0.02 + shoulder * 0.28 + dirt * (1.0 - packed) * 0.08 + verge * 0.6 + wet_bank * 0.4) * fine * (1.0 - rut);
    let pebbles = vg_scatter(p, pebble_density, 2.4, vec2f(0.04, 0.11), aa, 3.0);
    let grit = vg_scatter(p, (0.12 + dirt * 0.25) * fine, 6.5, vec2f(0.014, 0.03), aa, 9.0);

    // ---- Dirt: loose, darker earth where little drove, packed pale and combed
    // along the busy lines, dark damp ruts with crumbled berms, wet under the
    // grass edge. Pebbles take their warmth from it, so it is shaded wherever
    // either shows.
    var soil = vec3f(0.0);
    var clods = 0.0;
    if dirt > 0.0 || pebbles.x > 0.0 {
        // Which way the road runs, for streaks combed along it.
        let along = vg_strip(p.x, p.y, material.params[1].xyz, widths.x, widths.y, extent);
        let across = vg_strip(p.y, p.x, material.params[2].xyz, widths.z, widths.w, extent);
        let lane = select(vec2f(across.y, p.x), vec2f(along.y, p.y), along.x > across.x);
        let streaks = vg_noise(vec2f(lane.x * 6.0, lane.y * 0.09)) * 0.6 + vg_noise(vec2f(lane.x * 17.0, lane.y * 0.25)) * 0.4;
        let dirt_tex = mix(vec3f(dirt_luma), dirt_near.rgb, 0.45);
        let loose = mix(vg_linear(0x6e5844), vg_linear(0x846c53), smoothstep(0.25, 0.8, vg_fbm(p * 0.07 + 21.0)));
        let hard = mix(vg_linear(0x9d8a6e), vg_linear(0xb2a083), streaks);
        soil = mix(loose, hard, packed * 0.85);
        soil = soil * mix(0.72, 1.28, clamp((dirt_detail - 0.55) * mix(1.1, 0.7, packed), 0.0, 1.0))
            * (dirt_tex / max(dirt_luma, 0.05) * 0.3 + 0.7);
        // The worn margin, where grass roots hold damp soil, and the undercut lip.
        soil = mix(soil * vec3f(0.8, 0.79, 0.76), soil, smoothstep(0.55, 0.95, worn));
        soil = mix(soil, soil * vec3f(0.62, 0.6, 0.58), undercut * 0.8);
        // Dark clods of turned earth in the loose ground.
        clods = smoothstep(0.72, 0.8, vg_noise(p * 5.3 + 1.7)) * fine * (1.0 - packed * 0.7);
        soil = mix(soil, soil * 0.68, clods * 0.6);
        soil = mix(soil, soil * vec3f(1.14, 1.11, 1.05), berm * 0.75);
        soil = mix(soil, soil * vec3f(0.74, 0.71, 0.68), rut * (0.35 + fresh * 0.5));
        // Sandy creek banks outside the wall, and damp mud by the water.
        soil = mix(soil, vg_linear(0x9d917a) * mix(0.85, 1.12, dirt_detail * 0.5), (1.0 - inside) * 0.35);
        soil = mix(soil, soil * vec3f(0.55, 0.53, 0.5), wet);
    }
    let gravel_tone = vg_linear(0x8e8676) * mix(0.75, 1.2, dirt_detail * 0.5);

    let stone = mix(vg_linear(0x6f6a62), vg_linear(0xa29d90), pebbles.z);
    let stone_warm = mix(stone, soil * 1.15, 0.25 + pebbles.w * 0.4);

    // ---- Cottage gardens: flower beds on dark mulch, vegetable rows, and a
    // plank edging where a bed meets the lawn.
    let flower_bed = smoothstep(0.72, 0.9, garden);
    let veg_plot = smoothstep(0.3, 0.42, garden) * (1.0 - smoothstep(0.6, 0.72, garden));
    let edging = smoothstep(0.08, 0.16, garden) * (1.0 - smoothstep(0.24, 0.32, garden)) * fine;
    var bed = vec3f(0.0);
    var bed_relief = 0.0;
    var veg = vec3f(0.0);
    var veg_relief = 0.0;
    var plank = vec3f(0.0);
    // None below a garden level of 0.08.
    if garden > 0.08 {
        let mulch = vg_linear(0x3f3125) * mix(0.8, 1.2, vg_noise(p * 9.0));
        let bed_leaves = vg_scatter(p, 0.95, 5.5, vec2f(0.07, 0.12), aa, 71.0);
        let bed_blooms = vg_scatter(p, 0.8, 8.0, vec2f(0.035, 0.06), aa, 73.0);
        var bed_flower = vg_linear(0xe0574e);
        bed_flower = select(bed_flower, vg_linear(0xf2d04f), bed_blooms.z > 0.4);
        bed_flower = select(bed_flower, vg_linear(0xf4f0ec), bed_blooms.z > 0.65);
        bed_flower = select(bed_flower, vg_linear(0xc36fd0), bed_blooms.z > 0.85);
        bed = mix(mulch, vg_linear(0x3f6e2c) * mix(0.8, 1.15, bed_leaves.w), bed_leaves.x);
        bed = mix(bed, bed_flower, bed_blooms.x);
        bed_relief = bed_leaves.y * 0.5 + bed_blooms.y * 0.4;
        let veg_row = sin(p.x * 11.4) * 0.5 + 0.5;
        let heads = vg_scatter(vec2f(p.x, p.y), 0.9, 4.0, vec2f(0.1, 0.16), aa, 77.0);
        veg = mix(vg_linear(0x4a3828), vg_linear(0x5c8a33) * mix(0.85, 1.15, heads.w), smoothstep(0.45, 0.75, veg_row) * max(heads.x, 0.55));
        veg_relief = veg_row * 0.05 + heads.y * 0.3;
        plank = vg_linear(0x7a5a3a) * mix(0.85, 1.1, vg_noise(p * 3.0));
    }

    // ---- Combine colour, height and finish.
    var color = mix(grass, soil, dirt);
    color = mix(color, gravel_tone, verge * 0.85);
    color = mix(color, color * 0.72, grit.x * 0.5);
    color = mix(color, stone_warm, pebbles.x * (1.0 - flower_bed));
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
    // Puddles: still water mirroring the pale sky over a dark muddy floor.
    if puddle > 0.0 {
        let sky = mix(vg_linear(0x5d6f78), vg_linear(0xa9bcc4), vg_noise(p * 0.4));
        let depth = smoothstep(0.35, 1.0, standing);
        color = mix(color, mix(color * 0.32, sky * 0.55, 0.12 + depth * 0.18), puddle);
    }
    // Contact shade at the foot of walls, cottages, timber and trunks.
    color = color * mix(0.52, 1.0, open);

    let turf = 0.035 + lip * 0.02;
    let relief = mix(grass_detail * 0.02 + turf, dirt_detail * 0.02 - rut * 0.07 + berm * 0.02 + clods * 0.012 - undercut * 0.012, dirt)
        + pebbles.y * 0.6 + grit.y * 0.4 + blooms.y * 0.3 * (1.0 - dirt)
        + bed_relief * flower_bed + veg_relief * veg_plot
        + edging * 0.03 + crop.a * field;
    let normal = vg_bump(mix(relief, -0.08, puddle), f.world, (*s).normal, f.front_facing);

    (*s).color = color * material.color.rgb * f.tint.rgb;
    (*s).normal = normalize(mix(normal, (*s).normal, puddle));
    (*s).roughness = mix(mix(mix(0.95, mix(0.92, 0.82, packed), dirt), 0.68, wet * 0.6), 0.05, puddle);
    (*s).opacity = material.color.a * f.tint.a;
}

// Ridged relief on one plane: 0 in the creases, 1 on the ribs.
fn vg_ridged(q: vec2f) -> f32 {
    let r = 1.0 - abs(vg_noise(q) * 2.0 - 1.0);
    return r * r;
}

// Conifer crowns seen from afar on a jittered grid of `cell` metres, each round
// crown free to overlap its neighbours: x the coverage, y how high up its cone
// `p` lies (1 at the tip), zw the horizontal direction out from its axis.
fn vg_crowns(p: vec2f, cell: f32, aa: f32) -> vec4f {
    let q = p / cell;
    let i = floor(q);
    var best = vec4f(0.0, 0.0, 0.0, 1.0);
    var cover = 0.0;
    for (var j = -1; j <= 1; j++) {
        for (var k = -1; k <= 1; k++) {
            let c = i + vec2f(f32(k), f32(j));
            let jitter = vg_hash2(c + 17.0);
            let radius = mix(0.52, 0.8, jitter.y) * cell;
            let offset = p - (c + jitter) * cell;
            let d = length(offset);
            cover = max(cover, 1.0 - smoothstep(radius - aa, radius + aa, d));
            let height = 1.0 - d / radius;
            if (height > best.y) {
                best = vec4f(0.0, height, offset / max(d, 1e-3));
            }
        }
    }
    best.x = cover;
    return best;
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
    let ribs_x = vg_ridged(vec2f(p.z * 0.085, p.y * 0.03)) * 0.75 + vg_ridged(vec2f(p.z * 0.23, p.y * 0.09) + 3.7) * 0.25 * fine;
    let ribs_z = vg_ridged(vec2f(p.x * 0.085, p.y * 0.03) + 9.1) * 0.75 + vg_ridged(vec2f(p.x * 0.23, p.y * 0.09) + 5.3) * 0.25 * fine;
    let outcrop = vg_ridged(p.xz * 0.06 + 1.3) * 0.75 + vg_ridged(p.xz * 0.17) * 0.25 * fine;
    let crag = ribs_x * weights.x + outcrop * weights.y + ribs_z * weights.z;
    let relief_fade = 1.0 - smoothstep(1.5, 4.0, footprint);
    // The mesh's own triangles give the rock crisp planes that catch the light
    // in clean value groups; they are the true surface, so nothing smears.
    var flat_normal = normalize(cross(dpdx(p), dpdy(p)));
    flat_normal = flat_normal * sign(dot(flat_normal, smooth_normal));
    let planar = normalize(mix(smooth_normal, flat_normal, 0.6));
    let crag_normal = vg_bump(crag * 1.4 * relief_fade, p, planar, f.front_facing);

    // Bands, broken by the relief and broad noise so no contour line shows.
    let band_noise = vg_fbm(p.xz * 0.016 + 30.0);
    let treeline = 22.0 + band_noise * 15.0;
    let snowline = 41.0 + band_noise * 12.0;
    let meadow = 1.0 - smoothstep(5.0, 11.0, p.y + (band_noise - 0.5) * 6.0);

    // Rock: light, warm ribs over cool, dark gullies, faint bedding across the
    // faces, mossy toward its foot.
    let tone = vg_fbm(p.xz * 0.04 + p.y * 0.015);
    let bedding = sin(p.y * 0.9 + vg_noise(p.xz * 0.05) * 6.0) * 0.5 + 0.5;
    var rock = mix(vg_linear(0x6a675f), vg_linear(0xaaa290), smoothstep(0.1, 0.8, crag * 0.8 + tone * 0.3));
    rock = rock * mix(0.94, 1.04, bedding * fine);
    rock = mix(rock, rock * vec3f(0.8, 0.86, 0.95), (1.0 - smoothstep(0.08, 0.4, crag)) * 0.5);
    rock = mix(rock, vg_linear(0x5b684f), (1.0 - smoothstep(14.0, 30.0, p.y + crag * 6.0)) * 0.55);

    // Forest: a canopy of single conifer cones, lit on their sunny side and
    // dark between them, breaking into separate trees at a ragged treeline and
    // off the cliffs.
    let crowns = vg_crowns(p.xz, 1.7, max(footprint * 0.8, 0.1));
    let reach = treeline - p.y;
    let slope_ok = smoothstep(0.52, 0.64, up + crag * 0.06);
    // Solid canopy below the treeline; above it single crowns stand out of
    // the rock, thinning upward.
    let canopy_core = smoothstep(1.0, 3.0, reach);
    let scattered = smoothstep(-5.0, 0.5, reach + (crowns.y - 0.5) * 3.0) * crowns.x;
    let wooded = max(canopy_core, scattered) * slope_ok;
    let clearings = smoothstep(0.64, 0.72, vg_fbm(p.xz * 0.03 + 50.0));
    let distant = smoothstep(0.3, 0.9, footprint);
    let forest = wooded * (1.0 - clearings * (1.0 - scattered));
    let needles = mix(vg_linear(0x1f3d27), vg_linear(0x3a5f34), vg_noise(p.xz * 0.11) * 0.6 + crowns.y * 0.4);
    let gaps = vg_linear(0x14261a);
    let canopy = mix(mix(gaps, needles, crowns.x), needles * 0.85, distant);
    var color = mix(rock, canopy, forest);
    // Alpine grass between the scattered trees above the wood.
    let pasture = smoothstep(-6.0, 0.0, reach) * (1.0 - forest) * slope_ok;
    color = mix(color, vg_linear(0x587744) * mix(0.85, 1.1, tone), pasture * 0.75);
    let cone = normalize(vec3f(crowns.z, 0.75, crowns.w));
    var normal = normalize(mix(crag_normal, normalize(cone + smooth_normal * 0.4), forest * (1.0 - distant) * crowns.x));

    // Snow: crisp fields above the snowline on the gentler faces, fingers of it
    // down the gullies, the steep ribs blown bare.
    let snow_drive = (p.y - snowline) / 4.0 + (up - 0.55) * 5.0 + (0.45 - crag) * 2.2 + (tone - 0.5) * 1.2;
    let snow_aa = max(fwidth(snow_drive), 0.04);
    let snow = smoothstep(-snow_aa, snow_aa, snow_drive) * smoothstep(snowline - 16.0, snowline - 6.0, p.y);
    let snow_light = mix(vg_linear(0xbfcde0), vg_linear(0xf5f7f5), smoothstep(0.3, 0.85, up));
    color = mix(color, snow_light, snow);
    normal = normalize(mix(normal, normalize(mix(crag_normal, smooth_normal, 0.5)), snow));
    // Low meadows keep their vertex colour, mottled.
    color = mix(color, base * mix(0.9, 1.08, vg_fbm(p.xz * 0.08)), meadow * 0.8);

    (*s).color = color;
    (*s).normal = normalize(mix(normal, smooth_normal, meadow * 0.7));
    (*s).roughness = mix(0.94, 0.6, snow);
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    if material.params[0].x > 1.5 {
        village_ranges(s, f);
    } else if material.params[0].x > 0.5 {
        village_ground(s, f);
    }
}

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

// Where long grass stands, positive in the stands; `village_vegetation::long_grass`
// is the same field.
fn vg_long_grass(p: vec2f) -> f32 {
    return sin(p.x * 0.083 + p.y * 0.041) * cos(p.y * 0.067 - p.x * 0.029) * 0.65
        + sin(p.x * 0.19 - p.y * 0.16 + 1.3) * 0.35;
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
    // Detail a few centimetres across still reads from the gameplay camera.
    let mid = 1.0 - smoothstep(0.07, 0.16, footprint);

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
    // Cleat prints across a fresh tread: bars square to its run, even about
    // its centreline so the two halves meet without a seam.
    let tread_across = normalize(vec2f(tracks_x - tracks.r, tracks_z - tracks.r) + vec2f(1e-5, 0.0));
    let tread_along = dot(p, vec2f(-tread_across.y, tread_across.x));
    let cleat_fade = (1.0 - smoothstep(0.025, 0.07, footprint)) * smoothstep(0.2, 0.6, fresh);
    let cleat = smoothstep(0.1, 0.6, cos(tread_along * 28.56)) * rut * cleat_fade;

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
    let tufts = vg_scatter(p, 0.45, 1.0, vec2f(0.09, 0.19), aa, 41.0);
    let sods = vg_scatter(p, 0.5, 0.45, vec2f(0.2, 0.42), aa, 43.0);
    let clumps = select(sods, tufts, tufts.x > sods.x);
    let clump_sign = select(-1.0, 1.0, clumps.w > 0.55);
    var reveal = worn + (edge_noise * 0.8 + clump_sign * clumps.x * 0.55) * band;
    reveal = reveal + (vg_noise(p * 6.5) - 0.5) * 0.03;
    // Height blend: the tallest blades of the grass tile hold out over the
    // dirt and the dirt shows first between them, so the edge follows tufts
    // of blades rather than a cut line.
    reveal = reveal - clamp(grass_detail - 1.0, -0.6, 0.6) * mix(0.09, 0.16, crumbly);
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
    // The last half metre of grass before the dirt grows thin and straw-dry,
    // and the dirt just past it lies in the turf's shade.
    let fringe = smoothstep(0.22, 0.48, reveal) * (1.0 - dirt);
    let edge_shade = (1.0 - smoothstep(0.0, 0.1, reveal - 0.5)) * dirt * (1.0 - rut);
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


    // ---- The lie of the land. The floor is flat for the simulation, so the
    // swells are light and colour only: low mounds and hollows a few metres
    // across, the turf standing up in a bank along the worn ground, drier on
    // the crests and lusher where the water collects.
    let swell = (vg_fbm(p * 0.075 + 90.0) - 0.5) * 2.2 + (vg_noise(p * 0.21 + 4.0) - 0.5) * 0.45;
    let bank = (1.0 - smoothstep(0.08, 0.95, worn)) * 0.22;
    // Long grass grows in broad stands the mowers skip (the tall tufts of
    // `village_vegetation::long_grass` follow the same field) and never where
    // the ground is worn or walked.
    let stand_field = vg_long_grass(p) + (vg_fbm(p * 0.31 + 23.0) - 0.5) * 0.55;
    let long_grass = smoothstep(0.08, 0.4, stand_field) * (1.0 - smoothstep(0.05, 0.45, wear)) * (1.0 - field);
    // Gusts comb the long grass: bright bands where the blades bow and show
    // their pale sides, rolling downwind at one speed (the phase varies with
    // place, never the rate).
    let wind_along = dot(p, VG_WIND);
    let wind_across = dot(p, vec2f(-VG_WIND.y, VG_WIND.x));
    let gust = vg_noise(vec2f(wind_along * 0.11 - frame.camera_position.w * VG_GUST_SPEED, wind_across * 0.045))
        * 0.7 + vg_noise(vec2f(wind_along * 0.27 - frame.camera_position.w * VG_GUST_SPEED * 2.45, wind_across * 0.12 + 7.0)) * 0.3;
    // Close up the turf breaks into small tufts with lit crowns, and the long
    // grass lies combed over in wavy strokes downwind; both blend to their
    // average once they shrink under a few pixels.
    let clump_view = 1.0 - smoothstep(0.045, 0.11, footprint);
    var tuft_bed = vec4f(0.0);
    var comb = 0.5;
    if clump_view * (1.0 - dirt) > 0.0 {
        tuft_bed = vg_scatter(p, 0.45, 3.4, vec2f(0.07, 0.13), aa * 1.5, 83.0);
        let warp = vg_noise(p * 0.45 + 13.0) * 2.4;
        comb = vg_noise(vec2f(wind_along * 1.3 + warp, wind_across * 8.5)) * 0.65
            + vg_noise(vec2f(wind_along * 2.9 - warp, wind_across * 19.0 + 3.0)) * 0.35;
    }
    let clump_cover = tuft_bed.x * clump_view * (1.0 - dirt);
    let clump_height = (tuft_bed.y * 0.7 + (comb - 0.5) * 0.05 * long_grass) * clump_view * (1.0 - dirt);

    // ---- Grass: a living meadow, not a lawn. Short sunlit turf and darker
    // long stands, per-tussock hue, straw on the crests and the walked lines,
    // lusher green in the hollows, clover and wildflower drifts. Neither grass
    // nor its wildflowers show on bare dirt.
    var grass = vec3f(0.0);
    var blooms = vec4f(0.0);
    var bloom_color = vec3f(0.0);
    if dirt < 1.0 {
        let meso = vg_fbm(p * 0.15 - 11.0);
        let warmth = vg_fbm(p * 0.045 + 3.0);
        let short_turf = mix(vg_linear(0x5f8f35), vg_linear(0x86a845), smoothstep(0.3, 0.75, warmth));
        let long_turf = mix(vg_linear(0x4a7a3a), vg_linear(0x6a9442), smoothstep(0.3, 0.75, meso));
        grass = mix(short_turf, long_turf, long_grass);
        // Hollows hold the damp, crests dry off.
        grass = mix(grass, grass * vec3f(0.84, 0.93, 0.86), smoothstep(0.0, -0.45, swell) * 0.7);
        grass = mix(grass, mix(vg_linear(0x9aa052), vg_linear(0xb0a861), meso), smoothstep(0.2, 0.65, swell) * 0.28 * (1.0 - long_grass * 0.5));
        // Walked grass dries toward straw and olive.
        grass = mix(grass, mix(vg_linear(0x8d8a52), vg_linear(0xa39d5e), meso), wear * 0.6);
        let blades = mix(0.8, 1.2, clamp((grass_detail - 0.55) * 0.9, 0.0, 1.0));
        grass = grass * blades;
        // Tufts: each its own hue, bright crowned; the long grass in combed
        // strokes, bright where the blades lie toward the light.
        let crown_light = smoothstep(0.0, 0.9, tuft_bed.y / 0.12);
        let tuft_hue = mix(vec3f(1.05, 1.03, 0.9), vec3f(0.92, 0.99, 0.95), tuft_bed.z);
        grass = mix(grass, grass * tuft_hue * mix(0.96, 1.12, crown_light), clump_cover);
        let stroke = mix(0.8, 1.16, smoothstep(0.2, 0.8, comb));
        grass = grass * mix(1.0, stroke, long_grass * clump_view);
        // Bowed blades in a gust.
        let sheen = smoothstep(0.5, 0.85, gust) * mix(0.25, 1.0, long_grass);
        grass = mix(grass, grass * vec3f(1.2, 1.2, 1.02) + vg_linear(0x2a2a12) * 0.25, sheen * 0.32);
        // Clover and dark clumps break the carpet.
        let clover = smoothstep(0.66, 0.82, vg_noise(p * 1.3 + 2.0)) * (1.0 - wear) * (1.0 - long_grass);
        grass = mix(grass, vg_linear(0x3d6f2e) * blades, clover * 0.22);
        grass = mix(grass, mix(vg_linear(0x9c9455), vg_linear(0x8a9a4c), meso) * blades, fringe * 0.55);
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
    let pebble_density = (0.02 + shoulder * 0.28 + dirt * (1.0 - packed) * 0.05 + verge * 0.6 + wet_bank * 0.4) * fine * (1.0 - rut);
    let pebbles = vg_scatter(p, pebble_density, 2.4, vec2f(0.04, 0.11), aa, 3.0);
    let grit = vg_scatter(p, (0.12 + dirt * 0.25) * fine, 6.5, vec2f(0.014, 0.03), aa, 9.0);

    // Damp hollows in the loose ground: darker, cooler earth with a crisp
    // drying rim and a sheen, so they read as wet rather than as smudges. Its
    // derivative stays out of the branch below.
    let damp_field = vg_fbm(p * 0.13 + 33.0) * 0.6 - swell * 0.45 + (1.0 - packed) * 0.08 + standing * 0.3;
    let damp_aa = max(fwidth(damp_field), 0.004);
    let damp = smoothstep(0.62 - damp_aa, 0.62 + damp_aa, damp_field) * dirt;

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
        let loose = mix(vg_linear(0x7a634d), vg_linear(0x8b735a), smoothstep(0.25, 0.8, vg_fbm(p * 0.07 + 21.0)));
        let hard = mix(vg_linear(0x9a876b), vg_linear(0xab997c), streaks);
        soil = mix(loose, hard, packed * 0.75);
        // Earth mottled at hand scale: drier crumbs and darker loam.
        let loam = vg_fbm(p * 0.85 + 41.0);
        soil = soil * mix(0.88, 1.09, smoothstep(0.25, 0.75, loam)) * mix(1.0, mix(0.94, 1.05, vg_noise(p * 3.1 + 5.0)), mid);
        soil = soil * mix(0.72, 1.28, clamp((dirt_detail - 0.55) * mix(1.1, 0.7, packed), 0.0, 1.0))
            * (dirt_tex / max(dirt_luma, 0.05) * 0.3 + 0.7);
        // The worn margin, where grass roots hold damp soil, and the undercut lip.
        soil = mix(soil * vec3f(0.8, 0.79, 0.76), soil, smoothstep(0.55, 0.95, worn));
        soil = mix(soil, soil * vec3f(0.62, 0.6, 0.58), undercut * 0.8);
        let damp_rim = (1.0 - smoothstep(0.0, 0.025, abs(damp_field - 0.62))) * dirt * (1.0 - damp);
        soil = mix(soil, soil * vec3f(0.86, 0.84, 0.83), damp);
        soil = mix(soil, soil * vec3f(0.9, 0.88, 0.86), damp_rim * 0.5);
        // Sun-baked mud on the dry, packed lines crazes into plates; the cracks
        // fade out before they could shimmer.
        let baked = smoothstep(0.55, 0.72, vg_fbm(p * 0.09 + 71.0)) * packed * (1.0 - damp) * dirt;
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
        soil = mix(soil, soil * 0.68, clods * 0.6);
        soil = mix(soil, soil * vec3f(1.14, 1.11, 1.05), berm * 0.75);
        // Ruts: a worn-out tread is only pressed paler and smoother into the
        // dirt; fresh ones cut deeper and darker, with cleat prints across them.
        soil = mix(soil, soil * vec3f(1.06, 1.05, 1.03), tread_bed * (1.0 - smoothstep(0.2, 0.6, fresh)) * 0.7);
        soil = mix(soil, soil * vec3f(0.76, 0.73, 0.7), rut * smoothstep(0.25, 0.85, fresh) * 0.7);
        soil = mix(soil, soil * vec3f(0.8, 0.78, 0.76), cleat * 0.5);
        soil = mix(soil, soil * vec3f(0.8, 0.79, 0.77), edge_shade * 0.55);
        // Sandy creek banks outside the wall, and damp mud by the water.
        soil = mix(soil, vg_linear(0x9d917a) * mix(0.85, 1.12, dirt_detail * 0.5), (1.0 - inside) * 0.35);
        soil = mix(soil, soil * vec3f(0.55, 0.53, 0.5), wet);
    }
    let gravel_tone = vg_linear(0x8e8676) * mix(0.75, 1.2, dirt_detail * 0.5);

    // A few fist-sized stones along the shoulders, each with its own shadow
    // cast away from the sun.
    // Gravelly patches in the loose ground, where the topsoil washed off.
    let gravel_patch = smoothstep(0.58, 0.72, vg_fbm(p * 0.11 + 57.0)) * dirt * (1.0 - packed * 0.6);
    let stone_density = (shoulder * 0.22 + verge * 0.3 + dirt * (1.0 - packed) * 0.05 + gravel_patch * 0.35) * mid * (1.0 - rut);
    let sun_away = -normalize(frame.sun_direction.xz + vec2f(1e-4, 0.0)) * 0.08;
    let stones = vg_scatter(p, stone_density, 0.9, vec2f(0.08, 0.15), aa, 57.0);
    let stone_shadow = vg_scatter(p - sun_away, stone_density, 0.9, vec2f(0.08, 0.15), aa * 2.0, 57.0).x * (1.0 - stones.x);
    // Pebbles lit by their own dome: a dark foot, a pale crown.
    let pebble_crown = smoothstep(0.0, 0.85, clamp(pebbles.y / 0.08, 0.0, 1.0));
    let stone = mix(vg_linear(0x6f6a62), vg_linear(0xa29d90), pebbles.z) * mix(0.6, 1.12, pebble_crown);
    let stone_warm = mix(stone, soil * mix(0.62, 1.12, pebble_crown), 0.72 + pebbles.w * 0.2);

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
    color = mix(color, color * 0.6, stone_shadow * 0.75 * (1.0 - flower_bed));
    // Lit from the dome's height: a dark foot, a pale crown.
    let crown = clamp(stones.y / 0.11, 0.0, 1.0);
    let boulder = mix(soil * 0.85, mix(vg_linear(0x6d685e), vg_linear(0x978f80), stones.z), 0.55) * mix(0.5, 1.12, smoothstep(0.0, 0.8, crown));
    color = mix(color, boulder, stones.x * (1.0 - flower_bed));
    // Weeds on the loose ground the treads leave alone: small rosettes, dark
    // at the heart.
    let weed_density = dirt * (1.0 - packed) * (1.0 - smoothstep(0.0, 0.3, rut + fresh * 0.5)) * 0.3 * mid;
    let weeds = vg_rosette(p, weed_density, 1.25, vec2f(0.07, 0.13), aa, 91.0);
    let weed_leaf = mix(vg_linear(0x4a6428), vg_linear(0x7a8a3e), weeds.z) * mix(0.55, 1.05, smoothstep(0.05, 0.8, weeds.y));
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
        + pebbles.y * 0.6 + stones.y * 0.7 + weeds.x * 0.015 + grit.y * 0.4 + cleat * 0.012 * dirt + blooms.y * 0.3 * (1.0 - dirt)
        + bed_relief * flower_bed + veg_relief * veg_plot
        + edging * 0.03 + crop.a * field
        + clump_height + swell * mix(1.8, 0.6, dirt) + bank * inside;
    let normal = vg_bump(mix(relief, -0.08, puddle), f.world, (*s).normal, f.front_facing);

    (*s).color = color * material.color.rgb * f.tint.rgb;
    (*s).normal = normalize(mix(normal, (*s).normal, puddle));
    (*s).roughness = mix(mix(mix(mix(0.95, mix(0.92, 0.82, packed), dirt), 0.68, wet * 0.6), 0.55, damp * 0.8), 0.05, puddle);
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
// z which side of its trunk (-1 or 1), w a random tone. `cap` is the height
// above which trees thin out; `aa` the pixel footprint in metres.
fn vg_conifer_rows(u: f32, v: f32, seed: f32, cap: f32, aa: f32) -> vec4f {
    let width = 2.6;
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
            // Thinning above the cap: each tree stands or not as a whole.
            let room = smoothstep(cap + 3.0, cap - 4.0, base);
            if (pick.y > room * 0.97 + 0.03 * (1.0 - step(cap + 6.0, base))) {
                continue;
            }
            // Gaps in the stand let the dark forest floor through.
            if (vg_hash(vec2f(c * 2.9, r * 1.3) + seed + 9.0) < 0.14) {
                continue;
            }
            let h = tallest * mix(0.5, 1.0, fract(pick.y * 7.13)) * mix(0.7, 1.0, room);
            let foot = base + (tone - 0.5) * 1.3;
            let t = (v - foot) / h;
            if (t < 0.0 || t > 1.0) {
                continue;
            }
            let axis = shift + (c + 0.25 + pick.x * 0.5) * width;
            let offset = u - axis;
            // Stylised tiers: each whorl of branches flares out and tucks in.
            let tiers = fract(t * 4.0 + tone);
            let half = h * 0.3 * (1.0 - t) * mix(0.72, 1.0, tiers) + 0.05;
            let cover = 1.0 - smoothstep(half - aa, half + aa, abs(offset));
            if (cover > hit.x) {
                hit = vec4f(cover, t, select(-1.0, 1.0, offset > 0.0), tone);
            }
        }
        if (hit.x > 0.99) {
            break;
        }
    }
    return hit;
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
    let ribs_x = vg_ridged(vec2f(p.z * 0.085, p.y * 0.035)) * 0.75 + vg_ridged(vec2f(p.z * 0.23, p.y * 0.12) + 3.7) * 0.25 * fine;
    let ribs_z = vg_ridged(vec2f(p.x * 0.085, p.y * 0.035) + 9.1) * 0.75 + vg_ridged(vec2f(p.x * 0.23, p.y * 0.12) + 5.3) * 0.25 * fine;
    let outcrop = vg_ridged(p.xz * 0.06 + 1.3) * 0.75 + vg_ridged(p.xz * 0.17) * 0.25 * fine;
    let crag = ribs_x * weights.x + outcrop * weights.y + ribs_z * weights.z;
    let relief_fade = 1.0 - smoothstep(1.5, 4.0, footprint);
    // A touch of the mesh's own planes keeps the rock crisp without showing
    // the grid as facets.
    var flat_normal = normalize(cross(dpdx(p), dpdy(p)));
    flat_normal = flat_normal * sign(dot(flat_normal, smooth_normal));
    let planar = normalize(mix(smooth_normal, flat_normal, 0.25));
    let crag_normal = vg_bump(crag * 1.6 * relief_fade, p, planar, f.front_facing);

    // Bands, broken by the relief and broad noise so no contour line shows.
    let band_noise = vg_fbm(p.xz * 0.016 + 30.0);
    let treeline = 22.0 + band_noise * 15.0;
    let snowline = 41.0 + band_noise * 12.0;
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
    rock = mix(rock, rock * 1.18, ledge * 0.6);
    rock = mix(rock, rock * vec3f(0.62, 0.66, 0.74), overhang * 0.55);
    rock = rock * mix(0.94, 1.04, bedding * fine);
    rock = mix(rock, rock * vec3f(0.62, 0.7, 0.82), (1.0 - smoothstep(0.08, 0.4, crag)) * 0.6);
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
    let rows_a = vg_conifer_rows(dot(p.xz, vec2f(-sin(angle0), cos(angle0))), p.y, 11.0 + (sector % 4.0) * 7.0, cap, pixel);
    let rows_b = vg_conifer_rows(dot(p.xz, vec2f(-sin(angle1), cos(angle1))), p.y, 11.0 + ((sector + 1.0) % 4.0) * 7.0, cap, pixel);
    let gate_a = 1.0 - smoothstep(0.55, 0.8, turn);
    let gate_b = smoothstep(0.2, 0.45, turn);
    let b_wins = rows_b.x * gate_b > rows_a.x * gate_a;
    var trees = select(rows_a, rows_b, b_wins);
    trees.x = max(rows_a.x * gate_a, rows_b.x * gate_b);
    let along_angle = select(angle0, angle1, b_wins);
    let along = vec3f(-sin(along_angle), 0.0, cos(along_angle));
    // Far away the single trees blur into the canopy's average.
    let distant = smoothstep(0.3, 0.75, footprint);
    let below = smoothstep(-2.0, 4.0, treeline - p.y);
    let stand = slope_ok * (1.0 - chute * 0.9);
    let stands = vg_fbm(p.xz * 0.022 + 50.0);
    let needles_dark = mix(vg_linear(0x173322), vg_linear(0x22422a), smoothstep(0.3, 0.7, stands));
    let needles_lit = mix(vg_linear(0x2f5a33), vg_linear(0x4a7238), smoothstep(0.35, 0.75, stands));
    let floor_shade = vg_linear(0x0f1d14);
    let tree_tone = mix(0.85, 1.12, trees.w);
    let tree_color = mix(needles_dark, needles_lit, 0.35 + trees.y * 0.4) * tree_tone;
    let near_forest = mix(floor_shade, tree_color, trees.x);
    let far_forest = mix(floor_shade, mix(needles_dark, needles_lit, 0.5), 0.75);
    let canopy = mix(near_forest, far_forest, distant);
    let forest = stand * mix(trees.x, below * 0.85, distant);
    // Below the treeline the ground between trees is forest floor, above it
    // bare rock or alpine grass shows between them.
    let floor_amount = stand * below * (1.0 - distant);
    var color = mix(rock, floor_shade * mix(0.9, 1.2, tone), floor_amount * 0.9);
    color = mix(color, canopy, forest);
    // Alpine grass above the wood and down the chutes.
    let pasture = max(smoothstep(-6.0, 0.0, treeline - p.y) * slope_ok, chute * slope_ok * below)
        * (1.0 - forest);
    color = mix(color, vg_linear(0x5f7f45) * mix(0.85, 1.1, tone), pasture * 0.8);
    // Each tree is lit as a cone: the half toward the sun bright, the far half
    // in shade, so the rows read by their light like the near pines.
    let outward = normalize(vec3f(smooth_normal.x, 0.0, smooth_normal.z) + 1e-4);
    let cone = normalize(outward * 0.55 + vec3f(0.0, 0.45, 0.0) + along * trees.z * 0.7);
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

    (*s).color = color;
    (*s).normal = normalize(mix(normal, smooth_normal, meadow * 0.7));
    (*s).roughness = mix(0.94, 0.6, snow);
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    if material.params[0].x < 0.5 {
        // Tufts light like the turf they grow from from either side of a blade:
        // a back face keeps its upward normal instead of turning black.
        (*s).normal = (*s).normal * select(1.0, -1.0, (*s).normal.y < 0.0);
    } else if material.params[0].x > 1.5 {
        village_ranges(s, f);
    } else if material.params[0].x > 0.5 {
        village_ground(s, f);
    }
}

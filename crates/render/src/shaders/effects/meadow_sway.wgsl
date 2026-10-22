// Pine Village meadow (`effects_scenery::MEADOW_SWAY`). One effect serves three
// village materials, told apart by params[0].x (a uniform, so the branches stay
// in uniform control flow):
//
// 0: the meadow tufts (`village_vegetation.rs`) sway in the wind after the
//    instance transform (tufts are instanced at world placements, so this is
//    world space), by their height above their own base. instance_data.xy is the
//    tuft's wind origin (world x/z).
// 1: the village ground (`ground_surfaces.rs`): lush grass worn into dirt roads
//    with wheel ruts, pebbles, wildflowers and ragged grass edges, cottage
//    gardens and the farmland outside the wall, all in world space. The map is
//    the grass tile at the mesh UVs (world / 8 m), extra texture 0 the
//    packed-dirt tile, extra texture 1 the baked ground plan (`village_ground.rs`,
//    linear, unflipped). params[0] = (1, arena half size, unused,
//    unused); params[1].xyz the north-south roads' centre x; params[2].xyz the
//    east-west roads' centre z; params[3] = half widths (north-south outer,
//    north-south centre, east-west outer, east-west centre).
// 2: the mountain ranges (`village_landscape.rs`): the slopes break into broad
//    stylised rock faces, a dark conifer canopy below the treeline and crisp
//    snowfields on the gentler faces above the snowline, in world space; the
//    low meadows keep their vertex colours.
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

// The baked ground plan (`village_ground.rs`): the arena half and the valley half,
// sampled at a world position. Coordinates stay half a texel inside each half.
const VG_ARENA_PLAN = 64.0;
const VG_VALLEY_PLAN = 170.0;
const VG_PLAN_TEXEL = 1.0 / 1024.0;

fn vg_arena_uv(p: vec2f) -> vec2f {
    let uv = p / VG_ARENA_PLAN * 0.5 + 0.5;
    return vec2f(clamp(uv.x * 0.5, VG_PLAN_TEXEL, 0.5 - VG_PLAN_TEXEL), clamp(uv.y, VG_PLAN_TEXEL * 2.0, 1.0 - VG_PLAN_TEXEL * 2.0));
}

fn vg_valley_uv(p: vec2f) -> vec2f {
    let uv = p / VG_VALLEY_PLAN * 0.5 + 0.5;
    return vec2f(clamp(0.5 + uv.x * 0.5, 0.5 + VG_PLAN_TEXEL, 1.0 - VG_PLAN_TEXEL), clamp(uv.y, VG_PLAN_TEXEL * 2.0, 1.0 - VG_PLAN_TEXEL * 2.0));
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

    // ---- The baked plan: inside the wall the arena half, outside the valley.
    let arena_plan = textureSample(extra_texture1, extra_sampler1, vg_arena_uv(p));
    let valley_plan = textureSample(extra_texture1, extra_sampler1, vg_valley_uv(p));
    let valley_texel = vec2i(clamp(vg_valley_uv(p) * vec2f(1024.0, 512.0), vec2f(512.0, 0.0), vec2f(1023.0, 511.0)));
    let field_code = textureLoad(extra_texture1, valley_texel, 0).b * 255.0;
    let inside = 1.0 - smoothstep(extent + 2.0, extent + 3.0, max(abs(p.x), abs(p.y)));
    let worn = mix(valley_plan.r, arena_plan.r, inside);
    let trampled = arena_plan.g * inside;
    let garden = arena_plan.b * inside;
    let open = mix(1.0, arena_plan.a, inside);
    let field = valley_plan.g * (1.0 - inside);
    let wet_bank = (1.0 - valley_plan.a) * (1.0 - inside);

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

    // ---- Road strips for the ruts and the crown.
    let along = vg_strip(p.x, p.y, material.params[1].xyz, widths.x, widths.y, extent);
    let across = vg_strip(p.y, p.x, material.params[2].xyz, widths.z, widths.w, extent);
    // Where the plan's dirt starts, frayed at pixel scale. Some stretches of edge
    // crumble widely into the grass, others stay crisp.
    let crumble = mix(0.1, 0.42, smoothstep(0.3, 0.75, vg_noise(p * 0.06 + 5.0)));
    let clumps = vg_noise(p * 3.3 + 4.0) * 0.65 + vg_noise(p * 7.9) * 0.35;
    let reveal = worn + (vg_fbm(p * 0.8) - 0.5) * crumble * 2.0 + (clumps - 0.5) * crumble;
    let reveal_aa = max(fwidth(reveal), 0.02);
    // Trampled ground goes bald in patches; the noise can lift it at most 0.35,
    // so ground trampled less than a quarter never goes bald.
    var bald = 0.0;
    if trampled > 0.25 {
        bald = smoothstep(0.6, 1.0, trampled + (vg_fbm(p * 0.5 + 9.0) - 0.5) * 0.7) * 0.6;
    }
    let dirt = smoothstep(0.5 - reveal_aa, 0.5 + reveal_aa, max(reveal, bald + (clumps - 0.5) * 0.3));
    let wear = smoothstep(0.0, 0.9, max(worn, trampled * 0.85));
    // A gravel drip line along the foot of the perimeter wall (none past 1.35 m,
    // where even the noise's 0.25 m reach leaves it at full distance).
    let wall_gap = extent - max(abs(p.x), abs(p.y));
    var verge = 0.0;
    if wall_gap >= -0.5 && wall_gap < 1.35 {
        verge = 1.0 - smoothstep(0.35, 1.1, wall_gap + (vg_noise(p * 1.7) - 0.5) * 0.5);
    }

    // ---- Wheel ruts along both road families, softened where they cross.
    let in_along = smoothstep(-0.4, 0.6, along.x);
    let in_across = smoothstep(-0.4, 0.6, across.x);
    let crossing = in_along * in_across;
    var ruts_along = vec2f(0.0);
    var ruts_across = vec2f(0.0);
    if inside > 0.0 && in_along > 0.0 {
        ruts_along = vg_ruts(along.y, p.y, along.z, 1.0 + along.z) * in_along;
    }
    if inside > 0.0 && in_across > 0.0 {
        ruts_across = vg_ruts(across.y, p.x, across.z, 11.0 + across.z) * in_across;
    }
    let ruts = max(ruts_along, ruts_across) * (1.0 - crossing * 0.5) * inside;
    let rut = ruts.x * dirt;
    let berm = ruts.y * dirt;
    // The crown between the lanes stays pale and hard; the shoulders darker.
    let lane = select(vec2f(across.y / across.z, p.x), vec2f(along.y / along.z, p.y), along.x > across.x);
    let crown = (1.0 - smoothstep(0.1, 0.75, abs(lane.x))) * max(in_along, in_across) * inside;
    // Rain lingers in the deepest stretches of rut.
    var wet_patch = 0.0;
    if rut > 0.35 {
        wet_patch = smoothstep(0.7, 0.82, vg_fbm(p * 0.21 + 40.0)) * smoothstep(0.5, 0.7, vg_noise(p * 0.9 + 3.0));
    }
    let wet = max(smoothstep(0.35, 0.7, rut) * wet_patch, wet_bank * dirt);
    let puddle = smoothstep(0.6, 0.9, rut) * smoothstep(0.25, 0.6, wet_patch);

    // ---- Grass: lush greens, sunlit and shaded drifts, dry and flattened where
    // it is walked; and wildflowers in meadow drifts (daisies with gold hearts,
    // buttercups, violets and campion). Neither shows on bare dirt.
    var grass = vec3f(0.0);
    var blooms = vec4f(0.0);
    var bloom_color = vec3f(0.0);
    if dirt < 1.0 {
        let macro_tone = vg_fbm(p * 0.045 + 3.0);
        let meso = vg_fbm(p * 0.21 - 11.0);
        grass = mix(vg_linear(0x4a7d2e), vg_linear(0x7f9d40), smoothstep(0.3, 0.75, macro_tone));
        grass = mix(grass, vg_linear(0x3c6e30), smoothstep(0.5, 0.8, meso) * 0.45);
        grass = mix(grass, vg_linear(0x9c9d5a), wear * 0.55);
        let blades = mix(0.74, 1.26, clamp((grass_detail - 0.55) * 0.9, 0.0, 1.0));
        grass = grass * blades;
        // Clover and dark clumps break the carpet.
        let clover = smoothstep(0.66, 0.82, vg_noise(p * 1.3 + 2.0)) * (1.0 - wear);
        grass = mix(grass, vg_linear(0x386b2b) * blades, clover * 0.35);
        // Flower drifts tint the lawn even where single blooms are too small to draw.
        let flowers = vg_fbm(p * 0.08 + 61.0);
        let drift = smoothstep(0.5, 0.78, flowers) * (1.0 - wear) * (1.0 - fine);
        if drift > 0.0 {
            let drift_tint = mix(vg_linear(0xe9e6c8), vg_linear(0xe8c64a), smoothstep(0.4, 0.6, vg_noise(p * 0.05 + 7.0)));
            grass = mix(grass, drift_tint, drift * 0.16);
        }
        // Keep the bank tint and the far hills' vertex shading.
        grass = grass * f.vertex_color.rgb;

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

    // ---- Pebbles: thick along the road shoulders and the verge, sparse elsewhere.
    let shoulder = smoothstep(0.15, 0.45, wear) * (1.0 - smoothstep(0.7, 0.95, wear));
    let pebble_density = (0.02 + shoulder * 0.3 + dirt * 0.05 + verge * 0.6 + wet_bank * 0.4) * fine;
    let pebbles = vg_scatter(p, pebble_density, 2.4, vec2f(0.04, 0.11), aa, 3.0);
    let grit = vg_scatter(p, (0.12 + dirt * 0.25) * fine, 6.5, vec2f(0.014, 0.03), aa, 9.0);

    // ---- Dirt: a muted earth with a pale, packed crown between the wheel lanes,
    // darker shoulders, combed streaks, damp ruts and loose berms. Pebbles take
    // their warmth from it, so it is shaded wherever either shows.
    var soil = vec3f(0.0);
    var clods = 0.0;
    if dirt > 0.0 || pebbles.x > 0.0 {
        // Streaks combed along the direction of travel.
        let streaks = vg_noise(vec2f(lane.x * 7.0, lane.y * 0.11)) * 0.6 + vg_noise(vec2f(lane.x * 19.0, lane.y * 0.3)) * 0.4;
        let dirt_tex = mix(vec3f(dirt_luma), dirt_near.rgb, 0.45);
        soil = mix(vg_linear(0x7d6650), vg_linear(0x9c8466), smoothstep(0.3, 0.8, vg_fbm(p * 0.13 + 21.0)) * 0.6 + streaks * 0.4);
        soil = mix(soil, vg_linear(0xb19c7e), crown * 0.55);
        soil = soil * mix(0.66, 1.34, clamp((dirt_detail - 0.55) * 1.1, 0.0, 1.0))
            * (dirt_tex / max(dirt_luma, 0.05) * 0.3 + 0.7);
        // The worn margin, where grass roots hold damp soil.
        soil = mix(soil * vec3f(0.8, 0.8, 0.78), soil, smoothstep(0.55, 0.95, worn));
        // Damp hollows and dry, sun-bleached crusts.
        let damp = vg_fbm(p * 0.6 + 7.0);
        soil = soil * mix(vec3f(0.86, 0.85, 0.83), vec3f(1.08, 1.06, 1.03), smoothstep(0.3, 0.72, damp));
        // Dark clods of turned earth.
        clods = smoothstep(0.72, 0.8, vg_noise(p * 5.3 + 1.7)) * fine;
        soil = mix(soil, soil * 0.68, clods * 0.6);
        soil = mix(soil, soil * vec3f(1.16, 1.12, 1.06), berm * 0.7);
        soil = mix(soil, soil * vec3f(0.58, 0.55, 0.53), rut * 0.85);
        // Sandy creek banks outside the wall, and damp mud by the water.
        soil = mix(soil, vg_linear(0x9d917a) * mix(0.85, 1.12, dirt_detail * 0.5), (1.0 - inside) * 0.35);
        soil = mix(soil, soil * vec3f(0.55, 0.53, 0.5), wet);
    }
    let gravel_tone = vg_linear(0x8e8676) * mix(0.75, 1.2, dirt_detail * 0.5);
    let stone = mix(vg_linear(0x6f6a62), vg_linear(0xa29d90), pebbles.z);
    let stone_warm = mix(stone, soil * 1.15, 0.25 + pebbles.w * 0.4);

    // ---- Cottage gardens: flower beds on dark mulch, vegetable rows, and a
    // plank edging where a bed meets the lawn. None below a garden level of 0.08.
    let flower_bed = smoothstep(0.72, 0.9, garden);
    let veg_plot = smoothstep(0.3, 0.42, garden) * (1.0 - smoothstep(0.6, 0.72, garden));
    let edging = smoothstep(0.08, 0.16, garden) * (1.0 - smoothstep(0.24, 0.32, garden)) * fine;
    var bed = vec3f(0.0);
    var bed_relief = 0.0;
    var veg = vec3f(0.0);
    var veg_relief = 0.0;
    var plank = vec3f(0.0);
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
    // Puddles: still water mirroring the pale sky, with a dark muddy floor.
    if puddle > 0.0 {
        let sky = mix(vg_linear(0x5d6f78), vg_linear(0xa9bcc4), vg_noise(p * 0.4));
        color = mix(color, mix(color * 0.4, sky, 0.55), puddle);
    }
    // Contact shade at the foot of walls, cottages, timber and trunks.
    color = color * mix(0.52, 1.0, open);

    let relief = mix(grass_detail * 0.02, dirt_detail * 0.02 - rut * 0.06 + berm * 0.015 + clods * 0.012 + crown * 0.01, dirt)
        + pebbles.y * 0.6 + grit.y * 0.4 + blooms.y * 0.3 * (1.0 - dirt)
        + bed_relief * flower_bed + veg_relief * veg_plot
        + edging * 0.03 + crop.a * field;
    let normal = vg_bump(mix(relief, -0.07, puddle), f.world, (*s).normal, f.front_facing);

    (*s).color = color * material.color.rgb * f.tint.rgb;
    (*s).normal = normalize(mix(normal, (*s).normal, puddle));
    (*s).roughness = mix(mix(mix(0.95, 0.9, dirt), 0.7, wet * 0.6), 0.06, puddle);
    (*s).opacity = material.color.a * f.tint.a;
}

// Cellular rock facets: the nearest jittered cell centre to `q` (cell units),
// its id and how far `q` is from the cell's edge (for a crisp crease line).
fn vg_facet(q: vec2f) -> vec3f {
    let i = floor(q);
    let f = fract(q);
    var best = 8.0;
    var second = 8.0;
    var id = vec2f(0.0);
    for (var j = -1; j <= 1; j++) {
        for (var k = -1; k <= 1; k++) {
            let g = vec2f(f32(k), f32(j));
            let r = g + vg_hash2(i + g) * 0.8 + 0.1 - f;
            let d = dot(r, r);
            if (d < best) {
                second = best;
                best = d;
                id = i + g;
            } else if (d < second) {
                second = d;
            }
        }
    }
    return vec3f(id, sqrt(second) - sqrt(best));
}

fn village_ranges(s: ptr<function, Surface>, f: EffectFragment) {
    let p = f.world;
    let base = (*s).color;
    let footprint = max(length(dpdx(p)), length(dpdy(p)));
    let smooth_normal = (*s).normal;
    // Stylised rock: the slopes break into broad planar faces, each tilted its
    // own way, so the light falls on them in clean value groups. Cells lie in the
    // plane the slope faces most (gentle ground from above, cliffs from the
    // side), so steep faces break into upright slabs instead of smears.
    let steepness = 1.0 - smooth_normal.y;
    let side_on = abs(smooth_normal.x) > abs(smooth_normal.z);
    let wall_plane = select(vec2f(p.x, p.y * 1.4), vec2f(p.z, p.y * 1.4), side_on);
    let q = select(p.xz, wall_plane, smooth_normal.y < 0.62);
    let face = vg_facet(q / 26.0 + vec2f(0.0, vg_noise(q * 0.02) * 0.6));
    let tilt = (vg_hash2(face.xy + 3.1) - 0.5) * (0.5 + steepness * 0.6);
    let facet_normal = normalize(smooth_normal + vec3f(tilt.x, 0.0, tilt.y));
    let up = facet_normal.y;
    let facet_value = vg_hash(face.xy + 1.7);

    // Bands, broken by the faces so no contour line shows.
    let band_noise = vg_fbm(p.xz * 0.018 + 30.0);
    let treeline = 24.0 + band_noise * 14.0 + (facet_value - 0.5) * 6.0;
    let forest = (1.0 - smoothstep(treeline - 1.0, treeline + 1.0, p.y)) * smoothstep(0.4, 0.52, up);
    let snowline = 44.0 + band_noise * 10.0 + (facet_value - 0.5) * 8.0;
    let high = smoothstep(snowline - 0.6, snowline + 0.6, p.y);
    let meadow = 1.0 - smoothstep(5.0, 11.0, p.y + (band_noise - 0.5) * 6.0);

    // Rock in a few clean value groups, greener and darker toward its foot.
    var rock = mix(vg_linear(0x77766e), vg_linear(0x9f9a8a), facet_value);
    rock = mix(rock, vg_linear(0x66705f), (1.0 - smoothstep(16.0, 34.0, p.y)) * 0.5);
    // Faint scree streaks down the fall line, sharp only up close.
    let scree = vg_noise(vec2f(dot(p.xz, vec2f(smooth_normal.z, -smooth_normal.x)) * 0.35, p.y * 0.04));
    rock = rock * mix(1.0, mix(0.9, 1.06, scree), 1.0 - smoothstep(1.0, 3.0, footprint));

    // Forest: a dark conifer canopy, mottled by crowns and lighter clearings.
    let crowns = vg_noise(p.xz * 0.55 + 12.0) * 0.55 + vg_noise(p.xz * 1.6) * 0.45;
    let clearings = smoothstep(0.66, 0.74, vg_fbm(p.xz * 0.035 + 50.0));
    let canopy = mix(vg_linear(0x1e3b27), vg_linear(0x3c6233), smoothstep(0.3, 0.8, crowns))
        * mix(0.92, 1.06, facet_value);
    var color = mix(rock, canopy, forest * (1.0 - clearings));
    // Snow settles on the gentler faces above the snowline in crisp fields, and
    // dusts the steeper ones.
    let settles = smoothstep(0.5, 0.56, up) * high;
    let dusting = high * smoothstep(0.3, 0.45, up) * smoothstep(0.6, 0.75, vg_noise(vec2f(q.x * 0.12, q.y * 0.3))) * 0.5;
    let snow = clamp(max(settles, dusting), 0.0, 1.0);
    color = mix(color, vg_linear(0xf1f4f4) * mix(0.88, 1.0, up), snow);
    // Low meadows keep their vertex colour, mottled.
    color = mix(color, base * mix(0.9, 1.08, vg_fbm(p.xz * 0.08)), meadow * 0.8);

    (*s).color = color;
    (*s).normal = normalize(mix(facet_normal, smooth_normal, meadow * 0.7));
    (*s).roughness = mix(0.94, 0.6, snow);
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    if material.params[0].x > 1.5 {
        village_ranges(s, f);
    } else if material.params[0].x > 0.5 {
        village_ground(s, f);
    }
}

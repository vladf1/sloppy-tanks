// Baked soil with world-space grit (`quarry-terrain.ts` soilMaterial): the map is
// the soil bake at the mesh UVs; extra texture 0 is the grit. The bake's alpha
// carries the ground kind: 128 plain fine soil, above it how gravelly the ground
// is, below it how far damp mud has dried and cracked into plates.
//
// On top of the bake, in world space: loose stones on two lattices, scattered by
// the gravel amount; mud cracks; and relief from the bake itself, so ruts,
// windrows and ripples catch the low sun.

// Lattice spacing of the cobbles, the stones and the fine grit, and of the mud
// plates, metres.
const SOIL_COBBLE_CELL = 0.9;
const SOIL_STONE_CELL = 0.42;
const SOIL_GRIT_CELL = 0.15;
const SOIL_PLATE_CELL = 0.55;
// How far loose stones' tints settle toward the soil's own colour (1 all the way).
const SOIL_STONE_DUST = 0.4;
// Size of the patches loose stone gathers in, metres.
const SOIL_SPILL_CELL = 5.5;
// Relief of the bake's own shading (ruts, windrows, ripples), as bump scale.
const SOIL_BAKE_RELIEF = 2.6;
// The floor's broad basins, swells and hollows: wavelengths and heights,
// metres. Shading only: the floor stays the plane the tanks drive on, but the
// low sun picks out a worked, wind-shaped ground.
const SOIL_BASIN_SPAN = 46.0;
const SOIL_BASIN_HEIGHT = 2.2;
const SOIL_SWELL_SPAN = 17.0;
const SOIL_SWELL_HEIGHT = 1.1;
const SOIL_HUMMOCK_SPAN = 6.5;
const SOIL_HUMMOCK_HEIGHT = 0.24;
// Sand drifts across the +x wind: crest spacing along it, crest height, the
// share of each wavelength the steep lee face takes, how far the crests bow
// and the size of the fields they gather in, metres.
const SOIL_DRIFT_SPACING = 6.0;
const SOIL_DRIFT_HEIGHT = 0.2;
const SOIL_DRIFT_LEE = 0.34;
const SOIL_DRIFT_BOW = 8.0;
const SOIL_DRIFT_FIELD = 19.0;
// Length of one drift tongue along its crest, metres.
const SOIL_DRIFT_TONGUE = 7.0;

struct SoilRelief {
    // Height (metres) of the broad relief and its slope (d/dx, d/dz).
    height: f32,
    slope: vec2f,
    // How far up a drift the point lies (0 trough, 1 crest), and how much drift
    // sand covers it.
    drift: f32,
    drift_field: f32,
}

// The floor's shaded relief at x/z.
fn soil_relief(p: vec2f) -> SoilRelief {
    let basin = quarry_value_grad(p / SOIL_BASIN_SPAN + vec2f(-4.3, 1.9));
    let swell = quarry_value_grad(p / SOIL_SWELL_SPAN + vec2f(3.1, 7.7));
    let hummock = quarry_value_grad(p / SOIL_HUMMOCK_SPAN + vec2f(11.3, 2.9));
    var r: SoilRelief;
    r.height = (basin.x - 0.5) * SOIL_BASIN_HEIGHT + (swell.x - 0.5) * SOIL_SWELL_HEIGHT
        + (hummock.x - 0.5) * SOIL_HUMMOCK_HEIGHT;
    r.slope = basin.yz * (SOIL_BASIN_HEIGHT / SOIL_BASIN_SPAN)
        + swell.yz * (SOIL_SWELL_HEIGHT / SOIL_SWELL_SPAN)
        + hummock.yz * (SOIL_HUMMOCK_HEIGHT / SOIL_HUMMOCK_SPAN);
    // Drifts: crests run across the wind, bowing and wandering as they go, with
    // a long gentle back up from the windward side and a short steep slip face.
    let bow_q = vec2f(p.y / 13.0 + 2.3, p.x / 37.0 - 1.9);
    let bow = quarry_value_grad(bow_q);
    let u = (p.x + (bow.x - 0.5) * SOIL_DRIFT_BOW) / SOIL_DRIFT_SPACING;
    let du = vec2f(1.0 + bow.z * SOIL_DRIFT_BOW / 37.0, bow.y * SOIL_DRIFT_BOW / 13.0) / SOIL_DRIFT_SPACING;
    let phase = fract(u);
    let back = 1.0 - SOIL_DRIFT_LEE;
    let on_lee = phase > back;
    let t = select(phase / back, (1.0 - phase) / SOIL_DRIFT_LEE, on_lee);
    let rise = t * t * (3.0 - 2.0 * t);
    let rate = 6.0 * t * (1.0 - t) * select(1.0 / back, -1.0 / SOIL_DRIFT_LEE, on_lee);
    // Fields of drift sand come and go; each crest has its own size.
    let field = quarry_smoothstep(0.5, 0.8, quarry_value(p / SOIL_DRIFT_FIELD + vec2f(7.1, -3.3)));
    // Crests break into tongues that swell and die out along their length.
    let tongue = quarry_smoothstep(0.3, 0.75, quarry_value(vec2f(floor(u) * 3.7, p.y / SOIL_DRIFT_TONGUE)));
    let crest_size = (0.5 + 0.7 * quarry_hash(vec2f(floor(u), 5.0)).x) * tongue;
    let amount = SOIL_DRIFT_HEIGHT * field * crest_size;
    r.height += rise * amount;
    r.slope += du * (rate * amount);
    r.drift = rise;
    r.drift_field = field;
    return r;
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let map_uv = f.uv * material.map_transform.xy + material.map_transform.zw;
    let baked = textureSample(map_texture, map_sampler, map_uv);
    let footprint = quarry_footprint(f.world);
    let stony = clamp((baked.a - 0.5) * 2.0, 0.0, 1.0);
    let dried = clamp((0.5 - baked.a) * 2.0, 0.0, 1.0);
    let p = f.world.xz;
    let near = dot(textureSample(extra_texture0, extra_sampler0, p / 3.3).rgb, LUMA);
    let turned = vec2f(p.x * 0.6 + p.y * 0.8, p.y * 0.6 - p.x * 0.8) / 11.7;
    let far = textureSample(extra_texture0, extra_sampler0, turned);
    let detail = near * (dot(far.rgb, LUMA) + 0.327) / (0.327 * 0.327 * 2.0);
    // Stones gather where the bake is gravelly and thin out on fine soil, and
    // lie in drifts and spills rather than an even sprinkle: a few metres of
    // ground hold most of them, the sand between almost none.
    // Level ground only: the relief fades out up banks, spoil and talus.
    let level = quarry_smoothstep(0.92, 0.99, (*s).normal.y);
    let relief = soil_relief(p);
    let hollow = quarry_smoothstep(-0.2, -1.4, relief.height) * level;
    let crest = quarry_smoothstep(0.3, 1.4, relief.height) * level;
    let drift_sand = relief.drift_field * level;
    // Stones wash into the hollows and the ruts; the crests are winnowed sand.
    let gather = quarry_value(p / SOIL_SPILL_CELL) * 0.65 + quarry_value(p / (SOIL_SPILL_CELL * 0.37) + 7.0) * 0.35;
    // Drift sand buries the stones; they lie in the deflated ground between.
    let spill = quarry_smoothstep(0.54, 0.66, gather + stony * 0.25 + hollow * 0.3 - crest * 0.25 - drift_sand * 0.45);
    let stones = quarry_pebbles(p, SOIL_STONE_CELL, (stony * stony * 0.3 + 0.05) * spill * 1.8, footprint, 3.0, 0.75);
    let grit = quarry_pebbles(p, SOIL_GRIT_CELL, (0.03 + stony * 0.4) * mix(0.4, 1.4, spill), footprint, 11.0, 0.6);
    let cobbles = quarry_pebbles(p, SOIL_COBBLE_CELL, (0.004 + stony * 0.1) * spill * 1.8, footprint, 23.0, 0.85);
    // Mud plates only where the bake dried some; their crack depth joins the relief.
    var cracks = vec2f(0.0);
    if dried > 0.02 {
        // Toward the edge of a dried patch the plates grow and the cracks thin
        // out to hairlines, so the crazing feathers into the sand.
        let cell = SOIL_PLATE_CELL * mix(1.6, 1.0, quarry_smoothstep(0.1, 0.6, dried));
        cracks = quarry_cracks(p, cell, mix(0.35, 1.0, quarry_smoothstep(0.15, 0.7, dried)), footprint).xy
            * quarry_smoothstep(0.02, 0.55, dried);
    }
    let bake_height = dot(baked.rgb, LUMA);
    let height = near * mix(0.35, 1.6, stony) + bake_height * SOIL_BAKE_RELIEF
        - cracks.x * 0.04 + cracks.y * 0.03;
    var normal = derivative_bump(height, 1.0, f.world, (*s).normal, f.front_facing);
    normal = normalize(normal - vec3f(relief.slope.x, 0.0, relief.slope.y) * level);
    let lean = (cobbles.lean + stones.lean + grit.lean * 0.7) * 0.6;
    normal = normalize(normal + vec3f(lean.x, 0.0, lean.y));
    var color = baked.rgb * mix(1.0, detail, mix(0.55, 1.5, stony));
    // Hollows hold damp, darker fines; crests are bleached, paler sand.
    color *= mix(1.0, 0.86, hollow) * mix(1.0, 1.07, crest);
    color = mix(color, color * vec3f(0.96, 0.93, 0.91), hollow * 0.6);
    // Drift sand is finer and paler than the worked ground, palest on the crests;
    // the slip faces hold a cooler, softer sand.
    let drift_tint = mix(vec3f(1.04, 1.02, 0.98), vec3f(1.1, 1.07, 1.01), relief.drift);
    color = mix(color, color * drift_tint, drift_sand * 0.8);
    color *= 1.0 - (cobbles.shadow * 0.36 + stones.shadow * 0.3 + grit.shadow * 0.2);
    color *= 1.0 - (cobbles.contact * 0.12 + stones.contact * 0.1 + grit.contact * 0.06);
    // Stones are the floor's own rock under its dust: their tints pulled toward
    // the soil so they sit in its value range, a lit crown settling into a
    // shaded rim where they sink in, never dark chips or chocolate domes.
    color = mix(color, color * mix(cobbles.tint, vec3f(1.0), SOIL_STONE_DUST) * 0.96, cobbles.cover);
    color = mix(color, color * mix(stones.tint, vec3f(1.0), SOIL_STONE_DUST), stones.cover);
    color = mix(color, color * mix(grit.tint, vec3f(1.0), SOIL_STONE_DUST), grit.cover * 0.85);
    color *= 1.0 - (cobbles.rim * 0.2 + stones.rim * 0.18 + grit.rim * 0.08);
    // Dried plates are paler and cooler; the cracks hold shade and moisture.
    color *= 1.0 + cracks.y * 0.12;
    color = mix(color, color * vec3f(0.55, 0.5, 0.46), cracks.x * 0.65);
    (*s).color = color * f.vertex_color.rgb * material.color.rgb * f.tint.rgb;
    (*s).opacity = material.color.a * f.tint.a;
    (*s).normal = normal;
    (*s).roughness = mix((*s).roughness, 0.75, max(max(stones.cover, grit.cover), cobbles.cover) * 0.5);
}

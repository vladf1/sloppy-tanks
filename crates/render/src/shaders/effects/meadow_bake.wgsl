// The village ground's still layers (`village_ground::VILLAGE_GROUND_BAKE`, laid out
// as `vg_bake_uv` reads them), rendered once from the ground material
// (`effects::bakes`) with `meadow_sway.wgsl`'s functions: the lie of the land, the
// meadow's patches and stands, the earth's colours, where the turf gives way, the
// verge, gardens, fields and hedges, weighed as the gameplay camera sees them. What
// a page's texels cannot hold (the turf's hand-scale fraying, treads, pebbles,
// blooms, setts, the tiles' grain, gusts and puddles) stays in `village_ground`.

// The plan at `p` as the ground reads it (`village_ground.rs`).
struct VgPlan {
    // 1 inside the wall, 0 out in the valley.
    inside: f32,
    worn: f32,
    trampled: f32,
    garden: f32,
    // 1 on open ground, lower in the contact shade at the foot of what stands there.
    open: f32,
    field: f32,
    field_code: f32,
    // 1 at the creek's water.
    wet_bank: f32,
    // The arena's tracks: tread proximity, freshness, traffic, 1 - standing water.
    tracks: vec4f,
}

fn vg_plan_at(p: vec2f) -> VgPlan {
    let extent = material.params[0].y;
    var plan: VgPlan;
    let inside = 1.0 - smoothstep(extent + 2.0, extent + 3.0, max(abs(p.x), abs(p.y)));
    let arena = textureSampleLevel(extra_texture1, extra_sampler1, vg_plan_uv(p, VG_ARENA_PLAN, 0.0), 0.0);
    let valley = textureSampleLevel(extra_texture1, extra_sampler1, vg_plan_uv(p, VG_VALLEY_PLAN, 1.0), 0.0);
    let tracks = textureSampleLevel(extra_texture1, extra_sampler1, vg_plan_uv(p, VG_ARENA_PLAN, 2.0), 0.0);
    // The smooth filter only near the middle of the plan's ramp, where the turf's
    // edge is cut, eased into the bilinear one so no seam shows.
    var arena_worn = arena.r;
    let smooth_weight = (1.0 - smoothstep(0.2, 0.28, abs(arena_worn - 0.5))) * inside;
    if smooth_weight > 0.0 {
        arena_worn = mix(arena_worn, vg_arena_worn(p), smooth_weight);
    }
    plan.inside = inside;
    plan.worn = mix(valley.r, arena_worn, inside);
    plan.trampled = arena.g * inside;
    plan.garden = arena.b * inside;
    plan.open = mix(1.0, arena.a, inside);
    plan.field = valley.g * (1.0 - inside);
    let valley_texel = vec2i(clamp(vg_plan_uv(p, VG_VALLEY_PLAN, 1.0) * vec2f(VG_PLAN_WIDTH, VG_PLAN_HEIGHT), vec2f(512.0, 0.0), vec2f(1023.0, 511.0)));
    plan.field_code = textureLoad(extra_texture1, valley_texel, 0).b * 255.0;
    plan.wet_bank = (1.0 - valley.a) * (1.0 - inside);
    plan.tracks = vec4f(tracks.rgb * inside, mix(1.0, tracks.a, inside));
    return plan;
}

// Where the turf gives way to the earth, without the hand-scale tussocks and blades
// the shader frays it with. The plan ramps the dirt over metres
// (`village_ground::EDGE_RAMP`) and already wanders a few metres in and out; here
// the edge frays by a hand or two. How broadly it thins changes along it over tens
// of metres: a clean lip of turf in some stretches, a couple of metres of thinning
// clumps in others, trampled ground the broadest.
struct VgEdge {
    // Positive on the lawn's side of the edge, in plan units.
    lawn_side: f32,
    // How full the turf stands, 0 to 1.
    density: f32,
    // The cover drive: positive on the turf, through 0 where it meets the earth.
    cover: f32,
    // The damp earth along the edge, and the straw-dry tips of its turf.
    root_damp: f32,
    straw: f32,
}

fn vg_edge(p: vec2f, plan: VgPlan) -> VgEdge {
    var edge = VgEdge(0.5 - plan.worn, select(0.0, 1.0, plan.worn < 0.5), 0.0, 0.0, 0.0);
    var breadth = 0.0;
    // The tussocks' height over the earth, about 0.5 with their hand-scale lift at
    // its average.
    var tuft_height = 0.5;
    // Away from an edge the plan is flat (0 on a lawn, 1 on a road) and nothing
    // could reach the edge; the values are the edge's own averages.
    if (plan.worn > 0.004 && plan.worn < 0.996) || plan.trampled > 0.4 {
        let wander = (vg_noise(p * 0.7 + 31.0) - 0.5) * 0.07;
        breadth = max(
            smoothstep(0.34, 0.74, vg_noise(p * 0.035 + 5.0) * 0.8 + vg_noise(p * 0.12 + 2.0) * 0.2),
            smoothstep(0.3, 0.8, plan.trampled) * 0.85,
        );
        edge.lawn_side = 0.5 - plan.worn - wander;
        let spread = mix(0.08, 0.36, breadth);
        edge.density = smoothstep(-spread, spread, edge.lawn_side);
        // Trampled ground goes bald in scuffed patches.
        if plan.trampled > 0.4 {
            edge.density *= 1.0 - smoothstep(0.8, 1.1, plan.trampled + (vg_fbm(p * 0.5 + 9.0) - 0.5) * 0.7) * 0.45;
        }
        tuft_height = vg_noise(p * 0.85 + 29.0) * 0.5 + 0.25;
        edge.root_damp = smoothstep(-0.45, 0.05, edge.lawn_side) * mix(0.35, 0.8, smoothstep(0.3, 0.7, vg_noise(p * 0.08 + 61.0)));
        edge.straw = smoothstep(0.45, 0.8, vg_noise(p * 0.06 + 50.0));
    }
    edge.cover = (edge.density - 0.5) * 1.2 + (tuft_height - 0.5) * mix(0.25, 0.75, breadth);
    return edge;
}

// The earth's berm along each road's shoulder, a metre or two inside the turf,
// where the lanes' traffic pushed it.
fn vg_road_berm(p: vec2f, plan: VgPlan, packed: f32) -> f32 {
    if plan.worn <= 0.42 || plan.worn >= 0.9 || plan.inside <= 0.0 {
        return 0.0;
    }
    return smoothstep(0.42, 0.6, plan.worn) * (1.0 - smoothstep(0.68, 0.9, plan.worn)) * plan.inside * (1.0 - packed * 0.7)
        * smoothstep(0.25, 0.55, vg_noise(p * 0.3 + 61.0));
}

// The village square's paving at `p`: 1 on it out to its kerb.
fn vg_paved(p: vec2f, inside: f32) -> f32 {
    if inside <= 0.0 || abs(p.x) > 22.0 || abs(p.y) > 27.5 {
        return 0.0;
    }
    return 1.0 - smoothstep(-0.06, 0.06, -vg_square(p));
}

// The lie of the land in metres: low swells and hollows a few metres across, the
// banks where lawns stand a little proud of their worn roads, the berms and the
// earth's mounds on the roads.
fn vg_land(p: vec2f) -> f32 {
    let plan = vg_plan_at(p);
    let packed = smoothstep(0.2, 0.85, plan.tracks.b);
    let bank = (1.0 - smoothstep(0.08, 0.95, plan.worn)) * 0.3;
    let mounds = vg_noise(p * 0.55 + 23.0) * 0.16 * (1.0 - packed * 0.5) * smoothstep(0.3, 0.8, plan.worn) * (1.0 - vg_paved(p, plan.inside));
    return vg_swell(p) * 1.25 + bank * plan.inside * 0.6 + mounds + vg_road_berm(p, plan, packed) * 0.08;
}

// The meadow's colour without the tile's blades, the gusts and the far flower tint.
fn vg_bake_grass(p: vec2f, plan: VgPlan, edge: VgEdge, long_grass: f32, wear: f32, texel: f32) -> vec3f {
    let lawn = vg_lawn_tone(p);
    var grass = mix(lawn.rgb, lawn.rgb * vec3f(0.8, 0.92, 0.86), long_grass * 0.75);
    // Walked grass: flattened blades, paler and greyer, in scuffs a stride across
    // where the traffic spills onto the turf, and in only some stretches of the
    // turf's last metres, so no halo rings a lawn.
    let verge_gate = smoothstep(0.42, 0.72, vg_noise(p * 0.05 + 19.0) * 0.75 + vg_noise(p * 0.19 + 4.0) * 0.25);
    let walk_drive = max(plan.trampled * 0.9, smoothstep(0.15, 0.7, plan.worn) * 0.35 * verge_gate);
    var walked = 0.0;
    if walk_drive > 0.0 {
        let scuff = vg_noise(p * 1.1 + 31.0) * 0.6 + 0.2;
        walked = smoothstep(0.35, 0.7, walk_drive + (scuff - 0.5) * 0.8);
        let khaki = vec3f(dot(grass, VG_LUMA)) * vec3f(1.12, 1.02, 0.7) * mix(0.95, 1.1, scuff);
        grass = mix(grass, khaki, walked * 0.4);
    }
    // Clumps a couple of metres across, a shade lighter or darker, and drifts of
    // denser, darker tussocks and sparser, sunnier grazed spots.
    grass = grass * mix(0.9, 1.08, vg_noise(p * 0.55 + 77.0));
    let tussock_field = vg_noise(p * 0.62 + 17.0);
    grass = mix(grass, grass * vec3f(0.84, 0.9, 0.82), smoothstep(0.58, 0.78, tussock_field) * 0.55 * (1.0 - walked));
    grass = mix(grass, grass * vec3f(1.08, 1.06, 0.92), smoothstep(0.4, 0.22, tussock_field) * 0.4 * (1.0 - long_grass));
    // Clover and dark clumps break the carpet.
    let clover = smoothstep(0.66, 0.82, vg_noise(p * 1.3 + 2.0)) * (1.0 - wear) * (1.0 - long_grass);
    grass = mix(grass, vg_linear(0x3d6f2e), clover * 0.1);
    // The turf's last metre: the earth between thinning blades in some stretches,
    // straw-dry tips in others.
    let thin = (1.0 - edge.density) * smoothstep(-0.15, 0.15, edge.cover);
    grass = mix(grass, mix(grass, VG_ROOT_EARTH, 0.45), thin * (1.0 - edge.straw * 0.5) * 0.55);
    grass = mix(grass, vec3f(dot(grass, VG_LUMA)) * vec3f(1.1, 1.0, 0.72), thin * edge.straw * 0.15);
    // The verge: the last metre or two of turf before the earth is grazed short,
    // walked and sun-dried, paler and browner in drifts, so every lawn grades into
    // its road instead of stopping at a line.
    let verge_band = (1.0 - smoothstep(0.02, mix(0.06, 0.4, verge_gate), edge.lawn_side)) * plan.inside * mix(0.2, 1.0, verge_gate);
    if verge_band > 0.0 {
        let verge_mottle = mix(0.45, 1.0, smoothstep(0.25, 0.75, vg_noise(p * 0.7 + 13.0)));
        let verge_tone = mix(vec3f(dot(grass, VG_LUMA)) * vec3f(1.12, 1.04, 0.72), VG_ROOT_EARTH * 2.4, 0.45);
        grass = mix(grass, verge_tone, verge_band * verge_mottle * 0.45);
    }
    // At the foot of what stands on the turf the grass thins, darkens and browns.
    let foot = smoothstep(0.0, 0.45, 1.0 - plan.open) * plan.inside;
    grass = mix(grass, mix(grass * vec3f(0.8, 0.74, 0.6), VG_ROOT_EARTH * 1.7, 0.3), foot * 0.75);
    // Out in the valley the pasture is grazed short in some stretches and left
    // rough in others, bracken browning the crests, parcelled into fields of their
    // own tone by hedgerows.
    let valley = 1.0 - plan.inside;
    if valley > 0.0 {
        let meso = vg_noise(p * 0.15 - 11.0) * 0.62 + vg_noise(p * 0.31 + 6.0) * 0.38;
        let rough = smoothstep(0.42, 0.68, vg_noise(p * 0.045 + 13.0) * 0.8 + meso * 0.2);
        grass = mix(grass, grass * vec3f(0.84, 0.88, 0.8), rough * 0.55 * valley);
        grass = mix(grass, grass * vec3f(0.62, 0.7, 0.58), rough * 0.12 * valley);
        let bracken = smoothstep(0.6, 0.78, vg_noise(p * 0.026 + 70.0) + (meso - 0.5) * 0.25) * valley;
        grass = mix(grass, vg_linear(0x7d6a3e) * mix(0.8, 1.1, meso), bracken * 0.4);
        let lean = vec2f(p.x * 0.96 - p.y * 0.28, p.x * 0.28 + p.y * 0.96);
        let bend = (vec2f(vg_noise(p * 0.011 + 3.0), vg_noise(p * 0.011 + 9.0)) - 0.5) * 22.0;
        let q = (lean + bend) / VG_ENCLOSURE;
        let kind = vg_hash(floor(q) + 41.0);
        let parcel_tone = select(select(vec3f(1.0), vec3f(1.09, 1.06, 0.84), kind > 0.55), vec3f(0.86, 0.92, 0.84), kind > 0.82);
        grass = grass * mix(vec3f(1.0), parcel_tone, valley);
        // Mown fields keep the mower's broad stripes.
        let mown = select(0.0, 1.0, kind > 0.55 && kind <= 0.82);
        grass = grass * mix(1.0, mix(0.95, 1.05, step(0.5, fract(fract(q.x) * 7.0))), mown * valley);
    }
    return grass;
}

// The earth's colour without the tile's grain, the lanes' streaks, damp hollows,
// treads and puddles.
fn vg_bake_earth(p: vec2f, plan: VgPlan, edge: VgEdge, damp_field: f32) -> vec3f {
    let extent = material.params[0].y;
    let widths = material.params[3];
    let packed = smoothstep(0.2, 0.85, plan.tracks.b);
    // Which way the road runs, for the rolled lines along it.
    let along = vg_strip(p.x, p.y, material.params[1].xyz, widths.x, widths.y, extent);
    let across = vg_strip(p.y, p.x, material.params[2].xyz, widths.z, widths.w, extent);
    let lane = select(vec2f(across.y, p.x), vec2f(along.y, p.y), along.x > across.x);
    // Loose, darker and redder where little drives; packed pale and dusty along the
    // busy lines (their streaks at their average).
    let loose = mix(vg_linear(0x6a5442), vg_linear(0x846b53), smoothstep(0.25, 0.8, vg_fbm(p * 0.07 + 21.0)));
    let hard = mix(vg_linear(0x8b7a60), vg_linear(0xa18f72), 0.5);
    var soil = mix(loose, hard, smoothstep(0.0, 1.0, packed) * 0.9);
    // The busiest lines are rolled dark and smooth, with drier, paler ground
    // churned up between them in patches a few metres long.
    let rolled = smoothstep(0.55, 0.95, packed) * smoothstep(0.35, 0.65, vg_noise(vec2f(lane.x * 0.9, lane.y * 0.06) + 13.0));
    soil = mix(soil, soil * vec3f(0.84, 0.82, 0.8), rolled * 0.6);
    // Broad patches tens of metres across: redder loam, greyer dust and sandier ground.
    let region = vg_noise(p * 0.035 + 7.0);
    soil = soil * mix(1.0, mix(0.9, 1.06, region), packed);
    soil = mix(soil, soil * vec3f(1.1, 0.95, 0.82), smoothstep(0.58, 0.85, region) * 0.75);
    soil = mix(soil, soil * vec3f(0.92, 0.95, 1.0), smoothstep(0.42, 0.15, region) * 0.8);
    // Darker, richer loam down in the hollows, paler sun-dried dust over the crests.
    let lie = smoothstep(-0.9, 0.9, vg_swell(p));
    soil = soil * mix(vec3f(0.82, 0.79, 0.76), vec3f(1.1, 1.07, 1.02), lie);
    // Clay shows through in broad drifts, redder where the loam is thin.
    let clay = smoothstep(0.55, 0.78, vg_noise(vec2f(p.x * 0.6 - p.y * 0.8, p.x * 0.8 + p.y * 0.6) * 0.03 + 27.0));
    soil = mix(soil, soil * vec3f(1.1, 0.92, 0.8), clay * 0.5 * (1.0 - packed * 0.5));
    soil = mix(soil, soil * vec3f(0.88, 0.84, 0.8), vg_road_berm(p, plan, packed) * 0.55);
    // Earth mottled at a stride, the hand-scale mottle and lumps at their averages.
    let loam = vg_noise(p * 0.85 + 41.0);
    let stride = vg_noise(vec2f(p.x * 0.6 + p.y * 0.8, p.y * 0.6 - p.x * 0.8) * 0.33 + 3.0);
    soil = soil * mix(0.88, 1.08, smoothstep(0.25, 0.75, loam)) * mix(0.86, 1.08, stride) * 0.99 * 0.98;
    // The dark, damp earth along the turf's edge, in the stretches that keep it.
    soil = mix(soil, soil * vec3f(0.8, 0.77, 0.74), edge.root_damp * 0.7);
    // Sun-baked mud on the dry, packed lines, paler.
    let damp = smoothstep(0.57, 0.59, damp_field);
    let baked = smoothstep(0.55, 0.72, vg_noise(p * 0.09 + 71.0)) * packed * (1.0 - damp);
    soil = mix(soil, soil * vec3f(1.06, 1.05, 1.02), baked * 0.5);
    // Sandy creek banks outside the wall, and damp mud by the water.
    soil = mix(soil, vg_linear(0x9d917a) * 0.985, (1.0 - plan.inside) * 0.35);
    soil = mix(soil, soil * vec3f(0.55, 0.53, 0.5), plan.wet_bank);
    // Gravelly patches in the loose ground where the topsoil washed off the crests.
    let runoff = vg_noise(p * 0.12 + 33.0);
    let gravel_patch = smoothstep(0.5, 0.7, (1.0 - runoff) * 0.75 + vg_swell(p) * 0.3) * (1.0 - packed * 0.6) * (1.0 - vg_paved(p, plan.inside));
    soil = mix(soil, mix(soil, vg_linear(0x8e8676) * 0.975 * 0.92, 0.5), gravel_patch * 0.55);
    return soil;
}

// The damp field of the loose ground: it soaks the hollows and the puddles' banks
// (damp above 0.58).
fn vg_damp_field(p: vec2f, plan: VgPlan) -> f32 {
    let packed = smoothstep(0.2, 0.85, plan.tracks.b);
    let standing = clamp((1.0 - plan.tracks.a) / 0.6, 0.0, 1.0);
    return vg_noise(p * 0.12 + 33.0) * 0.6 - vg_swell(p) * 0.45 + (1.0 - packed) * 0.08 + standing * 0.3;
}

// What lies on the ground whatever grows there: the gravel drip line at the foot of
// the perimeter wall, the cottage gardens, crop fields and hedgerows (each settled
// to the colour it shows from the gameplay camera), and the contact shade at the
// foot of walls, cottages, timber and trunks.
fn vg_bake_finish(color_in: vec3f, p: vec2f, plan: VgPlan, texel: f32) -> vec3f {
    var color = color_in;
    let extent = material.params[0].y;
    let sun_flat = normalize(frame.sun_direction.xz + vec2f(1e-4, 0.0));
    let sun_run = min(length(frame.sun_direction.xz) / max(frame.sun_direction.y, 0.2), 2.6);
    let gravel_tone = vg_linear(0x8e8676) * 0.975;
    let wall_gap = extent - max(abs(p.x), abs(p.y));
    if wall_gap >= -0.5 && wall_gap < 1.35 {
        let verge = 1.0 - smoothstep(0.35, 1.1, wall_gap + (vg_noise(p * 1.7) - 0.5) * 0.5);
        color = mix(color, gravel_tone, verge * 0.85);
    }
    // Cottage gardens: flower beds on dark mulch, vegetable rows and a plank edging
    // where a bed meets the lawn.
    if plan.garden > 0.08 {
        let flower_bed = smoothstep(0.72, 0.9, plan.garden);
        let veg_plot = smoothstep(0.3, 0.42, plan.garden) * (1.0 - smoothstep(0.6, 0.72, plan.garden));
        let edging = smoothstep(0.08, 0.16, plan.garden) * (1.0 - smoothstep(0.24, 0.32, plan.garden));
        let bed = mix(vg_linear(0x46653a), vg_linear(0xc0806a), 0.25);
        let veg = mix(vg_linear(0x6b5440) * 0.975, vg_linear(0x6a9a3a), 0.4);
        color = mix(color, veg, veg_plot);
        color = mix(color, bed, flower_bed);
        color = mix(color, vg_linear(0x7a5a3a) * 0.975, edging * 0.85);
    }
    // Crop fields outside the wall, each worked to a headland a few metres in from
    // its hedge where the crop thins and the tractor's turning track runs round.
    var field_near = 0.0;
    var field_near_sun = 0.0;
    var field_sun = 0.0;
    if plan.inside < 1.0 {
        field_near = textureSampleLevel(extra_texture1, extra_sampler1, vg_plan_uv(p, VG_VALLEY_PLAN, 1.0), 2.0).g * (1.0 - plan.inside);
        let toward = p + sun_flat * sun_run * VG_HEDGE_HEIGHT;
        field_near_sun = textureSampleLevel(extra_texture1, extra_sampler1, vg_plan_uv(toward, VG_VALLEY_PLAN, 1.0), 2.0).g * (1.0 - plan.inside);
        field_sun = textureSampleLevel(extra_texture1, extra_sampler1, vg_plan_uv(toward, VG_VALLEY_PLAN, 1.0), 0.0).g * (1.0 - plan.inside);
    }
    if plan.field > 0.0 {
        let crop = vg_crop(p, plan.field_code, 1.0);
        let headland = 1.0 - smoothstep(0.55, 0.9, field_near);
        let turning = headland * smoothstep(0.35, 0.6, field_near) * (1.0 - smoothstep(0.6, 0.75, field_near));
        var crop_color = crop.rgb * 0.95;
        crop_color = mix(crop_color, mix(crop_color, vg_linear(0x8f8350), 0.45) * mix(0.9, 1.05, vg_noise(p * 0.4 + 7.0)), headland * 0.6);
        crop_color = mix(crop_color, vg_linear(0x6f5e45) * 0.95, turning * 0.55);
        // Patches of richer and poorer growth over a field.
        crop_color = crop_color * mix(0.88, 1.08, vg_noise(p * 0.06 + plan.field_code * 0.1));
        color = mix(color, crop_color, plan.field);
    }
    // Hedgerows of dark, lumpy shrubs: round the valley's enclosures, broken by
    // gaps, and along the field edges, kept off the farm lanes, lit along their
    // sunward side and casting their shade across the grass away from the sun.
    let valley = 1.0 - plan.inside;
    var hedge = 0.0;
    var hedge_sunward = 0.0;
    var shade_cast = 0.0;
    var enclosure_hedge = 0.0;
    if valley > 0.0 {
        enclosure_hedge = vg_enclosure_hedge(p, texel) * valley;
        hedge_sunward = 1.0 - vg_enclosure_hedge(p + sun_flat * 0.8, texel);
        shade_cast = vg_enclosure_hedge(p + sun_flat * sun_run * VG_HEDGE_HEIGHT, texel * 2.0) * valley * (1.0 - enclosure_hedge);
        hedge = enclosure_hedge * (1.0 - plan.field) * (1.0 - plan.worn);
    }
    if field_near > 0.06 && plan.field < 0.35 {
        hedge = max(hedge, smoothstep(0.06, 0.26, field_near) * (1.0 - smoothstep(0.02, 0.35, plan.field))
            * smoothstep(0.28, 0.42, vg_noise(p * 0.11 + 5.0)) * (1.0 - plan.worn));
    }
    if field_near_sun > 0.06 && field_sun < 0.35 {
        let toward = p + sun_flat * sun_run * VG_HEDGE_HEIGHT;
        let field_hedge = smoothstep(0.06, 0.26, field_near_sun) * (1.0 - smoothstep(0.02, 0.35, field_sun))
            * smoothstep(0.28, 0.42, vg_noise(toward * 0.11 + 5.0));
        shade_cast = max(shade_cast, field_hedge * (1.0 - hedge) * (1.0 - plan.field));
    }
    color = mix(color, color * vec3f(0.5, 0.54, 0.64), shade_cast * 0.7);
    if hedge > 0.0 {
        let shrubs = vg_noise(p * 1.4 + 3.0) * 0.6 + 0.2;
        let lit = max(hedge_sunward * enclosure_hedge, smoothstep(0.5, 0.8, shrubs) * 0.6);
        let leaves = mix(vg_linear(0x203b1d), vg_linear(0x56783a), smoothstep(0.25, 0.85, shrubs * 0.6 + lit * 0.55));
        color = mix(color, leaves, smoothstep(0.0, 0.5, hedge) * smoothstep(0.15, 0.35, shrubs + hedge * 0.3));
    }
    return color * mix(0.52, 1.0, plan.open);
}

// The bake's texel at `uv` over the whole target (`effects::bakes`).
fn village_ground_bake(uv: vec2f) -> vec4f {
    let texel = uv * VG_BAKE_SIZE;
    let page = min(floor(texel / VG_BAKE_PAGE), vec2f(2.0, 1.0));
    let local = texel / VG_BAKE_PAGE - page;
    let half = select(VG_ARENA_PLAN, VG_VALLEY_PLAN, page.y > 0.5);
    let p = (local - 0.5) * 2.0 * half;
    // One texel in metres: the softest edge a page can hold.
    let size = 2.0 * half / VG_BAKE_PAGE;
    let plan = vg_plan_at(p);
    let edge = vg_edge(p, plan);
    if page.x == VG_BAKE_FIELDS {
        // The land's slope by central differences over a texel.
        let h = size * 0.5;
        let slope = vec2f(vg_land(p + vec2f(h, 0.0)) - vg_land(p - vec2f(h, 0.0)), vg_land(p + vec2f(0.0, h)) - vg_land(p - vec2f(0.0, h))) / (2.0 * h);
        var extra = 0.0;
        if plan.inside > 0.0 && vg_in_square(p) {
            // Earth drifts in over the stones against the kerb and is trodden over
            // them in patches, a hand wide in some stretches and a couple of metres
            // in others.
            let packed = smoothstep(0.2, 0.85, plan.tracks.b);
            let inward = vg_square(p);
            let reach = mix(0.12, 2.6, smoothstep(0.35, 0.8, vg_noise(p * 0.16 + 3.0)));
            let against = 1.0 - smoothstep(0.0, reach, inward + (vg_noise(p * 1.3 + 5.0) - 0.5) * 0.3);
            let trodden = smoothstep(0.58, 0.86, vg_noise(p * 0.18 + 9.0) * 0.7 + vg_noise(p * 0.9 + 2.0) * 0.3) * (1.0 - packed * 0.4);
            extra = max(against * mix(0.75, 1.15, vg_noise(p * 0.6 + 11.0)), trodden * 0.8) / VG_SILT_RANGE;
        } else {
            // Wildflowers in meadow drifts, never where the ground is worn or walked.
            let wear = smoothstep(0.0, 0.9, max(plan.worn, plan.trampled * 0.85));
            let flowers = vg_noise(p * 0.08 + 61.0) * 0.65 + vg_noise(p * 0.17 + 3.0) * 0.35;
            extra = smoothstep(0.45, 0.72, flowers) * (1.0 - smoothstep(0.1, 0.4, wear));
        }
        return vec4f(vg_cover_encode(edge.cover), vg_slope_encode(slope.x), vg_slope_encode(slope.y), clamp(extra, 0.0, 1.0));
    }
    if page.x == VG_BAKE_EARTH {
        let damp_field = vg_damp_field(p, plan);
        let earth = vg_bake_finish(vg_bake_earth(p, plan, edge, damp_field), p, plan, size);
        return vec4f(sqrt(max(earth, vec3f(0.0))), vg_damp_encode(damp_field));
    }
    // Long grass grows in broad stands the mowers skip (the tall tufts of
    // `village_vegetation::long_grass` follow the same field) and never where the
    // ground is worn or walked.
    let wear = smoothstep(0.0, 0.9, max(plan.worn, plan.trampled * 0.85));
    let stand_field = vg_long_grass(p) + (vg_noise(p * 0.31 + 23.0) - 0.5) * 0.5;
    let long_grass = smoothstep(0.08, 0.4, stand_field) * (1.0 - smoothstep(0.05, 0.45, wear)) * (1.0 - plan.field);
    let grass = vg_bake_finish(vg_bake_grass(p, plan, edge, long_grass, wear, size), p, plan, size);
    return vec4f(sqrt(max(grass, vec3f(0.0))), long_grass);
}

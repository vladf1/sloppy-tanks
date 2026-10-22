// Harbor Havoc's surfaces worked out in world space (`effects_scenery::HARBOR_APRON`),
// so slabs, stains and wear never repeat with a texture tile. Extra texture 0 is
// the dock concrete tile, the grain of every mode; its broad blotches are divided
// out (a high-pass against a blurred sample) so they never stamp a pattern.
// params[0].x is the `ApronMode`: 0 concrete, 1 asphalt lane along z, 2 asphalt
// lane along x, 3 alpha-tested marking, 4 transparent label (its map is the
// lettering), 5 quay wall faces (the horizontal coordinate follows the face, y
// climbs out of the water), 6 ship hull plating, 7 the far shore (lit and hazed
// here, emitted as its final color with the material's fog off).

const APRON_LUMA = vec3f(0.2126, 0.7152, 0.0722);
// Slab edge in metres; the joints are sawn on this grid and every third one is a
// sealed expansion joint.
const APRON_SLAB = 6.0;
const APRON_EXPANSION = 18.0;
// Half width of a sawn joint and of an expansion joint's sealant, metres.
const APRON_JOINT = 0.014;
const APRON_SEALANT = 0.022;
// Mean linear luminance of the dock tile, so its grain multiplies around 1.
const APRON_TILE_MEAN = 0.2;
// Mip bias of the blurred grain sample that the high-pass divides by.
const APRON_GRAIN_BLUR = 5.0;
// Asphalt repair patches, oil spills and utility covers are scattered on these
// grids, metres.
const APRON_PATCH_CELL = 9.0;
const APRON_STAIN_CELL = 7.0;
const APRON_COVER_CELL = 13.0;
// Harbor water level (`harbor_water::HARBOR_WATER_HEIGHT`) and how far the tide
// stains the quay wall above it.
const APRON_SEA_LEVEL = -2.2;
const APRON_TIDE = 0.75;
// Spacing of the quay wall's vertical expansion joints, metres.
const APRON_WALL_JOINT = 12.0;
// Inner face of the boundary wall (`sim::data::ARENA`).
const APRON_ARENA = 60.0;
// Distance of a truck's wheel paths from the middle of its traffic half, metres.
const APRON_WHEEL_OFFSET = 0.9;
// Traffic lanes as (centre, half width): along z (centre x), then along x (centre
// z), as `harbor_scenery::LANES_ALONG_Z` and `LANES_ALONG_X`.
const APRON_LANES_Z = array<vec2f, 3>(vec2f(-52.0, 5.0), vec2f(0.0, 7.5), vec2f(52.0, 5.0));
const APRON_LANES_X = array<vec2f, 3>(vec2f(-45.0, 5.0), vec2f(0.0, 5.0), vec2f(45.0, 5.0));
// Fixed cover standing on the apron as (centre x, centre z, half x, half z): the
// indestructible containers (`harbor_layout`, the first twelve, which rust where
// their castings stand) and the concrete barriers. Ground beside them darkens with
// contact occlusion and collects grime.
const APRON_CONTAINERS = 12;
const APRON_FIXTURES = array<vec4f, 18>(
    vec4f(-30.0, -32.0, 3.0, 7.0),
    vec4f(-30.0, -12.0, 3.0, 7.0),
    vec4f(-30.0, 12.0, 3.0, 7.0),
    vec4f(-30.0, 32.0, 3.0, 7.0),
    vec4f(30.0, -32.0, 3.0, 7.0),
    vec4f(30.0, -12.0, 3.0, 7.0),
    vec4f(30.0, 12.0, 3.0, 7.0),
    vec4f(30.0, 32.0, 3.0, 7.0),
    vec4f(-13.0, -8.0, 6.0, 2.5),
    vec4f(-13.0, 8.0, 6.0, 2.5),
    vec4f(13.0, -8.0, 6.0, 2.5),
    vec4f(13.0, 8.0, 6.0, 2.5),
    vec4f(-10.0, -55.0, 4.0, 0.55),
    vec4f(-34.0, -55.0, 4.0, 0.55),
    vec4f(10.0, 55.0, 4.0, 0.55),
    vec4f(34.0, 55.0, 4.0, 0.55),
    vec4f(-43.0, 0.0, 0.6, 5.0),
    vec4f(43.0, 0.0, 0.6, 5.0),
);
// Far shore haze: metres of air that hide half of a color, and how much less
// haze the high ground gets.
const SHORE_HAZE_DISTANCE = 420.0;
const SHORE_HAZE_LIFT = 0.25;
// A far-shore vertex painted exactly this red is a warning light, and how
// brightly it shines.
const SHORE_LIGHT = vec3f(1.0, 0.0, 0.0);
const SHORE_LIGHT_GLOW = vec3f(2.4, 0.06, 0.03);

fn apron_hash(cell: vec2f) -> vec4f {
    var p = fract(cell.xyxy * vec4f(0.1031, 0.1030, 0.0973, 0.1099));
    p = p + dot(p, p.wzxy + 33.33);
    return fract((p.xxyz + p.yzzw) * p.zywx);
}

fn apron_noise(p: vec2f) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = apron_hash(i).x;
    let b = apron_hash(i + vec2f(1.0, 0.0)).x;
    let c = apron_hash(i + vec2f(0.0, 1.0)).x;
    let d = apron_hash(i + vec2f(1.0, 1.0)).x;
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn apron_box_distance(p: vec2f, b: vec4f) -> f32 {
    let q = abs(p - b.xy) - b.zw;
    return length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0);
}

// Wheel polish and the oil strip between the wheel paths, for a point `across`
// metres from the centre of a lane `half` metres wide.
fn apron_traffic(across: f32, half: f32) -> vec2f {
    let offset = abs(abs(across) - half * 0.5);
    let wheels = exp(-pow((offset - APRON_WHEEL_OFFSET) / 0.4, 2.0));
    let strip = exp(-pow(offset / 0.3, 2.0));
    return vec2f(wheels, strip) * step(abs(across), half);
}

// Offset from the centre of the nearest lane along z (x) and along x (y), with
// those lanes' half widths (z, w).
fn apron_lane_offsets(p: vec2f) -> vec4f {
    var out = vec4f(1e3, 1e3, 1.0, 1.0);
    for (var i = 0; i < 3; i++) {
        let along_z = APRON_LANES_Z[i];
        if abs(p.x - along_z.x) < abs(out.x) {
            out.x = p.x - along_z.x;
            out.z = along_z.y;
        }
        let along_x = APRON_LANES_X[i];
        if abs(p.y - along_x.x) < abs(out.y) {
            out.y = p.y - along_x.x;
            out.w = along_x.y;
        }
    }
    return out;
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let mode = material.params[0].x;
    let wall = mode > 4.5 && mode < 6.5;
    // A wall face runs along x or z; its plane coordinates are (along, height).
    let faces_x = abs((*s).normal.x) > abs((*s).normal.z);
    let along = select(f.world.x, f.world.z, faces_x);
    let p = select(f.world.xz, vec2f(along, f.world.y), wall);
    // Every sample and derivative first, in uniform control flow.
    let turned = vec2f(p.x * 0.8 - p.y * 0.6, p.x * 0.6 + p.y * 0.8);
    let grain_a = textureSample(extra_texture0, extra_sampler0, p / 4.7).rgb;
    let grain_b = textureSample(extra_texture0, extra_sampler0, turned / 9.3 + 0.31).rgb;
    let blur_a = textureSampleBias(extra_texture0, extra_sampler0, p / 4.7, APRON_GRAIN_BLUR).rgb;
    let blur_b = textureSampleBias(extra_texture0, extra_sampler0, turned / 9.3 + 0.31, APRON_GRAIN_BLUR).rgb;
    let fine = dot(textureSample(extra_texture0, extra_sampler0, turned / 1.3 + 0.17).rgb, APRON_LUMA) / APRON_TILE_MEAN;
    let streak_z = dot(textureSample(extra_texture0, extra_sampler0, vec2f(p.x / 1.9, p.y / 60.0)).rgb, APRON_LUMA);
    let streak_x = dot(textureSample(extra_texture0, extra_sampler0, vec2f(p.y / 1.9, p.x / 60.0) + 0.5).rgb, APRON_LUMA);
    let slab_uv = p / APRON_SLAB;
    let cell = floor(slab_uv);
    let to_edge = (0.5 - abs(fract(slab_uv) - 0.5)) * APRON_SLAB;
    let edge = min(to_edge.x, to_edge.y);
    let aa = max(fwidth(edge), 1e-4);
    let to_expansion = (0.5 - abs(fract(p / APRON_EXPANSION) - 0.5)) * APRON_EXPANSION;
    let expansion = min(to_expansion.x, to_expansion.y);
    let expansion_aa = max(fwidth(expansion), 1e-4);
    let patch_uv = p / APRON_PATCH_CELL;
    let patch_cell = floor(patch_uv);
    let patch_local = fract(patch_uv) * APRON_PATCH_CELL;
    let patch_aa = max(fwidth(patch_local.x) + fwidth(patch_local.y), 1e-4);
    let crack_noise = apron_noise(p / 3.1 + 13.0) + apron_noise(p * 0.9) * 0.18;
    let crack_width = max(fwidth(crack_noise), 1e-4);
    let wall_edge = (0.5 - abs(fract(along / APRON_WALL_JOINT) - 0.5)) * APRON_WALL_JOINT;
    let wall_aa = max(fwidth(wall_edge), 1e-4);
    // A utility cover in some cells: distance to its centre.
    let cover_cell = floor(p / APRON_COVER_CELL);
    let cover_hash = apron_hash(cover_cell + vec2f(41.0, 7.0));
    let cover_at = (cover_cell + 0.25 + 0.5 * cover_hash.xy) * APRON_COVER_CELL;
    let cover_d = length(p - cover_at);
    let cover_aa = max(fwidth(cover_d), 1e-4);
    // Hull plating: welded seams on a 2.4 x 1.6 m grid.
    let plate = (0.5 - abs(fract(vec2f(along / 2.4, (f.world.y + 0.2) / 1.6)) - 0.5)) * vec2f(2.4, 1.6);
    let plate_aa = max(fwidth(plate.x) + fwidth(plate.y), 1e-4);

    // The tile's grain with its broad blotches divided out.
    let grain = grain_a / max(blur_a, vec3f(0.02)) * 0.6 + grain_b / max(blur_b, vec3f(0.02)) * 0.4;
    let grain_luma = dot(grain, APRON_LUMA);
    // Broad mottling, and the macro variation that keeps the yard from reading as
    // one tile from the air.
    let mottle = apron_noise(p / 9.0) * 0.6 + apron_noise(p / 2.7 + 7.0) * 0.4;
    let broad = apron_noise(p / 43.0 + 3.0) * 0.65 + apron_noise(p / 15.0 + 8.0) * 0.35;
    let grime = mix(0.88, 1.05, mottle) * mix(0.82, 1.1, broad);

    // Damp hollows that hold water after a shower; the deepest are puddles.
    let hollow = apron_noise(p / 7.0 + 3.3) * 0.7 + apron_noise(p / 2.1) * 0.3;
    let damp = smoothstep(0.74, 0.79, hollow);
    let puddle = smoothstep(0.8, 0.815, hollow);

    var color = (*s).color;
    var roughness = (*s).roughness;
    if mode > 6.5 {
        // Far shore: lit by the sun and the sky fill, then hazed by the air between
        // it and the eye, emitted whole so the scene's fog never flattens the
        // layers. Painted warning lights shine through the haze.
        let albedo = f.vertex_color.rgb;
        let n = (*s).normal;
        let hemisphere = mix(frame.ground_color.rgb, frame.sky_color.rgb, n.y * 0.5 + 0.5);
        let sun = max(dot(n, frame.sun_direction.xyz), 0.0) * frame.sun_color.rgb;
        let lit = albedo * (sun + hemisphere) * 0.3183099;
        let distance = length(frame.camera_position.xyz - f.world);
        let lift = smoothstep(0.0, 60.0, f.world.y) * SHORE_HAZE_LIFT;
        let haze = (1.0 - exp2(-distance / SHORE_HAZE_DISTANCE)) * (1.0 - lift);
        var horizon = frame.sky_color.rgb * 0.3183099;
        if frame.fog_color.w > 0.5 {
            horizon = frame.fog_color.rgb;
        }
        var hazed = mix(lit, horizon, haze);
        if atmosphere_on() {
            // The scene's low sea mist: thick at the waterline and thinning up the
            // slopes, so the headlands rise out of it with clear green crests.
            let ray = (f.world - frame.camera_position.xyz) / max(distance, 1e-4);
            let mist = frame.haze_far.w * exp(-max(f.world.y, 0.0) / MIST_HEIGHT);
            hazed = mix(hazed, haze_tint(ray, mist), mist);
        }
        let light = select(0.0, 1.0, all(abs(albedo - SHORE_LIGHT) < vec3f(0.01)));
        (*s).emissive = mix(hazed, SHORE_LIGHT_GLOW, light);
        (*s).color = vec3f(0.0);
        (*s).roughness = 1.0;
        (*s).metalness = 0.0;
        return;
    }
    if mode > 5.5 {
        // Ship hull plating: welded seams, rust running down from the scuppers and
        // the deck edge, a slimy band at the waterline, fender scrapes and the
        // blotchy fade of a hull painted in sections.
        let height = f.world.y - APRON_SEA_LEVEL;
        let seam = clamp((0.012 - min(plate.x, plate.y)) / plate_aa + 0.5, 0.0, 1.0);
        let run_cell = floor(along / 2.7);
        let run_hash = apron_hash(vec2f(run_cell, 13.0));
        let run_x = (run_cell + 0.2 + 0.6 * run_hash.x) * 2.7;
        let run_width = 0.05 + 0.14 * run_hash.y;
        let run_wobble = (apron_noise(vec2f(along * 3.0, f.world.y * 0.7)) - 0.5) * 0.08;
        let run_top = 5.0 - run_hash.w * 0.6;
        let run_length = 1.0 + 3.5 * run_hash.z;
        let run = exp(-pow((along - run_x + run_wobble) / run_width, 2.0))
            * smoothstep(run_top - run_length, run_top - 0.3, height) * step(0.3, run_hash.z);
        let bleed = smoothstep(0.55, 0.85, apron_noise(vec2f(along * 0.6, height * 2.2)) * apron_noise(vec2f(along / 5.0, 3.0)) * 1.8);
        let slime = 1.0 - smoothstep(0.1, 0.55 + mottle * 0.4, height);
        let scrape = smoothstep(0.78, 0.9, apron_noise(vec2f(along * 0.35, height * 6.0)))
            * (1.0 - smoothstep(1.0, 3.2, height));
        let section = apron_hash(vec2f(floor(along / 9.0), floor(height / 2.5)) + 3.0).x;
        color = color * mix(0.88, 1.06, section) * mix(0.92, 1.05, mottle) * mix(0.9, 1.04, grain_luma * 0.5 + 0.25);
        color = color * (1.0 - seam * 0.32);
        color = mix(color, color * vec3f(1.15, 1.12, 1.08) + vec3f(0.02), scrape * 0.5);
        color = mix(color, vec3f(0.17, 0.065, 0.025), clamp(run * 0.85 + bleed * 0.35, 0.0, 1.0));
        color = mix(color, vec3f(0.03, 0.045, 0.03), slime * 0.8);
        roughness = mix(roughness, 0.85, run);
        roughness = mix(roughness, 0.4, slime);
        (*s).color = color;
        (*s).roughness = roughness;
        return;
    }
    // Ground modes: contact occlusion and grime against fixed cover and the
    // boundary wall, and rust where containers stand on their castings. Paint on
    // the wall tops, outside the apron, takes none.
    let outside = max(abs(p.x), abs(p.y)) - APRON_ARENA;
    var fixture = select(-outside, 1e3, outside > 0.0);
    var castings = 0.0;
    if !wall {
        for (var i = 0; i < 18; i++) {
            let b = APRON_FIXTURES[i];
            fixture = min(fixture, apron_box_distance(p, b));
            if i < APRON_CONTAINERS {
                let corner = b.xy + sign(p - b.xy) * (b.zw - 0.1);
                castings = max(castings, 1.0 - smoothstep(0.05, 0.6 + mottle * 0.6, length(p - corner)));
            }
        }
    }
    let contact = (1.0 - smoothstep(0.0, 0.8, fixture)) * 0.42 + (1.0 - smoothstep(0.0, 3.2, fixture)) * 0.16;
    let banked = (1.0 - smoothstep(0.0, 1.6 + mottle * 1.4, fixture)) * smoothstep(0.35, 0.6, mottle);
    let offsets = apron_lane_offsets(p);
    let lane_edge = min(abs(offsets.x) - offsets.z, abs(offsets.y) - offsets.w);
    // A cast-iron utility cover in some cells, clear of the lane edges and cover.
    let cover_radius = 0.34 + 0.12 * cover_hash.z;
    let has_cover = step(0.72, cover_hash.w) * step(0.9, abs(lane_edge)) * step(1.5, fixture);
    let cover = clamp((cover_radius - cover_d) / cover_aa + 0.5, 0.0, 1.0) * has_cover;
    let cover_gap = clamp(0.5 - abs(cover_d - cover_radius - 0.02) / cover_aa * 0.7, 0.0, 1.0) * has_cover;
    let checker = step(0.0, sin(p.x * 34.0) * sin(p.y * 34.0));
    let cover_rim = clamp((cover_d - cover_radius + 0.05) / cover_aa + 0.5, 0.0, 1.0);
    let iron = vec3f(0.05, 0.048, 0.046) * mix(0.8, 1.25, checker) * mix(1.0, 1.4, cover_rim);

    if wall {
        // Quay wall: weathered concrete panels, a dark wet tide band crusted with
        // barnacles and weed at the waterline, and rust and lime streaks running
        // down from the top.
        let height = f.world.y - APRON_SEA_LEVEL;
        let ragged = (apron_noise(vec2f(along * 0.7, 0.0)) - 0.5) * 0.35 + (grain_luma - 1.0) * 0.08;
        let wet = 1.0 - smoothstep(APRON_TIDE * 0.6, APRON_TIDE, height + ragged);
        let weed = 1.0 - smoothstep(0.05, 0.38, height + ragged * 0.6);
        let barnacles = smoothstep(0.62, 0.72, apron_noise(vec2f(along * 6.0, f.world.y * 6.0)))
            * (1.0 - smoothstep(0.25, 0.6, abs(height - 0.32 - ragged * 0.4)));
        let runs = apron_noise(vec2f(along * 1.3, f.world.y * 0.08));
        let rust = smoothstep(0.62, 0.8, runs) * smoothstep(-2.5, -0.4, f.world.y) * 0.6;
        let lime = smoothstep(0.7, 0.86, apron_noise(vec2f(along * 0.9 + 19.0, f.world.y * 0.1))) * 0.5;
        let joint = clamp((0.03 - wall_edge) / wall_aa + 0.5, 0.0, 1.0);
        color = material.color.rgb * f.tint.rgb * grain * grime;
        color = mix(color, color * vec3f(1.1, 1.08, 1.02), lime * (1.0 - wet));
        color = mix(color, color * vec3f(0.62, 0.42, 0.28), rust * (1.0 - wet));
        color = mix(color, color * vec3f(0.36, 0.38, 0.36), wet);
        color = mix(color, vec3f(0.035, 0.06, 0.035) * grain_luma, weed * 0.85);
        color = mix(color, vec3f(0.42, 0.42, 0.38) * grain_luma, barnacles * 0.7);
        color = color * (1.0 - joint * 0.55);
        roughness = mix(roughness, 0.35, wet * (1.0 - barnacles));
    } else if mode < 0.5 {
        // Concrete slabs: per-pour tone and hue drifting across the yard, a trowel
        // gradient, the odd patched slab, sealed expansion joints, grime washed
        // into the joints and banked against cover, tyre dirt where traffic cuts
        // the lane corners, rusty casting prints and scattered oil spills.
        let slab = apron_hash(cell + vec2f(17.0, 5.0));
        let patched = step(slab.y, 0.06);
        let tone = mix(0.83, 1.07, slab.x) * mix(1.0, 1.1, patched);
        let hue = mix(vec3f(1.025, 1.0, 0.955), vec3f(0.965, 0.995, 1.03), slab.w);
        let pour = 1.0 + dot(fract(slab_uv) - 0.5, slab.zw - 0.5) * 0.16;
        let groove = clamp((APRON_JOINT - edge) / aa + 0.5, 0.0, 1.0) * min(1.0, 2.0 * APRON_JOINT / aa);
        let sealant = clamp((APRON_SEALANT - expansion) / expansion_aa + 0.5, 0.0, 1.0)
            * min(1.0, 2.0 * APRON_SEALANT / expansion_aa);
        let fringe = (1.0 - smoothstep(0.0, 0.3 + mottle * 0.3, edge)) * (0.4 + mottle);
        // Hairline shrinkage cracks across some slabs, darker where dirt fills them.
        let hairline = (1.0 - smoothstep(0.0, crack_width * 1.2 + 0.003, abs(crack_noise - 0.5)))
            * step(0.72, slab.z) * (1.0 - patched) * smoothstep(0.55, 0.7, apron_noise(p / 1.7 + slab.xy * 9.0));
        let chipped = (1.0 - smoothstep(APRON_JOINT, APRON_JOINT * 3.5, edge)) * step(0.6, grain_luma * apron_noise(p * 6.0) * 1.4);
        let streak = select(streak_x, streak_z, abs(offsets.x) - offsets.z < abs(offsets.y) - offsets.w) / APRON_TILE_MEAN;
        let corner_dirt = (1.0 - smoothstep(0.0, 2.4, lane_edge)) * (0.45 + 0.55 * smoothstep(0.7, 1.3, streak));

        // Oil: a spill in some cells (more of them beside the containers, where
        // handlers park), dark and still wet at its core with a thin-film sheen.
        let stain_cell = floor(p / APRON_STAIN_CELL);
        let spill = apron_hash(stain_cell + vec2f(5.0, 29.0));
        let spill_at = (stain_cell + 0.2 + 0.6 * spill.xy) * APRON_STAIN_CELL;
        let spill_dir = vec2f(cos(spill.w * 6.283), sin(spill.w * 6.283));
        let rel = p - spill_at;
        let stretched = vec2f(dot(rel, spill_dir) * 0.55, dot(rel, vec2f(-spill_dir.y, spill_dir.x)));
        let spill_radius = 0.35 + 1.05 * spill.z * spill.z;
        let shape = length(stretched) / spill_radius + (apron_noise(p * 1.9 + spill.xy * 40.0) - 0.5) * 0.7
            + (grain_luma - 1.0) * 0.2;
        let near_cover = 1.0 - smoothstep(1.0, 7.0, fixture);
        let has_spill = step(fract(spill.x * 13.7 + spill.y * 7.1), 0.08 + 0.34 * near_cover);
        let oil = (1.0 - smoothstep(0.55, 1.0, shape)) * has_spill;
        let oil_core = (1.0 - smoothstep(0.05, 0.5, shape)) * has_spill;
        let film = smoothstep(0.2, 0.42, shape) * (1.0 - smoothstep(0.42, 0.62, shape)) * has_spill;
        let film_tint = 0.6 + 0.4 * cos(6.283 * (shape * 2.6 + vec3f(0.0, 0.33, 0.67)));

        color = material.color.rgb * f.tint.rgb * grain * tone * hue * pour * grime;
        color = color * (1.0 - fringe * 0.22) * mix(1.0, 1.1, chipped) * (1.0 - hairline * 0.4);
        color = mix(color, color * vec3f(0.62, 0.58, 0.53), corner_dirt * 0.45);
        color = mix(color, color * vec3f(0.66, 0.6, 0.52), banked * 0.5);
        color = mix(color, color * vec3f(0.85, 0.55, 0.34), castings * 0.75);
        color = mix(color, color * vec3f(0.24, 0.23, 0.22), oil * mix(0.55, 0.8, spill.z));
        color = mix(color, color * film_tint * 1.4, film * 0.35);
        color = color * (1.0 - oil_core * 0.35);
        color = color * (1.0 - groove * 0.55) * (1.0 - sealant * 0.65);
        color = color * (1.0 - contact);
        color = color * mix(1.0, 0.8, damp) * mix(1.0, 0.72, puddle);
        roughness = mix(roughness, 0.5, oil);
        roughness = mix(roughness, 0.14, oil_core);
        roughness = mix(roughness, 0.42, damp);
        roughness = mix(roughness, 0.12, puddle);
        // Cast-iron cover: a raised checker plate in a dark gap.
        color = mix(color, iron, cover);
        color = color * (1.0 - cover_gap * 0.7);
        roughness = mix(roughness, 0.5, cover);
        (*s).metalness = mix((*s).metalness, 0.55, cover);
    } else if mode < 2.5 {
        // Asphalt lanes: dark binder with pale aggregate, oxidised paler where the
        // traffic thins and ravelled at the edges, rubber-polished wheel paths with
        // an oil strip between them, the paving joint down the middle, tar-sealed
        // cracks, repair patches and cast-iron covers.
        let along_z = abs(mode - 1.0) < 0.5;
        let across = select(offsets.y, offsets.x, along_z);
        let half = select(offsets.w, offsets.z, along_z);
        let streak = select(streak_x, streak_z, along_z) / APRON_TILE_MEAN;
        let traffic = apron_traffic(across, half);
        let aggregate = smoothstep(1.3, 2.0, fine) - smoothstep(0.6, 0.25, fine) * 0.6;
        let worn = apron_noise(p / 11.0 + 5.0);
        let ravel = smoothstep(half - 1.1, half - 0.15, abs(across)) * (0.5 + 0.5 * mottle);
        let tyre = smoothstep(0.8, 1.35, streak) * mix(0.5, 1.0, mottle);
        let lane_along = select(p.x, p.y, along_z);
        let drips = smoothstep(0.5, 0.7, apron_noise(vec2f(across * 2.2, lane_along * 0.31))
            * 0.7 + apron_noise(vec2f(across * 5.0, lane_along * 1.7)) * 0.3) * traffic.y;
        let patch_hash = apron_hash(patch_cell + vec2f(3.0, 11.0));
        let patch_size = vec2f(2.0, 1.4) + patch_hash.zw * vec2f(3.5, 3.0);
        let patch_at = vec2f(0.6) + patch_hash.xy * (vec2f(APRON_PATCH_CELL) - patch_size - 1.2);
        let inside = min(
            min(patch_local.x - patch_at.x, patch_at.x + patch_size.x - patch_local.x),
            min(patch_local.y - patch_at.y, patch_at.y + patch_size.y - patch_local.y),
        );
        let has_patch = step(0.75, patch_hash.x * 0.6 + patch_hash.w * 0.6);
        let patched = clamp(inside / patch_aa + 0.5, 0.0, 1.0) * has_patch;
        let seal = (1.0 - smoothstep(0.0, 0.08, abs(inside))) * has_patch;
        let crack = (1.0 - smoothstep(0.0, crack_width * 1.4 + 0.006, abs(crack_noise - 0.5)))
            * smoothstep(0.6, 0.74, apron_noise(p / 6.0 + 2.0)) * (1.0 - patched);
        let centre_joint = (1.0 - smoothstep(0.015, 0.05, abs(across + (apron_noise(p * 0.8) - 0.5) * 0.06)))
            * smoothstep(0.35, 0.6, apron_noise(p / 4.0 + 17.0));
        let base = material.color.rgb * f.tint.rgb * mix(0.92, 1.06, grain_luma * 0.5)
            * mix(1.0, 1.28, worn * (1.0 - traffic.x * 0.7)) * mix(1.0, 1.2, ravel);
        color = base * (1.0 + aggregate * mix(0.35, 0.7, ravel)) * grime;
        color = color * mix(1.0, 0.82, patched);
        color = color * mix(1.0, 0.8, tyre * (0.35 + traffic.x * 0.65));
        color = color * mix(1.0, 0.85, traffic.x);
        color = mix(color, color * vec3f(0.5, 0.49, 0.48), drips * 0.6);
        color = color * (1.0 - seal * 0.4) * (1.0 - crack * 0.6) * (1.0 - centre_joint * 0.5);
        color = mix(color, color * vec3f(0.85, 0.55, 0.34), castings * 0.5);
        color = color * (1.0 - contact * 0.8);
        color = color * mix(1.0, 0.86, damp) * mix(1.0, 0.7, puddle);
        roughness = mix(roughness, 0.66, traffic.x * 0.7 + patched * 0.3);
        roughness = mix(roughness, 0.55, drips);
        roughness = mix(roughness, 0.3, clamp(crack + seal * 0.5, 0.0, 1.0));
        roughness = mix(roughness, 0.38, damp);
        roughness = mix(roughness, 0.12, puddle);
        color = mix(color, iron, cover);
        color = color * (1.0 - cover_gap * 0.7);
        roughness = mix(roughness, 0.5, cover);
        (*s).metalness = mix((*s).metalness, 0.55, cover);
    } else {
        // Paint: worn through in old zones and where wheels pass, dirtied by the
        // traffic and pitted by the aggregate it was rolled onto.
        let traffic_z = apron_traffic(offsets.x, offsets.z);
        let traffic_x = apron_traffic(offsets.y, offsets.w);
        let traffic = max(max(traffic_z.x, traffic_x.x), max(traffic_z.y, traffic_x.y) * 0.6);
        let age = apron_noise(p / 13.0 + 31.0);
        let scuff = apron_noise(p * 2.3) * 0.55 + (grain_luma - 1.0) * 0.6 + apron_noise(p / 3.0) * 0.45
            + (fine - 1.0) * 0.12;
        let threshold = 0.1 + smoothstep(0.45, 0.85, age) * 0.32 + traffic * 0.3;
        let worn = smoothstep(threshold, threshold + 0.07, scuff);
        if mode < 3.5 {
            color = material.color.rgb * f.vertex_color.rgb * f.tint.rgb;
        }
        let dirt = clamp(apron_noise(p / 4.0 + 2.0) * 0.6 + age * 0.4 + traffic * 0.3 - 0.25, 0.0, 1.0);
        color = mix(color, color * vec3f(0.62, 0.6, 0.55), dirt * 0.6);
        color = color * mix(0.78, 1.04, clamp(fine - 0.3, 0.0, 1.0)) * mix(0.93, 1.02, mottle);
        color = color * (1.0 - contact * 0.4);
        color = color * mix(1.0, 0.8, damp);
        roughness = mix(roughness, 0.08, puddle);
        if mode < 3.5 {
            (*s).opacity = worn;
        } else {
            (*s).opacity = (*s).opacity * mix(0.2, 1.0, worn) * mix(1.0, 0.75, age);
        }
    }
    (*s).color = color;
    (*s).roughness = roughness;
}

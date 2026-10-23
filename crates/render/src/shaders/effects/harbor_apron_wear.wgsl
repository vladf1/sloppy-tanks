// The harbor ground's wear, baked once into a texture over the arena
// (`effects::bakes::HARBOR_APRON_WEAR`) after `harbor_apron.wgsl`'s constants and
// helpers: for each texel, the layers of the concrete or asphalt there that vary
// slowly across the yard, as a color multiplier (rgb, over `APRON_WEAR_RANGE`) and a
// roughness (a). The ground's shader (`apron_ground`) multiplies its crisp per-pixel
// detail by it. Where a layer added light, it adds its share of a typical slab's
// albedo instead, and the dock tile's grain, which the bake does not sample, stands
// at its mean.

// A typical linear albedo of the bare slabs, for the few layers that add light.
const WEAR_ALBEDO = 0.33;
// Roughness of the bare concrete (`harbor_surfaces::APRON_ROUGHNESS`).
const WEAR_CONCRETE_ROUGHNESS = 0.92;

fn wear_tint(k: vec3f, w: f32) -> vec3f {
    return mix(vec3f(1.0), k, w);
}

// The concrete's wear at `p`: grime washed into the joints and banked against cover,
// old stains weathered pale inside a dark tide line, ghosts of removed bay paint,
// rubber arcs where straddle carriers turn, tyre dirt cut across the lane corners,
// the prints of containers lifted away, rusty castings, fresh oil with a wet core,
// soaked slabs and salt bloom along the foot of the wall.
fn wear_concrete(p: vec2f, grime: f32, mottle: f32, offsets: vec4f, fixture: vec2f, outside: f32) -> vec4f {
    let lane_edge = min(abs(offsets.x) - offsets.z, abs(offsets.y) - offsets.w);
    let slab_uv = p / APRON_SLAB;
    let cell = floor(slab_uv);
    let to_edge = (0.5 - abs(fract(slab_uv) - 0.5)) * APRON_SLAB;
    let edge = min(to_edge.x, to_edge.y);
    let slab = apron_hash(cell + vec2f(17.0, 5.0));
    var m = vec3f(grime);
    var roughness = WEAR_CONCRETE_ROUGHNESS;
    // Ghosts of bay lines ground off when the yard was re-marked.
    let ghost_cell = floor(p / APRON_GHOST_CELL);
    let ghost_hash = apron_hash(ghost_cell + vec2f(23.0, 51.0));
    let ghost_local = p - (ghost_cell + 0.5) * APRON_GHOST_CELL;
    let ghost_along_x = ghost_hash.x > 0.5;
    let ghost_run = select(ghost_local.y, ghost_local.x, ghost_along_x);
    let ghost_across = select(ghost_local.x, ghost_local.y, ghost_along_x) - (ghost_hash.y - 0.5) * 6.0;
    let ghost = (1.0 - smoothstep(0.06, 0.1, abs(ghost_across))) * step(abs(ghost_run), 1.5 + 3.0 * ghost_hash.z)
        * step(0.62, ghost_hash.w) * smoothstep(0.3, 0.6, apron_noise(p * 1.3 + 4.0) + 0.2);
    m = m * wear_tint(vec3f(1.08, 1.07, 1.03) + 0.012 / WEAR_ALBEDO, ghost * 0.55);
    // Grime washed into the sawn joints.
    let fringe = (1.0 - smoothstep(0.0, 0.38 + mottle * 0.4, edge)) * (0.45 + mottle);
    m = m * wear_tint(vec3f(0.66, 0.62, 0.56), fringe * 0.5);
    // Old stains: rust washed off steel, an old diesel spill or lime leached out
    // of the concrete, weathered pale at their heart and dark at their tide line.
    let old_cell = floor(p / APRON_OLD_STAIN_CELL);
    let old_hash = apron_hash(old_cell + vec2f(71.0, 13.0));
    let old_at = (old_cell + 0.25 + 0.5 * old_hash.xy) * APRON_OLD_STAIN_CELL;
    let old_dir = vec2f(cos(old_hash.w * 6.283), sin(old_hash.w * 6.283));
    let old_kind = fract(old_hash.y * 5.3 + old_hash.w * 2.1);
    let rusty = old_kind > 0.62;
    let has_old = step(fract(old_hash.x * 7.3 + old_hash.y * 3.1), 0.55);
    if has_old != 0.0 {
        let old_stretch = select(1.0 + 1.8 * fract(old_hash.z * 3.7), 2.8 + 2.2 * fract(old_hash.z * 3.7), rusty);
        let old_shape = apron_blotch(p, old_at, old_dir, old_stretch, select(1.4 + 2.6 * old_hash.z, 0.9 + 1.4 * old_hash.z, rusty),
            (apron_noise(p * 0.7 + old_hash.xy * 30.0) - 0.5) * 0.9);
        let old_stain = 1.0 - smoothstep(0.55, 1.0, old_shape);
        let old_line = smoothstep(0.75, 0.92, old_shape) * (1.0 - smoothstep(0.92, 1.05, old_shape));
        var old_tint = select(vec3f(1.07, 1.05, 1.0), vec3f(0.74, 0.74, 0.75), old_kind < 0.3);
        var old_rim = vec3f(0.66, 0.62, 0.57);
        if rusty && old_stain > 0.0 {
            // Drawn out along the way the water ran off, in rivulets with a dark
            // core and a pale orange fringe, never one even tinted wash.
            let old_rel = p - old_at;
            let run_u = dot(old_rel, old_dir);
            let run_v = dot(old_rel, vec2f(-old_dir.y, old_dir.x));
            let rivulets = apron_noise(vec2f(run_v * 4.6, run_u * 0.3) + old_hash.xy * 50.0);
            let core = (1.0 - smoothstep(0.05, 0.7, old_shape)) * smoothstep(0.42, 0.72, rivulets);
            old_tint = mix(vec3f(0.95, 0.8, 0.66), vec3f(0.5, 0.31, 0.19), core) * mix(0.86, 1.08, rivulets);
            old_rim = vec3f(0.84, 0.66, 0.5);
        }
        m = m * wear_tint(old_tint, old_stain * select(0.45, 0.7, rusty));
        m = m * wear_tint(old_rim, old_line * select(0.6, 0.35, rusty) + old_stain * 0.15);
    }
    // A few slabs soaked dark by years of drips where machines stood.
    let soaked = step(0.9, slab.y) * smoothstep(0.2, 0.7, apron_noise(p / 2.3 + slab.zw * 11.0) + 0.25);
    m = m * wear_tint(vec3f(0.62, 0.56, 0.5), soaked * 0.55);
    roughness = mix(roughness, 0.8, soaked * 0.5);
    // The print of a container that stood here: the ground under it kept clean of
    // the yard's grime, dirt banked along its sides and rust squares where its
    // castings stood.
    let print_cell = floor(p / APRON_FOOTPRINT_CELL);
    let print_hash = apron_hash(print_cell + vec2f(13.0, 61.0));
    if print_hash.w > 0.5 {
        let half_length = select(APRON_FOOTPRINT_HALF_LENGTHS.x, APRON_FOOTPRINT_HALF_LENGTHS.y, print_hash.z > 0.45);
        let along_x = print_hash.y > 0.5;
        let half = select(vec2f(APRON_FOOTPRINT_HALF_WIDTH, half_length), vec2f(half_length, APRON_FOOTPRINT_HALF_WIDTH), along_x);
        let centre = (print_cell + 0.5 + (print_hash.xy - 0.5) * 0.35) * APRON_FOOTPRINT_CELL;
        // Clear of the lanes, the yard's edge and the containers standing now.
        let lanes = apron_lane_offsets(centre);
        let clear = step(half.x + 0.6, abs(lanes.x) - lanes.z) * step(half.y + 0.6, abs(lanes.y) - lanes.w)
            * step(max(abs(centre.x) + half.x, abs(centre.y) + half.y), APRON_ARENA - 2.0)
            * smoothstep(0.5, 1.5, fixture.x);
        if clear > 0.0 {
            let rel = p - centre;
            let q = abs(rel) - half;
            let box_d = max(q.x, q.y);
            let fray = (apron_noise(p * 3.3) - 0.5) * 0.08;
            let inside = (1.0 - smoothstep(-0.12, 0.02, box_d + fray)) * clear;
            let rim_d = (box_d - 0.05 + fray) * (1.0 / 0.11);
            let rim = exp(-rim_d * rim_d) * clear * smoothstep(0.25, 0.6, apron_noise(p * 1.7 + 9.0) + 0.2);
            let corner = sign(rel) * (half - 0.1);
            let to_corner = abs(rel - corner);
            let casting_d = max(to_corner.x, to_corner.y);
            let casting = max(
                1.0 - smoothstep(0.07, 0.1, casting_d),
                (1.0 - smoothstep(0.1, 0.32 + mottle * 0.2, casting_d)) * 0.45 * smoothstep(0.35, 0.65, mottle),
            ) * clear;
            m = m * wear_tint(vec3f(1.03, 1.03, 1.02) / max(grime, 0.5), inside * 0.3);
            m = m * wear_tint(vec3f(0.6, 0.55, 0.48), rim * 0.6);
            m = m * wear_tint(vec3f(0.62, 0.36, 0.2), casting * 0.8);
        }
    }
    // Tyre dirt cut across the corners where trucks swing off a lane.
    let corner_dirt = (1.0 - smoothstep(0.0, 2.4, lane_edge)) * 0.72;
    m = m * wear_tint(vec3f(0.58, 0.54, 0.49), corner_dirt * 0.6);
    // Straddle carriers working the bays: pairs of broad wheel paths running with
    // the nearest lane, polished dark and grimy, in and out along their runs.
    let track_along_z = abs(offsets.x) - offsets.z < abs(offsets.y) - offsets.w;
    let track_across = select(p.y, p.x, track_along_z);
    let track_run = select(p.x, p.y, track_along_z);
    let track_cell = floor(track_across / APRON_TRACK_CELL);
    let track_hash = apron_hash(vec2f(track_cell, select(3.0, 7.0, track_along_z)));
    let track_centre = (track_cell + 0.3 + 0.4 * track_hash.x) * APRON_TRACK_CELL;
    let track_off = abs(abs(track_across - track_centre) - (1.5 + 0.5 * track_hash.y)) / (0.42 + 0.3 * track_hash.z);
    let track_clear = smoothstep(1.0, 2.5, fixture.x);
    let tracks = exp(-track_off * track_off) * step(0.25, track_hash.w) * track_clear * 0.71
        * smoothstep(0.2, 0.5, apron_noise(vec2f(track_run / 13.0, track_cell * 3.1 + 0.5)));
    m = m * wear_tint(vec3f(0.46, 0.44, 0.42), tracks * 0.85);
    roughness = mix(roughness, 0.8, tracks * 0.6);
    // Rubber and diesel soot polished into the slabs where trucks swing off a lane
    // into the bays: broad bands running with the lane, darkest beside it.
    let pull_off = max(lane_edge, 0.0);
    if pull_off < APRON_POLISH_REACH {
        let band = apron_noise(vec2f(track_run / 19.0, track_across / 4.5) + 41.0);
        let tyre = apron_noise(vec2f(track_run / 7.0, track_across * 2.1) + 3.0);
        let polish = (1.0 - smoothstep(0.6, APRON_POLISH_REACH, pull_off + (mottle - 0.5) * 2.0))
            * smoothstep(0.38, 0.68, band) * mix(0.55, 1.0, smoothstep(0.35, 0.75, tyre)) * track_clear;
        m = m * wear_tint(vec3f(0.6, 0.58, 0.56), polish * 0.65);
        roughness = mix(roughness, 0.82, polish * 0.5);
    }
    // Grime banked against the fixed cover, and rust where containers stand on
    // their castings.
    let banked = (1.0 - smoothstep(0.0, 1.6 + mottle * 1.4, fixture.x)) * smoothstep(0.35, 0.6, mottle);
    m = m * wear_tint(vec3f(0.66, 0.6, 0.52), banked * 0.5);
    m = m * wear_tint(vec3f(0.85, 0.55, 0.34), fixture.y * 0.75);
    // Rubber arcs left by turning straddle carriers: a pair of wheel tracks on a
    // circle, fading in and out along it.
    let scuff_cell = floor(p / APRON_SCUFF_CELL);
    let scuff_hash = apron_hash(scuff_cell + vec2f(91.0, 37.0));
    if fract(scuff_hash.z * 5.7) >= 0.62 {
        let scuff_at = (scuff_cell + 0.5 + (scuff_hash.xy - 0.5) * 0.15) * APRON_SCUFF_CELL;
        let scuff_radius = 3.0 + 2.6 * scuff_hash.z;
        let scuff_rel = p - scuff_at;
        let scuff_d = length(scuff_rel);
        let scuff_dir = vec2f(cos(scuff_hash.w * 6.283), sin(scuff_hash.w * 6.283));
        let scuff_span = smoothstep(0.45, 0.9, dot(scuff_rel / max(scuff_d, 1e-3), scuff_dir));
        let inner_track = (scuff_d - scuff_radius) * (1.0 / 0.13);
        let outer_track = (scuff_d - scuff_radius - 1.7) * (1.0 / 0.13);
        let scuff = (exp(-inner_track * inner_track) + exp(-outer_track * outer_track)) * scuff_span
            * smoothstep(0.3, 0.7, apron_noise(vec2f(scuff_d * 4.0, atan2(scuff_rel.y, scuff_rel.x) * 9.0)) + 0.3);
        m = m * wear_tint(vec3f(0.38, 0.37, 0.36), clamp(scuff, 0.0, 1.0) * 0.4);
        roughness = mix(roughness, 0.82, clamp(scuff, 0.0, 1.0));
    }
    // Fresh oil: a spill in some cells (more of them beside the containers, where
    // handlers park), dark and still wet at its core with a thin-film sheen, and a
    // drip trailing off it.
    let stain_cell = floor(p / APRON_STAIN_CELL);
    let spill = apron_hash(stain_cell + vec2f(5.0, 29.0));
    let near_cover = 1.0 - smoothstep(1.0, 7.0, fixture.x);
    if fract(spill.x * 13.7 + spill.y * 7.1) <= 0.04 + 0.34 * near_cover {
        let spill_at = (stain_cell + 0.2 + 0.6 * spill.xy) * APRON_STAIN_CELL;
        let spill_dir = vec2f(cos(spill.w * 6.283), sin(spill.w * 6.283));
        let spill_radius = 0.25 + 0.9 * spill.z * spill.z;
        let shape = apron_blotch(p, spill_at, spill_dir, 1.8, spill_radius, (apron_noise(p * 1.9 + spill.xy * 40.0) - 0.5) * 0.7);
        let drip_at = spill_at + spill_dir * spill_radius * (1.6 + spill.y);
        let drip = apron_blotch(p, drip_at, spill_dir, 1.0, 0.12 + 0.12 * spill.x, (apron_noise(p * 6.0) - 0.5) * 0.5);
        let oil = max(1.0 - smoothstep(0.55, 1.0, shape), (1.0 - smoothstep(0.6, 1.0, drip)) * 0.8);
        let oil_core = 1.0 - smoothstep(0.0, 0.7, shape);
        // A thin film's colours brighten the dark oil a little; they never darken
        // it into a ring.
        let film = smoothstep(0.2, 0.42, shape) * (1.0 - smoothstep(0.42, 0.62, shape));
        let film_tint = 0.9 + 0.3 * cos(6.283 * (shape * 2.6 + vec3f(0.0, 0.33, 0.67)));
        m = m * wear_tint(vec3f(0.24, 0.23, 0.22), oil * mix(0.55, 0.8, spill.z));
        m = m * wear_tint(film_tint * 1.25, film * 0.3);
        m = m * (1.0 - oil_core * 0.5);
        // Oil soaked into dusty concrete is a dark stain with a faint sheen, not a
        // mirror.
        roughness = mix(roughness, 0.72, oil);
        roughness = mix(roughness, 0.66, oil_core);
    }
    // Salt bloom from the spray along the foot of the boundary wall.
    let salt_reach = 1.0 - smoothstep(0.0, 2.2 + mottle * 2.0, -outside - 0.8);
    if salt_reach > 0.0 {
        let salt = salt_reach * smoothstep(0.45, 0.7, apron_noise(p * 0.9 + 17.0));
        m = m * wear_tint(vec3f(1.12, 1.12, 1.1) + 0.03 / WEAR_ALBEDO, salt * 0.45);
    }
    return vec4f(m, roughness);
}

// An asphalt lane's wear at `p`, `across` metres from its centre line: the binder
// oxidised paler and browner where the traffic thins and ravelled at the edges,
// each relaid section its own age, rubber-polished wheel paths with an oil strip
// between them, polished hardest before a crossing, skid streaks and drips.
fn wear_asphalt(p: vec2f, along_z: bool, grime: f32, mottle: f32, offsets: vec4f, fixture: vec2f) -> vec4f {
    let across = select(offsets.y, offsets.x, along_z);
    let half = select(offsets.w, offsets.z, along_z);
    let lane_along = select(p.x, p.y, along_z);
    let traffic = apron_traffic(across, half);
    let worn = apron_noise(p / 11.0 + 5.0);
    let ravel = smoothstep(half - 1.1, half - 0.15, abs(across)) * (0.5 + 0.5 * mottle);
    let skid = smoothstep(0.58, 0.8, apron_noise(vec2f(across * 3.2, lane_along * 0.09)) * 0.75
        + apron_noise(vec2f(across * 9.0, lane_along * 0.6)) * 0.25) * traffic.x;
    var drips = 0.0;
    if traffic.y > 0.0 {
        drips = smoothstep(0.5, 0.7, apron_noise(vec2f(across * 2.2, lane_along * 0.31)) * 0.7
            + apron_noise(vec2f(across * 5.0, lane_along * 1.7)) * 0.3) * traffic.y;
    }
    let lane_centre = select(p.y - offsets.y, p.x - offsets.x, along_z);
    let lane_seed = apron_hash(vec2f(lane_centre, 71.0));
    let section = apron_hash(vec2f(floor(lane_along / APRON_SECTION + lane_seed.x), lane_centre * 0.37 + 5.0));
    let oxidised = clamp(worn * (1.0 - traffic.x * 0.7) + (section.y - 0.5) * 0.6, 0.0, 1.0);
    var m = mix(0.88, 1.1, section.x) * mix(vec3f(0.92), vec3f(1.3, 1.26, 1.2), oxidised) * mix(1.0, 1.2, ravel) * grime;
    // Braking and pulling away polish the wheel paths hardest in the last stretch
    // before a crossing lane.
    let crossing_off = select(offsets.x, offsets.y, along_z);
    let crossing_half = select(offsets.z, offsets.w, along_z);
    let braking = 1.0 - smoothstep(0.0, 9.0, abs(crossing_off) - crossing_half);
    m = m * mix(1.0, 0.72, traffic.x * (0.75 + 0.25 * mottle)) * mix(1.0, 0.8, braking * traffic.x);
    m = m * mix(1.0, 0.8, traffic.y * smoothstep(0.3, 0.7, mottle + worn * 0.4));
    m = m * wear_tint(vec3f(0.42, 0.41, 0.4), skid * 0.55);
    m = m * wear_tint(vec3f(0.5, 0.49, 0.48), drips * 0.6);
    m = m * wear_tint(vec3f(0.85, 0.55, 0.34), fixture.y * 0.5);
    var roughness = mix(0.9, 0.82, oxidised);
    roughness = mix(roughness, 0.68, traffic.x * 0.8);
    roughness = mix(roughness, 0.72, skid);
    roughness = mix(roughness, 0.7, drips);
    // The drainage channel along the open edges is precast concrete: the lane's
    // binder and traffic stop at it, its dirt shades it.
    let edge_d = half - abs(across);
    let open_edge = smoothstep(0.0, 0.25, abs(crossing_off) - crossing_half);
    let gutter = (1.0 - smoothstep(APRON_GUTTER - 0.06, APRON_GUTTER + 0.06, edge_d)) * open_edge;
    let channel = grime * mix(0.72, 1.0, smoothstep(0.3, 0.7, mottle));
    m = mix(m, vec3f(channel), gutter);
    roughness = mix(roughness, WEAR_CONCRETE_ROUGHNESS, gutter);
    return vec4f(m, roughness);
}

@fragment
fn fs_bake(input: BakeOut) -> @location(0) vec4f {
    let p = (input.uv - 0.5) * APRON_WEAR_EXTENT;
    // The yard's mottling, its grime and its largest scale: whole bays bleached by
    // the sun and others darkened by years of traffic and run-off, held within a
    // few steps of value so the shadows stay the darkest thing on the yard.
    let mottle = apron_noise(p / 9.0) * 0.6 + apron_noise(p / 2.7 + 7.0) * 0.4;
    let broad = apron_noise(p / 43.0 + 3.0) * 0.65 + apron_noise(p / 15.0 + 8.0) * 0.35;
    let vast = apron_noise(p / 71.0 + 21.0) * 0.7 + apron_noise(p / 29.0 + 5.0) * 0.3;
    let grime = mix(0.88, 1.07, smoothstep(0.25, 0.75, mottle)) * mix(0.84, 1.1, smoothstep(0.28, 0.72, broad))
        * mix(0.84, 1.14, smoothstep(0.25, 0.75, vast));
    let offsets = apron_lane_offsets(p);
    let lane_edge = min(abs(offsets.x) - offsets.z, abs(offsets.y) - offsets.w);
    let fixture = apron_fixture(p, mottle);
    let outside = max(abs(p.x), abs(p.y)) - APRON_ARENA;
    // The lanes along x are laid over those along z where they cross.
    let along_x = abs(offsets.y) < offsets.w && abs(p.x) < APRON_LANE_REACH;
    let along_z = abs(offsets.x) < offsets.z && abs(p.y) < APRON_LANE_REACH;
    var wear = vec4f(0.0);
    if along_x || along_z {
        wear = wear_asphalt(p, !along_x, grime, mottle, offsets, fixture);
    } else {
        wear = wear_concrete(p, grime, mottle, offsets, fixture, outside);
    }
    // Contact occlusion against fixed cover and the boundary wall, and the damp
    // hollows that hold water after a shower, deepest as puddles: water lies only
    // where the yard is low, in the sawn joints, along the lanes' edges where the
    // camber drains, round the drain covers and at the foot of the wall.
    let contact = (1.0 - smoothstep(0.0, 0.8, fixture.x)) * 0.42 + (1.0 - smoothstep(0.0, 3.2, fixture.x)) * 0.16;
    let hollow = apron_noise(p / 7.0 + 3.3) * 0.7 + apron_noise(p / 2.1) * 0.3;
    let slab_uv = p / APRON_SLAB;
    let to_edge = (0.5 - abs(fract(slab_uv) - 0.5)) * APRON_SLAB;
    let edge = min(to_edge.x, to_edge.y);
    let cover_cell = floor(p / APRON_COVER_CELL);
    let cover_hash = apron_hash(cover_cell + vec2f(41.0, 7.0));
    let cover_d = length(p - (cover_cell + 0.25 + 0.5 * cover_hash.xy) * APRON_COVER_CELL);
    let has_cover = step(0.72, cover_hash.w) * step(0.9, abs(lane_edge)) * step(1.5, fixture.x);
    let low = max(
        max(1.0 - smoothstep(0.12, 0.7, edge), 1.0 - smoothstep(0.25, 1.4, abs(lane_edge))),
        max((1.0 - smoothstep(0.6, 2.4, cover_d)) * has_cover, 1.0 - smoothstep(0.3, 2.0, -outside)),
    );
    var damp = smoothstep(0.79, 0.86, hollow) * low;
    var puddle = smoothstep(0.86, 0.9, hollow) * low;
    let asphalt = along_x || along_z;
    if !asphalt {
        // Rain left standing in strips along the lanes' edges, where the camber
        // runs it off, broken into runs so it never outlines a lane evenly.
        let gutter_run = apron_noise(p / 5.5 + 31.0);
        let gutter = (1.0 - smoothstep(0.04, 0.16 + gutter_run * 0.2, abs(lane_edge) + (mottle - 0.5) * 0.12))
            * smoothstep(0.62, 0.78, gutter_run) * smoothstep(0.4, 1.2, fixture.x);
        damp = max(damp, smoothstep(0.0, 0.45, gutter));
        puddle = max(puddle, smoothstep(0.5, 0.8, gutter));
    }
    var m = wear.rgb * (1.0 - contact * select(1.0, 0.8, asphalt));
    m = m * mix(1.0, select(0.74, 0.84, asphalt), damp) * mix(1.0, select(0.66, 0.72, asphalt), puddle);
    // Standing water is a dull sheen under the yard's dust film, never a mirror.
    var roughness = mix(wear.a, 0.86, damp);
    roughness = mix(roughness, 0.74, puddle);
    // Rust bleeding out round a cast-iron cover's frame.
    let cover_radius = 0.34 + 0.12 * cover_hash.z;
    let cover_rust = (1.0 - smoothstep(cover_radius, cover_radius + 0.25 + mottle * 0.3, cover_d))
        * step(cover_radius, cover_d) * has_cover * smoothstep(0.3, 0.6, mottle);
    m = m * wear_tint(vec3f(0.8, 0.55, 0.38), cover_rust * select(0.6, 0.5, asphalt));
    return vec4f(clamp(m / APRON_WEAR_RANGE, vec3f(0.0), vec3f(1.0)), roughness);
}

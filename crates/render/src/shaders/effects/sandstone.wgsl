// Quarried sandstone (`quarry-surfaces.ts` sandstoneMaterial): the photo map
// sampled triplanar in world space for grain, a strata palette per bed with iron
// staining in thin wavy bands, cross-bedded laminations, desert varnish down
// steep faces, chipped pale edges and dark partings, lichen on the caprock, a
// centimetre grain for the close cameras, and soil at the foot. Extra texture 0 is the soil
// bake (`soil_at`), extra texture 1 the packed-dirt grit.
//
// `uv.x` names a bed (constant per triangle): positive on the cover rocks and
// rubble (`sandstone_rock`), negative on the backdrop faces (`quarry_bench`,
// from -100) and the distant mesas (from -300), whose palette is calmer so the
// arena leads; zero falls back to beds that follow world height.
// `uv.y` is wear: +1 a chipped lip, -1 a cavity or parting; -3 marks the sand
// skirt round a rock's toe, shaded as the floor it melts into; sawn, drilled
// faces carry their wear plus 3.

// Spacing of the drill traces along a sawn or blasted face, metres.
const DRILL_SPACING = 0.85;
// Warm light bounced off the sunlit floor into faces turned from the sky.
const FLOOR_BOUNCE = vec3f(1.0, 0.8, 0.62);

fn soil_at(p: vec3f) -> vec4f {
    let uv = vec2f(p.x / 210.0 + 0.5, 0.5 - p.z / 210.0);
    return textureSample(extra_texture0, extra_sampler0, uv);
}

fn strata_hash(n: f32) -> f32 {
    return fract(sin(n * 91.3458 + 3.17) * 47453.5453);
}

fn strata_noise(p: vec2f) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = strata_hash(i.x + i.y * 57.0);
    let b = strata_hash(i.x + 1.0 + i.y * 57.0);
    let c = strata_hash(i.x + (i.y + 1.0) * 57.0);
    let d = strata_hash(i.x + 1.0 + (i.y + 1.0) * 57.0);
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Bed colors: pale cream, buff, honey, a dusty rose and a rare bleached band.
// Iron oxide stains the beds in bands rather than painting a whole bed red.
fn bed_color(layer: f32) -> vec3f {
    let h = strata_hash(layer);
    let shade = strata_hash(layer + 17.3);
    var c = srgb_hex(0xdcc39au);
    c = select(c, srgb_hex(0xd2b088u), h > 0.25);
    c = select(c, srgb_hex(0xcaa37eu), h > 0.5);
    c = select(c, srgb_hex(0xc29a7eu), h > 0.74);
    c = select(c, srgb_hex(0xe2d3b4u), h > 0.92);
    return c * (0.94 + 0.1 * shade);
}

// Vertical half-pipe traces left by the drill holes of a blast or a cut: x is the
// groove (1 in its trough), y the groove's slope across the face.
fn drill_traces(along: f32, y: f32, salt: f32, footprint: f32) -> vec2f {
    let cell = along / DRILL_SPACING;
    let id = floor(cell);
    let h = quarry_hash(vec2f(id, salt));
    // Each hole was drilled to its own depth from the bench above.
    let reach = quarry_smoothstep(h.y * 2.5 - 0.5, h.y * 2.5, y - h.z * 1.5);
    let x = (fract(cell) - 0.5) * DRILL_SPACING / 0.07;
    let visible = quarry_smoothstep(0.08, 0.03, footprint);
    let groove = max(1.0 - x * x, 0.0) * reach * visible * step(0.15, h.x);
    return vec2f(groove, clamp(x, -1.0, 1.0) * groove);
}

// Spacing of the vertical joints across an outcrop's faces, metres.
const JOINT_SPACING = 1.4;

// Vertical joints splitting a face across its beds: x the crack (1 in it), y
// the paler broken lip beside it. Each joint wanders a little, is open over
// a few beds of the face rather than its whole height, and thins to a faint
// line before a pixel spans it.
fn face_joints(along: f32, y: f32, salt: f32, footprint: f32) -> vec2f {
    let wobble = (quarry_value(vec2f(along * 0.7, y * 1.3) + salt) - 0.5) * 0.45;
    let u = along / JOINT_SPACING + wobble;
    let id = floor(u);
    let h = quarry_hash(vec2f(id, salt + 3.0));
    let low = h.y * 2.2 - 0.6;
    let high = low + 0.9 + h.z * 2.4;
    let span = quarry_smoothstep(low, low + 0.15, y) * (1.0 - quarry_smoothstep(high - 0.15, high, y))
        * step(0.4, h.x);
    let x = abs(fract(u) - 0.5 - (h.w - 0.5) * 0.6) * JOINT_SPACING;
    let width = 0.022 + 0.022 * h.z;
    let soft = max(footprint, 0.004);
    let crack = (1.0 - quarry_smoothstep(width - soft, width + soft, x)) * width / max(width, footprint) * span;
    let lip = quarry_smoothstep(width * 2.5 + 0.05, width, x) * (1.0 - crack) * span;
    return vec2f(crack, lip);
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let n = (*s).normal;
    let weights = pow(abs(n), vec3f(6.0));
    let blend = weights / max(weights.x + weights.y + weights.z, 0.0001);
    let p = f.world;
    let footprint = quarry_footprint(p);
    // The cover rocks take the photo at a finer scale: its laminations shrink
    // to a sandy grain instead of stretching into timber figure.
    let cover_rock = f.uv.x > 0.5 && f.uv.x < 99.5;
    let q = p / select(4.5, 2.4, cover_rock);
    // Tops take the photo crossed with itself at a right angle, so its bedding
    // laminations average into weathered grain instead of streaking over a cap.
    let top_grain = (textureSample(map_texture, map_sampler, q.xz + vec2f(0.57, 0.43))
        + textureSample(map_texture, map_sampler, vec2f(q.z, -q.x) * 1.37 + vec2f(0.21, 0.66))) * 0.5;
    let grain = (textureSample(map_texture, map_sampler, q.zy + vec2f(0.31, 0.11)) * blend.x
        + top_grain * blend.y
        + textureSample(map_texture, map_sampler, q.xy + vec2f(0.13, 0.79)) * blend.z).rgb;
    let soil = soil_at(p);
    let ground = soil.rgb;
    let sand_grit = dot(textureSample(extra_texture1, extra_sampler1, p.xz / 2.7).rgb, LUMA);
    let turned = vec2f(p.x * 0.6 + p.z * 0.8, p.z * 0.6 - p.x * 0.8) / 11.7;
    let grit_far = dot(textureSample(extra_texture1, extra_sampler1, turned).rgb, LUMA);
    let near = dot(textureSample(extra_texture1, extra_sampler1, p.xz / 3.3).rgb, LUMA);
    // A finer octave of the grain on the face's dominant plane only, for the
    // close views: one more sample instead of three.
    let dominant = select(select(q.xy, q.xz, blend.y > blend.z), q.zy, blend.x > max(blend.y, blend.z));
    let fine = dot(textureSample(map_texture, map_sampler, dominant * 3.7 + 0.37).rgb, LUMA);
    // Close up (the hero and first-person cameras) a centimetre grain from the
    // grit tile takes over where the photo has run out of texels.
    let micro = dot(textureSample(extra_texture1, extra_sampler1, dominant * 5.3 + 0.19).rgb, LUMA);
    let close = quarry_smoothstep(0.02, 0.004, footprint);
    let luma = dot(grain, LUMA) * mix(1.0, fine / 0.42, 0.35 + 0.3 * close)
        * mix(1.0, micro / 0.33, 0.5 * close);
    let warp = sin(p.x * 0.061 + p.z * 0.047) * 0.9 + sin(p.x * 0.19 - p.z * 0.23) * 0.35;
    let bed = p.y + warp;
    let along = p.x * 0.8 + p.z * 0.6;
    let rock_layer = abs(f.uv.x);
    // The boundary's cut blocks (beds 100 to 199) are old worked stone: greyer,
    // granular rather than laminated, mottled and streaked with varnish.
    let worked = select(0.0, 1.0, rock_layer > 99.5 && rock_layer < 199.5);
    let rocky = rock_layer > 0.5;
    let backdrop = f.uv.x < -0.5;
    let mesa = f.uv.x < -299.5;
    let skirt = f.uv.y < -1.5;
    let sawn = select(0.0, 1.0, f.uv.y > 1.5);
    // The face's own horizontal axis, for features laid out on it.
    let face_axis = normalize(vec2f(-n.z, n.x) + vec2f(0.0001, 0.0));
    let face_along = dot(p.xz, face_axis);
    let face_uv = vec2f(face_along, p.y * 1.3);
    let upright = quarry_smoothstep(0.5, 0.2, abs(n.y));
    // Cross-bedding: sets of inclined laminae, each set dipping its own way,
    // faint inside massive beds so faces never streak like sawn timber.
    let lamina_set = floor(p.y * 1.4 + strata_noise(vec2f(face_along * 0.2, rock_layer)) * 0.8);
    let set_dip = (strata_hash(lamina_set + rock_layer * 3.1) - 0.5) * 0.9;
    let lamina_phase = p.y + face_along * set_dip;
    let lamina = sin(lamina_phase * 21.0 + sin(along * 0.9) * 1.2) * 0.5 + 0.5;
    // Drill traces: everywhere on sawn faces, in bands on the blasted benches.
    let bench_band = quarry_smoothstep(0.55, 0.7, strata_noise(vec2f(face_along * 0.05, 3.0)));
    let bench_drilled = select(0.0, bench_band, backdrop && !mesa);
    let drilled = max(sawn, bench_drilled) * upright;
    let drill = drill_traces(face_along, p.y, rock_layer + floor(face_along / 9.0), footprint) * drilled;
    let rock_body = select(0.0, 1.0, rocky && !backdrop);
    // Joint cracks across the caprock, cut into the relief so their walls take
    // the sun on one side and fall into shade on the other.
    let top = quarry_smoothstep(0.6, 0.9, n.y);
    var cap_cracks = vec2f(0.0);
    var crack_lip = 0.0;
    var plate_tone = 0.5;
    if top > 0.05 && rocky && !backdrop && worked < 0.5 {
        let crack_p = p.xz + vec2f(rock_layer * 7.3, rock_layer * 3.1);
        let joints = quarry_cracks(crack_p, 3.1, 3.4, footprint);
        cap_cracks = joints.xy * top;
        plate_tone = joints.z;
        // The crack's far wall faces the sun: a lit line on the side away from it.
        let toward_sun = normalize(frame.sun_direction.xz + vec2f(0.0001, 0.0));
        let lit_wall = quarry_cracks(crack_p - toward_sun * 0.07, 3.1, 3.4, footprint).x * top;
        crack_lip = lit_wall * (1.0 - cap_cracks.x);
    }
    // Split faces of the worked blocks keep the mason's dressing: patches of
    // parallel point-chisel strokes slanting across the face, fading out before
    // a pixel spans one stroke.
    let tool_patch = quarry_smoothstep(0.35, 0.65, strata_noise(face_uv * 0.9 + vec2f(rock_layer * 2.1, 0.0)));
    let tool_wobble = strata_noise(face_uv * 2.7 + 5.0) * 3.0;
    let strokes = sin((face_along * 0.75 + p.y * 0.66) * 52.0 + tool_wobble);
    let tooled = tool_patch * worked * upright * (1.0 - sawn) * quarry_smoothstep(0.03, 0.012, footprint);
    let tooling = strokes * tooled;
    // Fracture relief: ridged noise on the face's own plane breaks every face
    // into shallow conchoidal flakes and spalls, so a face reads as split
    // stone rather than one planed board. Calmer on the backdrop, none on
    // sawn faces.
    // Its slope is analytic, in metres per metre, so the flakes keep their
    // shading whatever a pixel spans (a derivative bump of a smooth field
    // fades to nothing at a distance).
    let on_top = n.y > 0.7;
    let fracture_uv = select(face_uv, p.xz, on_top) + vec2f(rock_layer * 1.37, rock_layer * 0.71);
    // The faces of cover rocks and cut blocks break into flat facets (their
    // tops already part into jointed plates); the backdrop only swells
    // gently, at a fraction of the cost.
    var facets = vec3f(0.0);
    if rocky && !backdrop && sawn < 0.5 && !on_top {
        facets = quarry_facets(fracture_uv, 0.95);
    }
    let swell = quarry_value_grad(fracture_uv * 1.6);
    let fracture_slope = select(swell.yz * (1.6 * 0.035) * select(0.0, 1.0, backdrop), facets.xy * 0.16, rocky && !backdrop)
        * (1.0 - sawn);
    let fracture_tilt = select(
        vec3f(face_axis.x, 0.0, face_axis.y) * fracture_slope.x + vec3f(0.0, fracture_slope.y * 1.3, 0.0),
        vec3f(fracture_slope.x, 0.0, fracture_slope.y),
        on_top,
    );
    var joints = vec2f(0.0);
    if rocky && !backdrop && worked < 0.5 && upright > 0.05 {
        joints = face_joints(face_along, p.y, rock_layer * 1.7, footprint) * upright;
    }
    let height = -joints.x * 0.1 + tooling * 0.035 + luma * mix(0.8, 0.35, sawn)
        + lamina * (0.06 + 0.06 * close) * (1.0 - worked * 0.8) * (1.0 - rock_body * 0.75)
        - drill.x * 0.25 + micro * mix(0.25 * close, 0.5, max(worked, rock_body * 0.6))
        - cap_cracks.x * 0.12;
    var bumped = derivative_bump(height, 0.55, p, n, f.front_facing);
    bumped = normalize(bumped - fracture_tilt);
    let face_side = vec3f(face_axis.x, 0.0, face_axis.y);
    bumped = normalize(bumped + face_side * drill.y * 0.6);
    // Tafoni: honeycomb weathering pits clustered on steep weathered faces,
    // overhung and dark inside, with a paler rim. They hollow out the soft beds
    // and partings (negative wear) and stretch along the bedding.
    let face_wear = select(f.uv.y - sawn * 3.0, 0.0, skirt);
    let soft_bed = quarry_smoothstep(0.05, 0.5, -face_wear);
    let bedded_uv = vec2f(face_along * 0.55, p.y * 1.6);
    let cluster = strata_noise(bedded_uv * 0.42 + vec2f(rock_layer * 3.1, 0.0));
    let pit_density = quarry_smoothstep(0.5, 0.85, cluster) * upright * select(0.55, 0.3, rocky) * (1.0 - sawn)
        * mix(0.3, 1.7, soft_bed);
    let pits = quarry_pebbles(bedded_uv, 0.34, pit_density, footprint, 5.0);
    bumped = normalize(bumped - (face_side * pits.lean.x + vec3f(0.0, pits.lean.y, 0.0)) * 0.9);
    // Rocks color by their geometric beds; benches by world height.
    let bench_bed = bed / 1.05;
    let bench_layer = floor(bench_bed) + 200.0;
    let bench_edge = quarry_smoothstep(0.86, 1.0, fract(bench_bed));
    let bench_color = mix(
        mix(bed_color(bench_layer), bed_color(bench_layer + 1.0), bench_edge),
        srgb_hex(0xd8b48cu),
        0.45,
    );
    let bed_tint = mix(bed_color(rock_layer), srgb_hex(0xd8b48cu), select(0.0, 0.35, backdrop));
    var layered = select(bench_color, bed_tint, rocky);
    // Sun-bleached caprock: flat tops weather to one pale cream whatever bed they
    // expose, so no top reads as a stamped patch of the face's colors.
    let bleached = quarry_smoothstep(0.7, 0.95, n.y) * select(0.0, 0.42, rocky && !backdrop);
    // Each outcrop's cap (its own bed number) weathers its own way: most bleach
    // pale, some keep a honey tone, a few carry a darker iron crust, so the
    // field of rocks never reads as one stamped model.
    let cap_pick = quarry_hash(vec2f(floor(rock_layer), 3.0)).x;
    var cap_tone = srgb_hex(0xdbc4a0u);
    cap_tone = select(cap_tone, srgb_hex(0xd0aa7cu), cap_pick > 0.45);
    cap_tone = select(cap_tone, srgb_hex(0xa38a74u), cap_pick > 0.78);
    layered = mix(layered, cap_tone, bleached * select(1.0, 1.4, cap_pick > 0.45));
    layered *= 0.97 + lamina * (0.05 + 0.06 * close) * (1.0 - worked * 0.85) * (1.0 - rock_body * 0.8);
    layered = mix(layered, vec3f(dot(layered, LUMA)) * vec3f(1.07, 1.0, 0.9), worked * 0.5);
    // Colour runs along the bedding: long, thin streaks of paler and redder
    // stone inside each bed, so a face never reads as one flat paint.
    let streak_scale = select(vec2f(0.18, 2.6), vec2f(0.55, 0.8), rocky && !backdrop);
    let bed_streak = strata_noise(vec2f(face_along, p.y) * streak_scale + vec2f(rock_layer * 1.3, 0.0));
    let streak_tint = mix(vec3f(0.88, 0.84, 0.82), vec3f(1.05, 1.03, 1.0), bed_streak);
    layered = mix(layered, layered * streak_tint, upright * select(0.35, 0.6, rocky && !backdrop));
    // Iron oxide: thin wavy liesegang bands in patches, muted, never a painted
    // stripe.
    let band_wave = p.y * 5.2 + strata_noise(vec2f(face_along * 0.35, p.y * 0.6)) * 2.6 + rock_layer * 1.7;
    let band_patch = quarry_smoothstep(0.35, 0.7, strata_noise(vec2f(face_along * 0.12 + 7.0, p.y * 0.25)));
    let liesegang = pow(sin(band_wave) * 0.5 + 0.5, 6.0) * band_patch;
    layered = mix(layered, layered * vec3f(0.98, 0.72, 0.55), liesegang * 0.6 * (1.0 - sawn * 0.6));
    let tone = sin(p.x * 0.043 + sin(p.z * 0.031) * 2.0)
        * sin(p.z * 0.057 + p.y * 0.11 - p.x * 0.02) * 0.5 + 0.5;
    layered = mix(layered, layered * vec3f(0.92, 0.9, 0.88), quarry_smoothstep(0.6, 0.9, tone) * 0.5);
    let relief = mix(1.0, luma / 0.4, mix(select(0.55, 0.38, cover_rock), 0.3, sawn));
    var rock = layered * relief;
    // Sawn faces are fresh, paler stone, their drill traces holding shade.
    rock = mix(rock, rock * vec3f(1.08, 1.06, 1.03), sawn);
    rock *= 1.0 - drill.x * 0.4;
    // Desert varnish: dark streaks running down from ledges on steep faces.
    let steep = quarry_smoothstep(0.55, 0.15, abs(n.y));
    let varnish_column = strata_noise(vec2f(along * 1.6, p.y * 0.18));
    let varnish_drip = strata_noise(vec2f(along * 0.35, p.y * 0.9 + 3.0));
    let varnish = quarry_smoothstep(0.6 - worked * 0.12, 0.88, varnish_column) * steep
        * mix(0.45, 1.0, varnish_drip) * select(0.4, 0.7, backdrop || !rocky) * (1.0 - sawn * (1.0 - worked * 0.6));
    rock = mix(rock, rock * vec3f(0.55, 0.45, 0.38), varnish);
    let mottle_uv = select(face_uv, p.xz, n.y > 0.7);
    let block_mottle = strata_noise(mottle_uv * 1.9 + rock_layer * 5.3) * 0.6
        + strata_noise(mottle_uv * 5.1 + rock_layer) * 0.4;
    rock *= mix(1.0, 0.78 + 0.36 * block_mottle, worked * 0.85);
    // Granular speckle of the worked stone at every distance its grit resolves.
    rock *= mix(1.0, mix(1.0, micro / 0.33, 0.45), max(worked, rock_body) * quarry_smoothstep(0.09, 0.02, footprint));
    rock *= 1.0 - (tooling * 0.5 + 0.5) * tooled * 0.08;
    // Joints hold deep shade; their broken lips are fresh, paler stone.
    rock = mix(rock, rock * vec3f(0.36, 0.32, 0.3), joints.x * 0.85);
    rock = mix(rock, mix(rock, srgb_hex(0xf0e2c4u), 0.35), joints.y * 0.4);
    // Chipped lips are fresh, paler stone; partings and joints hold shade.
    let wear = face_wear;
    let lip = clamp(wear, 0.0, 1.0);
    let cavity = clamp(-wear, 0.0, 1.0);
    // Edge wear comes and goes along a lip: fresh chips in runs, weathered
    // stretches between, never one even highlight like a planed board's edge.
    let chip_run = quarry_smoothstep(0.35, 0.65, quarry_value(face_uv * 2.3 + vec2f(3.0, rock_layer)));
    let chipped = quarry_smoothstep(0.2, 0.8, lip * (0.6 + luma)) * (0.35 + 0.65 * chip_run);
    // Each facet a shade of its own, its ridges struck paler: the face reads as
    // chipped planes in the shade as well as in the sun.
    rock *= 1.0 + (facets.x * 0.035 + facets.z * 0.06) * select(0.0, 1.0, rocky && !backdrop) * quarry_smoothstep(0.05, 0.02, footprint);
    rock = mix(rock, mix(rock, srgb_hex(0xf0e2c4u), 0.5) * 1.1, chipped * 0.5);
    rock *= 1.0 - cavity * 0.48;
    rock = mix(rock, rock * vec3f(1.0, 0.9, 0.82), cavity * 0.15);
    // Pits hold shade under their overhanging lip; their rims are fresh stone.
    rock *= 1.0 - pits.cover * (0.3 + 0.35 * clamp(pits.lean.y * 1.5, 0.0, 1.0));
    rock *= 1.0 + pits.contact * 0.1;
    // Caprock tops: mottled by weathering, with sand caught in the hollows.
    let mottle = strata_noise(p.xz * 0.55 + rock_layer) * 0.6 + strata_noise(p.xz * 1.7) * 0.4;
    rock *= mix(1.0, 0.72 + mottle * 0.42, top);
    // Weathered caprock darkens in broad patches of varnish and grime, so a top
    // reads as old stone rather than fresh plaster.
    let aged = quarry_smoothstep(0.45, 0.75, strata_noise(p.xz * 0.21 + rock_layer * 2.3));
    rock = mix(rock, rock * vec3f(0.8, 0.74, 0.68), aged * top * select(0.0, 0.6, rocky && !backdrop));
    // Crustose lichen spots the oldest caprock: grey-green rosettes and the odd
    // orange one. The joints themselves are cut into the mesh.
    let lichen = quarry_pebbles(p.xz * 1.6 + rock_layer * 5.1, 0.5, select(0.0, 0.1, rocky && !backdrop) * top, footprint, 41.0);
    let lichen_tint = select(vec3f(0.62, 0.62, 0.5), vec3f(1.0, 0.62, 0.34), lichen.tint.x > 1.1);
    rock = mix(rock, rock * lichen_tint, lichen.cover * 0.55 * quarry_smoothstep(0.4, 0.75, mottle));
    // Joints and frost cracks craze the caprock into plates: dark lines choked
    // with sand, their edges rounded paler, so a top reads as old broken rock
    // from the overhead camera rather than one smooth slab.
    // Each plate between the joints has weathered on its own: a little paler
    // or darker, so the cap reads as broken slabs rather than one painted top.
    rock *= mix(1.0, 0.88 + 0.22 * plate_tone, top * rock_body);
    rock = mix(rock, rock * vec3f(0.34, 0.3, 0.29), cap_cracks.x * 0.8);
    rock *= 1.0 + cap_cracks.y * 0.05;
    rock = mix(rock, mix(rock, srgb_hex(0xf3e6cau), 0.45) * 1.12, crack_lip * 0.75);
    // Sandstone is salmon and buff, not pine: the outcrops' colour leans from
    // yellow toward a dusty rose, a step greyer, so under the warm sun they
    // never read as the timber crates beside them.
    rock = mix(rock, vec3f(dot(rock, LUMA)) * vec3f(1.07, 0.98, 0.93), rock_body * mix(0.28, 0.42, upright));
    // A film of fine dust dulls every top: paler and less saturated than the
    // stone under it, so sunlit caps read as weathered rock, not glazed clay.
    let dust_film = top * select(0.3, 0.45, rocky && !backdrop);
    let dusted = vec3f(dot(rock, LUMA)) * vec3f(1.04, 0.98, 0.9);
    rock = mix(rock, dusted, dust_film) * (1.0 - top * 0.08);
    // Ledge undersides and undercuts sit in their own shade.
    let under = quarry_smoothstep(-0.05, -0.7, n.y);
    rock *= mix(1.0, 0.72, under);
    // Windblown sand settles on ledges and in the caprock's joints and hollows;
    // the backdrop's broad treads hold sheets of it.
    let up = quarry_smoothstep(0.5, 0.9, n.y);
    let patchy = sin(p.x * 0.37 + sin(p.z * 0.29) * 1.7) * sin(p.z * 0.41 - p.x * 0.13) * 0.5 + 0.5;
    let pockets = quarry_smoothstep(0.62, 0.8, mottle * 0.7 + sand_grit * 0.5) + cap_cracks.y * 0.6;
    let sheets = quarry_smoothstep(0.35, 0.65, patchy * 0.6 + sand_grit * 0.7 + up * 0.25);
    let sand_cover = up * select(sheets, clamp(pockets, 0.0, 1.0) * 0.8, rocky && !backdrop);
    let sand = ground * (sand_grit * 0.35 + 0.9) * vec3f(1.06, 1.03, 0.98);
    rock = mix(rock, sand, sand_cover * 0.85);
    // Far treads of the benches: varnished desert pavement, darker than the
    // sunlit sand below, so each shelf steps back in value instead of reading
    // as a pale strip between the cliffs.
    let pavement = up * select(0.0, 1.0, backdrop && !mesa) * quarry_smoothstep(88.0, 125.0, length(p.xz));
    rock = mix(rock, rock * vec3f(0.74, 0.68, 0.63), pavement * 0.65);
    let chips = quarry_pebbles(p.xz, 0.55, select(0.0, select(0.14, 0.05, !backdrop), rocky) * up, footprint, 17.0);
    // Blast rubble left lying on the benches' shelves: boulders a metre or so
    // across with their shadows, so a tread reads as worked ground from above.
    let rubble = quarry_pebbles(p.xz, 1.9, select(0.0, 0.12, backdrop && !mesa) * up, footprint, 53.0);
    rock *= 1.0 - (rubble.shadow * 0.45 + rubble.contact * 0.15 + rubble.rim * 0.3);
    rock = mix(rock, mix(rock, bed_tint, 0.5) * rubble.tint * 0.85, rubble.cover);
    rock *= 1.0 - chips.contact * 0.2;
    rock = mix(rock, mix(rock, bed_tint, 0.6) * chips.tint, chips.cover);
    bumped = normalize(bumped + vec3f(chips.lean.x + rubble.lean.x, 0.0, chips.lean.y + rubble.lean.y));
    // The distant mesas: dark, rust-stained caprock cliffs over pale talus, then
    // cooler and flatter with distance as the air between thickens.
    let cliff = quarry_smoothstep(0.75, 0.35, n.y) * select(0.0, 1.0, mesa);
    rock = mix(rock * vec3f(1.08, 1.02, 0.94), rock * vec3f(0.7, 0.54, 0.43), cliff);
    let far = quarry_smoothstep(140.0, 290.0, length(p.xz));
    // Aerial perspective: warm, lifted shadows near, cooling into the sky's haze
    // with distance, never grey card.
    // Faces turned from the sun cool toward the sky's blue; lit faces keep their
    // warmth, so the haze layers the distance instead of greying it.
    let shaded_side = quarry_smoothstep(0.3, -0.4, dot(n, frame.sun_direction.xyz));
    // The far mesas carry deeper shade, so the haze over them still leaves a
    // lit and a shaded side, a form rather than a flat card.
    let shade_depth = select(1.0, 0.62, mesa);
    let distant = rock * mix(vec3f(1.04, 0.99, 0.93), vec3f(0.86, 0.9, 1.04) * shade_depth, shaded_side) + vec3f(0.01, 0.014, 0.026);
    rock = mix(rock, distant, far);
    // The far mesas sink further into the warm haze, so the near benches lead
    // and the ranges step back in value.
    rock = mix(rock, srgb_hex(0xe6cdb0u) * 0.84, far * select(0.0, 0.45, mesa));
    // `quarry_ground_drop`: flat to the wall's outer face (62 m), down 1.8 m by 66 m.
    let floor_y = -min(max(max(abs(p.x), abs(p.z)) - 62.0, 0.0) * 0.45, 1.8);
    let lift = p.y - floor_y;
    let foot = 1.0 - quarry_smoothstep(0.02, 0.9, lift);
    let splash = (1.0 - quarry_smoothstep(0.0, 0.35, lift)) * 0.25;
    rock = mix(rock, ground * 0.86, foot * 0.55);
    rock *= 1.0 - splash;
    // Outcrops darken toward their feet, where the sky is half hidden and the
    // dust settles, so each stands on the floor instead of floating over it.
    rock *= select(1.0, mix(0.7, 1.0, quarry_smoothstep(0.0, 3.0, lift)), rocky && !backdrop);
    // The skirt: the floor's own soil and grit, drifted paler up against the toe.
    let stony = clamp((soil.a - 0.5) * 2.0, 0.0, 1.0);
    let detail = near * (grit_far + 0.327) / (0.327 * 0.327 * 2.0);
    var drift = ground * mix(1.0, detail, mix(0.55, 1.5, stony * 0.5));
    drift = mix(drift, sand, quarry_smoothstep(0.0, 0.18, p.y) * 0.45);
    let color = select(rock, drift, skirt);
    bumped = select(bumped, normalize(mix(n, vec3f(0.0, 1.0, 0.0), 0.7)), skirt);
    (*s).color = color * f.vertex_color.rgb * material.color.rgb * f.tint.rgb;
    (*s).opacity = material.color.a * f.tint.a;
    (*s).normal = bumped;
    // Faces turned from the sky catch warm light bounced off the sunlit floor,
    // so undercuts read as warm shade instead of blue-black holes.
    let away = quarry_smoothstep(0.35, -0.8, n.y);
    // Upright faces turned from the sun take the most of it: the sunlit floor
    // in front of them is what lights their shade.
    let sun_side = quarry_smoothstep(0.15, -0.55, dot(n, frame.sun_direction.xyz)) * upright;
    let bounce = (0.05 + 0.22 * away + 0.1 * sun_side) * select(1.0, 0.0, skirt) * (1.0 - far);
    (*s).emissive += (*s).color * FLOOR_BOUNCE * bounce;
}

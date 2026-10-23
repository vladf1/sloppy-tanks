// Quarried sandstone (`quarry-surfaces.ts` sandstoneMaterial): the photo map
// sampled triplanar in world space for grain, a strata palette per bed in thin
// bands, desert varnish down steep faces, chipped pale edges and dark partings,
// jointed caprock, a centimetre grain for the close cameras, and soil at the foot. Extra texture 0 is the soil bake (`soil_at`), extra
// texture 1 the packed-dirt grit.
//
// `uv.x` names a bed (constant per triangle): positive on the cover rocks and
// rubble (`sandstone_rock`, below 100) and the boundary's cut blocks (100 to
// 199), negative on the backdrop faces (`quarry_bench`, from -100) and the
// distant mesas (from -300), whose palette is calmer so the arena leads; zero
// falls back to beds that follow world height.
// `uv.y` is wear: +1 a chipped lip, -1 a cavity or parting; -3 marks the sand
// skirt round a rock's toe, shaded as the floor it melts into; sawn, drilled
// faces carry their wear plus 3, loose stones (scree, spoil, chips) plus 6.
//
// The rock covers much of the quarry's screen, so every layer works out its noise
// only for the kinds of stone (cover rock, cut block, bench, mesa, skirt) and the
// faces where its weight can be non-zero; a skipped pixel keeps the value it would
// have computed; the sand skirt skips the rock's layers altogether. A layer costs
// every rock pixel even where it is skipped (the registers it holds), so what
// reads only up close or on a few stones is left out: pores, lichen, freckles,
// close-up flakes, frost hairlines, the cross-bedded laminae, tafoni pits, the
// faces' chipped facets, crust blotches and vertical joints, drill traces, chisel
// dressing, iron banding, the worked stone's mottle, the benches' blast rubble,
// the noise in the bands' lenses and the varnish's drips, and the caps' patina
// (which the caps' mottle stands in for).

// Warm light bounced off the sunlit floor into faces turned from the sky.
const FLOOR_BOUNCE = vec3f(1.0, 0.8, 0.62);
// The beds' value where their cross-bedded laminae were: the laminae are left
// out (on rock they moved its value by a percent), their mean shade stays.
const LAMINA_SHADE = 0.97;
// The highest the sand banks against worked stone, metres above the floor
// (its wandering edge's most).
const SAND_BANK_TOP = 0.62;

fn soil_at(p: vec3f) -> vec4f {
    let uv = vec2f(p.x / 210.0 + 0.5, 0.5 - p.z / 210.0);
    return textureSample(extra_texture0, extra_sampler0, uv);
}

// A hash in [0, 1) without a sine (Hoskins' hash11): a sine per lattice corner
// cost WebKit's Metal a sixth of the rock's time.
fn strata_hash(n: f32) -> f32 {
    var x = fract(n * 0.1031);
    x *= x + 33.33;
    x *= x + x;
    return fract(x);
}

// Smooth value noise in [0, 1] on a unit lattice: the noise tile's first field
// (`ground_noise.wgsl`), which repeats far beyond any face and has no precision
// to lose at large arguments.
fn strata_noise(p: vec2f) -> f32 {
    return ground_noise(p).x;
}

// Smooth value noise in [0, 1] along a line, one of a family `salt` names (a row
// of the tile's second field), for fields that vary only along a face or a joint.
fn line_noise(x: f32, salt: f32) -> f32 {
    return ground_noise(vec2f(x, salt * 1.618 + 0.5)).y;
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

// Tint of one thin bed inside a course: cream, rose, ochre or a darker iron band.
fn strata_band(n: f32) -> vec3f {
    let pick = strata_hash(n);
    var t = vec3f(1.09, 1.06, 1.0);
    t = select(t, vec3f(1.0, 0.86, 0.82), pick > 0.36);
    t = select(t, vec3f(0.97, 0.85, 0.71), pick > 0.6);
    t = select(t, vec3f(0.92, 0.7, 0.6), pick > 0.78);
    t = select(t, vec3f(0.8, 0.72, 0.68), pick > 0.92);
    return t;
}

// Spacing of the caprock's two joint sets, and of the cracks across the
// boundary's cut blocks, metres.
const CAP_JOINT_SPACING = vec2f(2.6, 3.7);
const BLOCK_CRACK_SPACING = 4.6;

struct CapJoints {
    // The open crack (1 in it) and the rounded shoulder beside it (1 at the lip).
    crack: f32,
    shoulder: f32,
    // The shoulder's slope (d height / d x, d z) and the crack's width, metres.
    slope: vec2f,
    width: f32,
    // Which joint interval the point lies in, naming its plate.
    cell: f32,
}

// One set of parallel joints across a caprock, running across `dir`. Each joint
// wanders, opens wide and pinches shut along its length, and weathers a rounded
// shoulder down into it, so the low sun lights one lip and leaves the groove and
// the far lip in shade. A joint thins to a faint seam before a pixel spans it.
fn cap_joint_set(p: vec2f, dir: vec2f, spacing: f32, salt: f32, footprint: f32) -> CapJoints {
    let along = dot(p, vec2f(-dir.y, dir.x));
    let wander = (line_noise(along * 0.3, salt) - 0.5) * 1.1;
    let u = (dot(p, dir) + wander) / spacing;
    let id = floor(u);
    let h = ground_hash(vec2f(id, salt));
    let d = (fract(u) - 0.5 - (h.x - 0.5) * 0.35) * spacing;
    let open = line_noise(along * 0.45 + h.y * 13.0, id + salt);
    let present = quarry_smoothstep(0.2, 0.42, open) * step(0.25, fract(h.x * 7.3 + h.y * 3.1));
    let width = (0.025 + 0.13 * open * open) * present;
    let reach = width * 1.2 + 0.12;
    let x = abs(d);
    let soft = max(footprint, 0.004);
    var j: CapJoints;
    j.crack = (1.0 - quarry_smoothstep(width - soft, width + soft, x)) * width / max(width, footprint);
    let t = clamp((width + reach - x) / reach, 0.0, 1.0) * present;
    j.shoulder = t * t * (1.0 - j.crack);
    j.slope = dir * (sign(d) * t * 0.7 * (1.0 - j.crack) * quarry_smoothstep(0.07, 0.02, footprint));
    j.width = width;
    // Plates part at the joint, not at the lattice line between joints.
    j.cell = id + step(0.0, d);
    return j;
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let n = (*s).normal;
    let p = f.world;
    // The position's screen derivatives, taken here in uniform control flow: the
    // samples only some kinds of stone take run inside their branches with them.
    let dpx = dpdx(p);
    let dpy = dpdy(p);
    let footprint = max(length(dpx), length(dpy));
    // Graphics tiers without fine detail (`fine_detail`) skip the close-up grain;
    // with it this is 1 and every weight is what it always was.
    let tier_detail = select(0.0, 1.0, fine_detail());
    // The kinds of stone, constant per triangle: the cover rocks and the loose
    // stones round them (beds below 100), the boundary's cut blocks (100 to 199)
    // and the benches (-100 to -199), which are both worked stone, the mesas, and
    // the sand skirt round a rock's toe.
    let rock_layer = abs(f.uv.x);
    let cover_rock = f.uv.x > 0.5 && f.uv.x < 99.5;
    let skirt = f.uv.y < -1.5;
    let upright = quarry_smoothstep(0.5, 0.2, abs(n.y));
    // The face's dominant plane, for the finer octaves: one sample instead of three.
    let weights = n * n * n * n * n * n;
    let blend = weights / max(weights.x + weights.y + weights.z, 0.0001);
    let scale = select(4.5, 2.4, cover_rock);
    let q = p / scale;
    let qx = dpx / scale;
    let qy = dpy / scale;
    let dominant = select(select(q.xy, q.xz, blend.y > blend.z), q.zy, blend.x > max(blend.y, blend.z));
    let soil = soil_at(p);
    let ground = soil.rgb;
    let sand_grit = dot(textureSample(extra_texture1, extra_sampler1, p.xz / 2.7).rgb, LUMA);
    let near = dot(textureSample(extra_texture1, extra_sampler1, p.xz / 3.3).rgb, LUMA);
    // Close up (the hero and first-person cameras) a centimetre grain from the
    // grit tile takes over where the photo has run out of texels.
    let micro = dot(textureSample(extra_texture1, extra_sampler1, dominant * 5.3 + 0.19).rgb, LUMA);
    let close = quarry_smoothstep(0.02, 0.004, footprint) * tier_detail;
    let sand = ground * (sand_grit * 0.35 + 0.9) * vec3f(1.06, 1.03, 0.98);

    // The skirt is the floor's own soil, so it skips the rock's layers altogether;
    // the rock works out its colour, the height its relief is bumped from and the
    // analytic slopes of its layers (`tilt`), normalized once at the end.
    var color = vec3f(0.0);
    var height = micro * 0.3;
    var tilt = vec3f(0.0);
    var far = 0.0;
    if skirt {
        // The skirt: the floor's own soil and grit, drifted paler up against the toe.
        let turn = mat2x2f(0.6, -0.8, 0.8, 0.6) * (1.0 / 11.7);
        let grit_far = dot(textureSampleGrad(extra_texture1, extra_sampler1, turn * p.xz, turn * dpx.xz, turn * dpy.xz).rgb, LUMA);
        let stony = clamp((soil.a - 0.5) * 2.0, 0.0, 1.0);
        let detail = near * (grit_far + 0.327) / (0.327 * 0.327 * 2.0);
        color = ground * mix(1.0, detail, mix(0.55, 1.5, stony * 0.5));
        color = mix(color, sand, quarry_smoothstep(0.0, 0.18, p.y) * 0.45);
    } else {
        let worked = select(0.0, 1.0, rock_layer > 99.5 && rock_layer < 199.5);
        let rocky = rock_layer > 0.5;
        let backdrop = f.uv.x < -0.5;
        let mesa = f.uv.x < -299.5;
        let loose = f.uv.y > 4.5;
        // Rock in the arena (cover rocks, loose stones and the cut blocks) rather
        // than the backdrop.
        let outcrop = rocky && !backdrop;
        let block = outcrop && worked > 0.5;
        var luma = 0.0;
        if cover_rock {
            // The cover rocks (and the loose stones round them) are weathered to a
            // granular crust: the grit tile's speckle, not the photo's laminations,
            // which read as wood figure from above and curve into it over faces and
            // shoulders. The bedding comes from the beds' own bands instead. Faces
            // take the grit on their own dominant plane, never the top's projection
            // streaked down them.
            let dominant_dx = select(select(qx.xy, qx.xz, blend.y > blend.z), qx.zy, blend.x > max(blend.y, blend.z));
            let dominant_dy = select(select(qy.xy, qy.xz, blend.y > blend.z), qy.zy, blend.x > max(blend.y, blend.z));
            let face_grit = dot(textureSampleGrad(extra_texture1, extra_sampler1, dominant * 1.6 + 0.53, dominant_dx * 1.6, dominant_dy * 1.6).rgb, LUMA);
            luma = mix(sand_grit * 0.6 + near * 0.4, face_grit * 0.6 + micro * 0.4, 1.0 - blend.y) / 0.327 * 0.42;
        } else {
            // The worked stone and the backdrop take the photo triplanar. Tops take
            // it crossed with itself at a right angle, so its bedding laminations
            // average into weathered grain instead of streaking over a cap.
            let top_grain = (textureSampleGrad(map_texture, map_sampler, q.xz + vec2f(0.57, 0.43), qx.xz, qy.xz)
                + textureSampleGrad(map_texture, map_sampler, vec2f(q.z, -q.x) * 1.37 + vec2f(0.21, 0.66), vec2f(qx.z, -qx.x) * 1.37, vec2f(qy.z, -qy.x) * 1.37)) * 0.5;
            let grain = (textureSampleGrad(map_texture, map_sampler, q.zy + vec2f(0.31, 0.11), qx.zy, qy.zy) * blend.x
                + top_grain * blend.y
                + textureSampleGrad(map_texture, map_sampler, q.xy + vec2f(0.13, 0.79), qx.xy, qy.xy) * blend.z).rgb;
            luma = dot(grain, LUMA);
        }
        luma *= mix(1.0, micro / 0.33, 0.5 * close);
        let warp = sin(p.x * 0.061 + p.z * 0.047) * 0.9 + sin(p.x * 0.19 - p.z * 0.23) * 0.35;
        let sawn_cut = select(0.0, 1.0, f.uv.y > 1.5 && !loose);
        // A cover rock's sawn face has weathered back to broken stone since the
        // cut, so it never reads as a planed board.
        let sawn = select(sawn_cut, 0.0, cover_rock);
        // The face's own horizontal axis, for features laid out on it. The
        // benches' faces wander through every bearing over their buttresses and
        // gullies: an axis turning with the smooth normal would bend the features
        // laid out along it into a zigzag maze, so they take the nearest of the
        // world's two horizontal axes instead.
        let rounded_axis = normalize(vec2f(-n.z, n.x) + vec2f(0.0001, 0.0));
        let snapped_axis = select(vec2f(sign(-n.z + 0.0001), 0.0), vec2f(0.0, sign(n.x)), abs(n.x) > abs(n.z));
        let face_axis = select(rounded_axis, snapped_axis, backdrop);
        let face_along = dot(p.xz, face_axis);
        let face_side = vec3f(face_axis.x, 0.0, face_axis.y);
        let rock_body = select(0.0, 1.0, outcrop);
        // Joint cracks across the caprock, cut into the relief so their walls take
        // the sun on one side and fall into shade on the other.
        let top = quarry_smoothstep(0.6, 0.9, n.y);
        // Two sets of joints cross each outcrop's cap near square, the first
        // through-going, the second stopping and starting against it, as jointed
        // sandstone caprock parts: long weathered grooves with rounded lips, sand
        // in the wide ones, never an even crazing of tiles. Loose stones are too
        // small to carry a joint; the boundary's cut blocks carry only the odd
        // crack right across a block, so no two tops in the line read the same.
        var cap_cracks = vec2f(0.0);
        var cap_sand = 0.0;
        var plate_tone = 0.5;
        if top > 0.05 && outcrop && !loose {
            let cap_hash = quarry_hash(vec2f(floor(rock_layer), 11.0));
            let turn = cap_hash.x * 3.14;
            let first_dir = vec2f(cos(turn), sin(turn));
            let second_dir = vec2f(-sin(turn + 0.12), cos(turn + 0.12));
            // Each cap breaks at its own scale: big tops into broad slabs, small
            // ones into tighter blocks.
            let joint_scale = 0.75 + 0.6 * cap_hash.y;
            let first = cap_joint_set(p.xz, first_dir, select(CAP_JOINT_SPACING.x * joint_scale, BLOCK_CRACK_SPACING, block), rock_layer * 1.3, footprint);
            var second = CapJoints(0.0, 0.0, vec2f(0.0), 0.0, 0.0);
            if !block {
                second = cap_joint_set(p.xz, second_dir, CAP_JOINT_SPACING.y * joint_scale, rock_layer * 1.3 + 40.0, footprint);
            }
            cap_cracks = vec2f(max(first.crack, second.crack), max(first.shoulder, second.shoulder)) * top;
            let wide = select(second.width, first.width, first.crack >= second.crack);
            cap_sand = quarry_smoothstep(0.03, 0.07, wide) * top;
            // Each slab between the joints has settled and weathered on its own: a
            // shade and a slight tilt of its own, so the cap reads as broken slabs.
            let plate = quarry_hash(vec2f(first.cell, second.cell) + rock_layer * 0.37);
            plate_tone = plate.x;
            let cap_slope = first.slope + second.slope + (plate.yz - 0.5) * 0.09;
            tilt = -vec3f(cap_slope.x, 0.0, cap_slope.y) * top;
        }
        // The backdrop's faces swell gently, their slope analytic, in metres per
        // metre, so the swells keep their shading whatever a pixel spans.
        if backdrop {
            let fracture_uv = select(vec2f(face_along, p.y * 1.3), p.xz, n.y > 0.7) + vec2f(rock_layer * 1.37, rock_layer * 0.71);
            let fracture_slope = quarry_value_grad(fracture_uv * 1.6).yz * (1.6 * 0.035) * (1.0 - sawn);
            tilt -= select(
                face_side * fracture_slope.x + vec3f(0.0, fracture_slope.y * 1.3, 0.0),
                vec3f(fracture_slope.x, 0.0, fracture_slope.y),
                n.y > 0.7,
            );
        }
        height = luma * mix(0.8, 0.35, sawn)
            + micro * mix(0.25 * close, 0.5, max(worked, rock_body * 0.6))
            - luma * select(0.0, 0.4, backdrop)
            - cap_cracks.x * 0.08;
        let face_wear = f.uv.y - sawn_cut * 3.0 - select(0.0, 6.0, loose);
        // Rocks color by their geometric beds; anything without a bed by world height.
        let bed_tint = mix(bed_color(rock_layer), srgb_hex(0xd8b48cu), select(0.0, 0.35, backdrop));
        var layered = bed_tint;
        if !rocky {
            let bench_bed = (p.y + warp) / 1.05;
            let bench_layer = floor(bench_bed) + 200.0;
            let bench_edge = quarry_smoothstep(0.86, 1.0, fract(bench_bed));
            layered = mix(
                mix(bed_color(bench_layer), bed_color(bench_layer + 1.0), bench_edge),
                srgb_hex(0xd8b48cu),
                0.45,
            );
        }
        // Sun-bleached caprock: flat tops weather to one pale cream whatever bed they
        // expose, so no top reads as a stamped patch of the face's colors. Each
        // outcrop's cap (its own bed number) weathers its own way: most bleach
        // pale, some keep a honey tone, a few carry a darker iron crust, so the
        // field of rocks never reads as one stamped model.
        let bleached = quarry_smoothstep(0.7, 0.95, n.y) * select(0.0, 0.42, outcrop);
        let cap_pick = quarry_hash1(vec2f(floor(rock_layer), 3.0));
        var cap_tone = srgb_hex(0xd8c8b4u);
        cap_tone = select(cap_tone, srgb_hex(0xcfb096u), cap_pick > 0.45);
        cap_tone = select(cap_tone, srgb_hex(0xa08c80u), cap_pick > 0.78);
        layered = mix(layered, cap_tone, bleached * select(1.0, 1.4, cap_pick > 0.45));
        layered *= LAMINA_SHADE;
        // Beds of one outcrop differ in value as well as hue: hard, pale beds over
        // softer, darker ones, so the courses read as stone, not stacked boards.
        layered *= mix(1.0, 0.84 + 0.3 * strata_hash(rock_layer * 2.3 + 5.1), rock_body * upright);
        layered = mix(layered, vec3f(dot(layered, LUMA)) * vec3f(1.1, 1.04, 0.95), worked * 0.68);
        // Thin beds inside each course: bands a hand or two thick on the outcrops
        // (a long stride on the benches), each a tone of its own (cream, rose,
        // ochre, a darker iron band), parted by a fine seam under a paler lip, so a
        // face reads as bedded sandstone rather than one paint or a board's grain.
        // Steep faces only: across a sloping shoulder the bands would curve into
        // contour lines, a board's figure.
        let band_size = select(0.42, 0.8, backdrop);
        let sheer = quarry_smoothstep(0.3, 0.1, abs(n.y));
        let band_fade = quarry_smoothstep(band_size * 0.5, band_size * 0.12, footprint);
        let banding = sheer * band_fade * select(select(0.45, 0.55, rock_body > 0.5), 0.6, backdrop)
            * (1.0 - sawn * 0.5) * (1.0 - worked * 0.7) * select(0.6, 0.0, mesa || loose);
        var band_seam = 0.0;
        var band_lip = 0.0;
        if banding > 0.0 {
            // Gently undulating, never looping round a swell like a knot in a board.
            let band_y = p.y + warp * 0.25 + (line_noise(face_along * 0.11, rock_layer * 0.7) - 0.5) * band_size * 0.5;
            let band_t = band_y / band_size + 0.8 * sin(band_y / band_size * 0.45 + rock_layer * 1.9);
            let band_id = floor(band_t);
            let band_f = fract(band_t);
            let band_salt = floor(rock_layer) * 0.61 + 2.0;
            let band_mix = quarry_smoothstep(0.82, 1.0, band_f);
            let band_tint = mix(strata_band(band_id * 1.37 + band_salt), strata_band((band_id + 1.0) * 1.37 + band_salt), band_mix);
            layered = mix(layered, layered * band_tint, banding);
            band_seam = (1.0 - quarry_smoothstep(0.0, 0.08, band_f)) * quarry_smoothstep(0.03, 0.008, footprint) * banding;
            band_lip = quarry_smoothstep(0.06, 0.12, band_f) * (1.0 - quarry_smoothstep(0.12, 0.32, band_f)) * banding;
            // Each thin bed has weathered back at its own angle, leaning out or in a
            // little, so the sun picks the beds apart as well as their colours.
            let band_lean = (strata_hash(band_id * 2.71 + band_salt) - 0.5) * 0.32 * banding;
            tilt += vec3f(0.0, band_lean - band_seam * 0.3, 0.0);
        }
        let tone = sin(p.x * 0.043 + sin(p.z * 0.031) * 2.0)
            * sin(p.z * 0.057 + p.y * 0.11 - p.x * 0.02) * 0.5 + 0.5;
        layered = mix(layered, layered * vec3f(0.92, 0.9, 0.88), quarry_smoothstep(0.6, 0.9, tone) * 0.5);
        // The benches' photo laminae curve into wood figure over their long faces:
        // there the bands carry the bedding instead.
        let relief = mix(1.0, luma / 0.4, mix(select(select(0.55, 0.36, backdrop), 0.38, cover_rock), 0.3, sawn));
        var rock = layered * relief;
        rock *= (1.0 - band_seam * 0.4) * (1.0 + band_lip * 0.07);
        // Sawn faces are fresh, paler stone.
        rock = mix(rock, rock * vec3f(1.08, 1.06, 1.03), sawn);
        // Desert varnish: dark streaks running down from ledges on steep faces.
        let steep = quarry_smoothstep(select(0.55, 0.35, cover_rock), select(0.15, 0.08, cover_rock), abs(n.y));
        let varnish_weathered = 1.0 - sawn * (1.0 - worked * 0.6);
        if steep != 0.0 && varnish_weathered != 0.0 {
            let varnish_column = strata_noise(vec2f(face_along * 1.6 + rock_layer * 3.3, p.y * 0.18));
            let varnish = quarry_smoothstep(0.6 - worked * 0.12 - select(0.0, 0.1, cover_rock), 0.88, varnish_column) * steep
                * 0.8 * select(select(0.4, 0.62, cover_rock), 0.7, backdrop || !rocky) * varnish_weathered;
            rock = mix(rock, rock * vec3f(0.55, 0.45, 0.38), varnish);
        }
        // Each of the boundary's blocks came out of its own part of the bed: a value
        // of its own, so the line never reads as one block repeated.
        rock *= mix(1.0, 0.82 + 0.34 * strata_hash(floor(rock_layer) * 1.7 + 0.4), worked);
        // Granular speckle of the worked stone at every distance its grit resolves.
        rock *= mix(1.0, mix(1.0, micro / 0.33, select(0.45, 0.65, cover_rock)), max(worked, rock_body) * quarry_smoothstep(0.09, 0.02, footprint));
        // Chipped lips are fresh, paler stone; partings and joints hold shade.
        let lip = clamp(face_wear, 0.0, 1.0);
        let cavity = clamp(-face_wear, 0.0, 1.0);
        let chipped = quarry_smoothstep(0.2, 0.8, lip * (0.6 + luma)) * select(0.68, 0.48, cover_rock);
        rock = mix(rock, mix(rock, srgb_hex(0xf0e2c4u), 0.5) * 1.1, chipped * 0.5);
        rock *= 1.0 - cavity * 0.48;
        rock = mix(rock, rock * vec3f(1.0, 0.9, 0.82), cavity * 0.15);
        // Caprock tops: mottled by weathering, with sand caught in the hollows.
        // Read by the tops and, through the sand pockets, by up-facing cover rock.
        let up = quarry_smoothstep(0.5, 0.9, n.y);
        var mottle = 0.0;
        if top != 0.0 || (outcrop && up != 0.0) {
            mottle = strata_noise(p.xz * 0.55 + rock_layer) * 0.6 + strata_noise(p.xz * 1.7) * 0.4;
        }
        rock *= mix(1.0, 0.72 + mottle * 0.42, top);
        if top != 0.0 && outcrop {
            // Weathered caprock darkens in broad patches of varnish and grime, so a
            // top reads as old stone rather than fresh plaster, its plates each a
            // little paler or darker, and crisp-edged blotches of darker, greyer
            // varnish lie over the pale crust: all from the mottle.
            let aged = quarry_smoothstep(0.3, 0.75, 1.0 - mottle);
            rock = mix(rock, rock * vec3f(0.78, 0.72, 0.68), aged * top * 0.8);
            rock *= mix(1.0, 0.82 + 0.34 * plate_tone, top);
            let patina = quarry_smoothstep(0.56, 0.6, mottle * 0.5 + luma * 0.6) * top * select(0.55, 0.0, loose);
            rock = mix(rock, vec3f(dot(rock, LUMA)) * vec3f(0.84, 0.8, 0.76), patina);
        }
        // The weathered lips round off paler; the groove holds deep shade, and sand
        // drifts into the bottom of the wide ones.
        rock = mix(rock, mix(rock, srgb_hex(0xeee0c6u), 0.3), cap_cracks.y * 0.35);
        rock = mix(rock, rock * vec3f(0.3, 0.27, 0.26), cap_cracks.x * 0.85);
        rock = mix(rock, ground * 0.62, cap_sand * cap_cracks.x * 0.7);
        // Sandstone is salmon and buff, not pine: the outcrops' colour leans from
        // yellow toward a dusty rose, a step greyer, so under the warm sun they
        // never read as the timber crates beside them.
        rock = mix(rock, vec3f(dot(rock, LUMA)) * vec3f(1.1, 0.93, 0.93), rock_body * mix(0.66, 0.72, upright));
        // Outcrops across the floor come from different parts of the bed: some
        // pale and bleached, some deeper rose. A field broad enough that each
        // outcrop takes one tone, so no two read as the same stamped model.
        if cover_rock {
            let outcrop_tone = quarry_value(p.xz / 23.0 + vec2f(5.3, -2.1));
            rock *= mix(vec3f(1.05, 1.02, 0.97), vec3f(0.93, 0.83, 0.78), quarry_smoothstep(0.25, 0.75, outcrop_tone));
        }
        // A film of fine dust dulls every top: paler and less saturated than the
        // stone under it, so sunlit caps read as weathered rock, not glazed clay.
        let dust_film = top * select(0.3, 0.45, outcrop);
        let dusted = vec3f(dot(rock, LUMA)) * vec3f(1.04, 0.98, 0.9);
        rock = mix(rock, dusted, dust_film) * (1.0 - top * 0.08);
        // Ledge undersides and undercuts sit in their own shade.
        let under = quarry_smoothstep(-0.05, -0.7, n.y);
        rock *= mix(1.0, 0.72, under);
        // Windblown sand settles on ledges and in the caprock's joints and hollows;
        // the backdrop's broad treads hold sheets of it.
        var sand_cover = up * quarry_smoothstep(0.62, 0.8, mottle * 0.7 + sand_grit * 0.5) * 0.8;
        if !outcrop {
            let patchy = sin(p.x * 0.37 + sin(p.z * 0.29) * 1.7) * sin(p.z * 0.41 - p.x * 0.13) * 0.5 + 0.5;
            sand_cover = up * quarry_smoothstep(0.35, 0.65, patchy * 0.6 + sand_grit * 0.7 + up * 0.25);
        }
        rock = mix(rock, sand, sand_cover * 0.85);
        // Far treads of the benches: varnished desert pavement, darker than the
        // sunlit sand below, so each shelf steps back in value instead of reading
        // as a pale strip between the cliffs.
        if backdrop && !mesa && up > 0.0 {
            let pavement = up * quarry_smoothstep(66.0, 100.0, length(p.xz));
            rock = mix(rock, rock * vec3f(0.72, 0.66, 0.61), pavement * 0.78);
        }
        // The distant mesas: dark, rust-stained caprock cliffs over pale talus, then
        // cooler and flatter with distance as the air between thickens.
        let cliff = quarry_smoothstep(0.75, 0.35, n.y) * select(0.0, 1.0, mesa);
        rock = mix(rock * vec3f(1.08, 1.02, 0.94), rock * vec3f(0.7, 0.54, 0.43), cliff);
        far = quarry_smoothstep(140.0, 290.0, length(p.xz));
        if far > 0.0 {
            // Aerial perspective: warm, lifted shadows near, cooling into the sky's
            // haze with distance, never grey card. Faces turned from the sun cool
            // toward the sky's blue; lit faces keep their warmth, so the haze layers
            // the distance instead of greying it.
            let shaded_side = quarry_smoothstep(0.3, -0.4, dot(n, frame.sun_direction.xyz));
            // The far mesas carry deeper shade, so the haze over them still leaves a
            // lit and a shaded side, a form rather than a flat card.
            let shade_depth = select(1.0, 0.62, mesa);
            let distant = rock * mix(vec3f(1.04, 0.99, 0.93), vec3f(0.86, 0.9, 1.04) * shade_depth, shaded_side) + vec3f(0.01, 0.014, 0.026);
            rock = mix(rock, distant, far);
            // The far mesas sink further into the warm haze, so the near benches lead
            // and the ranges step back in value.
            rock = mix(rock, srgb_hex(0xe6cdb0u) * 0.84, far * select(0.0, 0.45, mesa));
        }
        // `quarry_ground_drop`: flat to the wall's outer face (62 m), down 1.8 m by 66 m.
        let floor_y = -min(max(max(abs(p.x), abs(p.z)) - 62.0, 0.0) * 0.45, 1.8);
        let lift = p.y - floor_y;
        let foot = 1.0 - quarry_smoothstep(0.02, 0.9, lift);
        let splash = (1.0 - quarry_smoothstep(0.0, 0.35, lift)) * 0.25;
        rock = mix(rock, ground * 0.86, foot * select(0.55, 0.2, loose));
        rock *= 1.0 - splash * select(1.0, 0.3, loose);
        // Outcrops darken toward their feet, where the sky is half hidden and the
        // dust settles, so each stands on the floor instead of floating over it.
        rock *= select(1.0, mix(select(0.7, 0.86, loose), 1.0, quarry_smoothstep(0.0, 3.0, lift)), outcrop);
        // Loose stones are the floor's own stone under a film of its dust: their
        // value sits inside the ground's, a step paler or darker, never black.
        rock = mix(rock, ground * vec3f(1.04, 1.0, 0.95) * (0.82 + 0.4 * luma), select(0.0, 0.28, loose));
        // Sand banks against the foot of the worked stone and into the joints between
        // the boundary's blocks, its edge wandering along the line, never higher
        // than `SAND_BANK_TOP`.
        if worked > 0.5 && upright > 0.0 && lift < SAND_BANK_TOP {
            let bank = 0.34 + 0.16 * sin(face_along * 0.9 + rock_layer * 1.3) + 0.12 * sin(face_along * 2.1 + rock_layer);
            let banked = upright * (1.0 - quarry_smoothstep(bank * 0.6, bank, lift));
            rock = mix(rock, sand * 0.97, banked * 0.85);
        }
        color = rock;
    }
    let bumped_height = derivative_bump(height, 0.55, p, n, f.front_facing);
    let bumped = select(normalize(bumped_height + tilt), normalize(mix(n, vec3f(0.0, 1.0, 0.0), 0.7)), skirt);
    (*s).color = color * f.vertex_color.rgb * material.color.rgb * f.tint.rgb;
    (*s).opacity = material.color.a * f.tint.a;
    (*s).normal = bumped;
    // Faces turned from the sky catch warm light bounced off the sunlit floor,
    // so undercuts read as warm shade instead of blue-black holes.
    let away = quarry_smoothstep(0.35, -0.8, n.y);
    // Upright faces turned from the sun take the most of it: the sunlit floor
    // in front of them is what lights their shade.
    let sun_side = quarry_smoothstep(0.15, -0.55, dot(n, frame.sun_direction.xyz)) * upright;
    let rock_light = select(1.0, 0.0, skirt);
    let bounce = (0.05 + 0.22 * away + 0.1 * sun_side) * rock_light * (1.0 - far);
    (*s).emissive += (*s).color * FLOOR_BOUNCE * bounce;
    // The cool sky fill would turn pale sandstone lavender: rock takes the sky's
    // light at its own brightness, a little warm, by trading the fill's blue
    // for that. Never more than the fill gives, so no channel goes negative.
    let sky = frame.sky_color.rgb;
    let warm_sky = vec3f(dot(sky, LUMA)) * vec3f(1.02, 1.0, 0.95);
    // Fully on the tops; faces keep some of the sky's cool, so their shade
    // never reads as warm timber, but not enough to turn mauve.
    let sky_share = (bumped.y * 0.5 + 0.5) * mix(0.45, 0.8, quarry_smoothstep(0.3, 0.9, bumped.y)) * rock_light;
    (*s).emissive += (*s).color * (warm_sky - sky) * sky_share * RECIPROCAL_PI;
}

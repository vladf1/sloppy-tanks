// Layered sandstone (`quarry-surfaces.ts` sandstoneMaterial): the photo map
// sampled triplanar in world space for grain, a strata palette per bed, fine
// laminations, desert varnish down steep faces, chipped pale edges and dark
// cavities, sand caught on ledges and soil at the foot. Extra texture 0 is the
// soil bake (`soilAt`), extra texture 1 the packed-dirt grit.
//
// `uv.x` names a bed (constant per triangle): positive on the cover rocks and
// rubble (`sandstone_rock`), negative on the backdrop faces (`quarry_bench`),
// whose palette is calmer so the arena leads; zero falls back to beds that
// follow world height.
// `uv.y` is wear: +1 a chipped lip, -1 a cavity or joint.

fn soil_at(p: vec3f) -> vec3f {
    let uv = vec2f(p.x / 210.0 + 0.5, 0.5 - p.z / 210.0);
    return textureSample(extra_texture0, extra_sampler0, uv).rgb;
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

// Bed colors: pale cream, buff, honey, salmon, rust and a rare bleached band.
fn bed_color(layer: f32) -> vec3f {
    let h = strata_hash(layer);
    let shade = strata_hash(layer + 17.3);
    var c = srgb_hex(0xe9d3aau);
    c = select(c, srgb_hex(0xddb788u), h > 0.22);
    c = select(c, srgb_hex(0xd6a476u), h > 0.45);
    c = select(c, srgb_hex(0xc98660u), h > 0.66);
    c = select(c, srgb_hex(0xb06a48u), h > 0.82);
    c = select(c, srgb_hex(0xefe2c6u), h > 0.94);
    return c * (0.93 + 0.12 * shade);
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let n = (*s).normal;
    let weights = pow(abs(n), vec3f(6.0));
    let blend = weights / max(weights.x + weights.y + weights.z, 0.0001);
    let p = f.world;
    let q = p / 4.5;
    let grain = (textureSample(map_texture, map_sampler, q.zy + vec2f(0.31, 0.11)) * blend.x
        + textureSample(map_texture, map_sampler, q.xz + vec2f(0.57, 0.43)) * blend.y
        + textureSample(map_texture, map_sampler, q.xy + vec2f(0.13, 0.79)) * blend.z).rgb;
    let ground = soil_at(p);
    let sand_grit = dot(textureSample(extra_texture1, extra_sampler1, p.xz / 2.7).rgb, LUMA);
    let luma = dot(grain, LUMA);
    let warp = sin(p.x * 0.061 + p.z * 0.047) * 0.9 + sin(p.x * 0.19 - p.z * 0.23) * 0.35;
    let bed = p.y + warp;
    // Fine laminations inside every bed, cross-bedded in some of them.
    let along = p.x * 0.8 + p.z * 0.6;
    let rock_layer = abs(f.uv.x);
    let rocky = rock_layer > 0.5;
    let backdrop = f.uv.x < -0.5;
    let cross_bedded = strata_hash(rock_layer + 5.1) > 0.6;
    let lamina_phase = select(bed, p.y + along * 0.32, rocky && cross_bedded);
    let lamina = sin(lamina_phase * 23.0 + sin(along * 0.9) * 1.4) * 0.5 + 0.5;
    let height = luma * 0.8 + lamina * 0.12;
    let bumped = derivative_bump(height, 0.55, p, n, f.front_facing);
    // Rocks color by their geometric beds; benches by world height.
    let bench_bed = bed / 1.05;
    let bench_layer = floor(bench_bed) + 200.0;
    let bench_edge = quarry_smoothstep(0.86, 1.0, fract(bench_bed));
    // The backdrop's beds are calmer than the cover's, so the arena leads.
    let bench_color = mix(
        mix(bed_color(bench_layer), bed_color(bench_layer + 1.0), bench_edge),
        srgb_hex(0xd8b48cu),
        0.45,
    );
    let bed_tint = mix(bed_color(rock_layer), srgb_hex(0xd8b48cu), select(0.0, 0.35, backdrop));
    var layered = select(bench_color, bed_tint, rocky);
    layered *= 0.96 + lamina * 0.07;
    let tone = sin(p.x * 0.043 + sin(p.z * 0.031) * 2.0)
        * sin(p.z * 0.057 + p.y * 0.11 - p.x * 0.02) * 0.5 + 0.5;
    layered = mix(layered, layered * vec3f(0.92, 0.9, 0.88), quarry_smoothstep(0.6, 0.9, tone) * 0.5);
    let relief = mix(1.0, luma / 0.4, 0.55);
    var rock = layered * relief;
    // Desert varnish: dark streaks running down from ledges on steep faces.
    let steep = quarry_smoothstep(0.55, 0.15, abs(n.y));
    let varnish_column = strata_noise(vec2f(along * 1.6, p.y * 0.18));
    let varnish_drip = strata_noise(vec2f(along * 0.35, p.y * 0.9 + 3.0));
    let varnish = quarry_smoothstep(0.58, 0.86, varnish_column) * steep
        * mix(0.45, 1.0, varnish_drip) * select(0.45, 0.75, backdrop || !rocky);
    rock = mix(rock, rock * vec3f(0.52, 0.42, 0.36), varnish);
    // Chipped lips are fresh, paler stone; cavities and joints hold shade.
    let wear = f.uv.y;
    let lip = clamp(wear, 0.0, 1.0);
    let cavity = clamp(-wear, 0.0, 1.0);
    let chipped = quarry_smoothstep(0.2, 0.8, lip * (0.6 + luma));
    rock = mix(rock, mix(rock, srgb_hex(0xf2e4c6u), 0.5) * 1.08, chipped * 0.55);
    rock *= 1.0 - cavity * 0.5;
    rock = mix(rock, rock * vec3f(1.0, 0.86, 0.74), cavity * 0.3);
    // Windblown sand settles on ledges and caps, patchy where the grit runs high.
    let up = quarry_smoothstep(0.5, 0.9, n.y);
    let patchy = sin(p.x * 0.37 + sin(p.z * 0.29) * 1.7) * sin(p.z * 0.41 - p.x * 0.13) * 0.5 + 0.5;
    let sand_cover = up * quarry_smoothstep(0.35, 0.65, patchy * 0.6 + sand_grit * 0.7 + up * 0.25);
    let sand = ground * (sand_grit * 0.35 + 0.9) * vec3f(1.06, 1.03, 0.98);
    rock = mix(rock, sand, sand_cover * 0.85);
    // `quarry_ground_drop`: flat to the wall's outer face (62 m), down 1.8 m by 66 m.
    let floor_y = -min(max(max(abs(p.x), abs(p.z)) - 62.0, 0.0) * 0.45, 1.8);
    let lift = p.y - floor_y;
    let foot = 1.0 - quarry_smoothstep(0.02, 0.9, lift);
    let splash = (1.0 - quarry_smoothstep(0.0, 0.35, lift)) * 0.25;
    rock = mix(rock, ground * 0.86, foot * 0.55);
    rock *= 1.0 - splash;
    (*s).color = rock * f.vertex_color.rgb * material.color.rgb * f.tint.rgb;
    (*s).opacity = material.color.a * f.tint.a;
    (*s).normal = bumped;
}

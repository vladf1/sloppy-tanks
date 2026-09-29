// Layered sandstone (`quarry-surfaces.ts` sandstoneMaterial): the photo map
// sampled triplanar in world space, bedding, tone patches, rain streaks, sand on
// ledges and soil at the foot. Extra texture 0 is the soil bake (`soilAt`).
fn soil_at(p: vec3f) -> vec3f {
    let uv = vec2f(p.x / 210.0 + 0.5, 0.5 - p.z / 210.0);
    return textureSample(extra_texture0, extra_sampler0, uv).rgb;
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let n = (*s).normal;
    let weights = pow(abs(n), vec3f(6.0));
    let blend = weights / max(weights.x + weights.y + weights.z, 0.0001);
    let p = f.world;
    let q = p / 6.5;
    let grain = (textureSample(map_texture, map_sampler, q.zy + vec2f(0.31, 0.11)) * blend.x
        + textureSample(map_texture, map_sampler, q.xz + vec2f(0.57, 0.43)) * blend.y
        + textureSample(map_texture, map_sampler, q.xy + vec2f(0.13, 0.79)) * blend.z).rgb;
    let ground = soil_at(p);
    let luma = dot(grain, LUMA);
    let relief = mix(vec3f(luma), grain, 0.35) / 0.4;
    let bumped = derivative_bump(luma, 0.6, p, n, f.front_facing);
    let warp = sin(p.x * 0.061 + p.z * 0.047) * 0.9 + sin(p.x * 0.19 - p.z * 0.23) * 0.35;
    let bed = p.y + warp;
    let broad = sin(bed * 1.3 + sin(bed * 0.47) * 1.8) * 0.5 + 0.5;
    let parting = quarry_smoothstep(0.82, 1.0, sin(bed * 4.7 + sin(bed * 1.9) * 2.1));
    var layered = mix(srgb_hex(0xdbc6a4u), srgb_hex(0xc99f7fu), broad);
    let tone = sin(p.x * 0.043 + sin(p.z * 0.031) * 2.0)
        * sin(p.z * 0.057 + p.y * 0.11 - p.x * 0.02) * 0.5 + 0.5;
    layered = mix(layered, srgb_hex(0xb8a58cu), quarry_smoothstep(0.62, 0.9, tone) * 0.55);
    layered = mix(layered, srgb_hex(0xc78a5cu), quarry_smoothstep(0.35, 0.08, tone) * 0.35);
    let steep = 1.0 - abs(n.y);
    let along = p.x + p.z;
    let streak = quarry_smoothstep(
        0.55,
        1.0,
        sin(along * 2.3 + sin(along * 0.61) * 3.0) * sin(along * 0.37 + p.y * 0.35),
    ) * steep;
    var rock = relief * layered * (1.0 - parting * 0.15 - streak * 0.14);
    let up = quarry_smoothstep(0.5, 0.92, n.y);
    let patchy = sin(p.x * 0.37 + sin(p.z * 0.29) * 1.7) * sin(p.z * 0.41 - p.x * 0.13) * 0.5 + 0.5;
    rock = mix(rock, ground * (dot(relief, LUMA) * 0.3 + 0.75), up * mix(0.2, 0.7, patchy));
    let floor_y = -min(max(max(abs(p.x), abs(p.z)) - 60.0, 0.0) * 0.3, 1.8);
    let foot = 1.0 - quarry_smoothstep(0.02, 0.75, p.y - floor_y);
    rock = mix(rock, ground * 0.9, foot * 0.6);
    (*s).color = rock * f.vertex_color.rgb * material.color.rgb * f.tint.rgb;
    (*s).opacity = material.color.a * f.tint.a;
    (*s).normal = bumped;
}

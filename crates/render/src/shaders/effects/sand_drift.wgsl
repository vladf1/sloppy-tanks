// Sand drifted around rock cover (`quarry-surfaces.ts` sandstoneFooting) and dust
// stained up the boundary wall: the soil bake under the fragment (the material's
// map) with grit, faded by the RGBA vertex color's alpha. Geometric normals. On
// upright faces the bake would smear into vertical streaks, so they take a
// heavily blurred mip and no grit.
fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let p = f.world;
    let upright = 1.0 - quarry_smoothstep(0.35, 0.7, abs((*s).normal.y));
    let soil_uv = vec2f(p.x / 210.0 + 0.5, 0.5 - p.z / 210.0);
    let ground = textureSampleBias(map_texture, map_sampler, soil_uv, upright * 7.0).rgb;
    let grit = quarry_grit(0.7 * (1.0 - upright), 0.0, f, (*s).normal);
    (*s).color = ground * grit.color * 1.05 * f.vertex_color.rgb * material.color.rgb * f.tint.rgb;
    (*s).opacity = material.color.a * f.vertex_color.a * f.tint.a;
}

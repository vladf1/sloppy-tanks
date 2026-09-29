// Sand drifted around rock cover (`quarry-surfaces.ts` sandstoneFooting): the
// soil bake under the fragment (the material's map) with grit, faded by the RGBA
// vertex color's alpha. Geometric normals.
fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let p = f.world;
    let soil_uv = vec2f(p.x / 210.0 + 0.5, 0.5 - p.z / 210.0);
    let ground = textureSample(map_texture, map_sampler, soil_uv).rgb;
    let grit = quarry_grit(0.7, 0.0, f, (*s).normal);
    (*s).color = ground * grit.color * 1.05 * f.vertex_color.rgb * material.color.rgb * f.tint.rgb;
    (*s).opacity = material.color.a * f.vertex_color.a * f.tint.a;
}

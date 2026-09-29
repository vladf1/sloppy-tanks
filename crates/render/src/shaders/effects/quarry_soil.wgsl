// Baked soil with world-space grit (`quarry-terrain.ts` soilMaterial): the map is
// the soil bake at the mesh UVs (alpha: grittiness); extra texture 0 is the grit.
fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let map_uv = f.uv * material.map_transform.xy + material.map_transform.zw;
    let baked = textureSample(map_texture, map_sampler, map_uv);
    let stony = (baked.a - 0.5) * 2.0;
    let grit = quarry_grit(mix(0.55, 1.5, stony), mix(0.35, 1.6, stony), f, (*s).normal);
    (*s).color = baked.rgb * grit.color * f.vertex_color.rgb * material.color.rgb * f.tint.rgb;
    (*s).opacity = material.color.a * f.tint.a;
    (*s).normal = grit.normal;
}

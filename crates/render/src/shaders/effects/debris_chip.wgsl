// Lit debris chips: shards, clods and splinters shaded like the scene.
// instance_data.w 0: a chip, with a per-instance glow for clods still hot from a
// fireball, instance_data.xyz the linear HDR glow added to the emissive light.
// instance_data.w 1: a piece of sawn wood (`wood_chips.rs`), whose vertex colour's red
// shades it and green marks its bare wood (a splinter's end grain and split sides, a
// board's back, edges and ends), which shows instance_data.xyz, the wood's own pale
// colour, where its faces show the tint, the struck wood's or paint's.
fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    if f.instance_data.w > 0.5 {
        (*s).color = mix(f.tint.rgb, f.instance_data.xyz, f.vertex_color.g) * f.vertex_color.r;
    } else {
        (*s).emissive += f.instance_data.xyz;
    }
}

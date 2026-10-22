// Lit debris chips: shards, clods and splinters shaded like the scene, with a
// per-instance glow for clods still hot from a fireball. instance_data.xyz =
// linear HDR glow added to the emissive light.
fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    (*s).emissive += f.instance_data.xyz;
}

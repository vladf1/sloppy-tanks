// Burnt wreck darkening (`wreck-aging.ts`): the base color (after vertex colors
// and map) and the emissive glow are both scaled by the brightness. Presentation
// feeds it per instance as instance_data = (brightness, 1, -, -), so one shared
// wreck model serves every age; otherwise params[0].x is the brightness.
fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let brightness = select(material.params[0].x, f.instance_data.x, f.instance_data.y > 0.5);
    (*s).color *= brightness;
    (*s).emissive *= brightness;
}

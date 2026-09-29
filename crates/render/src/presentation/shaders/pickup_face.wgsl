// Pickup pictograms glow with their own texture (Three's emissive white with the
// atlas as emissiveMap). params[0].x = glow strength.
fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    (*s).emissive += (*s).color * material.params[0].x;
}

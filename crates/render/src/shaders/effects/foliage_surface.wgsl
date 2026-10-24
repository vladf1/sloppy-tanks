// Tree foliage cards' surface (`effects_scenery::FOLIAGE`), shared with every lit
// surface variant (`EffectDefinition::shared_surface`). The standard shader flips a
// double-sided card's normal on its back face; foliage normals point out of the
// crown instead, so both faces keep them.
fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    if !f.front_facing {
        (*s).normal = -(*s).normal;
    }
}

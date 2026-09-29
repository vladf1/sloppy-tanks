// Sample surface effect: glowing bands scroll up the surface and thin its opacity.
// params[0] = (glow rgb, speed); params[1] = (band frequency, minimum opacity, -, -).
fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let glow = material.params[0];
    let shape = material.params[1];
    let band = 0.5 + 0.5 * sin(f.world.y * shape.x - frame.camera_position.w * glow.w);
    (*s).emissive += glow.rgb * band * band;
    (*s).opacity *= mix(shape.y, 1.0, band);
}

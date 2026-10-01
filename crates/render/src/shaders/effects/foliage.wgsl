// Tree foliage cards (`effects_scenery::FOLIAGE`). The crown sways with the square
// of the height above its instance origin (the trunk base for a standing tree), so
// the stump stays put and the top moves a few centimetres; neighbouring trees take
// their phase from the world x/z. A faster per-vertex flutter shimmers the leaves.
fn effect_world(w: ptr<function, EffectWorld>, v: EffectVertex) {
    let t = frame.camera_position.w;
    let p = (*w).position;
    let h = max(p.y - v.instance_origin.y, 0.0);
    let phase = dot(p.xz, vec2f(0.11, 0.07));
    let sway = (sin(t * 0.9 + phase) * 0.6 + sin(t * 1.7 + phase * 1.3) * 0.25) * 0.0025 * h * h;
    let flutter = sin(t * 6.0 + dot(p, vec3f(3.1, 2.3, 2.7))) * 0.01 * min(h, 1.0);
    (*w).position = p + vec3f(sway + flutter, flutter * 0.5, sway * 0.6 - flutter * 0.5);
}

// The standard shader flips a double-sided card's normal on its back face; foliage
// normals point out of the crown instead, so both faces keep them.
fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    if !f.front_facing {
        (*s).normal = -(*s).normal;
    }
}

// Tread marks (tracks.ts): a mark keeps its strength for four seconds, then fades
// out by TRACK_LIFETIME (24 s). instance_data = (birth s, strength); params[0].x
// is the trail clock in simulation seconds, written by the pool each frame, so
// marks freeze with the simulation while paused.
fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let age = material.params[0].x - f.instance_data.x;
    (*s).opacity = f.instance_data.y * (1.0 - smoothstep(4.0, 24.0, age)) * 0.38;
}

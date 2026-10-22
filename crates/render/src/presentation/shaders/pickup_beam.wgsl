// A pickup pad's light. Halo vertices (uv.x above 1.5) form rings that climb the
// column from the deck past the floating gem, narrowing and fading as they go,
// one after another. Every pad climbs at one rate; only the phase varies (from
// the pad's position), so the pattern never jumps with the large running clock.
// params[0] = (climb per second, climb height, rings, narrowing at the top).
// How much brighter than the column a halo glows.
const HALO_GAIN: f32 = 1.8;

fn effect_vertex(v: ptr<function, EffectVertex>) {
    if (*v).uv.x < 1.5 {
        return;
    }
    let p = material.params[0];
    let origin = (*v).instance_origin;
    let ring = floor((*v).uv.x - 2.0);
    let phase = fract(origin.x * 0.137 + origin.z * 0.291) + ring / p.z;
    let t = fract(frame.camera_position.w * p.x + phase);
    let narrow = 1.0 - p.w * t;
    (*v).position = vec3f((*v).position.x * narrow, (*v).position.y + t * p.y, (*v).position.z * narrow);
    // Swell in from the deck, then fade on the way up.
    (*v).color = (*v).color * HALO_GAIN * smoothstep(0.0, 0.1, t) * (1.0 - t * t);
}

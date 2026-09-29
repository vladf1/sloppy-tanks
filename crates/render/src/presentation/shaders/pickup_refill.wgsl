// The pickup refill arc grows segment by segment from twelve o'clock, like the
// ring geometry's draw range did. params[0] = (segments, outer radius, -, -);
// instance_data.x = segments drawn.
fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let tau = 6.28318530718;
    let local = (f.uv * 2.0 - 1.0) * material.params[0].y;
    var angle = atan2(local.y, local.x) - tau * 0.25;
    angle = angle - floor(angle / tau) * tau;
    let segment = floor(angle / (tau / material.params[0].x));
    if segment >= f.instance_data.x {
        (*s).opacity = 0.0;
    }
}

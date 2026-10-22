// A restocking supply drop's refill ticks: marks sprayed round the pallet from
// twelve o'clock, one after another, the next one filling in, the rest faint.
// params[0] = (ticks, outer radius, gap between ticks as a share of a tick, faint
// paint); instance_data.x = ticks sprayed, its fraction the next tick's coverage.
fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let tau = 6.28318530718;
    let p = material.params[0];
    let local = (f.uv * 2.0 - 1.0) * p.y;
    var angle = atan2(local.y, local.x) - tau * 0.25;
    angle = angle - floor(angle / tau) * tau;
    let slot = angle / (tau / p.x);
    let tick = floor(slot);
    if fract(slot) > 1.0 - p.z {
        (*s).opacity = 0.0;
    }
    let sprayed = clamp(f.instance_data.x - tick, 0.0, 1.0);
    (*s).color = (*s).color * mix(p.w, 1.0, sprayed);
}

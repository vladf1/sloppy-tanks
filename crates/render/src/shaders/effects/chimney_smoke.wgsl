// GPU-animated chimney wisps (`village-atmosphere.ts`): screen-sized quads that
// rise, drift and fade. instance_data = (origin xyz, phase); the instance
// transforms are identity. The wisp's age travels to the fragment in extra0.z.
fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let t = f.extra0.z;
    (*s).color = vec3f(0.72, 0.75, 0.7);
    (*s).opacity = (1.0 - smoothstep(0.2, 1.0, length(f.uv - 0.5) * 2.0))
        * smoothstep(0.0, 0.15, t) * pow(1.0 - t, 1.5) * 0.2;
}

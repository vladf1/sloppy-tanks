// GPU-animated chimney wisps (`village-atmosphere.ts`): screen-sized quads that
// rise, drift and fade. instance_data = (origin xyz, phase); the instance
// transforms are identity. The wisp's age travels to the fragment in extra0.z.
fn effect_vertex(v: ptr<function, EffectVertex>) {
    let time = frame.camera_position.w;
    let origin = (*v).instance_data.xyz;
    let phase = (*v).instance_data.w;
    let t = fract(time * 0.065 + phase);
    (*v).extra0 = vec4f((*v).position.xy, t, 0.0);
    (*v).position = origin + vec3f(
        t * 1.5 + sin(time * 0.55 + phase * 20.0) * t * 0.25,
        t * 4.2,
        t * 0.45,
    );
}

// Expand the point into a quad of `size` pixels on screen.
fn effect_clip(clip: ptr<function, vec4f>, w: EffectWorld, v: EffectVertex) {
    let view = frame.view * vec4f(w.position, 1.0);
    let size = clamp((v.extra0.z * 1.6 + 0.22) * 720.0 / -view.z, 1.0, 65.0);
    let offset = v.extra0.xy * size * 2.0 / frame.viewport.xy * (*clip).w;
    *clip = vec4f((*clip).xy + offset, (*clip).zw);
}

fn wisp_smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = clamp((x - e0) / (e1 - e0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let t = f.extra0.z;
    (*s).color = vec3f(0.72, 0.75, 0.7);
    (*s).opacity = (1.0 - wisp_smoothstep(0.2, 1.0, length(f.uv - 0.5) * 2.0))
        * wisp_smoothstep(0.0, 0.15, t) * pow(1.0 - t, 1.5) * 0.2;
}

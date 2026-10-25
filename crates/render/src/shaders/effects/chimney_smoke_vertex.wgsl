// A chimney wisp's motion and screen size (`chimney_smoke.wgsl`), a shared vertex
// hook (`EffectDefinition::shared_vertex`).
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

// Expand the point into a quad of `size` canvas pixels on screen, whatever the
// scene's render scale.
fn effect_clip(clip: ptr<function, vec4f>, w: EffectWorld, v: EffectVertex) {
    let view = frame.view * vec4f(w.position, 1.0);
    let size = clamp((v.extra0.z * 1.6 + 0.22) * 720.0 / -view.z, 1.0, 65.0) * frame.quality.w;
    let offset = v.extra0.xy * size * 2.0 / frame.viewport.xy * (*clip).w;
    *clip = vec4f((*clip).xy + offset, (*clip).zw);
}

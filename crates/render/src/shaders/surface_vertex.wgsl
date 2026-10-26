// The same vertex deformation and projection for surface and shadow passes.
@vertex
fn vs_main(input: VertexIn, @builtin(instance_index) index: u32) -> VertexOut {
    let instance = instance_at(index);
    var v = effect_input(input, instance);
    effect_vertex(&v);
    var w: EffectWorld;
    w.position = (instance.world * vec4f(v.position, 1.0)).xyz;
    w.normal = normal_matrix(instance.world) * v.normal;
    effect_world(&w, v);
    var out: VertexOut;
    out.clip = frame.view_projection * vec4f(w.position, 1.0);
    effect_clip(&out.clip, w, v);
    out.world = w.position;
    out.normal = w.normal;
    out.uv = v.uv;
    out.color = vec4f(v.color, v.color_alpha);
    out.tint = instance.tint;
    out.data = instance.data;
    out.extra0 = v.extra0;
    out.extra1 = v.extra1;
    return out;
}

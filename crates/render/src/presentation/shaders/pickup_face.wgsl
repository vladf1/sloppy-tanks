// A pickup's faces are mapped to the atlas's first tile; instance_data.xy moves
// them into the kind's tile, so every kind shares one mesh and material.
fn effect_vertex(v: ptr<function, EffectVertex>) {
    (*v).uv = (*v).uv + (*v).instance_data.xy;
}

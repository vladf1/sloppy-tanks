// A pickup's kind faces are mapped to the atlas's first tile; instance_data.xy
// moves them into the kind's tile, so every kind shares one material and a kind's
// pallets and items each draw as one instanced call. Faces mapped to the shared
// cells (brass, steel, rubber, the pallet's planks) lie outside the first tile and
// keep their UVs. The pallet's canvas bundle maps into the last cell and folds
// away while the supply is stocked (instance_data.z is 1): its load stands there.
fn effect_vertex(v: ptr<function, EffectVertex>) {
    let uv = (*v).uv;
    if uv.x < 0.25 && uv.y > 0.75 {
        (*v).uv = uv + (*v).instance_data.xy;
    } else if uv.x > 0.75 && uv.y < 0.25 && (*v).instance_data.z > 0.5 {
        (*v).position = vec3f(0.0);
    }
}

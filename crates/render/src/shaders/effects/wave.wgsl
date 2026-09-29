// Sample vertex effect: a travelling cloth ripple, pinned at uv.x = 0.
// params[0] = (amplitude, wavelength, speed, unused).
fn effect_vertex(v: ptr<function, EffectVertex>) {
    let p = material.params[0];
    let k = 6.28318530718 / p.y;
    let phase = (*v).position.x * k - frame.camera_position.w * p.z;
    let weight = clamp((*v).uv.x, 0.0, 1.0);
    (*v).position.z += sin(phase) * p.x * weight;
    // Slope of z = A·w·sin(kx − ωt) along x tilts the normal of either face.
    let slope = p.x * weight * cos(phase) * k;
    (*v).normal = normalize((*v).normal + vec3f(-slope * (*v).normal.z, 0.0, 0.0));
}

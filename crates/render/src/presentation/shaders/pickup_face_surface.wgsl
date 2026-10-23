// A pickup's weathering (`pickup_face.wgsl`), shared with every lit surface variant
// (`EffectDefinition::shared_surface`): instance_data.w is the drop's wear, for
// sun-greyed or darker boards and fresher or dirtier paint.
fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let w = fract(f.instance_data.w);
    let wood = f.uv.x > 0.75 && f.uv.y > 0.75;
    if wood {
        // Boards from a different stack: lighter and sun-greyed, or darker.
        let grey = vec3f(dot((*s).color, vec3f(0.2126, 0.7152, 0.0722)));
        let aged = mix((*s).color, grey, 0.35 * fract(w * 7.31));
        (*s).color = aged * (0.78 + 0.34 * w);
    } else {
        (*s).color = (*s).color * (0.9 + 0.14 * fract(w * 3.7));
    }
}

// A pickup's kind faces are mapped to the atlas's first tile; instance_data.xy
// moves them into the kind's tile, so every kind shares one material and a kind's
// pallets and items each draw as one instanced call. Faces mapped to the shared
// cells (brass, steel, rubber, the pallet's planks) lie outside the first tile and
// keep their UVs. A worn pallet (its wear past PALLET_WORN) has lost the deck
// board whose UVs lie above params[0].x. instance_data.w is the drop's wear:
// sun-greyed or darker boards, fresher or dirtier paint.
const PALLET_WORN: f32 = 0.66;

fn effect_vertex(v: ptr<function, EffectVertex>) {
    let uv = (*v).uv;
    let data = (*v).instance_data;
    if uv.x < 0.25 && uv.y > 0.75 {
        (*v).uv = uv + data.xy;
    } else if uv.x > 0.75 && uv.y > 0.75 {
        if uv.y > material.params[0].x && fract(data.w) > PALLET_WORN {
            (*v).position = vec3f(0.0);
        }
    }
}

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

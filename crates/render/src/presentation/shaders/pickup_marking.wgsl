// A supply drop's marking: the circle sprayed round its pallet and the kind's
// stencil sprayed across the deck. The stencil is mapped to the first kind's in the
// atlas's glyph cell and instance_data.yz moves it to the drop's kind. While the
// supply is restocked, fresh paint sweeps round the circle (instance_data.x, 1 when
// stocked) over paint that is faded: greyed and a little darker.
// params[0] = (paint cell u0, paint cell width, the u left of which is the glyph
// cell, -); params[1] = (faded paint's greying, its brightness, -, -).
fn effect_vertex(v: ptr<function, EffectVertex>) {
    if (*v).uv.x < material.params[0].z {
        (*v).uv = (*v).uv + (*v).instance_data.yz;
    }
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let p = material.params[0];
    if f.uv.x < p.z {
        return;
    }
    let along = (f.uv.x - p.x) / p.y;
    let fresh = 1.0 - smoothstep(f.instance_data.x, f.instance_data.x + 0.01, along);
    let faded = material.params[1];
    let grey = vec3f(dot((*s).color, vec3f(0.2126, 0.7152, 0.0722)));
    let worn = mix((*s).color, grey, faded.x) * faded.y;
    (*s).color = mix(worn, (*s).color, fresh);
}

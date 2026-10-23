// Sun shadow-map pass. `frame.view_projection` is the light's orthographic matrix.
// A fragment stage exists only for alpha-tested casters and fading instances;
// every other caster writes depth alone. Those casters share one fragment stage
// (`fs_shadow`), whichever they are, and so do the merged alpha-tested casters
// (`shadow_cutout.wgsl`): WebKit compiles every distinct stage at first use, about
// 50-90 ms apiece on a cold start for a few lines each.

struct ShadowOut {
    @builtin(position) clip: vec4f,
    @location(0) world: vec3f,
    @location(1) uv: vec2f,
    @location(2) tint: vec4f,
    @location(3) data: vec4f,
    @location(4) extra0: vec4f,
    @location(5) extra1: vec4f,
}

@vertex
fn vs_shadow(input: VertexIn, @builtin(instance_index) index: u32) -> ShadowOut {
    let instance = instance_at(index);
    var v = effect_input(input, instance);
    effect_vertex(&v);
    var w: EffectWorld;
    w.position = (instance.world * vec4f(v.position, 1.0)).xyz;
    w.normal = normal_matrix(instance.world) * v.normal;
    effect_world(&w, v);
    var out: ShadowOut;
    out.clip = frame.view_projection * vec4f(w.position, 1.0);
    effect_clip(&out.clip, w, v);
    out.world = w.position;
    out.uv = v.uv;
    out.tint = instance.tint;
    out.data = instance.data;
    out.extra0 = v.extra0;
    out.extra1 = v.extra1;
    return out;
}

@fragment
fn fs_shadow(input: ShadowOut, @builtin(front_facing) front_facing: bool) {
    // An alpha-tested caster's material has a cutoff above zero (`ShaderKey::shadow`).
    let cutoff = material.surface.z;
    if cutoff > 0.0 {
        let map_uv = input.uv * material.map_transform.xy + material.map_transform.zw;
        var texel = vec4f(1.0);
        if material_has(MATERIAL_MAP) {
            texel = textureSample(map_texture, map_sampler, map_uv);
        }
        var surface: Surface;
        surface.color = material.color.rgb * texel.rgb;
        surface.opacity = material.color.a * texel.a;
        surface.normal = vec3f(0.0, 1.0, 0.0);
        surface.roughness = material.surface.x;
        surface.metalness = material.surface.y;
        surface.emissive = material.emissive.rgb;
        // Only a caster whose effect shades its cutout runs the effect here: any
        // other effect's surface hook cannot change what the shadow keeps.
        if SHADOW_SURFACE {
            var fragment: EffectFragment;
            fragment.world = input.world;
            fragment.uv = input.uv;
            fragment.extra0 = input.extra0;
            fragment.extra1 = input.extra1;
            fragment.instance_data = input.data;
            fragment.view_direction = vec3f(0.0, 1.0, 0.0);
            fragment.front_facing = front_facing;
            fragment.vertex_color = vec4f(1.0);
            fragment.tint = input.tint;
            effect_surface(&surface, fragment);
        }
        if surface.opacity <= cutoff {
            discard;
        }
    }
    // A fading piece thins its shadow through an even mask: interleaved gradient
    // noise over the shadow map's texels, which the receivers' tent filter averages
    // into a smooth, partial shade rather than a speckle of hard texels. It is fixed
    // to the map's texels, so the shade holds still as the piece moves. A piece only
    // part faded (a supply the seated view sees through) still stands there in full
    // and casts its whole shadow, and only fading pieces draw below full opacity
    // (`Renderer::set_opacity`), so the mask leaves every other caster whole.
    let noise = fract(52.9829189 * fract(dot(input.clip.xy, vec2f(0.06711056, 0.00583715))));
    if smoothstep(0.0, SHADOW_FADE_SOLID, input.tint.a) <= noise {
        discard;
    }
}

// The opacity from which a fading caster's shadow is whole.
const SHADOW_FADE_SOLID: f32 = 0.6;

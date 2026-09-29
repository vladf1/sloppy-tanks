// Sun shadow-map pass. `frame.view_projection` is the light's orthographic matrix.
// A fragment stage exists only for alpha-tested casters and fading instances;
// every other caster writes depth alone.

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
    let instance = instances[index];
    var v = effect_input(input, instance);
    effect_vertex(&v);
    var w: EffectWorld;
    w.position = (instance.world * vec4f(v.position, 1.0)).xyz;
    w.normal = normal_matrix(instance.world) * v.normal;
    effect_world(&w, v);
    var out: ShadowOut;
    out.clip = frame.view_projection * vec4f(w.position, 1.0);
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
    let map_uv = input.uv * material.map_transform.xy + material.map_transform.zw;
    var texel = vec4f(1.0);
    if HAS_MAP {
        texel = textureSample(map_texture, map_sampler, map_uv);
    }
    var surface: Surface;
    surface.color = material.color.rgb * texel.rgb;
    surface.opacity = material.color.a * texel.a;
    surface.normal = vec3f(0.0, 1.0, 0.0);
    surface.roughness = material.surface.x;
    surface.metalness = material.surface.y;
    surface.emissive = material.emissive.rgb;
    var fragment: EffectFragment;
    fragment.world = input.world;
    fragment.uv = input.uv;
    fragment.extra0 = input.extra0;
    fragment.extra1 = input.extra1;
    fragment.instance_data = input.data;
    fragment.view_direction = vec3f(0.0, 1.0, 0.0);
    fragment.front_facing = front_facing;
    effect_surface(&surface, fragment);
    if ALPHA_TEST {
        if surface.opacity <= material.surface.z {
            discard;
        }
    }
    if SHADOW_FADE {
        // debris-fade.ts: a stable spatial mask thins the shadow as the piece fades.
        if input.tint.a <= tsl_hash(dot(input.world * 100.0, input.world)) {
            discard;
        }
    }
}

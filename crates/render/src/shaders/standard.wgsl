// Main-pass surface shader: Three r185 MeshStandardMaterial (LIT) or
// MeshBasicMaterial, with map, emissive map, bump map, RGBA vertex colors, flat
// shading, alpha test, fog, the sun's PCF shadow, hemisphere fill and up to four
// point lights.
// Lighting runs in world space; every dot and cross product Three evaluates in
// view space is invariant under the view's rotation. Output is linear HDR; the
// output pass tone maps the whole frame, as Three's frame-buffer target does.

struct VertexOut {
    @builtin(position) clip: vec4f,
    @location(0) world: vec3f,
    @location(1) normal: vec3f,
    @location(2) uv: vec2f,
    @location(3) color: vec4f,
    @location(4) tint: vec4f,
    @location(5) data: vec4f,
    @location(6) extra0: vec4f,
    @location(7) extra1: vec4f,
}

@vertex
fn vs_main(input: VertexIn, @builtin(instance_index) index: u32) -> VertexOut {
    let instance = instances[index];
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

fn dfg(roughness: f32, dot_nv: f32) -> vec2f {
    // Three's 16×16 DFG LUT; level 0 keeps it free of derivative rules.
    return textureSampleLevel(dfg_lut, lut_sampler, vec2f(roughness, dot_nv), 0.0).rg;
}

fn f_schlick(f0: vec3f, f90: f32, dot_vh: f32) -> vec3f {
    let fresnel = exp2((-5.55473 * dot_vh - 6.98316) * dot_vh);
    return f0 * (1.0 - fresnel) + f90 * fresnel;
}

fn v_ggx_smith_correlated(alpha: f32, dot_nl: f32, dot_nv: f32) -> f32 {
    let a2 = alpha * alpha;
    let gv = dot_nl * sqrt(a2 + (1.0 - a2) * dot_nv * dot_nv);
    let gl = dot_nv * sqrt(a2 + (1.0 - a2) * dot_nl * dot_nl);
    return 0.5 / max(gv + gl, 1e-6);
}

fn d_ggx(alpha: f32, dot_nh: f32) -> f32 {
    let a2 = alpha * alpha;
    let denom = 1.0 - dot_nh * dot_nh * (1.0 - a2);
    return a2 / (denom * denom) * RECIPROCAL_PI;
}

fn brdf_ggx(l: vec3f, v: vec3f, n: vec3f, f0: vec3f, f90: f32, roughness: f32) -> vec3f {
    let alpha = roughness * roughness;
    let h = normalize(l + v);
    let dot_nl = clamp(dot(n, l), 0.0, 1.0);
    let dot_nv = clamp(dot(n, v), 0.0, 1.0);
    let dot_nh = clamp(dot(n, h), 0.0, 1.0);
    let dot_vh = clamp(dot(v, h), 0.0, 1.0);
    return f_schlick(f0, f90, dot_vh) * v_ggx_smith_correlated(alpha, dot_nl, dot_nv) * d_ggx(alpha, dot_nh);
}

// r185 BRDF_GGX_Multiscatter: single scattering plus Turquin's energy compensation.
fn brdf_ggx_multiscatter(l: vec3f, v: vec3f, n: vec3f, f0: vec3f, f90: f32, roughness: f32) -> vec3f {
    let single = brdf_ggx(l, v, n, f0, f90, roughness);
    let dot_nl = clamp(dot(n, l), 0.0, 1.0);
    let dot_nv = clamp(dot(n, v), 0.0, 1.0);
    let dfg_v = dfg(roughness, dot_nv);
    let dfg_l = dfg(roughness, dot_nl);
    let fss_ess_v = f0 * dfg_v.x + f90 * dfg_v.y;
    let fss_ess_l = f0 * dfg_l.x + f90 * dfg_l.y;
    let ems_v = 1.0 - (dfg_v.x + dfg_v.y);
    let ems_l = 1.0 - (dfg_l.x + dfg_l.y);
    let favg = f0 + (1.0 - f0) * 0.047619;
    let fms = fss_ess_v * fss_ess_l * favg / (1.0 - ems_v * ems_l * favg * favg + 1e-6);
    return single + fms * (ems_v * ems_l);
}

// r185 PCFShadowFilter: five hardware-filtered compares on a Vogel disk rotated by
// interleaved gradient noise, with the light-space normal offset and depth bias.
fn sun_shadow(world: vec3f, normal: vec3f, pixel: vec2f) -> f32 {
    let position = frame.shadow_matrix * vec4f(world + normal * frame.shadow.y, 1.0);
    let projected = position.xyz / position.w;
    let coord = vec3f(projected.x, 1.0 - projected.y, projected.z + frame.shadow.x);
    let phi = fract(52.9829189 * fract(dot(pixel, vec2f(0.06711056, 0.00583715)))) * 6.28318530718;
    let radius = frame.shadow.w / frame.shadow.z;
    var sum = 0.0;
    for (var i = 0; i < 5; i++) {
        let r = sqrt((f32(i) + 0.5) / 5.0);
        let theta = f32(i) * 2.399963229728653 + phi;
        let offset = vec2f(cos(theta), sin(theta)) * r * radius;
        sum += textureSampleCompareLevel(shadow_map, shadow_sampler, coord.xy + offset, coord.z);
    }
    let inside = coord.x >= 0.0 && coord.x <= 1.0 && coord.y >= 0.0 && coord.y <= 1.0 && coord.z <= 1.0;
    return select(1.0, sum / 5.0, inside);
}

fn distance_attenuation(distance: f32, cutoff: f32, decay: f32) -> f32 {
    let falloff = 1.0 / max(pow(distance, decay), 0.01);
    if cutoff > 0.0 {
        let window = clamp(1.0 - pow(distance / cutoff, 4.0), 0.0, 1.0);
        return falloff * window * window;
    }
    return falloff;
}

struct Reflected {
    diffuse: vec3f,
    specular: vec3f,
}

fn add_direct_light(
    out: ptr<function, Reflected>,
    l: vec3f,
    color: vec3f,
    v: vec3f,
    n: vec3f,
    diffuse: vec3f,
    specular: vec3f,
    roughness: f32,
) {
    let irradiance = clamp(dot(n, l), 0.0, 1.0) * color;
    (*out).diffuse += irradiance * diffuse * RECIPROCAL_PI;
    (*out).specular += irradiance * brdf_ggx_multiscatter(l, v, n, specular, 1.0, roughness);
}

fn shade_standard(s: Surface, world: vec3f, v: vec3f, pixel: vec2f, geometry_roughness: f32) -> vec3f {
    let n = s.normal;
    let roughness = min(max(s.roughness, 0.0525) + geometry_roughness, 1.0);
    let diffuse = s.color * (1.0 - s.metalness);
    let specular = mix(vec3f(0.04), s.color, s.metalness);
    var reflected: Reflected;
    reflected.diffuse = vec3f(0.0);
    reflected.specular = vec3f(0.0);
    var sun = frame.sun_color.rgb;
    if RECEIVE_SHADOW {
        sun *= sun_shadow(world, n, pixel);
    }
    add_direct_light(&reflected, frame.sun_direction.xyz, sun, v, n, diffuse, specular, roughness);
    for (var i = 0; i < 4; i++) {
        let light = frame.point_lights[i];
        if dot(light.color.rgb, vec3f(1.0)) > 0.0 {
            let offset = light.position.xyz - world;
            let distance = length(offset);
            let color = light.color.rgb * distance_attenuation(distance, light.position.w, light.color.w);
            add_direct_light(&reflected, offset / max(distance, 1e-6), color, v, n, diffuse, specular, roughness);
        }
    }
    let hemisphere = mix(frame.ground_color.rgb, frame.sky_color.rgb, n.y * 0.5 + 0.5);
    let indirect = hemisphere * diffuse * RECIPROCAL_PI;
    return reflected.diffuse + indirect + reflected.specular + s.emissive;
}

@fragment
fn fs_main(input: VertexOut, @builtin(front_facing) front_facing: bool) -> @location(0) vec4f {
    // Every derivative and implicit-LOD sample happens here, before any branch
    // that depends on per-pixel values (WGSL uniformity rules).
    let face = select(-1.0, 1.0, front_facing);
    let dpx = dpdx(input.world);
    // Three's WGSL dFdy is -dpdy (framebuffer Y points down).
    let dpy = -dpdy(input.world);
    let flat_normal = normalize(cross(dpx, dpy));
    var geometry_normal = normalize(input.normal);
    if FLAT_SHADING {
        geometry_normal = flat_normal;
    }
    let geometry_view = normalize((frame.view * vec4f(geometry_normal, 0.0)).xyz);
    let normal_change = max(abs(dpdx(geometry_view)), abs(dpdy(geometry_view)));
    let geometry_roughness = max(max(normal_change.x, normal_change.y), normal_change.z);
    let map_uv = input.uv * material.map_transform.xy + material.map_transform.zw;
    let emissive_uv = input.uv * material.emissive_transform.xy + material.emissive_transform.zw;
    let bump_uv = input.uv * material.bump_transform.xy + material.bump_transform.zw;
    let bump_dx = dpdx(bump_uv);
    let bump_dy = -dpdy(bump_uv);
    var texel = vec4f(1.0);
    if HAS_MAP {
        texel = textureSample(map_texture, map_sampler, map_uv);
    }
    var emissive_texel = vec3f(1.0);
    if HAS_EMISSIVE_MAP {
        emissive_texel = textureSample(emissive_texture, emissive_sampler, emissive_uv).rgb;
    }
    var height = vec3f(0.0);
    if HAS_BUMP {
        height = vec3f(
            textureSample(bump_texture, bump_sampler, bump_uv).r,
            textureSample(bump_texture, bump_sampler, bump_uv + bump_dx).r,
            textureSample(bump_texture, bump_sampler, bump_uv + bump_dy).r,
        );
    }

    var base = material.color * texel;
    if VERTEX_COLORS {
        base = base * input.color;
    }
    base = vec4f(base.rgb * input.tint.rgb, base.a * input.tint.a);

    var normal = geometry_normal;
    if !FLAT_SHADING {
        if DOUBLE_SIDED {
            normal = normal * face;
        } else if BACK_SIDE {
            normal = -normal;
        }
    }
    if HAS_BUMP {
        // Mikkelsen's surface-gradient bump (Three perturbNormalArb).
        let dhdxy = vec2f(height.y - height.x, height.z - height.x) * material.surface.w;
        let sigma_x = normalize(dpx);
        let sigma_y = normalize(dpy);
        let r1 = cross(sigma_y, normal);
        let r2 = cross(normal, sigma_x);
        let det = dot(sigma_x, r1) * face;
        let gradient = sign(det) * (dhdxy.x * r1 + dhdxy.y * r2);
        normal = normalize(abs(det) * normal - gradient);
    }

    let view_direction = normalize(frame.camera_position.xyz - input.world);
    var surface: Surface;
    surface.color = base.rgb;
    surface.opacity = base.a;
    surface.normal = normal;
    surface.roughness = material.surface.x;
    surface.metalness = material.surface.y;
    surface.emissive = material.emissive.rgb * emissive_texel;
    var fragment: EffectFragment;
    fragment.world = input.world;
    fragment.uv = input.uv;
    fragment.extra0 = input.extra0;
    fragment.extra1 = input.extra1;
    fragment.instance_data = input.data;
    fragment.view_direction = view_direction;
    fragment.front_facing = front_facing;
    fragment.vertex_color = select(vec4f(1.0), input.color, VERTEX_COLORS);
    fragment.tint = input.tint;
    effect_surface(&surface, fragment);
    let alpha_width = fwidth(surface.opacity);

    var alpha = surface.opacity;
    if ALPHA_TEST {
        let cutoff = material.surface.z;
        if ALPHA_TO_COVERAGE {
            alpha = smoothstep(cutoff, cutoff + alpha_width, alpha);
            if alpha <= 0.0 {
                discard;
            }
        } else if alpha <= cutoff {
            discard;
        }
    }
    if FORCE_OPAQUE {
        alpha = 1.0;
    }

    var color = surface.color;
    if LIT {
        color = shade_standard(surface, input.world, view_direction, input.clip.xy, geometry_roughness);
    }
    if FOG {
        if frame.fog_color.w > 0.5 {
            let depth = -(frame.view * vec4f(input.world, 1.0)).z;
            color = mix(color, frame.fog_color.rgb, smoothstep(frame.fog_range.x, frame.fog_range.y, depth));
        }
    }
    return vec4f(color, alpha);
}

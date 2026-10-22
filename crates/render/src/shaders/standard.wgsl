// Main-pass surface shader: Three r185 MeshStandardMaterial (LIT) or
// MeshBasicMaterial, with map, emissive map, bump map, RGBA vertex colors, flat
// shading, alpha test, fog, the sun's PCF shadow, hemisphere fill and up to four
// point lights; with the atmosphere on, aerial haze, cloud shadows and a sky rim.
// Lighting, alpha test and shadow reception are variant constants; the cheap
// per-material features are uniform branches (`material_has`).
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

// The sun's shadow: Castaño's optimized PCF (The Witness), hardware-filtered
// compares weighted into a smooth tent with no dither noise, after the light-space
// normal offset and depth bias. Its taps sit exactly where the hardware's bilinear
// weights complete the tent, so the penumbra glides without stepping as things move:
// 7×7 texels from 16 taps, or 5×5 from 9 on the lighter WebGL build.
fn sun_shadow(world: vec3f, normal: vec3f) -> f32 {
    let position = frame.shadow_matrix * vec4f(world + normal * frame.shadow.y, 1.0);
    let projected = position.xyz / position.w;
    let coord = vec3f(projected.x, 1.0 - projected.y, projected.z + frame.shadow.x);
    let size = frame.shadow.z;
    let texels = coord.xy * size;
    let base = floor(texels + 0.5);
    let st = texels + 0.5 - base;
    let origin = (base - 0.5) / size;
    // The filter radius spreads the taps; at 1 they are exact.
    let texel = frame.shadow.w / size;
    var sum = 0.0;
    if LITE {
        let uw = vec3f(4.0 - 3.0 * st.x, 7.0, 1.0 + 3.0 * st.x);
        let u = vec3f((3.0 - 2.0 * st.x) / uw.x - 2.0, (3.0 + st.x) / uw.y, st.x / uw.z + 2.0) * texel;
        let vw = vec3f(4.0 - 3.0 * st.y, 7.0, 1.0 + 3.0 * st.y);
        let v = vec3f((3.0 - 2.0 * st.y) / vw.x - 2.0, (3.0 + st.y) / vw.y, st.y / vw.z + 2.0) * texel;
        for (var j = 0; j < 3; j++) {
            for (var i = 0; i < 3; i++) {
                let tap = origin + vec2f(u[i], v[j]);
                sum += uw[i] * vw[j] * textureSampleCompareLevel(shadow_map, shadow_sampler, tap, coord.z);
            }
        }
        sum /= 144.0;
    } else {
        let s = st.x;
        let t = st.y;
        let uw = vec4f(5.0 * s - 6.0, 11.0 * s - 28.0, -(11.0 * s + 17.0), -(5.0 * s + 1.0));
        let u = vec4f(
            (4.0 * s - 5.0) / uw.x - 3.0,
            (4.0 * s - 16.0) / uw.y - 1.0,
            -(7.0 * s + 5.0) / uw.z + 1.0,
            -s / uw.w + 3.0,
        ) * texel;
        let vw = vec4f(5.0 * t - 6.0, 11.0 * t - 28.0, -(11.0 * t + 17.0), -(5.0 * t + 1.0));
        let v = vec4f(
            (4.0 * t - 5.0) / vw.x - 3.0,
            (4.0 * t - 16.0) / vw.y - 1.0,
            -(7.0 * t + 5.0) / vw.z + 1.0,
            -t / vw.w + 3.0,
        ) * texel;
        for (var j = 0; j < 4; j++) {
            for (var i = 0; i < 4; i++) {
                let tap = origin + vec2f(u[i], v[j]);
                sum += uw[i] * vw[j] * textureSampleCompareLevel(shadow_map, shadow_sampler, tap, coord.z);
            }
        }
        sum /= 2704.0;
    }
    let inside = coord.x >= 0.0 && coord.x <= 1.0 && coord.y >= 0.0 && coord.y <= 1.0 && coord.z <= 1.0;
    return select(1.0, sum, inside);
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

fn shade_standard(s: Surface, world: vec3f, v: vec3f, geometry_roughness: f32) -> vec3f {
    let n = s.normal;
    let roughness = min(max(s.roughness, 0.0525) + geometry_roughness, 1.0);
    let diffuse = s.color * (1.0 - s.metalness);
    let specular = mix(vec3f(0.04), s.color, s.metalness);
    var reflected: Reflected;
    reflected.diffuse = vec3f(0.0);
    reflected.specular = vec3f(0.0);
    var sunlit = 1.0;
    if RECEIVE_SHADOW {
        sunlit = sun_shadow(world, n);
    }
    if atmosphere_on() {
        sunlit *= cloud_shadow(world);
    }
    let sun = frame.sun_color.rgb * sunlit;
    add_direct_light(&reflected, frame.sun_direction.xyz, sun, v, n, diffuse, specular, roughness);
    for (var i = 0; i < 4; i++) {
        let light = frame.point_lights[i];
        if dot(light.color.rgb, vec3f(1.0)) > 0.0 {
            let offset = light.position.xyz - world;
            let distance = length(offset);
            // Past the cutoff the attenuation window is exactly zero: skip the BRDF.
            let cutoff = light.position.w;
            if cutoff <= 0.0 || distance < cutoff {
                var decay = light.color.w;
                var gain = 1.0;
                if atmosphere_on() {
                    // Fire lights a wide, soft pool rather than a hot spot under
                    // itself: a gentler falloff than Three's inverse square (the
                    // cutoff window still ends it), and brighter overall, so blasts
                    // and burning wrecks visibly light the ground, walls and hulls
                    // around them in daylight.
                    decay *= FIRE_POOL_DECAY;
                    gain = BLAST_LIGHT_GAIN;
                }
                var color = light.color.rgb * (distance_attenuation(distance, cutoff, decay) * gain);
                if atmosphere_on() {
                    color = fire_light_hue(color);
                }
                add_direct_light(&reflected, offset / max(distance, 1e-6), color, v, n, diffuse, specular, roughness);
            }
        }
    }
    var hemisphere = mix(frame.ground_color.rgb, frame.sky_color.rgb, n.y * 0.5 + 0.5);
    // How directly the sun reaches this point: cast and cloud shadow, and faces
    // turned from it.
    let in_sun = sunlit * smoothstep(0.0, 0.35, dot(n, frame.sun_direction.xyz));
    var shade_color = diffuse;
    if atmosphere_on() {
        // Warm key, cool fill: the blue sky fill keeps its hue where it is the main
        // light (shade), but under direct sun it adds only its brightness, so sunlit
        // sand, concrete and grass keep their own warm albedo instead of drifting
        // toward mauve or grey while shadows read clearly cool.
        let fill_level = dot(hemisphere, vec3f(0.2126, 0.7152, 0.0722));
        hemisphere = mix(hemisphere, vec3f(fill_level), FILL_NEUTRAL_IN_SUN * in_sun);
        // Painterly shade: what the sky fill alone lights keeps a richer version of
        // its own color, so grass shade stays green and earth shade warm brown
        // instead of every shadow settling into the same grey-blue.
        let grey = dot(diffuse, vec3f(0.2126, 0.7152, 0.0722));
        shade_color = max(mix(vec3f(grey), diffuse, SHADE_SATURATION), vec3f(0.0));
    }
    var indirect = hemisphere * shade_color * RECIPROCAL_PI;
    if atmosphere_on() && frame.ground.w > 0.0 {
        // Warm bounce from the sunlit ground around: what the sun does not reach
        // directly (cast shadow, cloud shadow or a face turned away) still catches
        // some of its light off the ground, most on walls and hull sides, so shade
        // stays warm and luminous rather than sinking into one cold blue.
        let ground_tint = frame.ground_color.rgb / max(dot(frame.ground_color.rgb, vec3f(0.2126, 0.7152, 0.0722)), 1e-4);
        let sun_level = dot(frame.sun_color.rgb, vec3f(0.2126, 0.7152, 0.0722));
        let bounce = frame.ground.w * sun_level * (1.0 - in_sun) * (1.0 - 0.5 * max(n.y, 0.0));
        indirect += ground_tint * (bounce * RECIPROCAL_PI) * shade_color;
    }
    if atmosphere_on() {
        // Stylized sky rim: grazing walls and hull sides catch the bright horizon,
        // most on the side turned from the sun, so silhouettes read on any ground.
        let facing = 1.0 - clamp(dot(n, v), 0.0, 1.0);
        let away = 0.55 + 0.45 * clamp(-dot(n.xz, normalize(frame.sun_direction.xz + vec2f(1e-4))), 0.0, 1.0);
        let rim = facing * facing * facing * (1.0 - n.y * n.y) * away * frame.sky_sun.w;
        let rim_color = frame.sky_horizon.rgb * 0.6 + frame.sky_sun.rgb * 0.4;
        indirect += rim_color * diffuse * rim;
    }
    // Only glossy finishes (vehicle paint and steel) reflect the sky; matte
    // terrain and buildings keep the hemisphere fill their themes were tuned with.
    let reflections = frame.sky_color.w * (1.0 - smoothstep(GLOSSY_UNTIL, MATTE_FROM, s.roughness));
    if reflections > 0.0 {
        // Split-sum environment specular with Three's multiscatter terms
        // (`computeMultiscattering`): the reflected sky takes the energy it
        // reflects away from the diffuse fill, so grazing paint gains a sheen
        // and metals stop going dark outside the sun's highlight.
        let dot_nv = clamp(dot(n, v), 0.0, 1.0);
        let scale_bias = dfg(roughness, dot_nv);
        let single = specular * scale_bias.x + scale_bias.y;
        let single_energy = scale_bias.x + scale_bias.y;
        let missing_energy = 1.0 - single_energy;
        let average_fresnel = specular + (1.0 - specular) * 0.047619;
        let multi = single * average_fresnel / (1.0 - missing_energy * average_fresnel) * missing_energy;
        var radiance = environment_radiance(reflect(-v, n), roughness);
        if atmosphere_on() {
            // The drawn sky's bright horizon would wash glossy paint toward white at
            // grazing angles: reflect a dimmer sky so team colours hold.
            radiance *= ATMOSPHERE_REFLECTION;
        }
        let scattered = single + multi;
        let kept = 1.0 - reflections * max(max(scattered.r, scattered.g), scattered.b);
        indirect = indirect * kept
            + reflections * (single * radiance + multi * hemisphere * RECIPROCAL_PI);
    }
    return reflected.diffuse + indirect + reflected.specular + s.emissive;
}

// The sky a surface reflects, prefiltered analytically instead of from a cube
// map: the visible sky (the fog color, which is the background) at the horizon
// fading to the hemisphere fill's sky overhead, and the fill's ground below. A
// rougher lobe sees a softer horizon. The sun is left out: the direct GGX term
// already draws its highlight.
fn environment_radiance(direction: vec3f, roughness: f32) -> vec3f {
    let up = direction.y;
    let lobe = roughness * roughness;
    let zenith = frame.sky_color.rgb * RECIPROCAL_PI;
    var horizon = zenith;
    if frame.fog_color.w > 0.5 {
        horizon = frame.fog_color.rgb;
    }
    var sky = mix(horizon, zenith, sqrt(clamp(up, 0.0, 1.0)));
    if atmosphere_on() {
        // The drawn sky itself, its gradient softened toward the fill for rough lobes.
        // Its bright horizon band stays out of the reflection: glossy hulls seen at
        // a grazing angle would otherwise wash to white.
        let drawn = sky_gradient(normalize(vec3f(direction.x, max(up, REFLECTED_SKY_FLOOR), direction.z)));
        sky = mix(drawn, zenith, lobe * 0.5);
    }
    let ground = frame.ground_color.rgb * RECIPROCAL_PI;
    let horizon_width = HORIZON_SHARPEST + lobe;
    return mix(ground, sky, smoothstep(-horizon_width, horizon_width, up));
}

// The share of the sky fill's blue a fully sunlit surface gives up (it keeps the
// fill's brightness).
const FILL_NEUTRAL_IN_SUN: f32 = 0.7;
// Saturation of the sky fill's share of a surface's light (1 = its plain color).
const SHADE_SATURATION: f32 = 1.3;
// Point lights' (blast flashes') extra strength with the atmosphere on, and the
// share of their decay exponent they keep (a wider pool).
const BLAST_LIGHT_GAIN: f32 = 1.6;
const FIRE_POOL_DECAY: f32 = 0.85;
// Fire's own color at unit luminance, and how far a deep red-orange flash light turns
// toward it: a pure red-orange pool over sunlit ground and blue-filled shade reads as a
// pink screen tint rather than firelight.
const FIRE_AMBER: vec3f = vec3f(1.0, 0.56, 0.24);
const FIRE_AMBER_SHARE: f32 = 0.55;

// The luminance a flash light's pool eases toward right beside it, against a sun of
// about 3: firelight warms shade and the sunlit ground around a blast without washing
// a near-camera muzzle flash or fireball across the frame.
const FIRE_LIGHT_LIMIT: f32 = 1.2;

fn fire_light_hue(color: vec3f) -> vec3f {
    let level = dot(color, vec3f(0.2126, 0.7152, 0.0722));
    let amber = FIRE_AMBER / dot(FIRE_AMBER, vec3f(0.2126, 0.7152, 0.0722));
    return mix(color, amber * level, FIRE_AMBER_SHARE) / (1.0 + level / FIRE_LIGHT_LIMIT);
}
// Share of the drawn sky glossy finishes reflect.
const ATMOSPHERE_REFLECTION: f32 = 0.45;
// The lowest elevation of the drawn sky glossy finishes reflect.
const REFLECTED_SKY_FLOOR: f32 = 0.22;
// Half-width in direction Y of a mirror's horizon blend.
const HORIZON_SHARPEST: f32 = 0.03;
// Material roughness over which sky reflections fade out.
const GLOSSY_UNTIL: f32 = 0.45;
const MATTE_FROM: f32 = 0.6;

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
    if material_has(MATERIAL_FLAT_SHADING) {
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
    if material_has(MATERIAL_MAP) {
        texel = textureSample(map_texture, map_sampler, map_uv);
    }
    var emissive_texel = vec3f(1.0);
    // Emissive and bump maps light standard materials only; unlit variants drop them.
    if LIT {
        if material_has(MATERIAL_EMISSIVE_MAP) {
            emissive_texel = textureSample(emissive_texture, emissive_sampler, emissive_uv).rgb;
        }
    }
    var height = vec3f(0.0);
    if LIT {
        if material_has(MATERIAL_BUMP) {
            height = vec3f(
                textureSample(bump_texture, bump_sampler, bump_uv).r,
                textureSample(bump_texture, bump_sampler, bump_uv + bump_dx).r,
                textureSample(bump_texture, bump_sampler, bump_uv + bump_dy).r,
            );
        }
    }

    var base = material.color * texel;
    if material_has(MATERIAL_VERTEX_COLORS) {
        base = base * input.color;
    }
    base = vec4f(base.rgb * input.tint.rgb, base.a * input.tint.a);

    var normal = geometry_normal;
    if !material_has(MATERIAL_FLAT_SHADING) {
        // Pipelines cull by the material's side, so every drawn fragment of a
        // front-sided material faces the camera and every one of a back-sided
        // material faces away: this is Three's flip for back and double sides.
        normal = normal * face;
    }
    if LIT {
        if material_has(MATERIAL_BUMP) {
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
    fragment.vertex_color = select(vec4f(1.0), input.color, material_has(MATERIAL_VERTEX_COLORS));
    fragment.tint = input.tint;
    effect_surface(&surface, fragment);
    let alpha_width = fwidth(surface.opacity);
    if LIT {
        if atmosphere_on() {
            let ground = ground_macro(input.world, surface.normal, surface.roughness);
            surface.normal = ground.normal;
            surface.color = apply_ground_macro(surface.color, ground);
        }
    }

    // Three writes alpha 1 for opaque materials; here they draw with blending off,
    // and nothing reads the HDR target's alpha (the output and water passes read
    // rgb), so their alpha is left as computed.
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

    var color = surface.color;
    if LIT {
        color = shade_standard(surface, input.world, view_direction, geometry_roughness);
    }
    if material_has(MATERIAL_FOG) {
        color = apply_fog(color, input.world);
    }
    return vec4f(color, alpha);
}

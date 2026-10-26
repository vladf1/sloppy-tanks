// Main-pass surface shader: Three r185 MeshStandardMaterial (LIT) or
// MeshBasicMaterial, with map, emissive map, bump map, RGBA vertex colors, flat
// shading, alpha test, fog, the sun's PCF shadow, hemisphere fill and up to four
// point lights; with the atmosphere on, cloud shadows and a sky rim, and its aerial
// haze left to the output pass (`haze_alpha`).
// Lighting and alpha test are variant constants; shadow reception (a variant on
// the WebGL build, `RECEIVE_SHADOW`) and the cheap per-material features are uniform
// branches (`material_has`).
// Lighting runs in world space; every dot and cross product Three evaluates in
// view space is invariant under the view's rotation. Output is linear HDR; the
// output pass tone maps the whole frame, as Three's frame-buffer target does.

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

// The receiving triangle's own normal (from screen derivatives, before any bump or
// macro relief), set by `fs_main`: the plane the blocker search follows.
var<private> receiver_normal: vec3f;

// Sky the ground sees from inside a cast shadow: hard by its caster a wall or hull
// hides much of the sky, so the shade is deepest there and opens toward the
// shadow's tip. Blockers are found with the same hardware-filtered compares as the
// filter (continuous as things move, no noise): four taps around the receiver,
// against its own plane, find the casters above it, and only where there are any,
// three more depths sort them by how far up the sun's ray they stand
// (`penumbra_steps`): one gather per tap gives the four depths its compare
// filtered, which answer every step with the same bilinear weights.
const BLOCKER_SEARCH_TEXELS: f32 = 5.0;
// The caster distance each band of the search stands for, metres: below the first
// step, between the steps and beyond the last.
const BLOCKER_BANDS: vec4f = vec4f(0.5, 1.75, 3.75, 6.5);
// The share of the sky fill that remains at contact, rising to all of it over the
// caster distances in metres.
const SHADOW_CONTACT_FILL: f32 = 0.7;
const SHADOW_FILL_OPENING: vec2f = vec2f(0.3, 5.0);

// The share of the sky fill that reaches the receiver inside its cast shadow, set by
// `search_casters` (1 where nothing casts or no search ran).
var<private> shadow_fill: f32 = 1.0;

// The share of the four search spots' gathered `depths` (bilinearly `weighted`) that
// lies beyond each spot's reference depth: open to the sun, as the compares of the
// shadow sampler (less-equal) would find it.
fn blockers_open(depths: array<vec4f, 4>, references: vec4f, weights: vec4f) -> f32 {
    return dot(step(vec4f(references.x), depths[0]), weights) + dot(step(vec4f(references.y), depths[1]), weights)
        + dot(step(vec4f(references.z), depths[2]), weights) + dot(step(vec4f(references.w), depths[3]), weights);
}

fn search_casters(coord: vec3f, size: f32) {
    // The receiving plane's depth slope per texel along the map's u and its (flipped)
    // v, from its normal and the light's projection (`penumbra_*`, `gpu::penumbra_terms`).
    let c = dot(frame.penumbra_depth.xyz, receiver_normal);
    let across = vec2f(dot(frame.penumbra_u.xyz, receiver_normal), dot(frame.penumbra_v.xyz, receiver_normal));
    let limit = frame.penumbra_u.w;
    let slope = clamp(across * c / max(c * c, 1e-12), vec2f(-limit), vec2f(limit));
    let tolerance = frame.penumbra_v.w + abs(slope.x) + abs(slope.y);
    var references = vec4f(0.0);
    var spots = array<vec2f, 4>();
    var open = 0.0;
    for (var i = 0; i < 4; i++) {
        let offset = vec2f(f32(i & 1) * 2.0 - 1.0, f32(i >> 1u) * 2.0 - 1.0) * BLOCKER_SEARCH_TEXELS;
        references[i] = coord.z + dot(slope, offset) - tolerance;
        spots[i] = coord.xy + offset / size;
        open += textureSampleCompareLevel(shadow_map, shadow_sampler, spots[i], references[i]);
    }
    if open >= 4.0 {
        return;
    }
    // Under casters the spots' own depths answer every depth step: the spots lie whole
    // texels apart, so one set of bilinear weights serves all four gathers (their
    // texels in the gather's order: -u +v, +u +v, +u -v, -u -v).
    let f = fract(coord.xy * size - 0.5);
    let weights = vec4f((1.0 - f.x) * f.y, f.x * f.y, f.x * (1.0 - f.y), (1.0 - f.x) * (1.0 - f.y));
    var depths = array<vec4f, 4>();
    for (var i = 0; i < 4; i++) {
        depths[i] = textureGather(shadow_map, shadow_texel_sampler, spots[i]);
    }
    // The share of the search that casters cover, and those beyond each step.
    let blocked = 4.0 - blockers_open(depths, references, weights);
    if blocked <= 0.0 {
        return;
    }
    let steps = frame.penumbra_steps.xyz;
    var beyond = vec3f(0.0);
    for (var k = 0; k < 3; k++) {
        beyond[k] = 4.0 - blockers_open(depths, references - steps[k], weights);
    }
    // The casters' mean distance up the sun's ray.
    let bands = vec4f(blocked - beyond.x, beyond.x - beyond.y, beyond.y - beyond.z, beyond.z);
    let reach = dot(bands, BLOCKER_BANDS) / blocked;
    let found = smoothstep(0.0, 0.3, blocked);
    let ground = smoothstep(0.5, 0.8, receiver_normal.y);
    shadow_fill = mix(1.0, mix(SHADOW_CONTACT_FILL, 1.0, smoothstep(SHADOW_FILL_OPENING.x, SHADOW_FILL_OPENING.y, reach)), found * ground);
}

// The sun's shadow: Castaño's optimized PCF (The Witness), hardware-filtered
// compares weighted into a smooth 3×3 tent with no dither noise, after the
// light-space normal offset and depth bias. Its four taps sit exactly where the
// hardware's bilinear weights complete the tent, so the penumbra glides without
// stepping as things move; spread any wider or narrower, the taps no longer
// complete it and the shadow jumps at every texel. The tent stays this narrow
// because the sun's own penumbra is under a texel at a tank's height, and a wider
// one washed barrels, antennas and hull edges into a smudge.
fn sun_shadow(world: vec3f, normal: vec3f) -> f32 {
    let position = frame.shadow_matrix * vec4f(world + normal * frame.shadow.y, 1.0);
    let projected = position.xyz / position.w;
    let coord = vec3f(projected.x, 1.0 - projected.y, projected.z + frame.shadow.x);
    let size = frame.shadow.z;
    if !LITE {
        if atmosphere_on() {
            search_casters(coord, size);
        }
    }
    let texels = coord.xy * size;
    let base = floor(texels + 0.5);
    let st = texels + 0.5 - base;
    let origin = (base - 0.5) / size;
    let texel = frame.shadow.w / size;
    let uw = vec2f(3.0 - 2.0 * st.x, 1.0 + 2.0 * st.x);
    let u = vec2f((2.0 - st.x) / uw.x - 1.0, st.x / uw.y + 1.0) * texel;
    let vw = vec2f(3.0 - 2.0 * st.y, 1.0 + 2.0 * st.y);
    let v = vec2f((2.0 - st.y) / vw.x - 1.0, st.y / vw.y + 1.0) * texel;
    var sum = 0.0;
    for (var j = 0; j < 2; j++) {
        for (var i = 0; i < 2; i++) {
            let tap = origin + vec2f(u[i], v[j]);
            sum += uw[i] * vw[j] * textureSampleCompareLevel(shadow_map, shadow_sampler, tap, coord.z);
        }
    }
    sum /= 16.0;
    // Shadows fade out over the map's last texels rather than stopping at its edge,
    // where a cliff beyond the fitted box would leave a straight-cut band.
    let edge = min(min(coord.x, 1.0 - coord.x), min(coord.y, 1.0 - coord.y));
    let inside = smoothstep(0.0, SHADOW_EDGE_FADE, edge) * select(0.0, 1.0, coord.z <= 1.0);
    return mix(1.0, sum, inside);
}

// The share of the shadow map over which shadows fade out toward its edge.
const SHADOW_EDGE_FADE: f32 = 0.04;

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
    // Only faces turned toward the sun use its shadow and the clouds', and ground,
    // whose cast shadow also dims its sky fill (`shadow_fill`): on the rest every term
    // they scale is zero.
    let toward_sun = dot(n, frame.sun_direction.xyz) > 0.0;
    let shaded = toward_sun || receiver_normal.y > 0.5;
    // WebGL's translation paid for the uniform branch in every lit pixel.
    if LITE {
        if RECEIVE_SHADOW {
            if shaded {
                sunlit = sun_shadow(world, n);
            }
        }
    } else if material_has(MATERIAL_RECEIVE_SHADOW) && shaded {
        sunlit = sun_shadow(world, n);
    }
    // Inside a cast shadow, how much of the sky's fill the caster leaves open.
    let sky_open = mix(1.0, shadow_fill, 1.0 - sunlit);
    // Under a passing cloud the light is the cloud's own white scatter as much as the
    // blue sky, so cloud shade keeps the fill neutral as direct sun does; only cast
    // shadows take its blue.
    let cast_sunlit = sunlit;
    if atmosphere_on() && toward_sun {
        sunlit *= cloud_shadow(world);
    }
    let sun = frame.sun_color.rgb * sunlit;
    add_direct_light(&reflected, frame.sun_direction.xyz, sun, v, n, diffuse, specular, roughness);
    // The seated view looks along its own hull's deck at a grazing angle, where the
    // sky's Fresnel sheen and the sun's glint would bleach the paint to a pale sheet:
    // the nearest few metres reflect less of both and keep their paint.
    let near_view = 1.0 - smoothstep(VIEW_MODEL_NEAR, VIEW_MODEL_FAR, distance(world, frame.camera_position.xyz));
    if atmosphere_on() {
        // Satin paint (hulls, containers, vehicles) carries a clear coat: a tight
        // sun glint along plate edges and decks turned to the light, on top of the
        // paint's broad sheen, so painted steel reads as metal and its form turns.
        // Only satin finishes: rougher terrain and walls, and metals, skip it.
        if s.roughness > 0.38 && s.roughness < 0.66 && s.metalness < 0.45 {
            let coat = PAINT_GLINT * smoothstep(0.38, 0.45, s.roughness) * (1.0 - smoothstep(0.58, 0.66, s.roughness))
                * (1.0 - smoothstep(0.25, 0.45, s.metalness)) * (1.0 - near_view);
            // Its grazing Fresnel is capped: a stylized coat glints toward the sun but
            // never mirrors a whole deck seen edge-on.
            let l = frame.sun_direction.xyz;
            let glint = clamp(dot(n, l), 0.0, 1.0) * brdf_ggx(l, v, n, vec3f(0.04), PAINT_GLINT_F90, PAINT_GLINT_ROUGHNESS);
            reflected.specular += sun * (glint * coat);
        }
        // The sun's glint on wet ground and glossy paint eases toward a ceiling instead
        // of blowing out: a camera looking toward the sun across puddles and damp
        // concrete (whose gloss falls in the satin coat's band) saw broad white
        // sheets with bright rims where their gloss fades, rather than sparkle.
        // Satin ground (damp concrete, packed earth) facing up eases to a lower
        // ceiling: the broad lobe of its sheen otherwise lit a whole damp patch as
        // one pale sheet toward a low sun.
        let glare = max(max(reflected.specular.r, reflected.specular.g), reflected.specular.b);
        let satin_ground = smoothstep(0.8, 0.95, n.y) * smoothstep(0.3, 0.4, s.roughness);
        reflected.specular /= 1.0 + glare / mix(SUN_GLARE_LIMIT, GROUND_GLARE_LIMIT, satin_ground);
    }
    // The point lights (blast flashes and fires), all four at once in the lanes of
    // vectors: one copy of the code rather than four unrolled ones in every lit
    // variant. Their light is summed with its irradiance-weighted direction and lit
    // once, so the GGX lobe and its two DFG taps run once for all of them, and only
    // where a light reaches: exact but for several lights at once, whose highlights
    // on glossy paint merge.
    let lights = frame.point_lights;
    let strength = vec4f(
        dot(lights[0].color.rgb, vec3f(1.0)),
        dot(lights[1].color.rgb, vec3f(1.0)),
        dot(lights[2].color.rgb, vec3f(1.0)),
        dot(lights[3].color.rgb, vec3f(1.0)),
    );
    if any(strength > vec4f(0.0)) {
        let dx = vec4f(lights[0].position.x, lights[1].position.x, lights[2].position.x, lights[3].position.x) - world.x;
        let dy = vec4f(lights[0].position.y, lights[1].position.y, lights[2].position.y, lights[3].position.y) - world.y;
        let dz = vec4f(lights[0].position.z, lights[1].position.z, lights[2].position.z, lights[3].position.z) - world.z;
        let cutoff = vec4f(lights[0].position.w, lights[1].position.w, lights[2].position.w, lights[3].position.w);
        // With the atmosphere on, fire lights a wide, soft pool rather than a hot spot
        // under itself: a gentler falloff than Three's inverse square (the cutoff
        // window still ends it).
        let decay = vec4f(lights[0].color.w, lights[1].color.w, lights[2].color.w, lights[3].color.w)
            * select(1.0, FIRE_POOL_DECAY, atmosphere_on());
        let span = dx * dx + dy * dy + dz * dz;
        // Past the cutoff the attenuation window is exactly zero: the rest runs only
        // where some light reaches.
        let reaches = (strength > vec4f(0.0)) & ((cutoff <= vec4f(0.0)) | (span < cutoff * cutoff));
        if any(reaches) {
            let distance = sqrt(span);
            // Three's distance attenuation (`distance_attenuation`), lane by lane.
            let falloff = 1.0 / max(pow(distance, decay), vec4f(0.01));
            let ratio = distance / max(cutoff, vec4f(1e-6));
            let window = clamp(1.0 - ratio * ratio * ratio * ratio, vec4f(0.0), vec4f(1.0));
            let attenuation = select(vec4f(0.0), select(falloff, falloff * window * window, cutoff > vec4f(0.0)), reaches);
            let inverse = 1.0 / max(distance, vec4f(1e-6));
            let facing = (dx * n.x + dy * n.y + dz * n.z) * inverse;
            let luma = vec4f(
                dot(lights[0].color.rgb, vec3f(0.2126, 0.7152, 0.0722)),
                dot(lights[1].color.rgb, vec3f(0.2126, 0.7152, 0.0722)),
                dot(lights[2].color.rgb, vec3f(0.2126, 0.7152, 0.0722)),
                dot(lights[3].color.rgb, vec3f(0.2126, 0.7152, 0.0722)),
            );
            // What scales each light's colour.
            var scale = attenuation;
            var wrap_scale = vec4f(0.0);
            var albedo = diffuse;
            var share = 1.0;
            if atmosphere_on() {
                // Brighter overall, so blasts and burning wrecks visibly light the
                // ground, walls and hulls around them in daylight; but added over sunlit
                // grass, amber only yellowed the green into an acid blotch, so sunlit
                // faces take less of it. Each light eases toward its ceiling
                // (`FIRE_LIGHT_LIMIT`).
                let gain = BLAST_LIGHT_GAIN * (1.0 - FIRE_SUNLIT_DIM * sunlit * clamp(dot(n, frame.sun_direction.xyz), 0.0, 1.0));
                scale = attenuation * gain / (1.0 + luma * attenuation * gain / FIRE_LIGHT_LIMIT);
                // Fire is a wide, rolling source, not a point: its light wraps past the
                // terminator, so the ground and the sides of hulls and walls around a
                // blast glow rather than only the faces square to it.
                let wrapped = clamp((facing + FIRE_WRAP) / (1.0 + FIRE_WRAP), vec4f(0.0), vec4f(1.0)) - clamp(facing, vec4f(0.0), vec4f(1.0));
                wrap_scale = max(wrapped, vec4f(0.0)) * scale;
                // Amber light on green grass multiplies to an acid yellow-green blotch:
                // the surface lends firelight only part of its own hue, so the pool reads
                // as an orange glow on any ground.
                albedo = mix(diffuse, vec3f(dot(diffuse, vec3f(0.2126, 0.7152, 0.0722))), FIRE_ALBEDO_GREY);
                // Even greyed, warm light added over green grass reads lime: foliage
                // takes only a share of it, walls, earth and hulls all.
                let green = clamp((diffuse.g - max(diffuse.r, diffuse.b)) / max(diffuse.g, 1e-4), 0.0, 1.0);
                share = 1.0 - FIRE_ON_GREEN_CUT * green;
            }
            let lit_scale = clamp(facing, vec4f(0.0), vec4f(1.0)) * scale;
            let colors = mat4x3f(lights[0].color.rgb, lights[1].color.rgb, lights[2].color.rgb, lights[3].color.rgb);
            var irradiance = colors * lit_scale;
            var wrap = colors * wrap_scale;
            if atmosphere_on() {
                irradiance = fire_light_hue(irradiance);
                wrap = fire_light_hue(wrap);
            }
            reflected.diffuse += (irradiance + wrap) * (share * RECIPROCAL_PI) * albedo;
            if dot(irradiance, vec3f(1.0)) > 0.0 {
                let weight = luma * lit_scale * inverse;
                let direction = vec3f(dot(dx, weight), dot(dy, weight), dot(dz, weight));
                let l = direction / max(length(direction), 1e-6);
                reflected.specular += irradiance * share * brdf_ggx_multiscatter(l, v, n, specular, 1.0, roughness);
            }
        }
    }
    var hemisphere = mix(frame.ground_color.rgb, frame.sky_color.rgb, n.y * 0.5 + 0.5);
    // How directly the sun reaches this point: cast and cloud shadow, and faces
    // turned from it.
    let in_sun = sunlit * smoothstep(0.0, 0.35, dot(n, frame.sun_direction.xyz));
    var shade_color = diffuse;
    if atmosphere_on() {
        // Sky irradiance with form: a face sees less of the bright dome the further
        // it turns from up, and the sky is brightest around the sun, so roofs, hull
        // decks and the sides turned toward the sun's quarter read lighter than the
        // flanks and fronts turned away even where all of them are in shade. Without
        // it every shaded face of a hull takes the same fill and reads as one slab.
        let up = n.y * 0.5 + 0.5;
        let dome = up * up * (SKY_DOME_FORM + (1.0 - SKY_DOME_FORM) * up);
        let sun_side = dot(n.xz, frame.sun_heading.xy) * (1.0 - abs(n.y));
        hemisphere = mix(frame.ground_color.rgb, frame.sky_color.rgb, dome) * (1.0 + SKY_SUN_SIDE * sun_side);
        // Warm key, cool fill: the blue sky fill keeps its hue where it is the main
        // light (shade), but under direct sun it adds only its brightness, so sunlit
        // sand, concrete and grass keep their own warm albedo instead of drifting
        // toward mauve or grey while shadows read clearly cool.
        let fill_level = dot(hemisphere, vec3f(0.2126, 0.7152, 0.0722));
        let open_sky = cast_sunlit * smoothstep(0.0, 0.35, dot(n, frame.sun_direction.xyz));
        hemisphere = mix(hemisphere, vec3f(fill_level), FILL_NEUTRAL_IN_SUN * open_sky);
        // Painterly shade: what the sky fill alone lights keeps a richer version of
        // its own color, so grass shade stays green and earth shade warm brown
        // instead of every shadow settling into the same grey-blue.
        // Sand and bare earth keep only a little more: their shade reads cool, not
        // a brown wash, while grass and team paint stay rich.
        let grey = dot(diffuse, vec3f(0.2126, 0.7152, 0.0722));
        let warmth = diffuse.g / max(diffuse.r, 1e-4);
        let earth = smoothstep(0.4, 0.5, warmth) * (1.0 - smoothstep(0.85, 0.95, warmth))
            * step(diffuse.b, diffuse.g);
        shade_color = max(mix(vec3f(grey), diffuse, mix(SHADE_SATURATION, EARTH_SHADE_SATURATION, earth)), vec3f(0.0));
    }
    var indirect = hemisphere * shade_color * (RECIPROCAL_PI * sky_open);
    if atmosphere_on() {
        // Warm bounce from the sunlit ground around (`bounce`, the ground fill's hue at
        // the sun's level): what the sun does not reach directly (cast shadow, cloud
        // shadow or a face turned away) still catches some of its light off the ground,
        // most on walls and hull sides, so shade stays warm and luminous rather than
        // sinking into one cold blue.
        indirect += frame.bounce.rgb * ((1.0 - in_sun) * (1.0 - BOUNCE_FROM_BELOW * n.y)) * shade_color;
        // Stylized sky rim: grazing walls and hull sides catch the bright horizon,
        // most on the side turned from the sun, so silhouettes read on any ground.
        let facing = 1.0 - clamp(dot(n, v), 0.0, 1.0);
        let away = 0.55 + 0.45 * clamp(-dot(n.xz, frame.sun_heading.xy), 0.0, 1.0);
        let rim = facing * facing * facing * (1.0 - n.y * n.y) * away;
        indirect += frame.rim.rgb * diffuse * rim;
    }
    // Only glossy finishes (vehicle paint and steel) reflect the sky; matte
    // terrain and buildings keep the hemisphere fill their themes were tuned with.
    var reflections = frame.sky_color.w * (1.0 - smoothstep(GLOSSY_UNTIL, MATTE_FROM, s.roughness));
    if atmosphere_on() {
        reflections *= 1.0 - VIEW_MODEL_SHEEN_CUT * near_view;
    }
    if reflections > 0.0 {
        // Split-sum environment specular with Three's multiscatter terms
        // (`computeMultiscattering`): the reflected sky takes the energy it
        // reflects away from the diffuse fill, so grazing paint gains a sheen
        // and metals stop going dark outside the sun's highlight.
        let dot_nv = clamp(dot(n, v), 0.0, 1.0);
        let scale_bias = dfg(roughness, dot_nv);
        var single = specular * scale_bias.x + scale_bias.y;
        if atmosphere_on() {
            // Wet ground and puddles seen at a grazing angle would mirror the bright
            // sky toward the sun as whole white sheets: open ground keeps a sheen.
            single = min(single, vec3f(mix(1.0, GROUND_SHEEN_MOST, smoothstep(0.8, 0.95, n.y))));
        }
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
        let ray = normalize(vec3f(direction.x, max(up, REFLECTED_SKY_FLOOR), direction.z));
        var drawn = sky_dome(ray);
        // Toward the sun the reflected sky takes the sun's hue, and everywhere it
        // keeps only part of its blue: a clear blue mirrored over warm wet ground or
        // paint mixed to violet.
        let level = dot(drawn, vec3f(0.2126, 0.7152, 0.0722));
        let toward_sun = max(dot(ray, frame.sun_direction.xyz), 0.0);
        let sun_hue = frame.sky_sun.rgb * (level / max(dot(frame.sky_sun.rgb, vec3f(0.2126, 0.7152, 0.0722)), 1e-4));
        drawn = mix(mix(vec3f(level), drawn, REFLECTED_SKY_CHROMA), sun_hue, toward_sun * toward_sun * REFLECTED_SUN_HUE);
        sky = mix(drawn, zenith, lobe * 0.5);
    }
    let ground = frame.ground_color.rgb * RECIPROCAL_PI;
    let horizon_width = HORIZON_SHARPEST + lobe;
    return mix(ground, sky, smoothstep(-horizon_width, horizon_width, up));
}

// The clear-coat sun glint on satin paint (`shade_standard`): its strength, the
// coat's roughness and its Fresnel at grazing angles.
const PAINT_GLINT: f32 = 0.45;
// The level the sun's specular eases toward (`shade_standard`), against a sun of about 4.
const SUN_GLARE_LIMIT: f32 = 0.6;
const GROUND_GLARE_LIMIT: f32 = 0.2;
const PAINT_GLINT_ROUGHNESS: f32 = 0.22;
const PAINT_GLINT_F90: f32 = 0.3;
// Sky irradiance form (`shade_standard`): the sky's share of the fill follows
// up² × (k + (1 - k) × up) for up = n.y / 2 + 1/2, which keeps decks and roofs at the
// full sky and undersides at the ground but gives walls and hull flanks a third of
// the sky rather than half. Faces toward the sun's quarter of the sky take this much
// more fill, and those turned away this much less.
const SKY_DOME_FORM: f32 = 1.6;
const SKY_SUN_SIDE: f32 = 0.22;
// Bounce off the sunlit ground comes from below: walls and hull flanks take it in
// full, undersides more and decks and roofs little.
const BOUNCE_FROM_BELOW: f32 = 0.8;
// The share of the sky fill's blue a fully sunlit surface gives up (it keeps the
// fill's brightness).
const FILL_NEUTRAL_IN_SUN: f32 = 0.9;
// Saturation of the sky fill's share of a surface's light (1 = its plain color).
const SHADE_SATURATION: f32 = 1.12;
const EARTH_SHADE_SATURATION: f32 = 0.85;
// Point lights' (blast flashes') extra strength with the atmosphere on, and the
// share of their decay exponent they keep (a wider pool).
const BLAST_LIGHT_GAIN: f32 = 5.5;
const FIRE_POOL_DECAY: f32 = 0.72;
// How far past the terminator firelight wraps (0 = a point light's plain cosine).
const FIRE_WRAP: f32 = 0.6;
// Fire's own color at unit luminance, and how far a deep red-orange flash light turns
// toward it: a pure red-orange pool over sunlit ground and blue-filled shade reads as a
// pink screen tint rather than firelight.
const FIRE_AMBER: vec3f = vec3f(1.0, 0.48, 0.17);
// The share of firelight a fully sunlit face, square to the sun, does without.
const FIRE_SUNLIT_DIM: f32 = 0.75;
// The share of a surface's own hue firelight gives up for its brightness.
const FIRE_ALBEDO_GREY: f32 = 0.8;
// The share of firelight a fully green surface (grass, leaves) does without.
const FIRE_ON_GREEN_CUT: f32 = 0.8;
const FIRE_AMBER_SHARE: f32 = 0.85;

// The luminance a flash light's pool eases toward right beside it, against a sun of
// about 3: firelight warms shade and the sunlit ground around a blast without washing
// a near-camera muzzle flash or fireball across the frame.
const FIRE_LIGHT_LIMIT: f32 = 5.0;

// Firelight turned toward amber, keeping its luminance: linear, so the lights' sum
// takes it at once.
fn fire_light_hue(light: vec3f) -> vec3f {
    let amber = FIRE_AMBER / dot(FIRE_AMBER, vec3f(0.2126, 0.7152, 0.0722));
    return mix(light, amber * dot(light, vec3f(0.2126, 0.7152, 0.0722)), FIRE_AMBER_SHARE);
}
// The seated view's own hull (`shade_standard`): within the first distance of the
// eye its sky sheen is cut by this share and its glint entirely, easing back by the
// second.
const VIEW_MODEL_NEAR: f32 = 4.0;
const VIEW_MODEL_FAR: f32 = 8.0;
const VIEW_MODEL_SHEEN_CUT: f32 = 0.75;
// The most of the sky that upward-facing ground (puddles, wet concrete) reflects.
const GROUND_SHEEN_MOST: f32 = 0.22;
// Share of the drawn sky glossy finishes reflect.
const ATMOSPHERE_REFLECTION: f32 = 0.45;
// The share of the reflected sky's own colour glossy finishes keep, and how far it
// turns to the sun's hue looking toward the sun.
const REFLECTED_SKY_CHROMA: f32 = 0.6;
const REFLECTED_SUN_HUE: f32 = 0.7;
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
    receiver_normal = flat_normal;
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
    // so their alpha is free to carry the haze the output pass adds (`haze_alpha`).
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
        if ALPHA_TO_COVERAGE {
            // Its alpha is the coverage, so it hazes itself.
            color = apply_fog(color, input.world);
        } else if !atmosphere_on() {
            color = linear_fog(color, input.world);
        } else {
            // Faded instances (opacity below 1) blend too, their alpha their opacity.
            let blended = material_has(MATERIAL_BLENDED) || input.tint.a < 1.0;
            if beyond_haze_start(input.world) {
                let haze = haze_weights(frame.camera_position.xyz, input.world, view_depth(input.world));
                if blended {
                    color = blended_haze(color, haze);
                } else {
                    color = haze_kept(color, haze);
                    alpha = haze_alpha(haze);
                }
            } else if !blended {
                // No haze: `haze_alpha` of none.
                alpha = 1.0;
            }
        }
    }
    return vec4f(color, alpha);
}

// Post-processing: the bloom chain and the output pass. Bloom thresholds the
// resolved linear HDR frame into a half-size level, halves it down a short mip
// chain and adds each level back up with a tent filter. The output pass adds the
// bloom, darkens by the ambient occlusion, tone maps through Three r185's ACES
// filmic curve, grades (lift, gamma, gain, saturation, contrast, vignette) and
// encodes sRGB with a little dither against sky banding, into the (non-sRGB)
// canvas texture. With everything neutral it is Three's plain ACES output.

struct Output {
    // x exposure; the HDR row of a fragment is y + z × its framebuffer row: 0 and 1
    // where the target's rows run top-down like the HDR target's (WebGPU), height and
    // -1 on WebGL's bottom-up canvas. w bloom strength.
    settings: vec4f,
    // rgb lift, w saturation.
    lift: vec4f,
    // rgb gamma, w contrast.
    gamma: vec4f,
    // rgb gain, w vignette.
    gain: vec4f,
    // x bloom threshold, y its soft knee, z occlusion strength, w dither in 8-bit steps.
    effects: vec4f,
    // x vibrance: extra saturation for muted colors that leaves vivid ones alone.
    tone: vec4f,
}

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var<uniform> output: Output;
@group(0) @binding(2) var linear_clamp: sampler;
@group(0) @binding(6) var bloom: texture_2d<f32>;
@group(0) @binding(7) var occlusion: texture_2d<f32>;

@vertex
fn vs_fullscreen(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
    let uv = vec2f(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4f(uv * 2.0 - 1.0, 0.0, 1.0);
}

struct PostOut {
    @builtin(position) clip: vec4f,
    // Texture coordinates in WebGPU's top-down rows on both backends.
    @location(0) uv: vec2f,
}

@vertex
fn vs_post(@builtin(vertex_index) index: u32) -> PostOut {
    let corner = vec2f(f32((index << 1u) & 2u), f32(index & 2u));
    var out: PostOut;
    out.clip = vec4f(corner * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2f(corner.x, 1.0 - corner.y);
    return out;
}

fn luminance(color: vec3f) -> f32 {
    return dot(color, vec3f(0.2126, 0.7152, 0.0722));
}

fn tap(uv: vec2f) -> vec3f {
    return textureSampleLevel(source, linear_clamp, uv, 0.0).rgb;
}

// Four bilinear taps covering 4×4 source texels, each weighted down by its own
// brightness (Karis) so a lone hot pixel cannot flicker, then a soft threshold.
@fragment
fn fs_bloom_prefilter(input: PostOut) -> @location(0) vec4f {
    let texel = 1.0 / vec2f(textureDimensions(source));
    var sum = vec3f(0.0);
    var weight = 0.0;
    for (var i = 0; i < 4; i++) {
        let offset = vec2f(f32(i & 1) * 2.0 - 1.0, f32(i >> 1u) * 2.0 - 1.0) * texel;
        let color = min(tap(input.uv + offset), vec3f(64.0));
        let w = 1.0 / (1.0 + luminance(color));
        sum += color * w;
        weight += w;
    }
    let color = sum / weight;
    let threshold = output.effects.x;
    let knee = max(output.effects.y, 1e-4);
    let brightness = max(max(color.r, color.g), color.b);
    var soft = clamp(brightness - threshold + knee, 0.0, 2.0 * knee);
    soft = soft * soft / (4.0 * knee);
    let contribution = max(soft, brightness - threshold) / max(brightness, 1e-4);
    return vec4f(color * contribution, 1.0);
}

// Call of Duty's 13-tap downsample: a 6×6 texel footprint without aliasing.
@fragment
fn fs_bloom_down(input: PostOut) -> @location(0) vec4f {
    let t = 1.0 / vec2f(textureDimensions(source));
    let uv = input.uv;
    let a = tap(uv + t * vec2f(-2.0, -2.0));
    let b = tap(uv + t * vec2f(0.0, -2.0));
    let c = tap(uv + t * vec2f(2.0, -2.0));
    let d = tap(uv + t * vec2f(-2.0, 0.0));
    let e = tap(uv);
    let f = tap(uv + t * vec2f(2.0, 0.0));
    let g = tap(uv + t * vec2f(-2.0, 2.0));
    let h = tap(uv + t * vec2f(0.0, 2.0));
    let i = tap(uv + t * vec2f(2.0, 2.0));
    let j = tap(uv + t * vec2f(-1.0, -1.0));
    let k = tap(uv + t * vec2f(1.0, -1.0));
    let l = tap(uv + t * vec2f(-1.0, 1.0));
    let m = tap(uv + t * vec2f(1.0, 1.0));
    var color = e * 0.125 + (a + c + g + i) * 0.03125 + (b + d + f + h) * 0.0625;
    color += (j + k + l + m) * 0.125;
    return vec4f(color, 1.0);
}

// A 3×3 tent over the smaller level, added onto the larger one by the blend.
@fragment
fn fs_bloom_up(input: PostOut) -> @location(0) vec4f {
    let t = 1.0 / vec2f(textureDimensions(source));
    let uv = input.uv;
    var color = tap(uv) * 4.0;
    color += (tap(uv + vec2f(-t.x, 0.0)) + tap(uv + vec2f(t.x, 0.0))) * 2.0;
    color += (tap(uv + vec2f(0.0, -t.y)) + tap(uv + vec2f(0.0, t.y))) * 2.0;
    color += tap(uv + vec2f(-t.x, -t.y)) + tap(uv + vec2f(t.x, -t.y));
    color += tap(uv + vec2f(-t.x, t.y)) + tap(uv + vec2f(t.x, t.y));
    return vec4f(color / 16.0, 1.0);
}

fn rrt_and_odt_fit(v: vec3f) -> vec3f {
    let a = v * (v + 0.0245786) - 0.000090537;
    let b = v * (0.983729 * v + 0.4329510) + 0.238081;
    return a / b;
}

fn aces_filmic(color: vec3f, exposure: f32) -> vec3f {
    // Columns of Three's row-major mat3 constants.
    let input_matrix = mat3x3f(
        vec3f(0.59719, 0.07600, 0.02840),
        vec3f(0.35458, 0.90834, 0.13383),
        vec3f(0.04823, 0.01566, 0.83777),
    );
    let output_matrix = mat3x3f(
        vec3f(1.60475, -0.10208, -0.00327),
        vec3f(-0.53108, 1.10813, -0.07276),
        vec3f(-0.07367, -0.00605, 1.07602),
    );
    let mapped = rrt_and_odt_fit(input_matrix * (color * exposure / 0.6));
    return clamp(output_matrix * mapped, vec3f(0.0), vec3f(1.0));
}

fn srgb_encode(color: vec3f) -> vec3f {
    let high = pow(color, vec3f(0.41666)) * 1.055 - 0.055;
    let low = color * 12.92;
    return select(high, low, color <= vec3f(0.0031308));
}

// The grade on the tone-mapped display color: lift raises the blacks toward its
// color, gain scales the whites, gamma bends the mids, then saturation (and vibrance
// for muted colors) and contrast
// around mid grey.
fn grade(color: vec3f) -> vec3f {
    let lift = output.lift.rgb;
    var c = output.gain.rgb * (color + lift * (vec3f(1.0) - color));
    c = pow(max(c, vec3f(0.0)), 1.0 / output.gamma.rgb);
    let grey = luminance(c);
    let chroma = max(max(c.r, c.g), c.b) - min(min(c.r, c.g), c.b);
    let vibrance = 1.0 + output.tone.x * (1.0 - smoothstep(0.0, 0.6, chroma));
    c = mix(vec3f(grey), c, output.lift.w * vibrance);
    c = (c - 0.18) * output.gamma.w + 0.18;
    return clamp(c, vec3f(0.0), vec3f(1.0));
}

@fragment
fn fs_output(@builtin(position) position: vec4f) -> @location(0) vec4f {
    let row = output.settings.y + output.settings.z * position.y;
    let pixel = vec2f(position.x, row);
    let size = vec2f(textureDimensions(source));
    let uv = pixel / size;
    var color = textureLoad(source, vec2i(i32(position.x), i32(row)), 0).rgb;
    let strength = output.effects.z;
    if strength > 0.0 {
        // Four bilinear taps over the half-size occlusion: a 4×4 box that
        // averages its 16 rotations of the sample pattern away.
        let texel = 1.0 / vec2f(textureDimensions(occlusion));
        var ao = 0.0;
        for (var i = 0; i < 4; i++) {
            let offset = vec2f(f32(i & 1) * 2.0 - 1.0, f32(i >> 1u) * 2.0 - 1.0) * texel;
            ao += textureSampleLevel(occlusion, linear_clamp, uv + offset, 0.0).r;
        }
        color *= mix(1.0, ao * 0.25, strength);
    }
    let bloom_strength = output.settings.w;
    if bloom_strength > 0.0 {
        color += textureSampleLevel(bloom, linear_clamp, uv, 0.0).rgb * bloom_strength;
    }
    var display = grade(aces_filmic(color, output.settings.x));
    let vignette = output.gain.w;
    if vignette > 0.0 {
        let centered = (uv - 0.5) * vec2f(size.x / size.y, 1.0);
        display *= 1.0 - vignette * smoothstep(0.35, 1.05, length(centered));
    }
    var encoded = srgb_encode(display);
    let dither = output.effects.w;
    if dither > 0.0 {
        // Interleaved gradient noise, ±half an 8-bit step.
        let noise = fract(52.9829189 * fract(dot(position.xy, vec2f(0.06711056, 0.00583715))));
        encoded += (noise - 0.5) * dither / 255.0;
    }
    return vec4f(encoded, 1.0);
}

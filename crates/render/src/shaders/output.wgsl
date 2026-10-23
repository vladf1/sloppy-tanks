// Post-processing: the bloom chain, the occlusion's blur and the output pass. Bloom thresholds the
// resolved linear HDR frame into a quarter-size level, halves it down a short mip
// chain and adds each level back up with a tent filter. The output pass darkens by
// the ambient occlusion, adds the aerial haze's light the opaque surfaces left to it
// (`haze_alpha`), adds the bloom, tone maps (with the atmosphere on, a
// hue-keeping filmic curve on each colour's brightest channel; otherwise Three r185's
// ACES filmic curve with a gamut fit in place of a hard clip), grades (black point, lift,
// gamma, gain, split toning, a few tamed hues, saturation, contrast, vignette),
// sharpens edges and encodes sRGB with a little dither against sky banding, into
// the (non-sRGB) canvas texture. Where the scene rendered smaller than the canvas
// (the dynamic render scale, `quality.rs`) it upsamples it on the way. With
// everything neutral it is Three's plain ACES output.

struct Output {
    // x exposure; the top-down canvas row of a fragment is y + z × its framebuffer
    // row: 0 and 1 where the target's rows run top-down like the HDR target's
    // (WebGPU), height and -1 on WebGL's bottom-up canvas. w bloom strength.
    settings: vec4f,
    // rgb lift, w saturation.
    lift: vec4f,
    // rgb gamma, w contrast.
    gamma: vec4f,
    // rgb gain, w vignette.
    gain: vec4f,
    // x bloom threshold, y its soft knee, z occlusion strength, w dither in 8-bit steps.
    effects: vec4f,
    // x vibrance: extra saturation for muted colors that leaves vivid ones alone,
    // y black point, z how much of the way out-of-gamut colors desaturate rather
    // than clip, w where the brightest channel's shoulder begins (0 clamps).
    tone: vec4f,
    // How much x lime foliage turns toward a true green, y orange earth calms and
    // z red hulls shed the pink the blue sky fill gives them, each 0..1; w how
    // quickly a bright bloom saturates (0 adds it all).
    hues: vec4f,
    // Split toning: rgb the shadows' tint (unit luminance), w its strength.
    shadows: vec4f,
    // rgb the highlights' tint (unit luminance), w its strength.
    highlights: vec4f,
    // x edge sharpening 0..1 (0 skips its taps); yz the reciprocal canvas size and
    // w 1 where the scene rendered smaller than the canvas and is upsampled.
    detail: vec4f,
    // The filmic curve (`filmic`): x its toe power (0 for ACES), y its shoulder, zw
    // its b and c terms.
    curve: vec4f,
}

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var<uniform> output: Output;
@group(0) @binding(2) var linear_clamp: sampler;
@group(0) @binding(6) var bloom: texture_2d<f32>;
@group(0) @binding(7) var occlusion: texture_2d<f32>;
// The main view's uniform, for the haze (`common.wgsl`).
@group(0) @binding(8) var<uniform> frame: Frame;

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

// The haze's light the opaque surfaces at `uv` (top-down scene coordinates) left to
// this pass, `alpha` their `haze_alpha`, along the main view's ray through it.
fn scene_haze(color: vec3f, alpha: f32, uv: vec2f) -> vec3f {
    if !atmosphere_on() || alpha <= 1.0 {
        return color;
    }
    let ndc = vec2f(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let far = frame.sky_rays * vec4f(ndc, 1.0, 1.0);
    let near = frame.sky_rays * vec4f(ndc, 0.0, 1.0);
    let ray = normalize(far.xyz / far.w - near.xyz / near.w);
    return color + haze_inscatter(ray, haze_weights_of(alpha));
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

// The occlusion's 3×3 tent of bilinear taps on texel corners: 6×6 texels in which
// every rotation of the 4×4 sample pattern carries the same weight, so the pattern
// averages away without a dotted residue. Texel (i, j) of the result holds the tent
// at the corner where texels i and j begin, one size larger than the occlusion along
// each axis; the output pass reads it once for the pixels nearest that corner rather
// than taking nine taps for each of them.
@fragment
fn fs_occlusion_blur(@builtin(position) position: vec4f) -> @location(0) vec4f {
    let corner = floor(position.xy);
    let size = vec2f(textureDimensions(source));
    var ao = 0.0;
    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            let at = (corner + vec2f(f32(i), f32(j)) * 2.0) / size;
            let weight = f32((2 - abs(i)) * (2 - abs(j)));
            ao += textureSampleLevel(source, linear_clamp, at, 0.0).r * weight;
        }
    }
    return vec4f(ao / 16.0);
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
    let exposed = color * exposure / 0.6;
    let mapped = fit_gamut(output_matrix * rrt_and_odt_fit(input_matrix * exposed));
    // Per channel the curve bleaches a bright saturated colour: a sunlit red deck's
    // red channel flattens at the shoulder while its small green and blue climb, so
    // it turns salmon and reads as the same flat value as the shaded flank. For
    // saturated colours the other channels instead keep their ratio to the
    // brightest one, which keeps the level the curve gave it: lit and shaded faces
    // of a team-coloured hull keep their hue and their value steps. Fire and other
    // very hot colours still bleach toward white.
    let peak = max(max(exposed.r, exposed.g), exposed.b);
    let low = min(min(exposed.r, exposed.g), exposed.b);
    let saturated = smoothstep(HUE_KEEP_FROM, HUE_KEEP_FULL, (peak - low) / max(peak, 1e-5))
        * (1.0 - smoothstep(HUE_KEEP_HOT.x, HUE_KEEP_HOT.y, peak));
    if saturated <= 0.0 {
        return mapped;
    }
    let kept = exposed * (max(max(mapped.r, mapped.g), mapped.b) / max(peak, 1e-5));
    return mix(mapped, clamp(kept, vec3f(0.0), vec3f(1.0)), saturated * HUE_KEEP);
}

// The filmic curve x^a / (x^(a d) b + c) (Lottes 2016) on each colour's brightest
// channel, the others keeping their ratio to it: hue and saturation pass through, so
// a red hull stays red and a navy one navy in sun and shade alike, and the curve's
// toe presses only the deepest shade toward black instead of ACES's toe, which,
// with the black point after it, sent every dark painted face in shade to black.
// Only colours hotter than sunlit white (fire, the sun's disc) bleach toward white
// as they saturate.
fn filmic(color: vec3f, exposure: f32) -> vec3f {
    let exposed = max(color * exposure, vec3f(0.0));
    let peak = max(max(exposed.r, exposed.g), exposed.b);
    if peak <= 1e-6 {
        return vec3f(0.0);
    }
    let toned = filmic_curve(exposed);
    // The curve rises monotonically (its shoulder is at most 1), so the brightest
    // channel's is the largest.
    let toned_peak = max(max(toned.r, toned.g), toned.b);
    let hot = smoothstep(FILMIC_HOT.x, FILMIC_HOT.y, peak);
    let kept = toned_peak * mix(exposed / peak, vec3f(1.0), hot * hot);
    // Part of the way per channel: its toe deepens the lesser channels of muted
    // colours a little, so sunlit sand, grass and concrete keep a film's richness
    // instead of reading pale, while saturated paint keeps its hue.
    return mix(toned, kept, FILMIC_HUE_KEEP);
}

fn filmic_curve(x: vec3f) -> vec3f {
    // x^a and x^(a d) from one logarithm.
    let z = log2(x) * output.curve.x;
    return min(exp2(z) / (exp2(z * output.curve.y) * output.curve.z + output.curve.w), vec3f(1.0));
}

// Exposed levels over which hot colours bleach toward white (`filmic`), and the share
// of the hue-keeping mapping against the per-channel one.
const FILMIC_HOT: vec2f = vec2f(1.5, 7.0);
const FILMIC_HUE_KEEP: f32 = 0.4;

// Hue-keeping tone mapping (`aces_filmic`): the saturation (chroma over the brightest
// channel) where it begins and is complete, the exposed level over which very hot
// colours return to the per-channel curve, and its share at full.
const HUE_KEEP_FROM: f32 = 0.45;
const HUE_KEEP_FULL: f32 = 0.85;
const HUE_KEEP_HOT: vec2f = vec2f(2.4, 4.5);
const HUE_KEEP: f32 = 0.7;

// ACES's output matrix pushes saturated colors out of gamut: a red hull lands with
// green and blue below 0 and a hot one with red past 1. Clamping each channel turns
// such a hull into one flat primary. Instead negative channels are partly lifted by
// desaturating toward the color's own luminance (hue kept), and the brightest
// channel rolls over a soft shoulder that scales the whole color, so lit and shaded
// faces stay apart.
fn fit_gamut(color: vec3f) -> vec3f {
    let shoulder = output.tone.w;
    if shoulder <= 0.0 {
        return clamp(color, vec3f(0.0), vec3f(1.0));
    }
    let level = max(luminance(color), 0.0);
    let low = min(min(color.r, color.g), color.b);
    var c = color;
    if low < 0.0 {
        // Part way only: all the way would wash red hulls toward coral.
        let fitted = vec3f(level) + (c - vec3f(level)) * (level / max(level - low, 1e-5));
        c = mix(max(c, vec3f(0.0)), fitted, output.tone.z);
    }
    let peak = max(max(c.r, c.g), c.b);
    if peak > shoulder {
        // Slope 1 at the shoulder, easing toward 1 without reaching it.
        let room = 1.0 - shoulder;
        let over = peak - shoulder;
        c *= (shoulder + over * room / (over + room)) / peak;
    }
    return clamp(c, vec3f(0.0), vec3f(1.0));
}

fn srgb_encode(color: vec3f) -> vec3f {
    let high = pow(color, vec3f(0.41666)) * 1.055 - 0.055;
    let low = color * 12.92;
    return select(high, low, color <= vec3f(0.0031308));
}

// Hue of a color, 0..1 around the wheel (0 red, 1/3 green, 2/3 blue).
fn hue_of(c: vec3f) -> f32 {
    let high = max(max(c.r, c.g), c.b);
    let chroma = high - min(min(c.r, c.g), c.b);
    if chroma <= 1e-5 {
        return 0.0;
    }
    var h: f32;
    if high == c.r {
        h = (c.g - c.b) / chroma;
    } else if high == c.g {
        h = (c.b - c.r) / chroma + 2.0;
    } else {
        h = (c.r - c.g) / chroma + 4.0;
    }
    return fract(h / 6.0 + 1.0);
}

// 1 within `width` / 2 of `center` around the hue wheel, fading to 0 at `width`.
fn hue_band(hue: f32, center: f32, width: f32) -> f32 {
    let d = abs(fract(hue - center + 0.5) - 0.5);
    return 1.0 - smoothstep(width * 0.5, width, d);
}

// Hues the grade singles out: lime grass, orange earth and the pink a red hull
// picks up from the blue sky fill.
const LIME_HUE: f32 = 0.215;
const EARTH_HUE: f32 = 0.088;
const PINK_HUE: f32 = 0.955;
// Meadow green, and the share of its saturation the foliage grade takes at full.
const MEADOW_HUE: f32 = 0.27;
const MEADOW_CALM: f32 = 0.2;
// Occlusion: the linear scene levels of shade and of sunlit ground, the share of its
// strength sunlit pixels keep, and its per-channel power (nearly neutral: a warm
// crease under a cool shadow fill mixed to teal on sand).
const OCCLUSION_SHADE_LEVEL: f32 = 0.08;
const OCCLUSION_LIT_LEVEL: f32 = 0.45;
const OCCLUSION_IN_SUN: f32 = 0.75;
const OCCLUSION_HUE: vec3f = vec3f(0.95, 1.0, 1.05);
// The least light occlusion leaves anywhere.
const OCCLUSION_FLOOR: f32 = 0.45;
// The share of its own saturation the bloom keeps.
const BLOOM_CHROMA: f32 = 0.6;
// Display mid grey, the pivot of the contrast curve.
const MID_GREY: f32 = 0.18;
// The display levels where the shadow toning fades in above black and where it fades
// out toward the midtones (x..y in, z..w out).
const SHADE_BAND: vec4f = vec4f(0.006, 0.03, 0.12, 0.3);

// The grade on the tone-mapped display color: gain scales the whites, the black point
// holds the shadows down, lift and gamma bend them, split toning cools shadows and
// warms highlights, a few hues are tamed, then saturation (and vibrance for muted
// colors) and a contrast curve around mid grey that never clips the blacks.
fn grade(color: vec3f) -> vec3f {
    // Uniform branches skip the steps a neutral setting leaves as they are (the
    // tone-mapped colour is never negative).
    var c = color;
    if any(output.gain.rgb != vec3f(1.0)) {
        c *= output.gain.rgb;
    }
    let black = output.tone.y;
    if black != 0.0 {
        c = max(c - black, vec3f(0.0)) / (1.0 - black);
    }
    let lift = output.lift.rgb;
    if any(lift != vec3f(0.0)) {
        c = c + lift * (vec3f(1.0) - c);
    }
    if any(output.gamma.rgb != vec3f(1.0)) {
        c = pow(max(c, vec3f(0.0)), 1.0 / output.gamma.rgb);
    }
    let level = luminance(c);
    let high = max(max(c.r, c.g), c.b);
    let chroma = high - min(min(c.r, c.g), c.b);
    let colourful = chroma / max(high, 1e-4);
    // The hue grades below read the hue before the toning.
    let untoned = c;
    // Toning cools the shade band (cast shadows and faces the sun misses), not the
    // blacks below it, and leaves saturated colours alone: tyres and gun steel stay
    // near black, a navy hull stays navy and a red one red, while the grey and earth
    // shade of the ground takes the sky's blue. Highlights warm only a touch, so
    // sunlight reads warm without a veil over the frame.
    if output.shadows.w > 0.0 {
        let shade = smoothstep(SHADE_BAND.x, SHADE_BAND.y, level) * (1.0 - smoothstep(SHADE_BAND.z, SHADE_BAND.w, level))
            * (1.0 - smoothstep(0.25, 0.6, colourful)) * output.shadows.w;
        c *= mix(vec3f(1.0), output.shadows.rgb, shade);
    }
    let light = smoothstep(0.35, 0.95, level) * output.highlights.w;
    c *= mix(vec3f(1.0), output.highlights.rgb, light);
    // The hue grades leave near-grey colours alone (every one of them is zero up to
    // `colourful` 0.05), so those skip the hue.
    var earth = 0.0;
    var meadow = 0.0;
    if colourful > 0.05 {
        let hue = hue_of(untoned);
        let vivid = smoothstep(0.05, 0.25, colourful);
        // Lime turns toward a deeper, cooler green (less red, a touch of blue, a little
        // darker), its yellow fringe with it so sunlit blades do not stand out yellow;
        // orange earth toward tan; red stays red. The band stops short of the yellow of
        // team rings and hazard paint.
        let lime = hue_band(hue, LIME_HUE, 0.08) * output.hues.x * vivid;
        c.r *= 1.0 - 0.32 * lime;
        c.g *= 1.0 - 0.02 * lime;
        c.b = mix(c.b, c.g * 0.3, 0.12 * lime);
        earth = hue_band(hue, EARTH_HUE, 0.06) * output.hues.y * smoothstep(0.2, 0.5, colourful);
        // Broad foliage greens give up a little chroma, so meadows read lush, not neon.
        meadow = hue_band(hue, MEADOW_HUE, 0.12) * output.hues.x * vivid;
        let pink = hue_band(hue, PINK_HUE, 0.06) * output.hues.z * vivid;
        c.b = mix(c.b, min(c.b, c.g * 1.05), pink);
    }
    let grey = luminance(c);
    // Vibrance richens muted sunlit colours (sand, concrete, earth), not the shade:
    // there it would only pump the sky fill's blue into a violet decal.
    let vibrance = 1.0 + output.tone.x * (1.0 - smoothstep(0.0, 0.6, chroma)) * smoothstep(SHADE_BAND.z, SHADE_BAND.w, level);
    let saturation = output.lift.w * vibrance * (1.0 - 0.32 * earth) * (1.0 - MEADOW_CALM * meadow);
    c = max(mix(vec3f(grey), c, saturation), vec3f(0.0));
    c = contrast_curve(c, output.gamma.w);
    return highlight_rolloff(c);
}

// Contrast around mid grey: a power curve below it, which never clips the blacks,
// and above it a quadratic of the same slope at mid grey that lands exactly on
// white, so the brightest sunlit faces keep their gradations instead of the power
// curve pushing them past 1 into one flat colour.
fn contrast_curve(c: vec3f, contrast: f32) -> vec3f {
    // A firm toe: below mid grey the power pulls shade down toward black.
    let low = MID_GREY * pow(max(c, vec3f(0.0)) / MID_GREY, vec3f(contrast));
    let x = clamp((c - MID_GREY) / (1.0 - MID_GREY), vec3f(0.0), vec3f(1.0));
    let high = MID_GREY + (1.0 - MID_GREY) * x * (contrast - (contrast - 1.0) * x);
    return select(high, low, c <= vec3f(MID_GREY));
}

// Where a graded colour's brightest channel starts to roll off, and the headroom its
// shoulder eases into: a channel pushed up to about 1.1 still lands below white.
const GRADE_SHOULDER: f32 = 0.9;
const GRADE_HEADROOM: f32 = 0.25;

// Gain and warm highlight toning can push one channel of a saturated colour past
// white: a sunlit red hull would clip its red flat while green and blue climb on.
// The whole colour scales down over a soft shoulder instead, so its hue and the
// form shading between lit faces survive.
fn highlight_rolloff(c: vec3f) -> vec3f {
    let peak = max(max(c.r, c.g), c.b);
    if peak <= GRADE_SHOULDER {
        return max(c, vec3f(0.0));
    }
    let over = peak - GRADE_SHOULDER;
    let eased = GRADE_SHOULDER + over * GRADE_HEADROOM / (over + GRADE_HEADROOM);
    return clamp(c * (eased / peak), vec3f(0.0), vec3f(1.0));
}

// Edge sharpening: push a colour away from its four neighbours' mean, held within
// their range so edges steepen without a halo or ringing.
fn sharpened(color: vec3f, n: vec3f, s: vec3f, w: vec3f, e: vec3f, amount: f32) -> vec3f {
    let low = min(color, min(min(n, s), min(w, e)));
    let high = max(color, max(max(n, s), max(w, e)));
    return clamp(color + (color - (n + s + w + e) * 0.25) * amount, low, high);
}

@fragment
fn fs_output(@builtin(position) position: vec4f) -> @location(0) vec4f {
    let row = output.settings.y + output.settings.z * position.y;
    // The fragment's pixel in the scene's texels (top-down rows) and its texture
    // coordinates: the canvas's own pixel at full scale, a scaled one where the scene
    // rendered smaller and is upsampled here.
    var pixel = vec2f(position.x, row);
    let size = vec2f(textureDimensions(source));
    var uv = pixel / size;
    let sharpen = output.detail.x;
    var color: vec3f;
    var alpha: f32;
    if output.detail.w > 0.0 {
        uv = pixel * output.detail.yz;
        pixel = uv * size;
        // Bilinear, then sharpened against the filter's softness from bilinear taps a
        // scene texel away: five taps whatever the scale.
        let center = textureSampleLevel(source, linear_clamp, uv, 0.0);
        color = center.rgb;
        alpha = center.a;
        if sharpen > 0.0 {
            let t = 1.0 / size;
            color = sharpened(
                color,
                tap(uv - vec2f(0.0, t.y)),
                tap(uv + vec2f(0.0, t.y)),
                tap(uv - vec2f(t.x, 0.0)),
                tap(uv + vec2f(t.x, 0.0)),
                sharpen,
            );
        }
    } else {
        let texel = vec2i(i32(position.x), i32(row));
        let center = textureLoad(source, texel, 0);
        color = center.rgb;
        alpha = center.a;
        if sharpen > 0.0 {
            // Against the multisample resolve's softness.
            let last = vec2i(size) - 1;
            color = sharpened(
                color,
                textureLoad(source, clamp(texel - vec2i(0, 1), vec2i(0), last), 0).rgb,
                textureLoad(source, clamp(texel + vec2i(0, 1), vec2i(0), last), 0).rgb,
                textureLoad(source, clamp(texel - vec2i(1, 0), vec2i(0), last), 0).rgb,
                textureLoad(source, clamp(texel + vec2i(1, 0), vec2i(0), last), 0).rgb,
                sharpen,
            );
        }
    }
    let strength = output.effects.z;
    if strength > 0.0 {
        // The quarter-size occlusion's tent at the corner of its texels nearest this
        // pixel (`fs_occlusion_blur`).
        let ao = textureLoad(occlusion, vec2i(floor(pixel * 0.25 + 0.5)), 0).r;
        // Occlusion takes away sky fill, which sunlit surfaces barely depend on: it
        // darkens shade fully and bright sunlit ground only lightly, so a tank sits in
        // a soft contact shade rather than on a dark decal. What it removes is the
        // cool sky's share a little more, so creases deepen without turning blue.
        let lit = smoothstep(OCCLUSION_SHADE_LEVEL, OCCLUSION_LIT_LEVEL, luminance(color));
        // It never takes more than part of the light: contact shade stays a soft,
        // coloured shade under a hull, never a black pool.
        let open = max(mix(1.0, ao, strength * mix(1.0, OCCLUSION_IN_SUN, lit)), OCCLUSION_FLOOR);
        color *= pow(vec3f(open), OCCLUSION_HUE);
    }
    // After the occlusion, which shades the surface rather than the air before it.
    color = scene_haze(color, alpha, uv);
    let bloom_strength = output.settings.w;
    if bloom_strength > 0.0 {
        // Small glows add in full; a fireball filling the screen saturates instead of
        // washing the frame out.
        // Its colour is partly spent: a red fireball's wide glow over blue sky or
        // grey concrete would otherwise mix to magenta.
        var glow = textureSampleLevel(bloom, linear_clamp, uv, 0.0).rgb;
        glow = mix(vec3f(luminance(glow)), glow, BLOOM_CHROMA);
        color += glow * (bloom_strength / (1.0 + luminance(glow) * output.hues.w));
    }
    var toned: vec3f;
    if output.curve.x > 0.0 {
        toned = filmic(color, output.settings.x);
    } else {
        toned = aces_filmic(color, output.settings.x);
    }
    var display = grade(toned);
    let vignette = output.gain.w;
    if vignette > 0.0 {
        let centered = (uv - 0.5) * vec2f(size.x / size.y, 1.0);
        let reach = length(centered);
        display *= 1.0 - vignette * smoothstep(0.55, 1.25, reach) * smoothstep(0.55, 1.25, reach);
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

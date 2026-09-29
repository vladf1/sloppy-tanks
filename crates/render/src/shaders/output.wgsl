// Output pass: the resolved linear HDR frame through Three r185's ACES filmic
// tone mapping and sRGB transfer, written to the (non-sRGB) canvas texture.

struct Output {
    // x exposure
    settings: vec4f,
}

@group(0) @binding(0) var hdr: texture_2d<f32>;
@group(0) @binding(1) var<uniform> output: Output;

@vertex
fn vs_fullscreen(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
    let uv = vec2f(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4f(uv * 2.0 - 1.0, 0.0, 1.0);
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

@fragment
fn fs_output(@builtin(position) position: vec4f) -> @location(0) vec4f {
    let color = textureLoad(hdr, vec2i(position.xy), 0).rgb;
    return vec4f(srgb_encode(aces_filmic(color, output.settings.x)), 1.0);
}

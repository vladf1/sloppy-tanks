// Screen-space ambient occlusion at quarter size. The main view's depth comes from a
// header (`shader::occlusion_source`): WebGPU reads a half-size depth pass of the main
// view's opaque draws, WebGL2 a full-size single-sample copy the resolve blits. The
// header also sets `OCCLUSION_SCALE`, depth texels per occlusion texel, and
// `SAMPLES`, the probes per pixel: fewer on WebGL's weaker devices. Each pixel rebuilds its world position and normal from
// the main view's depth, then tests eight points of a hemisphere a metre or so around
// it against the depth buffer. The kernel's rotation follows a 4×4 Bayer pattern, so
// the output pass's 6×6 tent, which weighs every rotation alike, leaves no noise.
// Occlusion fades out with distance: it grounds tanks and props near the camera
// and never greys the horizon.

struct Occlusion {
    view_projection: mat4x4f,
    // Clip position to world.
    inverse_view_projection: mat4x4f,
    // x radius in metres, y depth bias in metres, z distance where it starts to
    // fade, w distance where it is gone.
    settings: vec4f,
    // x, y: view distance = x / (depth + y) for this projection; z contrast power.
    depth_terms: vec4f,
}

@group(0) @binding(1) var<uniform> occlusion: Occlusion;

@vertex
fn vs_occlusion(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
    let corner = vec2f(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4f(corner * 2.0 - 1.0, 0.0, 1.0);
}

const BAYER: array<f32, 16> = array<f32, 16>(
    0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0,
);

fn depth_at(pixel: vec2i, size: vec2i) -> f32 {
    return load_depth(clamp(pixel, vec2i(0), size - 1));
}

fn world_at(pixel: vec2i, z: f32, size: vec2f) -> vec3f {
    let ndc = vec2f((f32(pixel.x) + 0.5) / size.x * 2.0 - 1.0, 1.0 - (f32(pixel.y) + 0.5) / size.y * 2.0);
    let world = occlusion.inverse_view_projection * vec4f(ndc, z, 1.0);
    return world.xyz / world.w;
}

fn view_distance(z: f32) -> f32 {
    return occlusion.depth_terms.x / (z + occlusion.depth_terms.y);
}

@fragment
fn fs_occlusion(@builtin(position) position: vec4f) -> @location(0) vec4f {
    let size_i = vec2i(textureDimensions(depth));
    let size = vec2f(size_i);
    let cell = vec2i(position.xy);
    let pixel = cell * OCCLUSION_SCALE;
    let z = depth_at(pixel, size_i);
    if z >= 1.0 {
        return vec4f(1.0);
    }
    let distance = view_distance(z);
    let fade = 1.0 - smoothstep(occlusion.settings.z, occlusion.settings.w, distance);
    if fade <= 0.0 {
        return vec4f(1.0);
    }
    let center = world_at(pixel, z, size);
    // The normal from the nearer neighbour on each axis, so edges do not bend it.
    let right = world_at(pixel + vec2i(1, 0), depth_at(pixel + vec2i(1, 0), size_i), size);
    let left = world_at(pixel - vec2i(1, 0), depth_at(pixel - vec2i(1, 0), size_i), size);
    let down = world_at(pixel + vec2i(0, 1), depth_at(pixel + vec2i(0, 1), size_i), size);
    let up = world_at(pixel - vec2i(0, 1), depth_at(pixel - vec2i(0, 1), size_i), size);
    var dx = right - center;
    if length(center - left) < length(dx) {
        dx = center - left;
    }
    var dy = down - center;
    if length(center - up) < length(dy) {
        dy = center - up;
    }
    // Right and down on screen: their cross product faces the camera.
    let normal = normalize(cross(dy, dx));
    let tangent_seed = select(vec3f(1.0, 0.0, 0.0), vec3f(0.0, 0.0, 1.0), abs(normal.x) > 0.8);
    let tangent = normalize(cross(tangent_seed, normal));
    let bitangent = cross(normal, tangent);
    let rotation = BAYER[(cell.y & 3) * 4 + (cell.x & 3)] / 16.0 * 6.28318530718;
    let radius = occlusion.settings.x;
    let bias = occlusion.settings.y;
    var occluded = 0.0;
    for (var i = 0; i < SAMPLES; i++) {
        // A spiral over the hemisphere: rising elevation, growing reach.
        let t = (f32(i) + 0.5) / f32(SAMPLES);
        let angle = rotation + f32(i) * 2.39996323;
        // Mostly low over the surface, where walls and hulls beside it occlude.
        let elevation = 0.1 + 0.6 * fract(t * 3.7 + 0.13);
        let reach = radius * (0.25 + 0.75 * t);
        let spread = sqrt(1.0 - elevation * elevation);
        let offset = (tangent * cos(angle) * spread + bitangent * sin(angle) * spread + normal * elevation) * reach;
        let probe = center + offset;
        let clip = occlusion.view_projection * vec4f(probe, 1.0);
        // Only probes in front of the camera; no `continue`, so the loop unrolls.
        if clip.w > 0.0 {
            let ndc = clip.xy / clip.w;
            let at = vec2i(vec2f((ndc.x * 0.5 + 0.5) * size.x, (0.5 - ndc.y * 0.5) * size.y));
            let scene = view_distance(depth_at(at, size_i));
            let gap = clip.w - scene;
            // Behind the scene's surface: occluded, unless the surface stands so far
            // in front that it is a different object (the range check).
            if gap > bias {
                occluded += 1.0 - smoothstep(radius * 1.5, radius * 3.5, gap);
            }
        }
    }
    let open = 1.0 - occluded / f32(SAMPLES);
    let ao = pow(clamp(open, 0.0, 1.0), occlusion.depth_terms.z);
    return vec4f(mix(1.0, ao, fade));
}

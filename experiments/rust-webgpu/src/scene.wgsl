struct Frame { camera: mat4x4f, light: mat4x4f }
@group(0) @binding(0) var<uniform> frame: Frame;
@group(1) @binding(0) var shadow: texture_depth_2d;
@group(1) @binding(1) var shadow_sampler: sampler_comparison;

struct Instance {
    @location(0) a: vec4f, @location(1) b: vec4f,
    @location(2) c: vec4f, @location(3) d: vec4f,
    @location(4) color: vec4f,
}
struct Vertex {
    @builtin(position) clip: vec4f,
    @location(0) world: vec3f,
    @location(1) normal: vec3f,
    @location(2) color: vec4f,
    @location(3) local: vec3f,
    @location(4) light: vec4f,
}
const CORNERS = array<vec2f, 6>(vec2f(-1,-1), vec2f(1,-1), vec2f(1,1), vec2f(-1,-1), vec2f(1,1), vec2f(-1,1));
const NORMALS = array<vec3f, 6>(vec3f(1,0,0), vec3f(-1,0,0), vec3f(0,1,0), vec3f(0,-1,0), vec3f(0,0,1), vec3f(0,0,-1));
const TANGENTS = array<vec3f, 6>(vec3f(0,0,-1), vec3f(0,0,1), vec3f(1,0,0), vec3f(1,0,0), vec3f(1,0,0), vec3f(-1,0,0));
fn position(index: u32) -> vec3f {
    let face = index / 6u;
    let uv = CORNERS[index % 6u];
    return (NORMALS[face] + TANGENTS[face] * uv.x + cross(NORMALS[face], TANGENTS[face]) * uv.y) * 0.5;
}
@vertex fn vs_main(instance: Instance, @builtin(vertex_index) index: u32) -> Vertex {
    let model = mat4x4f(instance.a, instance.b, instance.c, instance.d);
    let local = position(index);
    let world = model * vec4f(local, 1);
    var out: Vertex;
    out.clip = frame.camera * world;
    out.world = world.xyz;
    out.normal = normalize((model * vec4f(NORMALS[index / 6u], 0)).xyz);
    out.color = instance.color;
    out.local = local;
    out.light = frame.light * world;
    return out;
}
@vertex fn vs_shadow(instance: Instance, @builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
    return frame.light * mat4x4f(instance.a, instance.b, instance.c, instance.d) * vec4f(position(index), 1);
}
@fragment fn fs_main(in: Vertex) -> @location(0) vec4f {
    let light = in.light.xyz / in.light.w;
    let shadow_uv = light.xy * vec2f(0.5, -0.5) + 0.5;
    let visibility = textureSampleCompare(shadow, shadow_sampler, shadow_uv, light.z - 0.002);
    let diffuse = max(dot(normalize(in.normal), normalize(vec3f(-8,16,10))), 0.0);
    let grid_width = max(fwidth(in.world.xz), vec2f(0.001));
    var color = in.color.rgb;
    if (in.color.a > 0.5 && in.normal.y > 0.9) {
        let line = abs(fract(in.world.xz + 0.5) - 0.5) / grid_width;
        let grid = 1.0 - clamp(min(line.x, line.y), 0.0, 1.0);
        color = mix(color, vec3f(0.36,0.47,0.49), grid * 0.5);
        if (max(abs(in.world.x), abs(in.world.z)) > 6.6) {
            let stripe = step(0.5, fract((in.world.x + in.world.z) * 1.4));
            color = mix(vec3f(0.15,0.20,0.20), vec3f(0.9,0.65,0.22), stripe);
        }
    } else {
        let edge = sort_edge(abs(in.local));
        color *= mix(0.72, 1.0, 1.0 - smoothstep(0.475, 0.495, edge));
    }
    color *= 0.40 + diffuse * (0.20 + visibility * 0.65);
    return vec4f(color, 1.0);
}
fn sort_edge(v: vec3f) -> f32 { return max(min(v.x,v.y), min(max(v.x,v.y),v.z)); }

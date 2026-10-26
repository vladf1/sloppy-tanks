// The varying layout shared by the surface and shadow stages. Unused varyings
// are left for the pipeline linker, so both passes can reuse one Metal library.
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

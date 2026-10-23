// The frame group of the surface, shadow, water and sky shaders: the view's uniform,
// the sun's shadow map, the DFG lookup table, the noise lattice and the atmosphere map
// (`instance_at`'s records follow).
@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var shadow_map: texture_depth_2d;
@group(0) @binding(2) var shadow_sampler: sampler_comparison;
@group(0) @binding(3) var dfg_lut: texture_2d<f32>;
@group(0) @binding(4) var lut_sampler: sampler;
@group(0) @binding(10) var noise_lattice: texture_2d<f32>;
@group(0) @binding(11) var atmosphere_map: texture_2d<f32>;

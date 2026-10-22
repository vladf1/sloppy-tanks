// The sky, drawn after the opaque scene of the main view and the water reflection
// as one full-screen triangle on the far plane, so it fills only pixels nothing
// covered: the gradient and sun halo of `sky_gradient`, a sun disc bright enough
// to bloom, and soft drifting clouds lit from the sun's side.

struct SkyOut {
    @builtin(position) clip: vec4f,
    @location(0) ndc: vec2f,
}

@vertex
fn vs_sky(@builtin(vertex_index) index: u32) -> SkyOut {
    let corner = vec2f(f32((index << 1u) & 2u), f32(index & 2u));
    var out: SkyOut;
    out.ndc = corner * 2.0 - 1.0;
    out.clip = vec4f(out.ndc, 1.0, 1.0);
    return out;
}

// Cloud layer features per unit of ray slope, how far the layer flattens toward
// the horizon, and its drift per second.
const CLOUD_SCALE: f32 = 1.35;
const CLOUD_HORIZON_LIFT: f32 = 0.1;
const CLOUD_DRIFT: vec2f = vec2f(0.006, 0.0025);
// Density above the coverage threshold over which a cloud goes from its soft rim
// to its full body: small keeps the edges crisp and stylized.
const CLOUD_EDGE: f32 = 0.09;
// The sun disc's brightness over the halo color.
const SUN_DISC: f32 = 24.0;

fn cloud_density(p: vec2f) -> f32 {
    var sum = 0.0;
    var amplitude = 0.5;
    var q = p;
    for (var i = 0; i < 5; i++) {
        sum += value_noise(q) * amplitude;
        // Rotate each octave so the lattice never lines up.
        q = mat2x2f(1.6, 1.2, -1.2, 1.6) * q + vec2f(3.1, 1.7);
        amplitude *= 0.5;
    }
    return sum / 0.96875;
}

fn sky_radiance(direction: vec3f) -> vec3f {
    var color = sky_gradient(direction);
    let sun = frame.sun_direction.xyz;
    let mu = dot(direction, sun);
    let disc_edge = frame.sky_horizon.w;
    color += frame.sky_sun.rgb * SUN_DISC * smoothstep(disc_edge, mix(disc_edge, 1.0, 0.35), mu);
    let coverage = frame.cloud_lit.w;
    if direction.y > 0.0 && coverage > 0.0 {
        let plane = direction.xz / (direction.y + CLOUD_HORIZON_LIFT) * CLOUD_SCALE
            + CLOUD_DRIFT * frame.camera_position.w;
        let density = cloud_density(plane);
        let threshold = mix(0.68, 0.36, coverage);
        let shape = smoothstep(threshold, threshold + CLOUD_EDGE, density);
        if shape > 0.0 {
            // Puffy stylized cumulus: the sun side of every billow (density falling
            // toward the sun) catches bright light, the thick cores and the side
            // away from it settle into the cool shade color.
            let toward_sun = normalize(sun.xz + vec2f(1e-4)) * 0.11;
            let behind = cloud_density(plane + toward_sun);
            let lit = smoothstep(-0.05, 0.08, density - behind);
            let core = smoothstep(threshold, threshold + 0.32, density);
            let light = clamp(lit * 0.75 + (1.0 - core) * 0.45, 0.0, 1.0);
            var cloud = mix(frame.cloud_shade.rgb, frame.cloud_lit.rgb, light);
            // Thin rims near the sun glow with it (silver lining).
            let rim = 1.0 - smoothstep(threshold, threshold + CLOUD_EDGE * 2.5, density);
            cloud += frame.sky_sun.rgb * (pow(max(mu, 0.0), 10.0) * 2.2 + 0.25) * rim;
            // Distant clouds melt into the horizon haze.
            let toward_horizon = pow(1.0 - direction.y, 7.0);
            cloud = mix(cloud, frame.sky_horizon.rgb, toward_horizon * 0.75);
            let fade = smoothstep(0.0, 0.16, direction.y);
            color = mix(color, cloud, shape * fade * 0.96);
        }
    }
    return color;
}

@fragment
fn fs_sky(input: SkyOut) -> @location(0) vec4f {
    let far = frame.sky_rays * vec4f(input.ndc, 1.0, 1.0);
    let near = frame.sky_rays * vec4f(input.ndc, 0.0, 1.0);
    let direction = normalize(far.xyz / far.w - near.xyz / near.w);
    return vec4f(sky_radiance(direction), 1.0);
}

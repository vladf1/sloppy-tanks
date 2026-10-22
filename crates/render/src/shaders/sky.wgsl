// The sky, drawn after the opaque scene of the main view and the water reflection
// as one full-screen triangle on the far plane, so it fills only pixels nothing
// covered: the gradient and sun halo of `sky_gradient`, a sun disc bright enough
// to bloom, high cirrus streaks, puffy drifting cumulus lit from the sun's side
// (softened in the water's mirror) and the distant ranges of `horizon_ridges`.

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
const CLOUD_SCALE: f32 = 0.95;
const CLOUD_HORIZON_LIFT: f32 = 0.1;
const CLOUD_DRIFT: vec2f = vec2f(0.006, 0.0025);
// Density above the coverage threshold over which a cloud goes from its soft rim
// to its full body: small keeps the edges crisp and stylized.
const CLOUD_EDGE: f32 = 0.09;
// Sunlit cloud tops over the sky's brightness, so they read as the whitest thing
// in the sky.
const CLOUD_BRIGHTNESS: f32 = 1.45;
// The sun disc's brightness over the halo color.
const SUN_DISC: f32 = 24.0;

fn cloud_density(p: vec2f) -> f32 {
    // A low-frequency warp turns the noise's blobs into rounded, billowing heaps.
    let warp = vec2f(value_noise(p * 0.5 + vec2f(7.1, 2.3)), value_noise(p * 0.5 + vec2f(-3.7, 9.4)));
    var sum = 0.0;
    var amplitude = 0.5;
    var q = p + (warp - 0.5) * 0.9;
    // WebGL's weaker devices skip the finest octave, a detail a few pixels across.
    let octaves = select(5, 4, LITE);
    for (var i = 0; i < octaves; i++) {
        sum += value_noise(q) * amplitude;
        // Rotate each octave so the lattice never lines up.
        q = mat2x2f(1.6, 1.2, -1.2, 1.6) * q + vec2f(3.1, 1.7);
        amplitude *= 0.5;
    }
    return sum / (1.0 - 2.0 * amplitude);
}

// High cirrus: faint streaks stretched along the wind that fill the open sky between
// the cumulus.
const CIRRUS_STRETCH: vec2f = vec2f(0.45, 2.6);
const CIRRUS_OPACITY: f32 = 0.32;
fn sky_radiance(direction: vec3f, aa: f32) -> vec3f {
    var color = sky_gradient(direction);
    let sun = frame.sun_direction.xyz;
    let mu = dot(direction, sun);
    let disc_edge = frame.sky_horizon.w;
    color += frame.sky_sun.rgb * SUN_DISC * smoothstep(disc_edge, mix(disc_edge, 1.0, 0.35), mu);
    let coverage = frame.cloud_lit.w;
    let strength = frame.haze_shape.z;
    if direction.y > 0.0 && coverage > 0.0 {
        let plane = direction.xz / (direction.y + CLOUD_HORIZON_LIFT) * CLOUD_SCALE
            + CLOUD_DRIFT * frame.camera_position.w;
        // Cirrus first, high above the cumulus.
        let streak_axis = normalize(CLOUD_DRIFT);
        let along = vec2f(dot(plane, streak_axis), dot(plane, vec2f(-streak_axis.y, streak_axis.x)));
        let cirrus_noise = value_noise(along * CIRRUS_STRETCH + vec2f(11.0, 3.0)) * 0.65
            + value_noise(along * CIRRUS_STRETCH * 2.3 + vec2f(-4.0, 7.0)) * 0.35;
        let cirrus = smoothstep(0.55, 0.85, cirrus_noise) * CIRRUS_OPACITY * smoothstep(0.05, 0.35, direction.y);
        color = mix(color, frame.cloud_lit.rgb * 1.05, cirrus * strength * coverage);
        let density = cloud_density(plane);
        let threshold = mix(0.68, 0.36, coverage);
        let shape = smoothstep(threshold, threshold + CLOUD_EDGE, density);
        if shape > 0.0 {
            // Puffy stylized cumulus: the sun side of every billow (density falling
            // toward the sun) catches bright light, the thick cores and the side
            // away from it settle into the cool shade color, and the flat bases
            // (seen low over the horizon) darken.
            let toward_sun = normalize(sun.xz + vec2f(1e-4)) * 0.11;
            let behind = cloud_density(plane + toward_sun);
            let lit = smoothstep(-0.06, 0.1, density - behind);
            let core = smoothstep(threshold, threshold + 0.3, density);
            let light = clamp(lit * 0.8 + (1.0 - core) * 0.35, 0.0, 1.0);
            var cloud = mix(frame.cloud_shade.rgb, frame.cloud_lit.rgb * CLOUD_BRIGHTNESS, light);
            // Thin rims near the sun glow with it (silver lining).
            let rim = 1.0 - smoothstep(threshold, threshold + CLOUD_EDGE * 2.5, density);
            cloud += frame.sky_sun.rgb * (pow(max(mu, 0.0), 10.0) * 2.2 + 0.2) * rim;
            // Distant clouds melt into the horizon haze.
            let toward_horizon = pow(1.0 - direction.y, 7.0);
            cloud = mix(cloud, frame.sky_horizon.rgb, toward_horizon * 0.6);
            let fade = smoothstep(0.0, 0.12, direction.y);
            color = mix(color, cloud, shape * fade * 0.97 * strength);
        }
    }
    return horizon_ridges(color, direction, aa);
}

@fragment
fn fs_sky(input: SkyOut) -> @location(0) vec4f {
    let far = frame.sky_rays * vec4f(input.ndc, 1.0, 1.0);
    let near = frame.sky_rays * vec4f(input.ndc, 0.0, 1.0);
    let direction = normalize(far.xyz / far.w - near.xyz / near.w);
    // A pixel's angular size, for antialiased ridge lines.
    let aa = max(fwidth(direction.y), 1e-5);
    return vec4f(sky_radiance(direction, aa), 1.0);
}

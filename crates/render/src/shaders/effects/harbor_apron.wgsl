// Harbor Havoc's quay concrete, asphalt and the paint on them
// (`effects_scenery::HARBOR_APRON`). Everything is placed in world space, so slabs,
// stains and wear never repeat with the ten-metre concrete tile. Extra texture 0 is
// the dock concrete tile, the grain of every mode. params[0].x is the `ApronMode`:
// 0 concrete, 1 asphalt lane along z, 2 asphalt lane along x, 3 alpha-tested
// marking, 4 transparent label (its map is the lettering), 5 quay wall faces
// (the horizontal coordinate follows the face, y climbs out of the water).

const APRON_LUMA = vec3f(0.2126, 0.7152, 0.0722);
// Slab edge in metres; the joints are sawn on this grid.
const APRON_SLAB = 6.0;
// Half width of a sawn joint and of the grime that collects beside it, metres.
const APRON_JOINT = 0.014;
const APRON_JOINT_GRIME = 0.16;
// Mean linear luminance of the dock tile, so its grain multiplies around 1.
const APRON_TILE_MEAN = 0.2;
// Asphalt repair patches are laid on this grid, metres.
const APRON_PATCH_CELL = 9.0;
// Harbor water level (`harbor_water::HARBOR_WATER_HEIGHT`) and how far the tide
// stains the quay wall above it.
const APRON_SEA_LEVEL = -2.2;
const APRON_TIDE = 0.75;
// Spacing of the quay wall's vertical expansion joints, metres.
const APRON_WALL_JOINT = 12.0;

fn apron_hash(cell: vec2f) -> vec4f {
    var p = fract(cell.xyxy * vec4f(0.1031, 0.1030, 0.0973, 0.1099));
    p = p + dot(p, p.wzxy + 33.33);
    return fract((p.xxyz + p.yzzw) * p.zywx);
}

fn apron_noise(p: vec2f) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = apron_hash(i).x;
    let b = apron_hash(i + vec2f(1.0, 0.0)).x;
    let c = apron_hash(i + vec2f(0.0, 1.0)).x;
    let d = apron_hash(i + vec2f(1.0, 1.0)).x;
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let mode = material.params[0].x;
    let wall = mode > 4.5;
    // A wall face runs along x or z; its plane coordinates are (along, height).
    let faces_x = abs((*s).normal.x) > 0.5;
    let along = select(f.world.x, f.world.z, faces_x);
    let p = select(f.world.xz, vec2f(along, f.world.y), wall);
    // Every sample and derivative first, in uniform control flow.
    let turned = vec2f(p.x * 0.8 - p.y * 0.6, p.x * 0.6 + p.y * 0.8);
    let grain_a = textureSample(extra_texture0, extra_sampler0, p / 4.7).rgb;
    let grain_b = textureSample(extra_texture0, extra_sampler0, turned / 9.3 + 0.31).rgb;
    let fine = dot(textureSample(extra_texture0, extra_sampler0, turned / 1.3 + 0.17).rgb, APRON_LUMA) / APRON_TILE_MEAN;
    let streak_z = dot(textureSample(extra_texture0, extra_sampler0, vec2f(p.x / 1.9, p.y / 60.0)).rgb, APRON_LUMA);
    let streak_x = dot(textureSample(extra_texture0, extra_sampler0, vec2f(p.y / 1.9, p.x / 60.0) + 0.5).rgb, APRON_LUMA);
    let slab_uv = p / APRON_SLAB;
    let cell = floor(slab_uv);
    let to_edge = (0.5 - abs(fract(slab_uv) - 0.5)) * APRON_SLAB;
    let edge = min(to_edge.x, to_edge.y);
    let aa = max(fwidth(edge), 1e-4);
    let patch_uv = p / APRON_PATCH_CELL;
    let patch_cell = floor(patch_uv);
    let patch_local = fract(patch_uv) * APRON_PATCH_CELL;
    let patch_aa = max(fwidth(patch_local.x) + fwidth(patch_local.y), 1e-4);
    let crack_noise = apron_noise(p / 3.1 + 13.0) + apron_noise(p * 0.9) * 0.18;
    let crack_width = max(fwidth(crack_noise), 1e-4);
    let wall_edge = (0.5 - abs(fract(along / APRON_WALL_JOINT) - 0.5)) * APRON_WALL_JOINT;
    let wall_aa = max(fwidth(wall_edge), 1e-4);

    let grain = (grain_a * 0.6 + grain_b * 0.4) / APRON_TILE_MEAN;
    let grain_luma = dot(grain, APRON_LUMA);
    // Broad mottling and grime, two octaves of value noise.
    let mottle = apron_noise(p / 9.0) * 0.6 + apron_noise(p / 2.7 + 7.0) * 0.4;
    let grime = mix(0.86, 1.05, mottle);

    // Damp hollows that hold water after a shower; the deepest are puddles.
    let hollow = apron_noise(p / 7.0 + 3.3) * 0.7 + apron_noise(p / 2.1) * 0.3;
    let damp = smoothstep(0.76, 0.8, hollow);
    let puddle = smoothstep(0.81, 0.825, hollow);

    // Sawn joint: a dark groove (coverage-weighted so it fades out, not shimmers,
    // when thinner than a pixel), a dirt fringe, and a pale chipped arris.
    let groove = clamp((APRON_JOINT - edge) / aa + 0.5, 0.0, 1.0) * min(1.0, 2.0 * APRON_JOINT / aa);
    let fringe = (1.0 - smoothstep(0.0, APRON_JOINT_GRIME, edge)) * (0.5 + mottle);
    let chipped = (1.0 - smoothstep(APRON_JOINT, APRON_JOINT * 3.5, edge)) * step(0.6, grain_luma * apron_noise(p * 6.0) * 1.4);

    var color = (*s).color;
    var roughness = (*s).roughness;
    if wall {
        // Quay wall: weathered concrete panels, a dark wet tide band with weed at
        // the waterline, and rust and lime streaks running down from the top.
        let height = f.world.y - APRON_SEA_LEVEL;
        let ragged = (apron_noise(vec2f(along * 0.7, 0.0)) - 0.5) * 0.35 + (grain_luma - 1.0) * 0.08;
        let wet = 1.0 - smoothstep(APRON_TIDE * 0.6, APRON_TIDE, height + ragged);
        let weed = 1.0 - smoothstep(0.05, 0.38, height + ragged * 0.6);
        let runs = apron_noise(vec2f(along * 1.3, f.world.y * 0.08));
        let rust = smoothstep(0.62, 0.8, runs) * smoothstep(-2.5, -0.4, f.world.y) * 0.6;
        let lime = smoothstep(0.7, 0.86, apron_noise(vec2f(along * 0.9 + 19.0, f.world.y * 0.1))) * 0.5;
        let joint = clamp((0.03 - wall_edge) / wall_aa + 0.5, 0.0, 1.0);
        color = material.color.rgb * f.tint.rgb * grain * grime;
        color = mix(color, color * vec3f(1.1, 1.08, 1.02), lime * (1.0 - wet));
        color = mix(color, color * vec3f(0.62, 0.42, 0.28), rust * (1.0 - wet));
        color = mix(color, color * vec3f(0.36, 0.38, 0.36), wet);
        color = mix(color, vec3f(0.035, 0.06, 0.035) * grain_luma, weed * 0.85);
        color = color * (1.0 - joint * 0.55);
        roughness = mix(roughness, 0.35, wet);
    } else if mode < 0.5 {
        // Concrete slabs: per-pour tone, the odd patched slab, an oil stain on some.
        let slab = apron_hash(cell + vec2f(17.0, 5.0));
        let patched = step(slab.y, 0.05);
        let tone = mix(0.93, 1.05, slab.x) * mix(1.0, 1.1, patched);
        let stain_at = (cell + 0.3 + 0.4 * slab.zw) * APRON_SLAB;
        let stain_radius = 0.3 + 0.7 * fract(slab.x * 7.3);
        let wobble = (apron_noise(p * 1.7) - 0.5) * 0.55 + (grain_luma - 1.0) * 0.25;
        let stain_shape = length(p - stain_at) / stain_radius + wobble;
        let stain = (1.0 - smoothstep(0.55, 1.0, stain_shape)) * step(slab.z, 0.09) * (1.0 - patched);
        let stain_core = 1.0 - smoothstep(0.2, 0.55, stain_shape);
        // Rust bloom where steel stood on the slab.
        let rust = smoothstep(0.83, 0.93, apron_noise(p * 0.9 + 41.0) * apron_noise(p / 5.0) * 1.6) * 0.4;
        color = material.color.rgb * f.tint.rgb * grain * tone * grime;
        color = mix(color, color * vec3f(0.3, 0.29, 0.28), stain * mix(0.45, 0.75, slab.w));
        color = mix(color, color * vec3f(0.95, 0.72, 0.52), rust);
        color = color * (1.0 - fringe * 0.14);
        color = color * mix(1.0, 1.1, chipped);
        color = color * (1.0 - groove * 0.68);
        color = color * mix(1.0, 0.84, damp) * mix(1.0, 0.8, puddle);
        roughness = mix(roughness, 0.55, stain_core * stain);
        roughness = mix(roughness, 0.42, damp);
        roughness = mix(roughness, 0.06, puddle);
    } else if mode < 2.5 {
        // Asphalt lanes: dark binder with pale aggregate, oxidised paler where the
        // traffic thins, rubber polish along the lane, sealed cracks and patches.
        let along_z = f32(abs(mode - 1.0) < 0.5);
        let streak = (streak_z * along_z + streak_x * (1.0 - along_z)) / APRON_TILE_MEAN;
        let across = select(p.y, p.x, along_z > 0.5);
        let aggregate = smoothstep(1.3, 2.0, fine) - smoothstep(0.6, 0.25, fine) * 0.6;
        let worn = apron_noise(p / 11.0 + 5.0);
        let tyre = smoothstep(0.85, 1.35, streak) * mix(0.5, 1.0, mottle);
        // Wheel paths the lane's traffic has polished.
        let paths = smoothstep(0.55, 1.0, sin(across * 1.15 + apron_noise(p / 4.0) * 1.2) * 0.5 + 0.5);
        let patch_hash = apron_hash(patch_cell + vec2f(3.0, 11.0));
        let patch_size = vec2f(2.0, 1.4) + patch_hash.zw * vec2f(3.5, 3.0);
        let patch_at = vec2f(0.6) + patch_hash.xy * (vec2f(APRON_PATCH_CELL) - patch_size - 1.2);
        let inside = min(
            min(patch_local.x - patch_at.x, patch_at.x + patch_size.x - patch_local.x),
            min(patch_local.y - patch_at.y, patch_at.y + patch_size.y - patch_local.y),
        );
        let has_patch = step(0.8, patch_hash.x * 0.6 + patch_hash.w * 0.6);
        let patched = clamp(inside / patch_aa + 0.5, 0.0, 1.0) * has_patch;
        let seal = (1.0 - smoothstep(0.0, 0.07, abs(inside))) * has_patch;
        let crack = (1.0 - smoothstep(0.0, crack_width * 1.4 + 0.004, abs(crack_noise - 0.5)))
            * smoothstep(0.68, 0.8, apron_noise(p / 6.0 + 2.0)) * (1.0 - patched);
        let drip_noise = apron_noise(p * 2.6 + 23.0) * apron_noise(p / 2.0 + 9.0);
        let drip = smoothstep(0.56, 0.64, drip_noise) * (0.3 + paths * 0.5);
        let base = material.color.rgb * f.tint.rgb * mix(0.92, 1.06, grain_luma * 0.5)
            * mix(1.0, 1.25, worn * (1.0 - paths * 0.6));
        color = base * (1.0 + aggregate * 0.4) * grime;
        color = color * mix(1.0, 0.86, patched);
        color = color * mix(1.0, 0.84, tyre * (0.4 + paths * 0.6));
        color = color * (1.0 - seal * 0.35) * (1.0 - crack * 0.38);
        color = mix(color, color * vec3f(0.45, 0.43, 0.41), drip);
        color = color * mix(1.0, 0.88, damp) * mix(1.0, 0.75, puddle);
        roughness = mix(roughness, 0.7, paths * 0.6 + patched * 0.3);
        roughness = mix(roughness, 0.45, drip);
        roughness = mix(roughness, 0.38, damp);
        roughness = mix(roughness, 0.05, puddle);
    } else {
        // Paint: worn through where tyres and boots scuff it.
        let scuff = apron_noise(p * 2.3) * 0.55 + (grain_luma - 1.0) * 0.6 + apron_noise(p / 3.0) * 0.45;
        let worn = smoothstep(0.16, 0.24, scuff);
        if mode < 3.5 {
            color = material.color.rgb * f.vertex_color.rgb * f.tint.rgb;
        }
        color = color * mix(0.86, 1.04, clamp(grain_luma - 0.4, 0.0, 1.0)) * mix(0.93, 1.02, mottle);
        color = color * mix(1.0, 0.8, damp);
        roughness = mix(roughness, 0.08, puddle);
        if mode < 3.5 {
            (*s).opacity = worn;
        } else {
            (*s).opacity = (*s).opacity * mix(0.75, 1.0, worn);
        }
    }
    (*s).color = color;
    (*s).roughness = roughness;
}

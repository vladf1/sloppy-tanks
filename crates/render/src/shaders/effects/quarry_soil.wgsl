// Baked soil with world-space grit (`quarry-terrain.ts` soilMaterial): the map is
// the soil bake at the mesh UVs; extra texture 0 is the grit. The bake's alpha
// carries the ground kind: 128 plain fine soil, above it how gravelly the ground
// is, below it how far damp mud has dried and cracked into plates.
//
// On top of the bake, in world space: loose stones on two lattices, scattered by
// the gravel amount; mud cracks; and relief from the bake itself, so ruts,
// windrows and ripples catch the low sun.

// Lattice spacing of the cobbles, the stones and the fine grit, and of the mud
// plates, metres.
const SOIL_COBBLE_CELL = 0.9;
const SOIL_STONE_CELL = 0.42;
const SOIL_GRIT_CELL = 0.15;
const SOIL_PLATE_CELL = 0.55;
// Size of the patches loose stone gathers in, metres.
const SOIL_SPILL_CELL = 5.5;
// Relief of the bake's own shading (ruts, windrows, ripples), as bump scale.
const SOIL_BAKE_RELIEF = 2.6;
// The floor's broad swells and hollows: wavelengths and heights, metres. Shading
// only: the floor stays the plane the tanks drive on, but the low sun picks out
// a worked, wind-shaped ground.
const SWELL_SPAN = 21.0;
const SWELL_HEIGHT = 0.9;
const HUMMOCK_SPAN = 7.5;
const HUMMOCK_HEIGHT = 0.22;
// Windblown sand ridges across the +x wind: span along and across it, height.
const DUNE_SPAN = vec2f(6.5, 17.0);
const DUNE_HEIGHT = 0.3;

// Height (metres) of the floor's shaded relief at x/z, with its slope.
fn soil_relief(p: vec2f) -> vec3f {
    let swell = quarry_value_grad(p / SWELL_SPAN + vec2f(3.1, 7.7));
    let hummock = quarry_value_grad(p / HUMMOCK_SPAN + vec2f(11.3, 2.9));
    let dune = quarry_value_grad(p / DUNE_SPAN + vec2f(5.3, 1.1));
    return vec3f(
        (swell.x - 0.5) * SWELL_HEIGHT + (hummock.x - 0.5) * HUMMOCK_HEIGHT + (dune.x - 0.5) * DUNE_HEIGHT,
        swell.y * SWELL_HEIGHT / SWELL_SPAN + hummock.y * HUMMOCK_HEIGHT / HUMMOCK_SPAN + dune.y * DUNE_HEIGHT / DUNE_SPAN.x,
        swell.z * SWELL_HEIGHT / SWELL_SPAN + hummock.z * HUMMOCK_HEIGHT / HUMMOCK_SPAN + dune.z * DUNE_HEIGHT / DUNE_SPAN.y,
    );
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let map_uv = f.uv * material.map_transform.xy + material.map_transform.zw;
    let baked = textureSample(map_texture, map_sampler, map_uv);
    let footprint = quarry_footprint(f.world);
    let stony = clamp((baked.a - 0.5) * 2.0, 0.0, 1.0);
    let dried = clamp((0.5 - baked.a) * 2.0, 0.0, 1.0);
    let p = f.world.xz;
    let near = dot(textureSample(extra_texture0, extra_sampler0, p / 3.3).rgb, LUMA);
    let turned = vec2f(p.x * 0.6 + p.y * 0.8, p.y * 0.6 - p.x * 0.8) / 11.7;
    let far = textureSample(extra_texture0, extra_sampler0, turned);
    let detail = near * (dot(far.rgb, LUMA) + 0.327) / (0.327 * 0.327 * 2.0);
    // Stones gather where the bake is gravelly and thin out on fine soil, and
    // lie in drifts and spills rather than an even sprinkle: a few metres of
    // ground hold most of them, the sand between almost none.
    // Level ground only: the relief fades out up banks, spoil and talus.
    let level = quarry_smoothstep(0.92, 0.99, (*s).normal.y);
    let relief = soil_relief(p);
    let hollow = quarry_smoothstep(0.05, -0.35, relief.x) * level;
    let crest = quarry_smoothstep(0.1, 0.4, relief.x) * level;
    // Stones wash into the hollows and the ruts; the crests are winnowed sand.
    let gather = quarry_value(p / SOIL_SPILL_CELL) * 0.65 + quarry_value(p / (SOIL_SPILL_CELL * 0.37) + 7.0) * 0.35;
    let spill = quarry_smoothstep(0.42, 0.7, gather + stony * 0.25 + hollow * 0.3 - crest * 0.25);
    let stones = quarry_pebbles(p, SOIL_STONE_CELL, (stony * stony * 0.3 + 0.04) * spill * 1.6, footprint, 3.0);
    let grit = quarry_pebbles(p, SOIL_GRIT_CELL, (0.03 + stony * 0.4) * mix(0.4, 1.4, spill), footprint, 11.0);
    let cobbles = quarry_pebbles(p, SOIL_COBBLE_CELL, (0.004 + stony * 0.1) * spill * 1.8, footprint, 23.0);
    // Mud plates only where the bake dried some; their crack depth joins the relief.
    var cracks = vec2f(0.0);
    if dried > 0.02 {
        // Toward the edge of a dried patch the plates grow and the cracks thin
        // out to hairlines, so the crazing feathers into the sand.
        let cell = SOIL_PLATE_CELL * mix(1.6, 1.0, quarry_smoothstep(0.1, 0.6, dried));
        cracks = quarry_cracks(p, cell, mix(0.35, 1.0, quarry_smoothstep(0.15, 0.7, dried)), footprint)
            * quarry_smoothstep(0.02, 0.55, dried);
    }
    let bake_height = dot(baked.rgb, LUMA);
    let height = near * mix(0.35, 1.6, stony) + bake_height * SOIL_BAKE_RELIEF
        - cracks.x * 0.04 + cracks.y * 0.03;
    var normal = derivative_bump(height, 1.0, f.world, (*s).normal, f.front_facing);
    normal = normalize(normal - vec3f(relief.y, 0.0, relief.z) * level);
    let lean = cobbles.lean + stones.lean + grit.lean * 0.7;
    normal = normalize(normal + vec3f(lean.x, 0.0, lean.y));
    var color = baked.rgb * mix(1.0, detail, mix(0.55, 1.5, stony));
    // Hollows hold damp, darker fines; crests are bleached, paler sand.
    color *= mix(1.0, 0.9, hollow) * mix(1.0, 1.05, crest);
    color = mix(color, color * vec3f(0.97, 0.95, 0.94), hollow * 0.5);
    color *= 1.0 - (cobbles.shadow * 0.5 + stones.shadow * 0.42 + grit.shadow * 0.25);
    color *= 1.0 - (cobbles.contact * 0.12 + stones.contact * 0.1 + grit.contact * 0.06);
    color = mix(color, color * cobbles.tint, cobbles.cover);
    color = mix(color, color * stones.tint, stones.cover);
    color = mix(color, color * grit.tint, grit.cover * 0.85);
    // Dried plates are paler and cooler; the cracks hold shade and moisture.
    color *= 1.0 + cracks.y * 0.12;
    color = mix(color, color * vec3f(0.55, 0.5, 0.46), cracks.x * 0.65);
    (*s).color = color * f.vertex_color.rgb * material.color.rgb * f.tint.rgb;
    (*s).opacity = material.color.a * f.tint.a;
    (*s).normal = normal;
    (*s).roughness = mix((*s).roughness, 0.75, max(max(stones.cover, grit.cover), cobbles.cover) * 0.5);
}

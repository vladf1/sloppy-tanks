// Baked soil with world-space grit (`quarry-terrain.ts` soilMaterial): the map is
// the soil bake at the mesh UVs; extra texture 0 is the grit. The bake's alpha
// carries the ground kind: 128 plain fine soil, above it how gravelly the ground
// is, below it how far damp mud has dried and cracked into plates. The floor's
// broad relief (basins, swells and sand drifts, `quarry_relief.rs`) is in the
// bake's color: it is static and broad, and worked out here it took a fifth of
// the floor's time.
//
// On top of the bake, in world space: loose stones on two lattices, scattered by
// the gravel amount; mud cracks; and relief from the bake itself, so ruts,
// windrows and ripples catch the low sun. The finest grit is the grit tile's own.

// Lattice spacing of the cobbles and the stones, and of the mud plates, metres.
const SOIL_COBBLE_CELL = 0.9;
const SOIL_STONE_CELL = 0.42;
const SOIL_PLATE_CELL = 0.55;
// How far loose stones' tints settle toward the soil's own colour (1 all the way).
const SOIL_STONE_DUST = 0.4;
// Size of the patches loose stone gathers in, metres.
const SOIL_SPILL_CELL = 5.5;
// Relief of the bake's own shading (ruts, windrows, ripples), as bump scale.
const SOIL_BAKE_RELIEF = 2.6;

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
    let gather = quarry_value(p / SOIL_SPILL_CELL) * 0.65 + quarry_value(p / (SOIL_SPILL_CELL * 0.37) + 7.0) * 0.35;
    let spill = quarry_smoothstep(0.54, 0.66, gather + stony * 0.25);
    // Graphics tiers without fine detail (`fine_detail`) keep only the cobbles;
    // with it this is 1 and the stones are what they always were.
    let tier_detail = select(0.0, 1.0, fine_detail());
    let stones = quarry_pebbles(p, SOIL_STONE_CELL, (stony * stony * 0.3 + 0.05) * spill * 1.8 * tier_detail, footprint, 3.0, 0.75);
    let cobbles = quarry_pebbles(p, SOIL_COBBLE_CELL, (0.004 + stony * 0.1) * spill * 1.8, footprint, 23.0, 0.85);
    // Mud plates only where the bake dried some; their crack depth joins the relief.
    var cracks = vec2f(0.0);
    if dried > 0.02 {
        // Toward the edge of a dried patch the plates grow and the cracks thin
        // out to hairlines, so the crazing feathers into the sand.
        let cell = SOIL_PLATE_CELL * mix(1.6, 1.0, quarry_smoothstep(0.1, 0.6, dried));
        cracks = quarry_cracks(p, cell, mix(0.35, 1.0, quarry_smoothstep(0.15, 0.7, dried)), footprint).xy
            * quarry_smoothstep(0.02, 0.55, dried);
    }
    let bake_height = dot(baked.rgb, LUMA);
    let height = near * mix(0.35, 1.6, stony) + bake_height * SOIL_BAKE_RELIEF
        - cracks.x * 0.04 + cracks.y * 0.03;
    var normal = derivative_bump(height, 1.0, f.world, (*s).normal, f.front_facing);
    let lean = (cobbles.lean + stones.lean) * 0.6;
    normal = normalize(normal + vec3f(lean.x, 0.0, lean.y));
    var color = baked.rgb * mix(1.0, detail, mix(0.55, 1.5, stony));
    color *= 1.0 - (cobbles.shadow * 0.36 + stones.shadow * 0.3);
    color *= 1.0 - (cobbles.contact * 0.12 + stones.contact * 0.1);
    // Stones are the floor's own rock under its dust: their tints pulled toward
    // the soil so they sit in its value range, a lit crown settling into a
    // shaded rim where they sink in, never dark chips or chocolate domes.
    color = mix(color, color * mix(cobbles.tint, vec3f(1.0), SOIL_STONE_DUST) * 0.96, cobbles.cover);
    color = mix(color, color * mix(stones.tint, vec3f(1.0), SOIL_STONE_DUST), stones.cover);
    color *= 1.0 - (cobbles.rim * 0.2 + stones.rim * 0.18);
    // Dried plates are paler and cooler; the cracks hold shade and moisture.
    color *= 1.0 + cracks.y * 0.12;
    color = mix(color, color * vec3f(0.55, 0.5, 0.46), cracks.x * 0.65);
    (*s).color = color * f.vertex_color.rgb * material.color.rgb * f.tint.rgb;
    (*s).opacity = material.color.a * f.tint.a;
    (*s).normal = normal;
    (*s).roughness = mix((*s).roughness, 0.75, max(stones.cover, cobbles.cover) * 0.5);
}

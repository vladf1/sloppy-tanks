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
    // Stones gather where the bake is gravelly and thin out on fine soil.
    let stones = quarry_pebbles(p, SOIL_STONE_CELL, stony * stony * 0.36, footprint, 3.0);
    let grit = quarry_pebbles(p, SOIL_GRIT_CELL, 0.06 + stony * 0.4, footprint, 11.0);
    let cobbles = quarry_pebbles(p, SOIL_COBBLE_CELL, 0.02 + stony * 0.11, footprint, 23.0);
    // Mud plates only where the bake dried some; their crack depth joins the relief.
    var cracks = vec2f(0.0);
    if dried > 0.02 {
        cracks = quarry_cracks(p, SOIL_PLATE_CELL, 1.0, footprint) * quarry_smoothstep(0.02, 0.4, dried);
    }
    let bake_height = dot(baked.rgb, LUMA);
    let height = near * mix(0.35, 1.6, stony) + bake_height * SOIL_BAKE_RELIEF
        - cracks.x * 0.04 + cracks.y * 0.03;
    var normal = derivative_bump(height, 1.0, f.world, (*s).normal, f.front_facing);
    let lean = cobbles.lean + stones.lean + grit.lean * 0.7;
    normal = normalize(normal + vec3f(lean.x, 0.0, lean.y));
    var color = baked.rgb * mix(1.0, detail, mix(0.55, 1.5, stony));
    color *= 1.0 - (cobbles.contact * 0.28 + stones.contact * 0.22 + grit.contact * 0.12);
    color = mix(color, color * cobbles.tint, cobbles.cover);
    color = mix(color, color * stones.tint, stones.cover);
    color = mix(color, color * grit.tint, grit.cover * 0.85);
    // Dried plates are paler and cooler; the cracks hold shade and moisture.
    color *= 1.0 + cracks.y * 0.12;
    color = mix(color, color * vec3f(0.5, 0.45, 0.41), cracks.x * 0.75);
    (*s).color = color * f.vertex_color.rgb * material.color.rgb * f.tint.rgb;
    (*s).opacity = material.color.a * f.tint.a;
    (*s).normal = normal;
    (*s).roughness = mix((*s).roughness, 0.75, max(max(stones.cover, grit.cover), cobbles.cover) * 0.5);
}

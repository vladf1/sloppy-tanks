// Baked soil with world-space grit (`quarry-terrain.ts` soilMaterial): the map is
// the soil bake at the mesh UVs; extra texture 0 is the grit. The bake's alpha
// carries the ground kind: 128 plain fine soil, above it how gravelly the ground
// is, below it how far damp mud has dried and cracked into plates. The floor's
// broad relief (basins, swells and sand drifts, `quarry_relief.rs`) is in the
// bake's color: it is static and broad, and worked out here it took a fifth of
// the floor's time.
//
// On top of the bake, in world space: the grit tile's detail, stronger where the
// ground is gravelly; mud cracks; and relief from the bake itself, so ruts,
// windrows and ripples catch the low sun. The floor takes most of the quarry's
// pixels, so it leaves loose stones to the bake's own speckle and the rubble
// meshes: a lattice of shaded stones here was its costliest layer.

// Lattice spacing of the mud plates, metres.
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
    var color = baked.rgb * mix(1.0, detail, mix(0.55, 1.5, stony));
    // Dried plates are paler and cooler; the cracks hold shade and moisture.
    color *= 1.0 + cracks.y * 0.12;
    color = mix(color, color * vec3f(0.55, 0.5, 0.46), cracks.x * 0.65);
    (*s).color = color * f.vertex_color.rgb * material.color.rgb * f.tint.rgb;
    (*s).opacity = material.color.a * f.tint.a;
    (*s).normal = derivative_bump(height, 1.0, f.world, (*s).normal, f.front_facing);
}

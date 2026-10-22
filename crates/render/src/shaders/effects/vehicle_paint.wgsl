// Vehicle paint (`tank_surfaces.rs`, `VEHICLE_PAINT_EFFECT`): the painted armor of
// tanks and the Humvee, worn the way a hand-painted stylized vehicle is.
//
// Inputs, all from the livery's mesh and its one tile:
// - vertex rgb: the clean paint (team color or trim) with its plate variation and
//   the light-to-dark gradient down each plate; the instance tint multiplies it;
// - vertex alpha: 0.2 plus 0.3 x how low the vertex sits on the hull (0 at the
//   dusty band's top, 1 at the ground), raised by 0.4 on chamfered plate edges. The
//   edge flag is the same on a whole triangle, so interpolation keeps the two apart;
// - the paint tile (`map`, model-space UVs, 4 m per UV unit): rgb is the finish's
//   value (brush strokes, orange peel, rain streaks, scratches) and its alpha the
//   wear field, low where the paint gives way first. Standard shading has already
//   multiplied both into the surface, so they are divided back out here.
//
// The paint chips to bright bare steel where the wear field falls under how exposed
// the spot is (plate edges most, the low hull more than the roof), with a thin
// darker rim; chamfers wear lighter, which outlines every plate like a bevel
// highlight. Dust and wet mud climb the lower hull in a broken, splashed line, and
// roughen the finish. Painted armor glows faintly in its own color, so it never
// crushes to black in shade, and catches a team-colored fresnel rim, so a vehicle's
// side reads at range and under smoke.
//
// params[0] = (team rim rgb linear, rim strength);
// params[1] = (bare steel rgb linear, ambient lift);
// params[2] = (dust rgb linear, dust strength);
// params[3] = (mud rgb linear, mud strength).

fn vehicle_hash(p: vec2f) -> f32 {
    var q = fract(vec3f(p.x, p.y, p.x) * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn vehicle_noise(p: vec2f) -> f32 {
    let cell = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = vehicle_hash(cell);
    let b = vehicle_hash(cell + vec2f(1.0, 0.0));
    let c = vehicle_hash(cell + vec2f(0.0, 1.0));
    let d = vehicle_hash(cell + vec2f(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Base exposure of flat plates, of the lowest hull and of chamfered edges.
const VEHICLE_FLAT_EXPOSURE: f32 = 0.12;
const VEHICLE_LOW_EXPOSURE: f32 = 0.34;
const VEHICLE_EDGE_EXPOSURE: f32 = 0.56;
// Width of a chip's soft edge and of its darker rim in wear-field units.
const VEHICLE_CHIP_SOFT: f32 = 0.025;
const VEHICLE_CHIP_RIM: f32 = 0.045;
// Paint rubbed lighter along chamfered edges, toward a pale primer.
const VEHICLE_EDGE_LIFT: f32 = 1.55;
const VEHICLE_WORN_EDGE: vec3f = vec3f(0.62, 0.58, 0.5);
// Finish: satin paint, rough dirt, smooth bare steel.
const VEHICLE_PAINT_ROUGHNESS: f32 = 0.46;
const VEHICLE_DIRT_ROUGHNESS: f32 = 0.9;
const VEHICLE_STEEL_ROUGHNESS: f32 = 0.3;
const VEHICLE_STEEL_METALNESS: f32 = 0.8;
// Grime streaks down upright plates, and the plate-scale weathering spread.
const VEHICLE_STREAK_DARKEN: f32 = 0.22;
const VEHICLE_WEATHER_SPREAD: f32 = 0.12;
// The tile's mean value (`PAINT_TEXTURE_MEAN` in Rust).
const VEHICLE_TILE_MEAN: f32 = 0.8;

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let paint = f.vertex_color.rgb * f.tint.rgb;
    let finish = (*s).color / max(paint, vec3f(1e-5));
    let wear = (*s).opacity / max(f.vertex_color.a * f.tint.a, 1e-5);
    (*s).opacity = f.tint.a;
    let edge = step(0.55, f.vertex_color.a);
    let low = clamp((f.vertex_color.a - 0.2 - 0.4 * edge) / 0.3, 0.0, 1.0);
    let value = (finish.r + finish.g + finish.b) / 3.0;

    var color = paint * finish;
    // Patina: the coat faded unevenly, a little lighter and greyer in broad
    // patches, so no plate reads as one flat printed color.
    let patina = vehicle_noise(f.uv * 5.0 + 3.1) * 0.6 + vehicle_noise(f.uv * 16.0 + 1.7) * 0.4;
    let faded = smoothstep(0.35, 0.8, patina);
    let luma = vec3f(dot(color, vec3f(0.3, 0.55, 0.15)));
    color = mix(color * 0.9, mix(color, luma, 0.15) * 1.08, faded);
    // Sun-faded upward faces: a little lighter and greyer than the sides, so the
    // hull's form reads from the overhead camera.
    let up = smoothstep(0.55, 0.95, (*s).normal.y);
    let grey = vec3f(dot(color, vec3f(0.3, 0.55, 0.15)));
    color = mix(color, mix(color, grey, 0.12) * 1.04, up);
    // Grime run down the upright plates from bolts and seams: narrow dark streaks,
    // longest below the plate tops, readable from the hero camera.
    let upright = 1.0 - smoothstep(0.3, 0.6, abs((*s).normal.y));
    let streak = vehicle_noise(vec2f(f.uv.x * 60.0, f.uv.y * 3.0 + 11.0));
    let streak_mask = smoothstep(0.55, 0.85, streak) * (0.55 + 0.45 * vehicle_noise(f.uv * 9.0 + 5.0));
    color *= 1.0 - VEHICLE_STREAK_DARKEN * streak_mask * upright;
    // Broad weathering: some panels sun-bleached, others grimier, at the scale of
    // a plate so it survives the overview camera.
    let weather = vehicle_noise(f.uv * 2.2 + 9.4);
    color *= mix(1.0 - VEHICLE_WEATHER_SPREAD, 1.0 + VEHICLE_WEATHER_SPREAD * 0.5, weather);
    // Chamfers: paint rubbed lighter, a bevel highlight round every plate.
    let lifted = mix(paint * VEHICLE_EDGE_LIFT, VEHICLE_WORN_EDGE, 0.3) * value;
    color = mix(color, lifted, edge * 0.6);

    // Dust and mud thrown up by the tracks: a broken line, splashed higher in spots.
    let splash = vehicle_noise(f.uv * 14.0) * 0.6 + vehicle_noise(f.uv * 41.0 + 7.3) * 0.4;
    let dusty = smoothstep(0.02, 0.5, low + (splash - 0.5) * 0.6) * material.params[2].w;
    let muddy = smoothstep(0.55, 0.85, low + (splash - 0.5) * 0.45) * material.params[3].w;
    color = mix(color, material.params[2].rgb * value / VEHICLE_TILE_MEAN, dusty);
    color = mix(color, material.params[3].rgb * value / VEHICLE_TILE_MEAN, muddy);
    let dirt = max(dusty, muddy);

    // Chips to bare steel where the wear field dips under the spot's exposure,
    // fewer under the dirt, with a darker rim of lifted paint round each.
    let exposure = mix(VEHICLE_FLAT_EXPOSURE, VEHICLE_LOW_EXPOSURE, low)
        + edge * (VEHICLE_EDGE_EXPOSURE - VEHICLE_FLAT_EXPOSURE) - dirt * 0.1;
    // The tile's fine wear field blurs out with distance; a coarser field of
    // scuffs keeps worn edges and corners visible from the follow camera.
    let scuff = vehicle_noise(f.uv * 26.0 + 2.3) * 0.65 + vehicle_noise(f.uv * 70.0) * 0.35;
    let worn = min(wear, scuff + 0.18 + (1.0 - edge) * 0.2);
    let chip = 1.0 - smoothstep(exposure - VEHICLE_CHIP_SOFT, exposure, worn);
    let rim = (1.0 - smoothstep(exposure, exposure + VEHICLE_CHIP_RIM, worn)) * (1.0 - chip);
    color *= 1.0 - rim * 0.45;
    let steel = material.params[1].rgb * (0.75 + 0.5 * value);
    color = mix(color, steel, chip);

    // The finish's roughness follows its brush strokes and streaks, the dirt and
    // the bare steel.
    var roughness = VEHICLE_PAINT_ROUGHNESS + (VEHICLE_TILE_MEAN - value) * 0.8;
    roughness = mix(roughness, VEHICLE_DIRT_ROUGHNESS, dirt);
    roughness = mix(roughness, VEHICLE_STEEL_ROUGHNESS, chip);
    (*s).roughness = clamp(roughness, 0.25, 0.95);
    (*s).metalness = mix((*s).metalness, VEHICLE_STEEL_METALNESS, chip);
    (*s).color = color;

    // A faint glow in the paint's own color and the team rim at grazing angles.
    let facing = clamp(dot((*s).normal, f.view_direction), 0.0, 1.0);
    let grazing = 1.0 - facing;
    let rim_light = grazing * grazing * grazing * material.params[0].w * (1.0 - dirt) * (1.0 - chip);
    (*s).emissive += color * material.params[1].w + material.params[0].rgb * rim_light;
}

// Harbor Havoc's far shore (`harbor_backdrop`, `effects_scenery::HARBOR_SHORE`):
// vertex colors lit by the sun and the sky fill, then hazed by the air between the
// coast and the eye and emitted whole, so its layers keep their separation whatever
// the scene fog. Its parts are flagged by their uv.y: tower facades with floors of
// windows, the terminal's container stacks, the countryside on the headlands
// (fields, woods with lit crowns, rock), the far pines, the tank farm's welded
// steel, the slipformed bins and the clad sheds. Pure red vertices are warning
// lights. Kept apart from the apron's effect so neither pipeline carries the
// other's code (one shader holding both crashed the WebGL2 build's Metal compile).

const SHORE_LUMA = vec3f(0.2126, 0.7152, 0.0722);
// Harbor water level (`harbor_water::HARBOR_WATER_HEIGHT`).
const SHORE_SEA_LEVEL = -2.2;
// Far shore haze: metres of air that hide half of a color beyond the first
// clear stretch, and how much less haze the high ground gets.
const SHORE_HAZE_DISTANCE = 560.0;
const SHORE_HAZE_CLEAR = 130.0;
const SHORE_HAZE_LIFT = 0.25;
// A far-shore vertex painted exactly this red is a warning light, and how
// brightly it shines.
const SHORE_LIGHT = vec3f(1.0, 0.0, 0.0);
const SHORE_LIGHT_GLOW = vec3f(2.4, 0.06, 0.03);
// Pitch of the profiled cladding's ribs on the far sheds, metres.
const SHORE_CLAD_RIB = 0.9;
// Window pitch on the far city's towers: across the face and floor to floor, metres.
const SHORE_WINDOW = vec2f(2.6, 3.4);
// Linear glow of a lit office window across the bay.
const SHORE_WINDOW_GLOW = vec3f(0.9, 0.55, 0.24);
// The headlands' countryside: a field's size along and across its furrows,
// metres; the linear paint of its crops (lush, ripe, ploughed and hay), of the
// woods' shaded canopy and of bare rock.
const SHORE_FIELD = vec2f(52.0, 30.0);
const SHORE_CROPS = array<vec3f, 4>(
    vec3f(0.07, 0.15, 0.035),
    vec3f(0.22, 0.2, 0.075),
    vec3f(0.12, 0.085, 0.05),
    vec3f(0.15, 0.19, 0.07),
);
const SHORE_FOREST = vec3f(0.03, 0.078, 0.026);
// Spacing of the far woods' crowns, metres, and the heights over which they
// thin out to the open crests (with the woods' own noise raising or lowering
// the line), metres above the sea.
const SHORE_CROWN = 3.2;
const SHORE_TREELINE = vec2f(28.0, 70.0);
// The far woods' crowns, and the share of the hillside's haze in front of them.
const SHORE_CANOPY = vec3f(0.03, 0.058, 0.04);
const SHORE_CANOPY_HAZE = 0.45;
// The share of the scene's low sea mist the far shore takes: enough to seat the
// coast in it, little enough that the terminal and the woods keep their colour.
const SHORE_MIST = 0.45;
// How much of its saturation the headlands' countryside keeps.
const SHORE_GRASS_SATURATION = 0.78;
const SHORE_ROCK = vec3f(0.2, 0.19, 0.165);
// Linear paint of the far terminal's boxes (`harbor_backdrop::BOX_PAINT`).
const SHORE_BOXES = array<vec3f, 6>(
    vec3f(0.58, 0.16, 0.044),
    vec3f(0.046, 0.25, 0.26),
    vec3f(0.11, 0.21, 0.38),
    vec3f(0.47, 0.37, 0.16),
    vec3f(0.33, 0.078, 0.06),
    vec3f(0.75, 0.72, 0.64),
);

fn shore_hash(cell: vec2f) -> vec4f {
    var p = fract(cell.xyxy * vec4f(0.1031, 0.1030, 0.0973, 0.1099));
    p = p + dot(p, p.wzxy + 33.33);
    return fract((p.xxyz + p.yzzw) * p.zywx);
}

fn shore_noise(p: vec2f) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = shore_hash(i).x;
    let b = shore_hash(i + vec2f(1.0, 0.0)).x;
    let c = shore_hash(i + vec2f(0.0, 1.0)).x;
    let d = shore_hash(i + vec2f(1.0, 1.0)).x;
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    // Far shore: lit by the sun and the sky fill, then hazed by the air between
    // it and the eye, emitted whole so the scene's fog never flattens the
    // layers. Painted warning lights shine through the haze, and the city's
    // towers (flagged by their uv) take floors of windows mirroring the sky.
    let n = (*s).normal;
    let across = select(f.world.x, f.world.z, abs(n.x) > abs(n.z));
    let window_cell = vec2f(across / SHORE_WINDOW.x, (f.world.y - SHORE_SEA_LEVEL) / SHORE_WINDOW.y);
    let window_aa = fwidth(window_cell) * 0.75;
    let pane_at = fract(window_cell);
    let pane = smoothstep(0.2 - window_aa.x, 0.2 + window_aa.x, pane_at.x)
        * (1.0 - smoothstep(0.8 - window_aa.x, 0.8 + window_aa.x, pane_at.x))
        * smoothstep(0.32 - window_aa.y, 0.32 + window_aa.y, pane_at.y)
        * (1.0 - smoothstep(0.86 - window_aa.y, 0.86 + window_aa.y, pane_at.y));
    let walls = abs(n.y) < 0.5;
    let facade = select(0.0, 1.0, f.uv.y > 5.0 && f.uv.y < 15.0 && walls);
    let window_hash = shore_hash(floor(window_cell) + vec2f(3.0, 9.0));
    let blinds = step(0.72, window_hash.x);
    // The low sun has some offices lit already: a scatter of warm windows
    // that shine through the haze.
    let lit_window = pane * facade * step(0.9, window_hash.y) * (1.0 - blinds);
    let glass = mix(frame.sky_color.rgb * 0.22 + vec3f(0.015, 0.025, 0.035), f.vertex_color.rgb * 0.7, blinds);
    var albedo = mix(f.vertex_color.rgb, glass, pane * facade);
    // Container stacks: every tier and 6 m bay its own box, in a line's paint,
    // with dark seams between them.
    let box_cell = floor(vec2f(across / 6.1, f.world.y / 2.6));
    let box_paint = SHORE_BOXES[u32(shore_hash(box_cell + vec2f(11.0, 5.0)).x * 6.0) % 6u];
    let seam = max(smoothstep(0.86, 0.95, abs(fract(f.world.y / 2.6) - 0.5) * 2.0),
        smoothstep(0.9, 0.97, abs(fract(across / 6.1) - 0.5) * 2.0));
    let stack = select(0.0, 1.0, f.uv.y > 15.0 && f.uv.y < 25.0);
    albedo = mix(albedo, box_paint * select(0.8, 1.0, walls) * (1.0 - seam * 0.6), stack);
    // Countryside on the headlands' green (flagged by its uv, and only where
    // the painted ground is green, not the beach, the rock or the quays): broad
    // dry and lush grass, a patchwork of fields with hedgerows on the gentle
    // ground, woods in the hollows and up the slopes with their crowns catching
    // the light, and bare rock in strata where it is steep.
    let ground = f.world.xz;
    let slope = 1.0 - n.y;
    let terrain = select(0.0, 1.0, f.uv.y > 25.0 && f.uv.y < 35.0)
        * smoothstep(0.004, 0.03, f.vertex_color.g - f.vertex_color.r);
    let shore_footprint = length(fwidth(f.world));
    let dry = shore_noise(ground / 61.0) * 0.6 + shore_noise(ground / 23.0 + 5.0) * 0.4;
    let woods = shore_noise(ground / 34.0 + 11.0) * 0.65 + shore_noise(ground / 11.0 + 3.0) * 0.35;
    // The woods climb from the hollows to a ragged tree line short of the
    // crests, which stay open grass and scree, so every ridge reads as a
    // landform with a wooded skirt rather than camouflage blotches.
    let treeline = 1.0 - smoothstep(SHORE_TREELINE.x, SHORE_TREELINE.y, f.world.y + (woods - 0.5) * 30.0);
    // The woods' edge is ragged by the crowns along it, never a smooth blob.
    let crown_noise = shore_noise(ground / 4.2) * 0.6 + shore_noise(ground / 1.6 + 7.0) * 0.4;
    let woods_edge = woods * 0.8 + slope * 0.3 + 0.12 - (1.0 - treeline) * 0.55 + (crown_noise - 0.5) * 0.09;
    let woods_aa = max(fwidth(woods_edge), 1e-4);
    let forest = smoothstep(0.5 - woods_aa, 0.5 + woods_aa, woods_edge);
    // Crowns: a canopy of round tops, each lit on its sun side and shading
    // the next, with dark gaps between, faded to its mean where a pixel spans
    // a crown. Only worked out where the woods stand.
    let crown_detail = (1.0 - smoothstep(0.18, 0.6, shore_footprint / SHORE_CROWN)) * forest * terrain;
    var crowns = mix(0.5, crown_noise, 1.0 - smoothstep(1.5, 5.0, shore_footprint));
    if crown_detail > 0.0 {
        let cell = ground / SHORE_CROWN;
        let base = floor(cell);
        let local = cell - base;
        var nearest = 4.0;
        var toward = vec2f(0.0);
        var tone = 1.0;
        for (var j = -1; j <= 1; j++) {
            for (var i = -1; i <= 1; i++) {
                let c = vec2f(f32(i), f32(j));
                let h = shore_hash(base + c + vec2f(41.0, 17.0));
                let off = (local - c - 0.15 - 0.7 * h.xy) / mix(0.62, 0.98, h.z);
                let d = dot(off, off);
                if d < nearest {
                    nearest = d;
                    toward = off;
                    tone = mix(0.7, 1.2, h.w);
                }
            }
        }
        let dome = vec3f(toward.x, 1.1 * sqrt(max(1.0 - nearest, 0.0)) + 0.2, toward.y);
        let sunlit = max(dot(normalize(dome), frame.sun_direction.xyz), 0.0);
        let canopy_value = mix(0.2, mix(0.32, 0.95, sunlit) * tone, 1.0 - smoothstep(0.55, 1.0, nearest));
        crowns = mix(crowns, canopy_value, crown_detail);
    }
    var land = f.vertex_color.rgb * mix(vec3f(0.88, 1.0, 0.86), vec3f(1.2, 1.1, 0.72), smoothstep(0.45, 0.8, dry));
    let field_at = vec2f(ground.x * 0.83 - ground.y * 0.56, ground.x * 0.56 + ground.y * 0.83) / SHORE_FIELD;
    let field = shore_hash(floor(field_at) + vec2f(7.0, 19.0));
    let field_to_edge = (0.5 - abs(fract(field_at) - 0.5)) * SHORE_FIELD;
    let field_edge = min(field_to_edge.x, field_to_edge.y);
    let fields = step(0.3, field.x) * smoothstep(0.16, 0.1, slope) * (1.0 - forest);
    let crop = SHORE_CROPS[u32(field.y * 4.0) % 4u] * mix(0.85, 1.12, field.z)
        * mix(0.92, 1.06, shore_noise(field_at * vec2f(40.0, 3.0)));
    let hedge = 1.0 - smoothstep(0.6, 1.6 + shore_footprint, field_edge);
    land = mix(land, crop, fields * 0.75);
    land = mix(land, SHORE_FOREST * 1.3, hedge * fields);
    // Crowns: deep shade between them, and sunlit tops a warmer green.
    let crown_lit = smoothstep(0.25, 0.75, crowns);
    land = mix(land, SHORE_FOREST * mix(vec3f(0.45), vec3f(2.1, 2.0, 1.5), crown_lit), forest);
    let rock = smoothstep(0.3, 0.46, slope + (shore_noise(ground / 7.0) - 0.5) * 0.25) * (1.0 - forest * 0.6);
    let strata = 0.82 + 0.18 * sin(f.world.y * 1.6 + shore_noise(ground / 9.0) * 3.0);
    land = mix(land, SHORE_ROCK * strata, rock);
    // Grass seen across a bay is never lime: its green greys a little.
    land = mix(vec3f(dot(land, SHORE_LUMA)), land, SHORE_GRASS_SATURATION);
    albedo = mix(albedo, land, terrain);
    var hemisphere = mix(frame.ground_color.rgb, frame.sky_color.rgb, n.y * 0.5 + 0.5);
    // The far pines (flagged by their uv) are lit mostly by the sky: a crown
    // seen across the bay is a dark mass, and the sun catching the edges of
    // its few facets drew them as pale outlines. Each clump steps in value
    // from its shaded heart (uv.x 0 at a crown's foot) to the tips that catch
    // the sky, so the woods read as massed crowns rather than cut-out spikes.
    let tree = select(0.0, 1.0, f.uv.y > 35.0 && f.uv.y < 45.0);
    albedo = albedo * mix(1.0, mix(0.5, 1.2, f.uv.x), tree);
    // Fuel tanks (flagged by their uv): welded courses of plate, rust run down
    // from the roof and a shaded side the sky barely reaches, so the farm
    // reads as round steel and not pale cut-outs.
    let steel_tank = select(0.0, 1.0, f.uv.y > 45.0 && f.uv.y < 55.0);
    let course_aa = max(fwidth(f.world.y) / 1.8, 1e-3);
    let rib_at = across / SHORE_CLAD_RIB;
    let rib_aa = max(fwidth(rib_at), 1e-3);
    // Round steel and concrete turn away from the sun into a deep shaded side
    // the sky barely reaches, so tanks and bins read round, not as pale cut-outs.
    let away = 1.0 - max(dot(normalize(vec3f(n.x, 0.0, n.z) + vec3f(0.0, 1e-4, 0.0)), normalize(vec3f(frame.sun_direction.x, 0.0, frame.sun_direction.z))), 0.0);
    let side = select(1.0, 0.0, n.y > 0.5);
    if steel_tank > 0.0 {
        let course = abs(fract((f.world.y - SHORE_SEA_LEVEL) / 1.8) - 0.5) * 2.0;
        let weld = smoothstep(1.0 - course_aa * 2.0, 1.0, course);
        let streaks = shore_noise(vec2f(across * 1.4, f.world.y * 0.15)) * 0.7
            + shore_noise(vec2f(across * 3.7, f.world.y * 0.4)) * 0.3;
        albedo = albedo * (1.0 - weld * 0.25) * mix(1.0, 0.72, smoothstep(0.55, 0.8, streaks) * side);
        hemisphere = hemisphere * mix(1.0, 0.45, away * side);
    }
    // Slipformed bins: rain run down from the gallery in long grey streaks,
    // darker toward the foot where the yard's dust splashes.
    if f.uv.y > 55.0 && f.uv.y < 65.0 {
        let streaks = shore_noise(vec2f(across * 2.2, f.world.y * 0.08)) * 0.65
            + shore_noise(vec2f(across * 5.9, f.world.y * 0.3)) * 0.35;
        let foot = 1.0 - smoothstep(0.0, 9.0, f.world.y - SHORE_SEA_LEVEL - 2.0);
        albedo = albedo * mix(1.0, 0.7, smoothstep(0.5, 0.78, streaks) * side) * mix(1.0, 0.72, foot * side);
        hemisphere = hemisphere * mix(1.0, 0.4, away * side);
    }
    // Profiled cladding: ribs that catch the light on one flank, a darker band
    // of dirt along the foot and the shaded walls kept dark.
    if f.uv.y > 65.0 {
        let rib = abs(fract(rib_at) - 0.5) * 2.0;
        let flank = mix(smoothstep(0.35 - rib_aa, 0.35 + rib_aa, rib), 0.5, smoothstep(0.3, 0.8, rib_aa));
        let foot = 1.0 - smoothstep(0.0, 2.5, f.world.y - SHORE_SEA_LEVEL - 2.0);
        albedo = albedo * mix(1.0, mix(0.78, 1.08, flank), side) * mix(1.0, 0.66, foot * side);
        hemisphere = hemisphere * mix(1.0, 0.55, away * side);
    }
    // Woods take no sun and little sky: their crowns' upturned normals caught
    // more light than the shaded slopes round them and glowed as pale cut-outs
    // against the hills, where a wood seen across a bay is a dark mass.
    let sun = max(dot(n, frame.sun_direction.xyz), 0.0) * frame.sun_color.rgb * (1.0 - tree);
    let lit = albedo * (sun + hemisphere * (1.0 - tree * 0.7)) * 0.3183099;
    let distance = length(frame.camera_position.xyz - f.world);
    let lift = smoothstep(0.0, 60.0, f.world.y) * SHORE_HAZE_LIFT;
    let haze = (1.0 - exp2(-max(distance - SHORE_HAZE_CLEAR, 0.0) / SHORE_HAZE_DISTANCE)) * (1.0 - lift);
    var horizon = frame.sky_color.rgb * 0.3183099;
    if atmosphere_on() {
        // The scene's own aerial perspective: thin haze cools and darkens the
        // near headlands, thick haze melts the ranges into the sky behind them,
        // so the layers recede in steps of value and saturation.
        horizon = haze_tint((f.world - frame.camera_position.xyz) / max(distance, 1e-3), haze);
    } else if frame.fog_color.w > 0.5 {
        horizon = frame.fog_color.rgb;
    }
    var hazed = mix(lit, horizon, haze);
    // Seen through this much air a wood is a cool shadow on the hillside, a
    // step darker and bluer than the haze round it: the warm haze alone left
    // the crowns glowing tan, like cut-outs pasted on the hills.
    // Against the low sun they stand darker still, as backlit silhouettes
    // rather than taking its warm glow.
    let backlit = smoothstep(0.0, 0.75, dot((f.world - frame.camera_position.xyz) / max(distance, 1e-3), frame.sun_direction.xyz));
    // The canopy keeps its own dark conifer green under a little less air than
    // the open hillside, so near woods read green and far ones blue, in steps.
    let canopy = SHORE_CANOPY * mix(0.5, 1.3, f.uv.x) * mix(1.0, 0.5, backlit);
    let wood = mix(canopy, horizon, haze * SHORE_CANOPY_HAZE);
    hazed = mix(hazed, wood, tree);
    if atmosphere_on() {
        // The scene's low sea mist: thick at the waterline and thinning up the
        // slopes, so the headlands rise out of it with clear green crests.
        let ray = (f.world - frame.camera_position.xyz) / max(distance, 1e-4);
        let mist = frame.haze_far.w * SHORE_MIST * exp(-max(f.world.y, 0.0) / MIST_HEIGHT) * (1.0 - tree * 0.6);
        hazed = mix(hazed, haze_tint(ray, mist), mist);
    }
    // The painted warning lights are flagged by their vertex colour, since the
    // albedo above is repainted for the countryside and containers.
    let light = select(0.0, 1.0, all(abs(f.vertex_color.rgb - SHORE_LIGHT) < vec3f(0.01)));
    hazed = hazed + SHORE_WINDOW_GLOW * lit_window * (1.0 - haze * 0.5);
    (*s).emissive = mix(hazed, SHORE_LIGHT_GLOW, light);
    (*s).color = vec3f(0.0);
    (*s).roughness = 1.0;
    (*s).metalness = 0.0;
}

// Vehicle paint (`tank_surfaces.rs`, `VEHICLE_PAINT_EFFECT`): the painted armor of
// tanks and the Humvee, a strong team color worn where a real vehicle wears, and the
// fill light of their running gear.
//
// Inputs from the livery's mesh and its wear tile:
// - vertex red and green: two edge channels. Each describes a pair of parallel
//   plate border lines the triangle lies between as `state + 0.04 + 0.92 t`, t
//   running from 0 on the near line to 1 on the far one. t is affine over the
//   triangle, so its screen-space gradient gives the plate's width in metres and the
//   direction across it, and so both distances and which way each edge faces. The
//   integer state is the near border plus 3 x the far border: 0 plain, 1 a convex
//   edge, 2 a recess. Smooth surfaces hold 0.5, which has no gradient.
// - vertex blue: the paint's value code, trim below 0.5 and team paint above
//   (`paint_value_code`): a slight shift per plate and a light-to-dark gradient down
//   each upright plate, darker at the ground line. Its integer part is the coat
//   (`coat_code`): 0 the team paint or trim, 1 the markings' white (which flakes
//   to the team paint under it), 2 a lamp lens;
// - vertex alpha: 0.2 plus 0.3 x how low the vertex sits on the hull (0 at the
//   dusty band's top, 1 at the ground), raised by 0.4 on chamfer strips;
// - the wear tile (`extra_texture0`, model-space UVs): grey is the grime field
//   around 0.5 (splashes, streaks), alpha the chip field.
//
// The paint is one clean, deep team color over the upper panels. Convex plate edges
// chip to bright steel in broken runs and catch a narrow bevel highlight: the normal
// rolls over the edge and the paint there is rubbed lighter, so every boxy form
// catches the light. Recesses collect dark grime, and dust and mud climb the lower
// hull in a splashed line. Painted armor takes a fill light of its own, strongest on
// the side turned from the sun, so it never crushes to black in shade, and a rim in
// its team's hue at grazing angles, so a vehicle's side reads at range in any grade.
// Running gear (params[3].w 0) keeps its own colors and takes only the fill.
// The track belt (params[3].w 3, `track_belt_material`) draws its links from its
// UVs: u counts links round the loop, v's integer part is the face (0 ground face,
// 1 running face, 2 outer edge, 3 inner edge) and its fraction the position across
// it. The links scroll by instance_data.x model units of travel (params[0].x links
// per unit); wrecks (instance_data.y 1) keep theirs still.
// Glass (params[3].w 2, `vehicle_glass`) is a dark tint that mirrors the sky
// toward grazing angles, so windows and sights read as glass from any camera
// rather than as flat painted or glowing panels.
//
// params[0] = (team paint rgb linear, rim strength), or the glass tint;
// params[1] = (trim rgb linear, fill strength);
// params[2] = (rim hue rgb linear, wear tile repeat);
// params[3] = (bare steel rgb linear, 1 painted armor, 0 running gear, 2 glass or
// 3 track belt).

const VEHICLE_EDGE_MARGIN: f32 = 0.04;
const VEHICLE_EDGE_SPAN: f32 = 0.92;
// `VALUE_RANGE` in Rust.
const VEHICLE_VALUE_RANGE: f32 = 1.45;
// The bevel band: about this many pixels wide, within these widths in metres, and
// how far the normal rolls over in it.
const VEHICLE_BEVEL_PIXELS: f32 = 4.5;
const VEHICLE_BEVEL_MIN: f32 = 0.016;
const VEHICLE_BEVEL_MAX: f32 = 0.12;
const VEHICLE_BEVEL_ROLL: f32 = 0.45;
// Panel shading: paint along a plate's borders is this much darker, fading out over
// this fraction of the plate's narrowest width (within these metres).
const VEHICLE_PANEL_BORDER: f32 = 0.66;
const VEHICLE_PANEL_REACH: f32 = 0.3;
const VEHICLE_PANEL_MIN: f32 = 0.03;
const VEHICLE_PANEL_MAX: f32 = 0.35;
const VEHICLE_PANEL_PIXEL_FROM: f32 = 0.018;
const VEHICLE_PANEL_PIXEL_TO: f32 = 0.045;
// How far from a convex edge chips reach, and from a recess its grime (metres).
const VEHICLE_CHIP_REACH: f32 = 0.09;
const VEHICLE_RECESS_REACH: f32 = 0.12;
// Chips fade out as a pixel grows past their size, so distant hulls never sparkle.
const VEHICLE_CHIP_PIXEL_FROM: f32 = 0.012;
const VEHICLE_CHIP_PIXEL_TO: f32 = 0.04;
// Plates narrower than these (metres) never chip: bolt heads, latches and trim
// strips would otherwise scatter grey specks evenly over every deck.
const VEHICLE_CHIP_PLATE_FROM: f32 = 0.05;
const VEHICLE_CHIP_PLATE_TO: f32 = 0.11;
// Paint rubbed lighter on a bevel and on chamfer strips: a painted edge highlight,
// brightest where the edge turns toward the sun or the sky and still lifted on the
// edges in shade, so every boxy form reads crafted from any side.
const VEHICLE_BEVEL_LIFT: f32 = 1.4;
const VEHICLE_BEVEL_LIFT_LIT: f32 = 3.0;
// The lit highlight resolves while a pixel is smaller than these sizes: on a card or
// from the overhead camera a wide, bright edge would wash narrow plates out.
const VEHICLE_BEVEL_LIT_PIXEL_FROM: f32 = 0.006;
const VEHICLE_BEVEL_LIT_PIXEL_TO: f32 = 0.015;
const VEHICLE_BEVEL_PALE: vec3f = vec3f(0.03, 0.022, 0.018);
const VEHICLE_BEVEL_TINT: f32 = 0.9;
// Dust and wet mud (linear) and how strongly they cover the lowest armor. In a
// round (the atmosphere on) the dust takes this share of its hue from the map's
// ground light (sand in the quarry, brown earth in the village, grey concrete at
// the harbor) at its own brightness; studio scenes keep the plain dust.
const VEHICLE_DUST: vec3f = vec3f(0.12, 0.092, 0.06);
const VEHICLE_DUST_LOCAL: f32 = 0.6;
const VEHICLE_MUD: vec3f = vec3f(0.075, 0.053, 0.034);
const VEHICLE_DUST_STRENGTH: f32 = 0.82;
const VEHICLE_DUST_STUDIO: f32 = 0.12;
const VEHICLE_BELT_DUST_STUDIO: f32 = 0.2;
const VEHICLE_MUD_STRENGTH: f32 = 0.7;
// Grime in recesses: a dark brown multiplier.
const VEHICLE_GRIME: vec3f = vec3f(0.34, 0.29, 0.24);
const VEHICLE_GRIME_STRENGTH: f32 = 0.8;
// Finish: satin paint, rough dirt, and bare steel scuffed matte, so a chip reads as
// grey metal rather than as a mirror of the sky.
// Upright plates are satin, so their sides catch the sky as painted steel does;
// decks are flat-finished (and stay under the ground relief's roughness, which
// would otherwise texture them in world space as they drive).
const VEHICLE_PAINT_ROUGHNESS: f32 = 0.44;
const VEHICLE_DECK_ROUGHNESS: f32 = 0.76;
const VEHICLE_SHEEN_BREAKUP: f32 = 0.1;
const VEHICLE_DIRT_ROUGHNESS: f32 = 0.92;
const VEHICLE_STEEL_ROUGHNESS: f32 = 0.55;
const VEHICLE_STEEL_METALNESS: f32 = 0.45;
const VEHICLE_ENAMEL_ROUGHNESS: f32 = 0.56;
// A livery whose finish is rougher than the enamel by more than this is a truck's.
const VEHICLE_ARMOR_FINISH: f32 = 0.09;
// The team rim's brightness at a full-strength silhouette. In a round its hue
// blends this far toward a cool sky white, so a hull's silhouette parts from warm
// sand, brick and rust props of its own hue as well as from cool asphalt; studio
// cards keep the pure team hue (a cool rim turned the red card pink).
const VEHICLE_RIM_GLOW: f32 = 0.5;
const VEHICLE_RIM_COOL: vec3f = vec3f(0.72, 0.84, 1.0);
const VEHICLE_RIM_COOL_SHARE: f32 = 0.08;
// In a round, team paint keeps this share of its saturation: the maps' grades
// saturate, and a full-strength red clipped every sunlit plate to one value.
const VEHICLE_ROUND_SATURATION: f32 = 0.9;
// Paint lifted above its base value (the turret roof, sunlit decks) gains this much
// saturation per unit of lift, so a lighter plate reads as a brighter red or blue
// rather than paling toward salmon or sky under ACES's shoulder.
const VEHICLE_LIFT_SATURATION: f32 = 0.5;
// Light bounced off the ground onto upright and downward plates, strongest low on
// the hull (a share of the hemisphere's ground light): the lower hull and the rear
// plate in shade read as painted steel lit by the earth beside them, never as a
// black hole. Studio cards keep some of it from their floor.
const VEHICLE_BOUNCE: f32 = 0.42;
const VEHICLE_BOUNCE_STUDIO: f32 = 0.5;
// The paint's value on studio cards (the menu's previews).
const VEHICLE_STUDIO_VALUE: f32 = 0.92;
// And its saturation there: ACES's shoulder bleaches a lit red deck toward salmon.
const VEHICLE_STUDIO_SATURATION: f32 = 1.18;
// Rain streaks down upright plates: columns this many per paint UV across and this
// many per UV along their length, darkening by up to this much, and resolved only
// while a pixel is smaller than a streak.
const VEHICLE_STREAK_ACROSS: f32 = 95.0;
const VEHICLE_STREAK_ALONG: f32 = 2.6;
const VEHICLE_STREAK_DARKEN: f32 = 0.3;
const VEHICLE_STREAK_PIXEL_FROM: f32 = 0.02;
const VEHICLE_STREAK_PIXEL_TO: f32 = 0.045;
// Metres of armor per paint UV (`PAINT_UV_PER_METRE` in Rust).
const VEHICLE_METRES_PER_UV: f32 = 4.0;
// Welded panel seams across broad plates: this far apart (metres), on plates at
// least this wide, a groove this wide with a sunlit weld lip beside it, resolved
// while a pixel is smaller than these sizes.
const VEHICLE_SEAM_SPACING: f32 = 0.5;
const VEHICLE_SEAM_MIN_PLATE: f32 = 0.25;
const VEHICLE_SEAM_WIDTH: f32 = 0.01;
const VEHICLE_SEAM_LIP: f32 = 0.014;
const VEHICLE_SEAM_PIXEL_FROM: f32 = 0.012;
const VEHICLE_SEAM_PIXEL_TO: f32 = 0.028;
// On decks a second, wider set of seams runs the other way, so a broad plate reads
// as bolted access panels, each a shade apart (this much either way); rivet heads
// run beside every seam this far apart and this far out from its groove.
const VEHICLE_SEAM_LONG: f32 = 0.8;
const VEHICLE_PANEL_SHIFT: f32 = 0.07;
const VEHICLE_SEAM_RIVET_PITCH: f32 = 0.11;
const VEHICLE_SEAM_RIVET_SET: f32 = 0.03;
// Rivet rows: heads this far apart along a plate's border and this far inside it,
// this round with a shadow ring this wide, on plates at least this wide, resolved
// while a pixel is smaller than these sizes (the hero and first-person cameras).
const VEHICLE_RIVET_PITCH: f32 = 0.13;
const VEHICLE_RIVET_INSET: f32 = 0.045;
const VEHICLE_RIVET_RADIUS: f32 = 0.011;
const VEHICLE_RIVET_RING: f32 = 0.006;
const VEHICLE_RIVET_MIN_PLATE: f32 = 0.2;
const VEHICLE_RIVET_PIXEL_FROM: f32 = 0.006;
const VEHICLE_RIVET_PIXEL_TO: f32 = 0.012;
// Non-skid coating on decks: a grain this many cells per metre, varying the paint
// by this much either way, resolved only close up (the turret roof seen from the
// commander's hatch).
const VEHICLE_GRAIN_CELLS: f32 = 55.0;
const VEHICLE_GRAIN_VALUE: f32 = 0.34;
const VEHICLE_GRAIN_PIXEL_FROM: f32 = 0.003;
const VEHICLE_GRAIN_PIXEL_TO: f32 = 0.009;
// Tread plate on the trim's decks (fender tops, steps): raised lozenges in a cell
// this many metres square, alternating their angle cell by cell, this long and
// wide (shares of a cell), their tops scuffed toward bare steel, resolved while a
// pixel is smaller than these sizes (the hero and first-person cameras).
const VEHICLE_TREAD_CELL: f32 = 0.045;
const VEHICLE_TREAD_LENGTH: f32 = 0.36;
const VEHICLE_TREAD_WIDTH: f32 = 0.11;
const VEHICLE_TREAD_WEAR: f32 = 0.55;
const VEHICLE_TREAD_PIXEL_FROM: f32 = 0.0035;
const VEHICLE_TREAD_PIXEL_TO: f32 = 0.008;
// Dust settled on decks: a thin film over broad upward plates (patchy at this many
// cells per metre, on plates at least this wide, resolved while a pixel is smaller
// than these sizes), drifted thicker into recesses, in the map's ground color where
// the atmosphere is on.
const VEHICLE_DECK_DUST_CELLS: f32 = 6.5;
const VEHICLE_DECK_DUST_FILM: f32 = 0.16;
const VEHICLE_DECK_DUST_DRIFT: f32 = 0.6;
const VEHICLE_DECK_DUST_MIN_PLATE: f32 = 0.25;
const VEHICLE_DECK_DUST_PIXEL_FROM: f32 = 0.012;
const VEHICLE_DECK_DUST_PIXEL_TO: f32 = 0.03;
// The track splash's line: this many waves per metre along the hull, rising and
// falling by this much of the dusty band.
const VEHICLE_SPLASH_CELLS: f32 = 3.4;
const VEHICLE_SPLASH_RISE: f32 = 0.22;
// A finer octave that frays the splash line's edge, and how much the wear tile's
// broad grime field may still lift or lower it.
const VEHICLE_SPLASH_FRAY_CELLS: f32 = 11.0;
const VEHICLE_SPLASH_FRAY: f32 = 0.1;
const VEHICLE_SPLASH_GRIME: f32 = 0.2;
// Where the caked band's crisp edge sits on the band (0 its top, 1 the ground),
// and the fine dust film over the paint just above it.
const VEHICLE_SPLASH_EDGE: f32 = 0.3;
const VEHICLE_DUST_FILM: f32 = 0.1;
// The road film's strength at the ground line.
const VEHICLE_ROAD_FILM: f32 = 0.38;
// Cells per metre of the fine breakup in settled deck dust.
const VEHICLE_DECK_DUST_GRAIN: f32 = 9.0;
// Edge wear: how far toward a convex edge the chip field is exposed, and how much
// more where two edges meet at a corner. Chips are small and sparse: a dark core of
// primer and bare steel ringed by a bright, scuffed lip of metal.
const VEHICLE_CHIP_EXPOSURE: f32 = 0.34;
const VEHICLE_CHIP_CORNER: f32 = 0.42;
const VEHICLE_CHIP_CORE: vec3f = vec3f(0.028, 0.026, 0.024);
const VEHICLE_CHIP_LIP: f32 = 0.045;
const VEHICLE_CHIP_STUDIO: f32 = 0.4;
// Cells per metre of the field that gathers chips into worn stretches.
const VEHICLE_CHIP_CLUSTER: f32 = 2.2;
// Soft, sun-faded variation across the paint, this many cells per metre and this
// much either way: broad enough never to read as speckle.
const VEHICLE_FADE_CELLS: f32 = 3.2;
const VEHICLE_FADE_VALUE: f32 = 0.035;
// Worn paint along convex edges: before it chips through, the paint along an
// edge rubs thin and pale in broken runs a few centimetres wide, lighter and a
// little greyer than the panel. Resolved while a pixel is smaller than these sizes,
// so it carries from the hero and first-person cameras down to the menu cards.
const VEHICLE_RUB_CELLS: f32 = 7.0;
const VEHICLE_RUB_STRENGTH: f32 = 0.3;
const VEHICLE_RUB_LIFT: f32 = 1.55;
const VEHICLE_RUB_GREY: f32 = 0.3;
const VEHICLE_RUB_PIXEL_FROM: f32 = 0.022;
const VEHICLE_RUB_PIXEL_TO: f32 = 0.05;
// Mud spatter thrown above the caked splash line: drops this many cells per metre
// (wider than tall), within this share of the dusty band above the line, resolved
// while a pixel is smaller than these sizes.
const VEHICLE_SPATTER_CELLS: vec2f = vec2f(24.0, 30.0);
const VEHICLE_SPATTER_REACH: f32 = 0.38;
const VEHICLE_SPATTER_PIXEL_FROM: f32 = 0.014;
const VEHICLE_SPATTER_PIXEL_TO: f32 = 0.034;
// Roughness drifts this much either way across a hull with the sun-fade field, so
// a panel's sheen breaks up like weathered enamel.
const VEHICLE_ROUGHNESS_DRIFT: f32 = 0.08;
// Markings: a warm, slightly dirty stencil white (linear), flaking through to the
// team paint where the chip field dips under this exposure (more where the wear
// clusters), and as matte as the dusty paint.
const VEHICLE_MARKING: vec3f = vec3f(0.6, 0.57, 0.5);
const VEHICLE_MARKING_WEAR: f32 = 0.1;
const VEHICLE_MARKING_WEAR_CLUSTER: f32 = 0.22;
const VEHICLE_MARKING_DUST: f32 = 0.16;
const VEHICLE_MARKING_ROUGHNESS: f32 = 0.72;
// Lamp lenses: clear glass over a silvered reflector, dark at the bezel ring and
// mirroring the sky. A lamp burns only when the light is low: under a daylight sun
// (its luminance at or above `VEHICLE_LAMP_DAY`) the filament keeps this share of
// its glow, a warm glint behind the glass rather than a lit sticker.
const VEHICLE_LAMP_RIM: vec3f = vec3f(0.035, 0.034, 0.032);
const VEHICLE_LAMP_GLASS: vec3f = vec3f(0.3, 0.29, 0.26);
const VEHICLE_LAMP_GLOW: vec3f = vec3f(1.5, 1.12, 0.62);
const VEHICLE_LAMP_GLOW_RIM: f32 = 0.05;
const VEHICLE_LAMP_DUSK: f32 = 0.6;
const VEHICLE_LAMP_DAY: f32 = 2.4;
const VEHICLE_LAMP_DAY_GLOW: f32 = 0.05;
const VEHICLE_LAMP_MIRROR: f32 = 0.55;

struct VehicleEdges {
    // 1 on a convex edge, fading out over the chip reach.
    near_edge: f32,
    // The same for the second nearest convex edge: high only near a corner, where
    // two edges meet and armor takes the most knocks.
    corner: f32,
    // 1 on a convex edge, fading out over the bevel band.
    bevel: f32,
    // 1 in a recess, fading out over its reach.
    recess: f32,
    // Outward directions of the bevels the pixel lies in, weighted by the band.
    roll: vec3f,
    // Metres to the nearest marked border, and the plate's narrowest width.
    nearest: f32,
    narrowest: f32,
}

fn vehicle_border(kind: f32, distance: f32, outward: vec3f, bevel_width: f32, e: ptr<function, VehicleEdges>) {
    if kind > 0.5 {
        (*e).nearest = min((*e).nearest, distance);
    }
    if kind > 1.5 {
        (*e).recess = max((*e).recess, 1.0 - smoothstep(0.0, VEHICLE_RECESS_REACH, distance));
    } else if kind > 0.5 {
        let band = 1.0 - smoothstep(0.0, bevel_width, distance);
        (*e).bevel = max((*e).bevel, band);
        (*e).roll += outward * band;
        let near = 1.0 - smoothstep(0.0, VEHICLE_CHIP_REACH, distance);
        (*e).corner = max((*e).corner, min((*e).near_edge, near));
        (*e).near_edge = max((*e).near_edge, near);
    }
}

fn vehicle_channel(c: f32, gradient: vec3f, bevel_width: f32, e: ptr<function, VehicleEdges>) {
    let g = length(gradient);
    if g < 1e-6 {
        return;
    }
    let state = floor(c);
    let t = clamp((c - state - VEHICLE_EDGE_MARGIN) / VEHICLE_EDGE_SPAN, 0.0, 1.0);
    let width = VEHICLE_EDGE_SPAN / g;
    (*e).narrowest = min((*e).narrowest, width);
    let across = gradient / g;
    let far = floor(state / 3.0);
    let near = state - far * 3.0;
    vehicle_border(near, t * width, -across, bevel_width, e);
    vehicle_border(far, (1.0 - t) * width, across, bevel_width, e);
}

// Glass: how much of the sky it mirrors head-on and at grazing angles, and a soft
// diagonal sheen band (along the world's x + z, this many per metre, this wide)
// that slides across windows as the vehicle turns, as a real reflection does.
const VEHICLE_GLASS_FACING: f32 = 0.16;
const VEHICLE_GLASS_GRAZING: f32 = 0.7;
const VEHICLE_GLASS_ROUGHNESS: f32 = 0.08;
const VEHICLE_GLASS_SHEEN: f32 = 0.7;
const VEHICLE_GLASS_SHEEN_CELLS: f32 = 0.9;
const VEHICLE_GLASS_SHEEN_WIDTH: f32 = 0.09;
// How much of the sky's color the glass keeps: a clear blue sky mirrored at full
// saturation turns windows into flat blue panels.
const VEHICLE_GLASS_SKY_SATURATION: f32 = 0.35;

// The sky a window mirrors: the drawn sky's gradient while the atmosphere is on,
// the hemisphere fill otherwise, and the ground's light below the horizon.
fn vehicle_sky(direction: vec3f) -> vec3f {
    var sky = frame.sky_color.rgb * 0.32;
    if atmosphere_on() {
        sky = sky_gradient(normalize(vec3f(direction.x, max(direction.y, 0.05), direction.z)));
    }
    let ground = frame.ground_color.rgb * 0.32;
    let mirrored = mix(ground, sky, smoothstep(-0.08, 0.08, direction.y));
    let grey = dot(mirrored, vec3f(0.2126, 0.7152, 0.0722));
    return mix(vec3f(grey), mirrored, VEHICLE_GLASS_SKY_SATURATION);
}

fn vehicle_glass(s: ptr<function, Surface>, f: EffectFragment) {
    let n = normalize((*s).normal);
    let v = f.view_direction;
    let facing = clamp(dot(n, v), 0.0, 1.0);
    let fresnel = VEHICLE_GLASS_FACING + (VEHICLE_GLASS_GRAZING - VEHICLE_GLASS_FACING) * pow(1.0 - facing, 4.0);
    let sheen_phase = fract((f.world.x + f.world.z) * VEHICLE_GLASS_SHEEN_CELLS + f.world.y * 0.4);
    let sheen = 1.0 - smoothstep(0.0, VEHICLE_GLASS_SHEEN_WIDTH, abs(sheen_phase - 0.5));
    (*s).color = material.params[0].rgb;
    (*s).roughness = VEHICLE_GLASS_ROUGHNESS;
    (*s).metalness = 0.0;
    (*s).emissive += vehicle_sky(reflect(-v, n)) * (fresnel + sheen * VEHICLE_GLASS_SHEEN);
}

// The vehicle's own fill: the sky and ground light again, strongest on faces
// turned from the sun.
fn vehicle_fill(n: vec3f) -> vec3f {
    let hemisphere = mix(frame.ground_color.rgb, frame.sky_color.rgb, n.y * 0.5 + 0.5);
    let away = 1.0 - clamp(dot(n, frame.sun_direction.xyz), 0.0, 1.0);
    return hemisphere * (0.3 + 0.7 * away);
}

// The running gear's fill: the paint fill's brightness in a warm neutral, half
// the ground light's own hue, so dark rubber and steel in the hull's shadow read
// as rubber and steel rather than taking the sky's cold blue.
fn vehicle_gear_fill(n: vec3f) -> vec3f {
    let luma = vec3f(0.2126, 0.7152, 0.0722);
    let level = dot(vehicle_fill(n), luma);
    let ground = frame.ground_color.rgb;
    let tint = ground / max(dot(ground, luma), 1e-4);
    return level * mix(vec3f(1.0, 0.97, 0.93), tint, 0.25);
}

// The track belt: dark worn steel shoes with rubber pads on the ground face, a
// polished path where the twin road wheels run either side of the guide horns on
// the running face, and bright end connectors bridging each joint on the edges.
// Joints are dark gaps; the pattern fades to its average where a link is only a
// few pixels long, so a distant track never shimmers.
const BELT_STEEL: vec3f = vec3f(0.095, 0.09, 0.082);
const BELT_POLISHED: vec3f = vec3f(0.23, 0.22, 0.2);
const BELT_RUBBER: vec3f = vec3f(0.022, 0.021, 0.02);
const BELT_GAP: f32 = 0.25;
// Links per pixel over which the pattern fades out.
const BELT_PIXEL_FROM: f32 = 0.16;
const BELT_PIXEL_TO: f32 = 0.42;
// How far the pads' and connectors' sloped ends tilt the normal.
const BELT_RELIEF: f32 = 0.55;
// Dust caked on the ground face and climbing the edges.
const BELT_DUST_GROUND: f32 = 0.14;
const BELT_DUST_EDGE: f32 = 0.07;

fn belt_band(x: f32, low: f32, high: f32, soft: f32) -> f32 {
    return smoothstep(low - soft, low + soft, x) * (1.0 - smoothstep(high - soft, high + soft, x));
}

// The map's dust (`VEHICLE_DUST` tinted by its ground light in a round).
fn vehicle_dust() -> vec3f {
    let luma = vec3f(0.2126, 0.7152, 0.0722);
    let ground = frame.ground_color.rgb;
    let ground_level = dot(ground, luma);
    let local_dust = ground * (dot(VEHICLE_DUST, luma) / max(ground_level, 1e-4));
    let local_share = select(0.0, VEHICLE_DUST_LOCAL, atmosphere_on() && ground_level > 1e-4);
    return mix(VEHICLE_DUST, local_dust, local_share);
}

fn vehicle_belt(s: ptr<function, Surface>, f: EffectFragment, tangent: vec3f, links_per_pixel: f32) {
    let still = f.instance_data.y > 0.5;
    let scroll = select(f.instance_data.x, 0.0, still) * material.params[0].x;
    let face = floor(f.uv.y);
    let across = fract(f.uv.y);
    let phase = fract(f.uv.x + scroll);
    let resolved = 1.0 - smoothstep(BELT_PIXEL_FROM, BELT_PIXEL_TO, links_per_pixel);
    let soft = clamp(links_per_pixel * 0.75, 0.015, 0.2);
    // The joint between links: a dark gap at phase 0.
    let joint = 1.0 - belt_band(phase, 0.06, 0.94, soft);
    var color = BELT_STEEL;
    var average = BELT_STEEL;
    var roughness = 0.62;
    var metalness = 0.25;
    var relief = 0.0;
    var dust = 0.0;
    if face < 0.5 {
        // Ground face: a rubber pad on each shoe between the end connectors.
        let pad = belt_band(phase, 0.16, 0.84, soft) * belt_band(across, 0.1, 0.9, soft);
        let connector = belt_band(phase, 0.0, 0.18, soft) + belt_band(phase, 0.82, 1.0, soft);
        let ends = 1.0 - belt_band(across, 0.1, 0.9, soft);
        color = mix(BELT_STEEL, BELT_RUBBER, pad);
        color = mix(color, BELT_POLISHED * 0.8, connector * ends);
        average = mix(BELT_STEEL, BELT_RUBBER, 0.55);
        roughness = mix(0.6, 0.9, pad);
        metalness = 0.25 * (1.0 - pad);
        relief = belt_band(phase, 0.1, 0.22, soft) - belt_band(phase, 0.78, 0.9, soft);
        dust = BELT_DUST_GROUND * (0.6 + 0.4 * pad);
    } else if face < 1.5 {
        // Running face: two polished wheel paths, the guide horns between them.
        let paths = belt_band(across, 0.12, 0.4, soft) + belt_band(across, 0.6, 0.88, soft);
        let horn = belt_band(across, 0.44, 0.56, soft) * belt_band(phase, 0.3, 0.7, soft);
        color = mix(BELT_STEEL, BELT_POLISHED, paths * 0.7);
        color = mix(color, BELT_POLISHED * 1.15, horn);
        average = mix(BELT_STEEL, BELT_POLISHED, 0.4);
        roughness = mix(0.62, 0.46, max(paths, horn));
        metalness = mix(0.25, 0.45, max(paths, horn));
    } else {
        // Edges: the rubber pad along the ground side, the shoe above it and the
        // end connector bridging each joint across the shoe.
        let pad = 1.0 - smoothstep(0.32 - soft, 0.32 + soft, across);
        let outer = face < 2.5;
        let connector = (belt_band(phase, 0.0, 0.2, soft) + belt_band(phase, 0.8, 1.0, soft))
            * belt_band(across, 0.22, 0.82, soft) * select(0.4, 1.0, outer);
        color = mix(BELT_STEEL, BELT_RUBBER, pad);
        color = mix(color, BELT_POLISHED, connector);
        average = mix(mix(BELT_STEEL, BELT_RUBBER, 0.32), BELT_POLISHED, select(0.12, 0.3, outer));
        roughness = mix(mix(0.62, 0.9, pad), 0.42, connector);
        metalness = mix(0.25 * (1.0 - pad), 0.5, connector);
        relief = (belt_band(phase, 0.76, 0.84, soft) - belt_band(phase, 0.16, 0.24, soft)) * select(0.0, 1.0, outer);
        dust = BELT_DUST_EDGE * (1.0 - smoothstep(0.0, 0.7, across));
    }
    color = mix(color, color * BELT_GAP, joint);
    color = mix(average, color, resolved) * f.vertex_color.rgb * f.tint.rgb;
    let grime = select(VEHICLE_BELT_DUST_STUDIO, 1.0, atmosphere_on());
    color = mix(color, vehicle_dust() * 0.7, dust * grime);
    let normal = normalize((*s).normal - tangent * relief * BELT_RELIEF * resolved);
    (*s).color = color;
    (*s).normal = normal;
    (*s).roughness = mix(roughness, VEHICLE_DIRT_ROUGHNESS, dust * grime);
    (*s).metalness = metalness * (1.0 - dust * grime);
    (*s).emissive += color * vehicle_gear_fill(normal) * material.params[1].w;
}

// Rivet rows along a plate's marked borders, from one edge channel: its value and
// its screen derivatives, and the paint UVs' (`d_uv` = (du/dx, du/dy, dv/dx,
// dv/dy)). The channel's gradient in UV space gives the direction along its
// border lines, and so a coordinate along them for the heads' spacing. Returns
// the strongest head coverage (x) and its dark ring (y).
fn vehicle_rivets(c: f32, dcx: f32, dcy: f32, uv: vec2f, d_uv: vec4f, metres_wide: f32, aa: f32) -> vec2f {
    let det = d_uv.x * d_uv.w - d_uv.z * d_uv.y;
    if abs(det) < 1e-20 {
        return vec2f(0.0);
    }
    let g = vec2f(dcx * d_uv.w - d_uv.z * dcy, d_uv.x * dcy - dcx * d_uv.y) / det;
    let g_length = length(g);
    if g_length < 1e-6 {
        return vec2f(0.0);
    }
    let state = floor(c);
    let far = floor(state / 3.0);
    let near = state - far * 3.0;
    let t = clamp((c - state - VEHICLE_EDGE_MARGIN) / VEHICLE_EDGE_SPAN, 0.0, 1.0);
    let along = dot(uv, vec2f(-g.y, g.x) / g_length) * VEHICLE_METRES_PER_UV;
    let q = (fract(along / VEHICLE_RIVET_PITCH) - 0.5) * VEHICLE_RIVET_PITCH;
    var head = 0.0;
    var ring = 0.0;
    for (var k = 0; k < 2; k++) {
        let kind = select(near, far, k == 1);
        let distance = select(t, 1.0 - t, k == 1) * metres_wide;
        if kind > 0.5 {
            let r = length(vec2f(q, distance - VEHICLE_RIVET_INSET));
            head = max(head, 1.0 - smoothstep(VEHICLE_RIVET_RADIUS - aa, VEHICLE_RIVET_RADIUS + aa, r));
            ring = max(ring, 1.0 - smoothstep(VEHICLE_RIVET_RADIUS, VEHICLE_RIVET_RADIUS + VEHICLE_RIVET_RING + aa, r));
        }
    }
    return vec2f(head, ring);
}

// A welded seam's groove (x) and the weld lip beside it (y) at `offset` metres
// from the seam's centre line.
fn vehicle_seam(offset: f32, aa: f32) -> vec2f {
    let groove = 1.0 - smoothstep(VEHICLE_SEAM_WIDTH * 0.5, VEHICLE_SEAM_WIDTH * 0.5 + aa, abs(offset));
    let lip_centre = abs(offset - (VEHICLE_SEAM_WIDTH * 0.5 + VEHICLE_SEAM_LIP * 0.5));
    let lip = 1.0 - smoothstep(VEHICLE_SEAM_LIP * 0.5, VEHICLE_SEAM_LIP * 0.5 + aa, lip_centre);
    return vec2f(groove, lip);
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    // Derivatives and the tile sample first, in uniform control flow.
    let tile = textureSample(extra_texture0, extra_sampler0, f.uv * material.params[2].w);
    let dpx = dpdx(f.world);
    let dpy = dpdy(f.world);
    let dcx = dpdx(f.vertex_color.rg);
    let dcy = dpdy(f.vertex_color.rg);
    let dux = dpdx(f.uv.x);
    let duy = dpdy(f.uv.x);
    let dvx = dpdx(f.uv.y);
    let dvy = dpdy(f.uv.y);
    (*s).opacity = f.tint.a;
    if material.params[3].w > 2.5 {
        // The belt's tangent: the world direction in which its link count grows.
        let n = cross(dpx, dpy);
        let det = dot(dpx, cross(dpy, n));
        let gradient = (cross(dpy, n) * dux + cross(n, dpx) * duy) * select(0.0, 1.0 / det, abs(det) > 1e-30);
        let tangent = gradient / max(length(gradient), 1e-6);
        vehicle_belt(s, f, tangent, max(abs(dux), abs(duy)));
        return;
    }
    if material.params[3].w > 1.5 {
        vehicle_glass(s, f);
        return;
    }
    if material.params[3].w < 0.5 {
        // Running gear: its own colors, lifted out of the hull's shadow.
        (*s).emissive += (*s).color * vehicle_gear_fill((*s).normal) * material.params[1].w;
        return;
    }
    // The shadow pass has no vertex colors and needs no paint.
    if f.vertex_color.a > 0.95 {
        return;
    }

    // A livery's own finish: its material roughness over the tanks' enamel
    // (`PAINT_ROUGHNESS` in Rust) shifts every paint roughness, so the soft-skinned
    // Humvee's flatter paint reads apart from the tanks' satin.
    let finish = (*s).roughness - VEHICLE_ENAMEL_ROUGHNESS;
    // Bolted plates, rivets and tread plate belong to armor: the soft-skinned
    // Humvee's flatter paint (a rougher finish) keeps smooth panels.
    let armor = step(finish, VEHICLE_ARMOR_FINISH);
    let coat = floor(f.vertex_color.b);
    let code = f.vertex_color.b - coat;
    let team = step(0.5, code);
    let value = (code - 0.5 * team) * 2.0 * VEHICLE_VALUE_RANGE;
    let paint_rgb = material.params[0].rgb;
    let paint_luma = dot(paint_rgb, vec3f(0.2126, 0.7152, 0.0722));
    // Studio cards light the paint with a strong key and rim under ACES, whose
    // shoulder turns a bright red pink: they show it a step deeper.
    let round_paint = mix(vec3f(paint_luma), paint_rgb, select(VEHICLE_STUDIO_SATURATION, VEHICLE_ROUND_SATURATION, atmosphere_on()))
        * select(VEHICLE_STUDIO_VALUE, 1.0, atmosphere_on());
    let lit_base = mix(material.params[1].rgb, round_paint, team) * value * f.tint.rgb;
    let lift_saturation = 1.0 + VEHICLE_LIFT_SATURATION * max(value - 1.0, 0.0) * team;
    let base = max(mix(vec3f(dot(lit_base, vec3f(0.2126, 0.7152, 0.0722))), lit_base, lift_saturation), vec3f(0.0));
    let chamfer = step(0.55, f.vertex_color.a);
    let low = clamp((f.vertex_color.a - 0.2 - 0.4 * chamfer) / 0.3, 0.0, 1.0);
    let grime = tile.r;
    let wear = tile.a;

    // The plate's borders from the edge channels' gradients across the surface.
    let n = cross(dpx, dpy);
    let r1 = cross(dpy, n);
    let r2 = cross(n, dpx);
    let det = dot(dpx, r1);
    let inverse = select(0.0, 1.0 / det, abs(det) > 1e-30);
    let gradient_u = (r1 * dcx.x + r2 * dcy.x) * inverse;
    let gradient_v = (r1 * dcx.y + r2 * dcy.y) * inverse;
    let pixel = max(length(dpx), length(dpy));
    let bevel_width = clamp(pixel * VEHICLE_BEVEL_PIXELS, VEHICLE_BEVEL_MIN, VEHICLE_BEVEL_MAX);
    var edges: VehicleEdges;
    edges.nearest = 1e3;
    edges.narrowest = 1e3;
    vehicle_channel(f.vertex_color.r, gradient_u, bevel_width, &edges);
    vehicle_channel(f.vertex_color.g, gradient_v, bevel_width, &edges);
    let bevel = edges.bevel * (1.0 - chamfer);
    let normal = normalize((*s).normal + edges.roll * (1.0 - chamfer) * VEHICLE_BEVEL_ROLL);
    let metres = f.uv * VEHICLE_METRES_PER_UV;
    let paint_fill = vehicle_fill(normal) * material.params[1].w;

    if coat > 1.5 {
        // A lamp lens: hot in its middle, falling off to the reflector ring.
        let core = smoothstep(0.15, 0.9, clamp(2.0 * edges.nearest / max(edges.narrowest, 1e-4), 0.0, 1.0));
        let lens_normal = normalize((*s).normal);
        let sun_level = dot(frame.sun_color.rgb, vec3f(0.2126, 0.7152, 0.0722));
        let burning = mix(1.0, VEHICLE_LAMP_DAY_GLOW, smoothstep(VEHICLE_LAMP_DUSK, VEHICLE_LAMP_DAY, sun_level));
        let facing = clamp(dot(lens_normal, f.view_direction), 0.0, 1.0);
        let mirror = VEHICLE_LAMP_MIRROR * (0.3 + 0.7 * pow(1.0 - facing, 2.0)) * core;
        (*s).color = mix(VEHICLE_LAMP_RIM, VEHICLE_LAMP_GLASS, core) * f.tint.rgb;
        (*s).roughness = 0.14;
        (*s).metalness = 0.6 * core;
        (*s).normal = lens_normal;
        (*s).emissive += (VEHICLE_LAMP_GLOW * mix(VEHICLE_LAMP_GLOW_RIM, 1.0, core * core) * burning
            + vehicle_sky(reflect(-f.view_direction, lens_normal)) * mirror) * f.tint.rgb;
        return;
    }
    if coat > 0.5 {
        // A marking: stencil white flaking to the team paint, under the map's dust.
        let rubbed = smoothstep(0.25, 0.75, value_noise(metres * VEHICLE_CHIP_CLUSTER + 17.9));
        let exposure = VEHICLE_MARKING_WEAR + VEHICLE_MARKING_WEAR_CLUSTER * rubbed;
        let marking_resolved = 1.0 - smoothstep(VEHICLE_CHIP_PIXEL_FROM, VEHICLE_CHIP_PIXEL_TO, pixel);
        // Far away the flaking averages out to a slightly duller white.
        let flaked = mix(exposure * 0.5, 1.0 - smoothstep(exposure - 0.04, exposure, wear), marking_resolved);
        let white = VEHICLE_MARKING * value * (0.9 + 0.2 * grime) * f.tint.rgb;
        var color = mix(white, round_paint * value * f.tint.rgb, flaked);
        let dusted = VEHICLE_MARKING_DUST * select(VEHICLE_DUST_STUDIO, 1.0, atmosphere_on()) * (0.6 + 0.8 * grime);
        color = mix(color, vehicle_dust(), dusted);
        (*s).color = color;
        (*s).roughness = clamp(VEHICLE_MARKING_ROUGHNESS + finish, 0.0, 1.0);
        (*s).metalness = 0.0;
        (*s).normal = normalize((*s).normal);
        (*s).emissive += color * paint_fill;
        return;
    }

    // Clean paint: only a whisper of the grime field's drift on open panels, each
    // panel a shade lighter in its middle than along its borders, so plates read
    // as separate pieces of armor under flat light.
    // The border shading fades out as pixels grow, so from the overhead camera a
    // hull keeps its full, clean team color instead of darkening plate by plate.
    let panel_reach = clamp(edges.narrowest * VEHICLE_PANEL_REACH, VEHICLE_PANEL_MIN, VEHICLE_PANEL_MAX);
    let panel_resolved = 1.0 - smoothstep(VEHICLE_PANEL_PIXEL_FROM, VEHICLE_PANEL_PIXEL_TO, pixel);
    let panel = mix(1.0, mix(VEHICLE_PANEL_BORDER, 1.0, smoothstep(0.0, panel_reach, edges.nearest)), panel_resolved);
    let fade = value_noise(metres * VEHICLE_FADE_CELLS) * 0.65 + value_noise(metres * VEHICLE_FADE_CELLS * 2.7 + 3.1) * 0.35;
    var color = base * (1.0 + (fade - 0.5) * 2.0 * VEHICLE_FADE_VALUE) * panel;
    // Rubbed edges: a lighter band on each bevel and over chamfer strips.
    let roll_length = length(edges.roll);
    let roll_direction = edges.roll / max(roll_length, 1e-4);
    let edge_light = select(0.0, clamp(max(dot(roll_direction, frame.sun_direction.xyz), roll_direction.y), 0.0, 1.0), roll_length > 1e-4);
    let lit_resolved = 1.0 - smoothstep(VEHICLE_BEVEL_LIT_PIXEL_FROM, VEHICLE_BEVEL_LIT_PIXEL_TO, pixel);
    let lifted = color * mix(VEHICLE_BEVEL_LIFT, VEHICLE_BEVEL_LIFT_LIT, edge_light * lit_resolved) + VEHICLE_BEVEL_PALE;
    color = mix(color, lifted, max(bevel, chamfer * 0.45) * VEHICLE_BEVEL_TINT);
    // Worn paint along convex edges of broad plates, in broken runs.
    let rub_resolved = 1.0 - smoothstep(VEHICLE_RUB_PIXEL_FROM, VEHICLE_RUB_PIXEL_TO, pixel);
    let rub_plate = smoothstep(VEHICLE_CHIP_PLATE_FROM, VEHICLE_CHIP_PLATE_TO, edges.narrowest);
    let rub_field = value_noise(metres * VEHICLE_RUB_CELLS + 9.1) * 0.65 + grime * 0.35;
    let rub = edges.near_edge * edges.near_edge * smoothstep(0.42, 0.58, rub_field) * select(0.3, 1.0, atmosphere_on())
        * rub_resolved * rub_plate * (1.0 - chamfer) * VEHICLE_RUB_STRENGTH;
    let rub_grey = vec3f(dot(color, vec3f(0.2126, 0.7152, 0.0722)));
    color = mix(color, mix(color, rub_grey, VEHICLE_RUB_GREY) * VEHICLE_RUB_LIFT + VEHICLE_BEVEL_PALE * 0.5, rub);
    // Grime settled into recesses, patchy along their length.
    let recess = clamp(edges.recess * (0.75 + (0.5 - grime) * 0.9), 0.0, 1.0);
    color = mix(color, color * VEHICLE_GRIME, recess * VEHICLE_GRIME_STRENGTH);

    // Rivet heads in rows just inside the borders of broad plates, close up only: a
    // bright domed head in a dark ring of shadow, so a plate reads as bolted armor.
    let rivet_resolved = (1.0 - smoothstep(VEHICLE_RIVET_PIXEL_FROM, VEHICLE_RIVET_PIXEL_TO, pixel))
        * smoothstep(VEHICLE_RIVET_MIN_PLATE, VEHICLE_RIVET_MIN_PLATE * 1.5, edges.narrowest) * (1.0 - chamfer) * armor;
    var rivet = vec2f(0.0);
    if rivet_resolved > 0.0 {
        let d_uv = vec4f(dux, duy, dvx, dvy);
        let aa = max(pixel * 0.75, 1e-4);
        let gu = length(gradient_u);
        let gv = length(gradient_v);
        if gu > 1e-6 {
            rivet = max(rivet, vehicle_rivets(f.vertex_color.r, dcx.x, dcy.x, f.uv, d_uv, VEHICLE_EDGE_SPAN / gu, aa));
        }
        if gv > 1e-6 {
            rivet = max(rivet, vehicle_rivets(f.vertex_color.g, dcx.y, dcy.y, f.uv, d_uv, VEHICLE_EDGE_SPAN / gv, aa));
        }
        rivet *= rivet_resolved;
    }
    color = color * (1.0 - (rivet.y - rivet.x) * 0.55) * (1.0 + rivet.x * 0.45);

    // Welded seams cut broad plates into panels: across decks, and upright on the
    // sides and faces, so a deck seam turns the corner down the side. A dark groove
    // with a lighter weld lip beside it.
    let seam_resolved = (1.0 - smoothstep(VEHICLE_SEAM_PIXEL_FROM, VEHICLE_SEAM_PIXEL_TO, pixel))
        * step(VEHICLE_SEAM_MIN_PLATE, edges.narrowest) * step(edges.narrowest, 100.0)
        * (1.0 - chamfer) * armor;
    var seam = 0.0;
    var lip = 0.0;
    var seam_rivet = vec2f(0.0);
    var panel_shift = 0.0;
    if seam_resolved > 0.0 {
        // Paint UVs run along the model's x and z on decks and across upright faces,
        // with v up the upright ones: v on a deck and u on an upright face cross it.
        let on_deck = (*s).normal.y > 0.6;
        let across = select(metres.x, metres.y, on_deck);
        let along = select(metres.y, metres.x, on_deck);
        let aa = max(pixel, 1e-4);
        let offset = (fract(across / VEHICLE_SEAM_SPACING) - 0.5) * VEHICLE_SEAM_SPACING;
        var groove = vehicle_seam(offset, aa);
        var long_offset = 1e3;
        if on_deck {
            long_offset = (fract(along / VEHICLE_SEAM_LONG) - 0.5) * VEHICLE_SEAM_LONG;
            groove = max(groove, vehicle_seam(long_offset, aa));
        }
        seam = groove.x * seam_resolved;
        lip = groove.y * seam_resolved;
        // Rivet heads in a row either side of each seam.
        let rivet_set = VEHICLE_SEAM_WIDTH * 0.5 + VEHICLE_SEAM_RIVET_SET;
        let q_along = (fract(along / VEHICLE_SEAM_RIVET_PITCH) - 0.5) * VEHICLE_SEAM_RIVET_PITCH;
        var r = length(vec2f(q_along, abs(offset) - rivet_set));
        if on_deck {
            let q_across = (fract(across / VEHICLE_SEAM_RIVET_PITCH) - 0.5) * VEHICLE_SEAM_RIVET_PITCH;
            r = min(r, length(vec2f(q_across, abs(long_offset) - rivet_set)));
        }
        seam_rivet = vec2f(
            1.0 - smoothstep(VEHICLE_RIVET_RADIUS - aa, VEHICLE_RIVET_RADIUS + aa, r),
            1.0 - smoothstep(VEHICLE_RIVET_RADIUS, VEHICLE_RIVET_RADIUS + VEHICLE_RIVET_RING + aa, r),
        ) * seam_resolved;
        // Each panel a shade apart; the step hides in the seam's groove.
        let cell = vec2i(floor(vec2f(across / VEHICLE_SEAM_SPACING, select(0.0, along / VEHICLE_SEAM_LONG, on_deck))));
        panel_shift = (lattice_hash(cell + vec2i(37, 91)) - 0.5) * 2.0 * VEHICLE_PANEL_SHIFT * seam_resolved;
    }
    color = color * (1.0 - seam * 0.65) * (1.0 + lip * 0.3) * (1.0 + panel_shift);
    color = color * (1.0 - (seam_rivet.y - seam_rivet.x) * 0.55) * (1.0 + seam_rivet.x * 0.45);

    // Non-skid grain on decks close up.
    let deck = smoothstep(0.55, 0.8, (*s).normal.y);
    let grain_resolved = (1.0 - smoothstep(VEHICLE_GRAIN_PIXEL_FROM, VEHICLE_GRAIN_PIXEL_TO, pixel)) * deck * (1.0 - chamfer);
    var grain = 0.5;
    if grain_resolved > 0.0 {
        grain = value_noise(metres * VEHICLE_GRAIN_CELLS) * 0.6 + value_noise(metres * VEHICLE_GRAIN_CELLS * 2.3 + 1.7) * 0.4;
    }
    color *= 1.0 + (grain - 0.5) * 2.0 * VEHICLE_GRAIN_VALUE * grain_resolved;

    // Tread plate on the trim's decks: each lozenge a scuffed steel top in a ring of
    // shadow, so a fender reads as a steel walkway rather than painted card.
    let tread_resolved = (1.0 - smoothstep(VEHICLE_TREAD_PIXEL_FROM, VEHICLE_TREAD_PIXEL_TO, pixel)) * deck * (1.0 - team) * (1.0 - chamfer) * armor;
    var lug = 0.0;
    var lug_ring = 0.0;
    if tread_resolved > 0.0 {
        let p = metres / VEHICLE_TREAD_CELL;
        let cell = floor(p);
        let local = fract(p) - 0.5;
        let odd = fract((cell.x + cell.y) * 0.5) > 0.25;
        let axis = select(vec2f(0.7071, 0.7071), vec2f(0.7071, -0.7071), odd);
        let d = length(vec2f(dot(local, axis) / VEHICLE_TREAD_LENGTH, dot(local, vec2f(-axis.y, axis.x)) / VEHICLE_TREAD_WIDTH));
        let soft = clamp(pixel / (VEHICLE_TREAD_CELL * VEHICLE_TREAD_WIDTH), 0.05, 1.0);
        lug = (1.0 - smoothstep(1.0 - soft, 1.0 + soft, d)) * tread_resolved;
        lug_ring = (1.0 - smoothstep(1.0, 1.0 + 0.6 + soft, d)) * (1.0 - lug) * tread_resolved;
    }
    color *= 1.0 - lug_ring * 0.4;
    color = mix(color, material.params[3].rgb * (0.7 + 0.5 * grime), lug * VEHICLE_TREAD_WEAR);

    // Rain streaks: thin dark runs down upright plates, broken along their length,
    // fading where a pixel grows past their width so far hulls stay clean.
    let upright = 1.0 - smoothstep(0.3, 0.6, abs((*s).normal.y));
    let streak_resolved = 1.0 - smoothstep(VEHICLE_STREAK_PIXEL_FROM, VEHICLE_STREAK_PIXEL_TO, pixel);
    let column = value_noise(vec2f(f.uv.x * VEHICLE_STREAK_ACROSS, f.uv.y * VEHICLE_STREAK_ALONG));
    let run = value_noise(vec2f(f.uv.x * VEHICLE_STREAK_ACROSS * 0.21 + 7.3, f.uv.y * VEHICLE_STREAK_ALONG * 3.0));
    let streak = smoothstep(0.58, 0.92, column) * smoothstep(0.35, 0.75, run) * upright * streak_resolved;
    color = mix(color, color * VEHICLE_GRIME, streak * VEHICLE_STREAK_DARKEN / (1.0 - VEHICLE_GRIME.g));

    // Dust and mud thrown up by the tracks, following the hull's shape rather than
    // a flat line: a splashed band along the lowest plates, drifts in the recesses
    // and on the ledges low on the hull (fender tops, skirt lips), and clean paint
    // on the open upright panels above. Studio scenes (the menu cards) show the
    // paint clean, a tank fresh from the depot: on a card's small, evenly lit hull
    // the splash band read as stains.
    let studio = select(VEHICLE_DUST_STUDIO, 1.0, atmosphere_on());
    let dust_strength = studio * VEHICLE_DUST_STRENGTH;
    let ledge = smoothstep(0.55, 0.85, (*s).normal.y) * smoothstep(0.3, 0.7, low);
    // The splash line rises and falls along the hull and breaks up with the grime
    // field, with a crisp edge: caked earth below it and only a thin film of fine
    // dust above, so the band reads as thrown dirt rather than as faded paint.
    let splash = value_noise(metres * vec2f(VEHICLE_SPLASH_CELLS, VEHICLE_SPLASH_CELLS * 0.4) + 11.3);
    let fray = value_noise(metres * VEHICLE_SPLASH_FRAY_CELLS + 23.1);
    let line = low + (grime - 0.5) * VEHICLE_SPLASH_GRIME + (splash - 0.5) * VEHICLE_SPLASH_RISE
        + (fray - 0.5) * VEHICLE_SPLASH_FRAY;
    let caked = smoothstep(VEHICLE_SPLASH_EDGE, VEHICLE_SPLASH_EDGE + 0.04, line);
    // Thrown dirt cakes the upright plates; a ledge only catches a lighter coat.
    let band = max(caked * (0.82 + 0.18 * smoothstep(0.5, 0.8, line)) * mix(0.4, 1.0, upright), smoothstep(-0.05, 0.2, line) * VEHICLE_DUST_FILM * select(0.0, 1.0, atmosphere_on()));
    // Spatter: drops of mud flung above the caked line, thinning out with height.
    let spatter_resolved = 1.0 - smoothstep(VEHICLE_SPATTER_PIXEL_FROM, VEHICLE_SPATTER_PIXEL_TO, pixel);
    let above = smoothstep(VEHICLE_SPLASH_EDGE - VEHICLE_SPATTER_REACH, VEHICLE_SPLASH_EDGE, line) * (1.0 - caked);
    let drops = value_noise(metres * VEHICLE_SPATTER_CELLS + 41.3) * 0.75 + value_noise(metres * VEHICLE_SPATTER_CELLS * 2.6 + 7.7) * 0.25;
    let spatter = smoothstep(0.86 - 0.22 * above, 0.9 - 0.22 * above, drops) * above * spatter_resolved * upright;
    let drifted = edges.recess * smoothstep(0.08, 0.5, low) * (0.6 + 0.8 * grime);
    // Ledges low on the hull (fender tops, skirt lips) hold dust in drifts and
    // swept patches, not as one even coat that turns a fender into a brown slab.
    let patches = value_noise(metres * VEHICLE_DECK_DUST_CELLS + 5.7) * 0.6
        + value_noise(metres * VEHICLE_DECK_DUST_CELLS * 2.9 + 2.3) * 0.4;
    let ledge_dust = ledge * (0.08 + 0.5 * smoothstep(0.45, 0.65, patches * 0.7 + grime * 0.3));
    let dusty = clamp(max(max(band, spatter * 0.9), max(drifted, ledge_dust)), 0.0, 1.0) * dust_strength;
    let muddy = smoothstep(0.74, 0.94, low + (grime - 0.5) * 0.3) * VEHICLE_MUD_STRENGTH * studio;
    let dust = vehicle_dust();
    color = mix(color, dust * (0.8 + 0.4 * grime), dusty);
    color = mix(color, VEHICLE_MUD, muddy);
    // Road film: a soft gradient of the map's dust down every upright plate toward
    // the tracks, so the lower hull reads earthier and lighter rather than crushing
    // to black in the shade of the fenders.
    let road = smoothstep(0.05, 0.65, low) * VEHICLE_ROAD_FILM * upright * select(0.0, 1.0, atmosphere_on());
    color = mix(color, dust * (0.75 + 0.35 * grime), road);
    // Dust settled on the decks: a patchy film on broad plates, drifted into
    // recesses, never on the bevels the crew's boots and the wind keep clean. The
    // film resolves only close up: from the overhead camera and on the menu cards
    // its patches would read as camouflage blotches over the team color.
    let broad = smoothstep(VEHICLE_DECK_DUST_MIN_PLATE, VEHICLE_DECK_DUST_MIN_PLATE * 2.0, edges.narrowest);
    // Studio scenes (the menu cards) show the paint showroom clean.
    let film = VEHICLE_DECK_DUST_FILM * broad * select(0.0, 1.0, atmosphere_on())
        * (1.0 - smoothstep(VEHICLE_DECK_DUST_PIXEL_FROM, VEHICLE_DECK_DUST_PIXEL_TO, pixel));
    // A fine, even film broken up only at the grain of blown sand, thicker where it
    // drifts against the base of a wall: no broad blotches to read as camouflage.
    let sand = value_noise(metres * VEHICLE_DECK_DUST_GRAIN + 3.3) * 0.7 + grime * 0.3;
    let settled = deck * (1.0 - chamfer) * (1.0 - bevel)
        * (film * (0.7 + 0.3 * sand) + VEHICLE_DECK_DUST_DRIFT * recess * (0.55 + 0.45 * sand));
    color = mix(color, dust * (0.95 + 0.3 * grime), settled);
    let dirt = max(max(max(dusty, muddy), settled * 0.8), road);

    // Chips to bare steel along convex edges of broad plates (a few low on the hull),
    // where the chip field dips under the spot's exposure, with a darker rim of
    // lifted paint.
    let resolved = 1.0 - smoothstep(VEHICLE_CHIP_PIXEL_FROM, VEHICLE_CHIP_PIXEL_TO, pixel);
    let plate = smoothstep(VEHICLE_CHIP_PLATE_FROM, VEHICLE_CHIP_PLATE_TO, edges.narrowest);
    // Wear clusters where hands, boots and branches rub: a broad field gathers the
    // chips into worn stretches and leaves others intact, so they read as history
    // rather than as one stamped size of fleck along every edge.
    let rubbed = smoothstep(0.3, 0.8, value_noise(metres * VEHICLE_CHIP_CLUSTER + 17.9));
    let edge_wear = edges.near_edge * edges.near_edge * VEHICLE_CHIP_EXPOSURE * (0.3 + 1.2 * rubbed)
        + edges.corner * VEHICLE_CHIP_CORNER * (0.5 + 0.8 * rubbed);
    // Studio cards (the menu) show a tank fresh from the depot: only the hardest-worn
    // corners have chipped.
    let worn = select(VEHICLE_CHIP_STUDIO, 1.0, atmosphere_on());
    let exposure = (edge_wear + low * 0.04) * (1.0 - dirt * 0.6) * resolved * plate * worn;
    let chip = 1.0 - smoothstep(exposure - 0.015, exposure, wear);
    // The lip only rings real chips: where nothing is exposed it would light the
    // chip field's lowest spots into specks over the whole panel.
    let chip_lip = (1.0 - smoothstep(exposure, exposure + VEHICLE_CHIP_LIP, wear)) * (1.0 - chip)
        * smoothstep(0.0, 0.05, exposure);
    color = mix(color, material.params[3].rgb * (0.85 + 0.4 * grime), chip_lip * 0.85);
    color = mix(color, VEHICLE_CHIP_CORE * (0.8 + 0.5 * grime), chip);

    // The sheen breaks up a little with the grime field, so a panel's highlight
    // reads as painted steel rather than moulded plastic.
    var roughness = mix(VEHICLE_PAINT_ROUGHNESS, VEHICLE_DECK_ROUGHNESS, deck)
        + (0.5 - grime) * VEHICLE_SHEEN_BREAKUP * (1.0 - deck) + streak * 0.08
        + (grain - 0.5) * 0.08 * grain_resolved + (fade - 0.5) * 2.0 * VEHICLE_ROUGHNESS_DRIFT;
    roughness = mix(roughness, VEHICLE_DIRT_ROUGHNESS, max(dirt, recess * 0.6));
    roughness = mix(roughness, VEHICLE_STEEL_ROUGHNESS, max(max(chip, chip_lip), lug * VEHICLE_TREAD_WEAR));
    (*s).roughness = clamp(roughness + finish, 0.0, 1.0);
    (*s).metalness = mix((*s).metalness, VEHICLE_STEEL_METALNESS, max(chip_lip * (1.0 - chip), lug * VEHICLE_TREAD_WEAR));
    (*s).normal = normal;
    (*s).color = color;

    // The paint's own fill, and its team rim at grazing angles, strongest on the
    // side turned from the sun and on bevels that roll toward the view's edge.
    let paint = 1.0 - chip;
    (*s).emissive += color * paint_fill;
    let bounce_facing = 1.0 - smoothstep(-0.2, 0.6, normal.y);
    let bounce = VEHICLE_BOUNCE * select(VEHICLE_BOUNCE_STUDIO, 1.0, atmosphere_on()) * bounce_facing * (0.35 + 0.65 * low);
    (*s).emissive += color * frame.ground_color.rgb * bounce;
    let grazing = 1.0 - clamp(dot(normal, f.view_direction), 0.0, 1.0);
    let away = 1.0 - clamp(dot(normal, frame.sun_direction.xyz), 0.0, 1.0);
    // Only upright plates take it: decks seen at a grazing angle from a low camera
    // would wash the whole roof toward the rim's cool white.
    let side = 1.0 - smoothstep(0.45, 0.8, normal.y);
    let rim = grazing * grazing * (0.45 + 0.55 * away) * side * material.params[0].w * (1.0 - dirt) * paint;
    let rim_hue = mix(material.params[2].rgb, VEHICLE_RIM_COOL, select(0.0, VEHICLE_RIM_COOL_SHARE, atmosphere_on()));
    (*s).emissive += rim_hue * value * rim * VEHICLE_RIM_GLOW;
}

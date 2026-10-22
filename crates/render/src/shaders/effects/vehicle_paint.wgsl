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
//   each upright plate, darker at the ground line;
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
const VEHICLE_BEVEL_PIXELS: f32 = 3.2;
const VEHICLE_BEVEL_MIN: f32 = 0.016;
const VEHICLE_BEVEL_MAX: f32 = 0.12;
const VEHICLE_BEVEL_ROLL: f32 = 0.3;
// Panel shading: paint along a plate's borders is this much darker, fading out over
// this fraction of the plate's narrowest width (within these metres).
const VEHICLE_PANEL_BORDER: f32 = 0.66;
const VEHICLE_PANEL_REACH: f32 = 0.3;
const VEHICLE_PANEL_MIN: f32 = 0.03;
const VEHICLE_PANEL_MAX: f32 = 0.35;
const VEHICLE_PANEL_PIXEL_FROM: f32 = 0.018;
const VEHICLE_PANEL_PIXEL_TO: f32 = 0.045;
// How far from a convex edge chips reach, and from a recess its grime (metres).
const VEHICLE_CHIP_REACH: f32 = 0.075;
const VEHICLE_RECESS_REACH: f32 = 0.12;
// Chips fade out as a pixel grows past their size, so distant hulls never sparkle.
const VEHICLE_CHIP_PIXEL_FROM: f32 = 0.006;
const VEHICLE_CHIP_PIXEL_TO: f32 = 0.017;
// Plates narrower than these (metres) never chip: bolt heads, latches and trim
// strips would otherwise scatter grey specks evenly over every deck.
const VEHICLE_CHIP_PLATE_FROM: f32 = 0.05;
const VEHICLE_CHIP_PLATE_TO: f32 = 0.11;
// Paint rubbed lighter on a bevel and on chamfer strips.
const VEHICLE_BEVEL_LIFT: f32 = 2.1;
const VEHICLE_BEVEL_PALE: vec3f = vec3f(0.03, 0.022, 0.018);
const VEHICLE_BEVEL_TINT: f32 = 0.85;
// Dust and wet mud (linear) and how strongly they cover the lowest armor. In a
// round (the atmosphere on) the dust takes this share of its hue from the map's
// ground light (sand in the quarry, brown earth in the village, grey concrete at
// the harbor) at its own brightness; studio scenes keep the plain dust.
const VEHICLE_DUST: vec3f = vec3f(0.12, 0.092, 0.06);
const VEHICLE_DUST_LOCAL: f32 = 0.6;
const VEHICLE_MUD: vec3f = vec3f(0.075, 0.053, 0.034);
const VEHICLE_DUST_STRENGTH: f32 = 0.82;
const VEHICLE_DUST_STUDIO: f32 = 0.0;
const VEHICLE_BELT_DUST_STUDIO: f32 = 0.6;
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
const VEHICLE_DECK_ROUGHNESS: f32 = 0.64;
const VEHICLE_SHEEN_BREAKUP: f32 = 0.1;
const VEHICLE_DIRT_ROUGHNESS: f32 = 0.92;
const VEHICLE_STEEL_ROUGHNESS: f32 = 0.48;
const VEHICLE_STEEL_METALNESS: f32 = 0.45;
// The team rim's brightness at a full-strength silhouette.
const VEHICLE_RIM_GLOW: f32 = 0.4;
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
const VEHICLE_SEAM_SPACING: f32 = 0.6;
const VEHICLE_SEAM_MIN_PLATE: f32 = 0.4;
const VEHICLE_SEAM_WIDTH: f32 = 0.01;
const VEHICLE_SEAM_LIP: f32 = 0.014;
const VEHICLE_SEAM_PIXEL_FROM: f32 = 0.012;
const VEHICLE_SEAM_PIXEL_TO: f32 = 0.028;
// Non-skid coating on decks: a grain this many cells per metre, varying the paint
// by this much either way, resolved only close up (the turret roof seen from the
// commander's hatch).
const VEHICLE_GRAIN_CELLS: f32 = 55.0;
const VEHICLE_GRAIN_VALUE: f32 = 0.34;
const VEHICLE_GRAIN_PIXEL_FROM: f32 = 0.003;
const VEHICLE_GRAIN_PIXEL_TO: f32 = 0.009;
// Dust settled on decks: a thin film over broad upward plates (patchy at this many
// cells per metre, on plates at least this wide, resolved while a pixel is smaller
// than these sizes), drifted thicker into recesses, in the map's ground color where
// the atmosphere is on.
const VEHICLE_DECK_DUST_CELLS: f32 = 1.6;
const VEHICLE_DECK_DUST_FILM: f32 = 0.7;
const VEHICLE_DECK_DUST_DRIFT: f32 = 0.6;
const VEHICLE_DECK_DUST_MIN_PLATE: f32 = 0.25;
const VEHICLE_DECK_DUST_PIXEL_FROM: f32 = 0.012;
const VEHICLE_DECK_DUST_PIXEL_TO: f32 = 0.03;
// The track splash's line: this many waves per metre along the hull, rising and
// falling by this much of the dusty band.
const VEHICLE_SPLASH_CELLS: f32 = 1.3;
const VEHICLE_SPLASH_RISE: f32 = 0.4;
// Where the caked band's crisp edge sits on the band (0 its top, 1 the ground),
// and the fine dust film over the paint just above it.
const VEHICLE_SPLASH_EDGE: f32 = 0.2;
const VEHICLE_DUST_FILM: f32 = 0.2;
// Edge wear: how far toward a convex edge the chip field is exposed.
const VEHICLE_CHIP_EXPOSURE: f32 = 0.5;
// Cells per metre of the field that gathers chips into worn stretches.
const VEHICLE_CHIP_CLUSTER: f32 = 2.2;
// Soft, sun-faded variation across the paint, this many cells per metre and this
// much either way: broad enough never to read as speckle.
const VEHICLE_FADE_CELLS: f32 = 2.4;
const VEHICLE_FADE_VALUE: f32 = 0.09;
// The turret roof's bleach (in a round, under the saturating grade; studio cards
// keep the clean color): paint values from these (the turret's roof and its
// highest decks; hull decks stay below) lose this share of their saturation
// toward a warm grey and lighten by this much.
const VEHICLE_ROOF_FROM: f32 = 0.95;
const VEHICLE_ROOF_TO: f32 = 1.08;
const VEHICLE_ROOF_BLEACH: f32 = 0.12;
const VEHICLE_ROOF_LIFT: f32 = 0.12;
const VEHICLE_ROOF_TINT: vec3f = vec3f(1.1, 0.96, 0.9);

struct VehicleEdges {
    // 1 on a convex edge, fading out over the chip reach.
    near_edge: f32,
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
        (*e).near_edge = max((*e).near_edge, 1.0 - smoothstep(0.0, VEHICLE_CHIP_REACH, distance));
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

// The track belt: dark worn steel shoes with rubber pads on the ground face, a
// polished path where the twin road wheels run either side of the guide horns on
// the running face, and bright end connectors bridging each joint on the edges.
// Joints are dark gaps; the pattern fades to its average where a link is only a
// few pixels long, so a distant track never shimmers.
const BELT_STEEL: vec3f = vec3f(0.062, 0.059, 0.054);
const BELT_POLISHED: vec3f = vec3f(0.2, 0.19, 0.172);
const BELT_RUBBER: vec3f = vec3f(0.016, 0.016, 0.017);
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
    var metalness = 0.4;
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
        metalness = 0.4 * (1.0 - pad);
        relief = belt_band(phase, 0.1, 0.22, soft) - belt_band(phase, 0.78, 0.9, soft);
        dust = BELT_DUST_GROUND * (0.6 + 0.4 * pad);
    } else if face < 1.5 {
        // Running face: two polished wheel paths, the guide horns between them.
        let paths = belt_band(across, 0.12, 0.4, soft) + belt_band(across, 0.6, 0.88, soft);
        let horn = belt_band(across, 0.44, 0.56, soft) * belt_band(phase, 0.3, 0.7, soft);
        color = mix(BELT_STEEL, BELT_POLISHED, paths * 0.85);
        color = mix(color, BELT_POLISHED * 1.15, horn);
        average = mix(BELT_STEEL, BELT_POLISHED, 0.5);
        roughness = mix(0.62, 0.38, max(paths, horn));
        metalness = mix(0.4, 0.7, max(paths, horn));
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
        metalness = mix(0.4 * (1.0 - pad), 0.65, connector);
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
    (*s).emissive += color * vehicle_fill(normal) * material.params[1].w;
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
        (*s).emissive += (*s).color * vehicle_fill((*s).normal) * material.params[1].w;
        return;
    }
    // The shadow pass has no vertex colors and needs no paint.
    if f.vertex_color.a > 0.95 {
        return;
    }

    let code = f.vertex_color.b;
    let team = step(0.5, code);
    let value = (code - 0.5 * team) * 2.0 * VEHICLE_VALUE_RANGE;
    let base = mix(material.params[1].rgb, material.params[0].rgb, team) * value * f.tint.rgb;
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

    // Clean paint: only a whisper of the grime field's drift on open panels, each
    // panel a shade lighter in its middle than along its borders, so plates read
    // as separate pieces of armor under flat light.
    // The border shading fades out as pixels grow, so from the overhead camera a
    // hull keeps its full, clean team color instead of darkening plate by plate.
    let panel_reach = clamp(edges.narrowest * VEHICLE_PANEL_REACH, VEHICLE_PANEL_MIN, VEHICLE_PANEL_MAX);
    let panel_resolved = 1.0 - smoothstep(VEHICLE_PANEL_PIXEL_FROM, VEHICLE_PANEL_PIXEL_TO, pixel);
    let panel = mix(1.0, mix(VEHICLE_PANEL_BORDER, 1.0, smoothstep(0.0, panel_reach, edges.nearest)), panel_resolved);
    let metres = f.uv * VEHICLE_METRES_PER_UV;
    let fade = value_noise(metres * VEHICLE_FADE_CELLS) * 0.65 + value_noise(metres * VEHICLE_FADE_CELLS * 2.7 + 3.1) * 0.35;
    var color = base * (1.0 + (fade - 0.5) * 2.0 * VEHICLE_FADE_VALUE) * panel;
    // The turret roof, the highest and lightest paint, is sun-bleached a little
    // lighter and greyer than the decks below it: a pure team color clips to one
    // flat slab under a saturating grade, while this keeps the turret's footprint
    // standing off the hull from the overhead camera.
    let roof = smoothstep(0.75, 0.92, (*s).normal.y) * smoothstep(VEHICLE_ROOF_FROM, VEHICLE_ROOF_TO, value) * team
        * select(0.0, 1.0, atmosphere_on());
    let luma = dot(color, vec3f(0.2126, 0.7152, 0.0722));
    color = mix(color, vec3f(luma) * VEHICLE_ROOF_TINT, roof * VEHICLE_ROOF_BLEACH) * (1.0 + roof * VEHICLE_ROOF_LIFT);
    // Rubbed edges: a lighter band on each bevel and over chamfer strips.
    let lifted = color * VEHICLE_BEVEL_LIFT + VEHICLE_BEVEL_PALE;
    color = mix(color, lifted, max(bevel, chamfer * 0.45) * VEHICLE_BEVEL_TINT);
    // Grime settled into recesses, patchy along their length.
    let recess = clamp(edges.recess * (0.75 + (0.5 - grime) * 0.9), 0.0, 1.0);
    color = mix(color, color * VEHICLE_GRIME, recess * VEHICLE_GRIME_STRENGTH);

    // Welded seams cut broad plates into panels: across decks, and upright on the
    // sides and faces, so a deck seam turns the corner down the side. A dark groove
    // with a lighter weld lip beside it.
    let seam_resolved = (1.0 - smoothstep(VEHICLE_SEAM_PIXEL_FROM, VEHICLE_SEAM_PIXEL_TO, pixel))
        * step(VEHICLE_SEAM_MIN_PLATE, edges.narrowest) * step(edges.narrowest, 100.0)
        * (1.0 - chamfer);
    var seam = 0.0;
    var lip = 0.0;
    if seam_resolved > 0.0 {
        // Paint UVs run along the model's x and z on decks and across upright faces,
        // with v up the upright ones: v on a deck and u on an upright face cross it.
        let q = select(f.uv.x, f.uv.y, (*s).normal.y > 0.6) * VEHICLE_METRES_PER_UV / VEHICLE_SEAM_SPACING;
        let offset = (fract(q) - 0.5) * VEHICLE_SEAM_SPACING;
        let aa = max(pixel, 1e-4);
        seam = (1.0 - smoothstep(VEHICLE_SEAM_WIDTH * 0.5, VEHICLE_SEAM_WIDTH * 0.5 + aa, abs(offset))) * seam_resolved;
        let lip_centre = abs(offset - (VEHICLE_SEAM_WIDTH * 0.5 + VEHICLE_SEAM_LIP * 0.5));
        lip = (1.0 - smoothstep(VEHICLE_SEAM_LIP * 0.5, VEHICLE_SEAM_LIP * 0.5 + aa, lip_centre)) * seam_resolved;
    }
    color = color * (1.0 - seam * 0.65) * (1.0 + lip * 0.3);

    // Non-skid grain on decks close up.
    let deck = smoothstep(0.55, 0.8, (*s).normal.y);
    let grain_resolved = (1.0 - smoothstep(VEHICLE_GRAIN_PIXEL_FROM, VEHICLE_GRAIN_PIXEL_TO, pixel)) * deck * (1.0 - chamfer);
    var grain = 0.5;
    if grain_resolved > 0.0 {
        grain = value_noise(metres * VEHICLE_GRAIN_CELLS) * 0.6 + value_noise(metres * VEHICLE_GRAIN_CELLS * 2.3 + 1.7) * 0.4;
    }
    color *= 1.0 + (grain - 0.5) * 2.0 * VEHICLE_GRAIN_VALUE * grain_resolved;

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
    let line = low + (grime - 0.5) * 0.55 + (splash - 0.5) * VEHICLE_SPLASH_RISE;
    let caked = smoothstep(VEHICLE_SPLASH_EDGE, VEHICLE_SPLASH_EDGE + 0.07, line);
    let band = max(caked * (0.82 + 0.18 * smoothstep(0.5, 0.8, line)), smoothstep(-0.05, 0.2, line) * VEHICLE_DUST_FILM * select(0.0, 1.0, atmosphere_on()));
    let drifted = edges.recess * smoothstep(0.08, 0.5, low) * (0.6 + 0.8 * grime);
    let dusty = clamp(max(band, max(drifted, ledge * (0.35 + 0.5 * grime))), 0.0, 1.0) * dust_strength;
    let muddy = smoothstep(0.74, 0.94, low + (grime - 0.5) * 0.3) * VEHICLE_MUD_STRENGTH * studio;
    let dust = vehicle_dust();
    color = mix(color, dust * (0.8 + 0.4 * grime), dusty);
    color = mix(color, VEHICLE_MUD, muddy);
    // Dust settled on the decks: a patchy film on broad plates, drifted into
    // recesses, never on the bevels the crew's boots and the wind keep clean. The
    // film resolves only close up: from the overhead camera and on the menu cards
    // its patches would read as camouflage blotches over the team color.
    let patches = value_noise(metres * VEHICLE_DECK_DUST_CELLS + 5.7) * 0.7 + grime * 0.3;
    let broad = smoothstep(VEHICLE_DECK_DUST_MIN_PLATE, VEHICLE_DECK_DUST_MIN_PLATE * 2.0, edges.narrowest);
    // Studio scenes (the menu cards) show the paint showroom clean.
    let film = VEHICLE_DECK_DUST_FILM * broad * select(0.0, 1.0, atmosphere_on())
        * (1.0 - smoothstep(VEHICLE_DECK_DUST_PIXEL_FROM, VEHICLE_DECK_DUST_PIXEL_TO, pixel));
    let settled = deck * (1.0 - chamfer) * (1.0 - bevel)
        * smoothstep(0.3, 0.7, patches + recess * 0.3)
        * (film + VEHICLE_DECK_DUST_DRIFT * recess);
    color = mix(color, dust * (0.95 + 0.3 * grime), settled);
    let dirt = max(max(dusty, muddy), settled * 0.8);

    // Chips to bare steel along convex edges of broad plates (a few low on the hull),
    // where the chip field dips under the spot's exposure, with a darker rim of
    // lifted paint.
    let resolved = 1.0 - smoothstep(VEHICLE_CHIP_PIXEL_FROM, VEHICLE_CHIP_PIXEL_TO, pixel);
    let plate = smoothstep(VEHICLE_CHIP_PLATE_FROM, VEHICLE_CHIP_PLATE_TO, edges.narrowest);
    // Wear clusters where hands, boots and branches rub: a broad field gathers the
    // chips into worn stretches and leaves others intact, so they read as history
    // rather than as one stamped size of fleck along every edge.
    let rubbed = smoothstep(0.25, 0.75, value_noise(metres * VEHICLE_CHIP_CLUSTER + 17.9));
    let exposure = (edges.near_edge * VEHICLE_CHIP_EXPOSURE * (0.35 + 1.1 * rubbed) + low * 0.05) * (1.0 - dirt * 0.6) * resolved * plate;
    let chip = 1.0 - smoothstep(exposure - 0.03, exposure, wear);
    // The rim only rings real chips: where nothing is exposed it would darken the
    // chip field's lowest spots into specks over the whole panel.
    let chip_rim = (1.0 - smoothstep(exposure, exposure + 0.07, wear)) * (1.0 - chip) * smoothstep(0.0, 0.05, exposure);
    color *= 1.0 - chip_rim * 0.55;
    color = mix(color, material.params[3].rgb * (0.75 + 0.5 * grime), chip);

    // The sheen breaks up a little with the grime field, so a panel's highlight
    // reads as painted steel rather than moulded plastic.
    var roughness = mix(VEHICLE_PAINT_ROUGHNESS, VEHICLE_DECK_ROUGHNESS, deck)
        + (0.5 - grime) * VEHICLE_SHEEN_BREAKUP * (1.0 - deck) + streak * 0.08
        + (grain - 0.5) * 0.08 * grain_resolved;
    roughness = mix(roughness, VEHICLE_DIRT_ROUGHNESS, max(dirt, recess * 0.6));
    roughness = mix(roughness, VEHICLE_STEEL_ROUGHNESS, chip);
    (*s).roughness = roughness;
    (*s).metalness = mix((*s).metalness, VEHICLE_STEEL_METALNESS, chip);
    (*s).normal = normal;
    (*s).color = color;

    // The paint's own fill, and its team rim at grazing angles, strongest on the
    // side turned from the sun and on bevels that roll toward the view's edge.
    let paint = 1.0 - chip;
    (*s).emissive += color * vehicle_fill(normal) * material.params[1].w;
    let grazing = 1.0 - clamp(dot(normal, f.view_direction), 0.0, 1.0);
    let away = 1.0 - clamp(dot(normal, frame.sun_direction.xyz), 0.0, 1.0);
    let rim = grazing * grazing * (0.45 + 0.55 * away) * material.params[0].w * (1.0 - dirt) * paint;
    (*s).emissive += material.params[2].rgb * value * rim * VEHICLE_RIM_GLOW;
}

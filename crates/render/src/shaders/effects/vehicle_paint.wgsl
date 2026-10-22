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
//
// params[0] = (team paint rgb linear, rim strength);
// params[1] = (trim rgb linear, fill strength);
// params[2] = (rim hue rgb linear, wear tile repeat);
// params[3] = (bare steel rgb linear, 1 painted armor or 0 running gear).

const VEHICLE_EDGE_MARGIN: f32 = 0.04;
const VEHICLE_EDGE_SPAN: f32 = 0.92;
// `VALUE_RANGE` in Rust.
const VEHICLE_VALUE_RANGE: f32 = 1.45;
// The bevel band: about this many pixels wide, within these widths in metres, and
// how far the normal rolls over in it.
const VEHICLE_BEVEL_PIXELS: f32 = 2.6;
const VEHICLE_BEVEL_MIN: f32 = 0.016;
const VEHICLE_BEVEL_MAX: f32 = 0.12;
const VEHICLE_BEVEL_ROLL: f32 = 0.3;
// Panel shading: paint along a plate's borders is this much darker, fading out over
// this fraction of the plate's narrowest width (within these metres).
const VEHICLE_PANEL_BORDER: f32 = 0.74;
const VEHICLE_PANEL_REACH: f32 = 0.3;
const VEHICLE_PANEL_MIN: f32 = 0.03;
const VEHICLE_PANEL_MAX: f32 = 0.35;
// How far from a convex edge chips reach, and from a recess its grime (metres).
const VEHICLE_CHIP_REACH: f32 = 0.07;
const VEHICLE_RECESS_REACH: f32 = 0.12;
// Chips fade out as a pixel grows past their size, so distant hulls never sparkle.
const VEHICLE_CHIP_PIXEL_FROM: f32 = 0.01;
const VEHICLE_CHIP_PIXEL_TO: f32 = 0.024;
// Paint rubbed lighter on a bevel and on chamfer strips.
const VEHICLE_BEVEL_LIFT: f32 = 1.7;
const VEHICLE_BEVEL_PALE: vec3f = vec3f(0.03, 0.022, 0.018);
const VEHICLE_BEVEL_TINT: f32 = 0.8;
// Dust and wet mud (linear) and how strongly they cover the lowest armor.
const VEHICLE_DUST: vec3f = vec3f(0.27, 0.205, 0.12);
const VEHICLE_MUD: vec3f = vec3f(0.075, 0.053, 0.034);
const VEHICLE_DUST_STRENGTH: f32 = 0.82;
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
const VEHICLE_DECK_ROUGHNESS: f32 = 0.53;
const VEHICLE_SHEEN_BREAKUP: f32 = 0.1;
const VEHICLE_DIRT_ROUGHNESS: f32 = 0.92;
const VEHICLE_STEEL_ROUGHNESS: f32 = 0.62;
const VEHICLE_STEEL_METALNESS: f32 = 0.2;
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
const VEHICLE_GRAIN_CELLS: f32 = 85.0;
const VEHICLE_GRAIN_VALUE: f32 = 0.16;
const VEHICLE_GRAIN_PIXEL_FROM: f32 = 0.003;
const VEHICLE_GRAIN_PIXEL_TO: f32 = 0.009;
// Soft, sun-faded variation across the paint, this many cells per metre and this
// much either way: broad enough never to read as speckle.
const VEHICLE_FADE_CELLS: f32 = 2.4;
const VEHICLE_FADE_VALUE: f32 = 0.1;

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

// The vehicle's own fill: the sky and ground light again, strongest on faces
// turned from the sun.
fn vehicle_fill(n: vec3f) -> vec3f {
    let hemisphere = mix(frame.ground_color.rgb, frame.sky_color.rgb, n.y * 0.5 + 0.5);
    let away = 1.0 - clamp(dot(n, frame.sun_direction.xyz), 0.0, 1.0);
    return hemisphere * (0.3 + 0.7 * away);
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    // Derivatives and the tile sample first, in uniform control flow.
    let tile = textureSample(extra_texture0, extra_sampler0, f.uv * material.params[2].w);
    let dpx = dpdx(f.world);
    let dpy = dpdy(f.world);
    let dcx = dpdx(f.vertex_color.rg);
    let dcy = dpdy(f.vertex_color.rg);
    (*s).opacity = f.tint.a;
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
    let panel_reach = clamp(edges.narrowest * VEHICLE_PANEL_REACH, VEHICLE_PANEL_MIN, VEHICLE_PANEL_MAX);
    let panel = mix(VEHICLE_PANEL_BORDER, 1.0, smoothstep(0.0, panel_reach, edges.nearest));
    let metres = f.uv * VEHICLE_METRES_PER_UV;
    let fade = value_noise(metres * VEHICLE_FADE_CELLS) * 0.65 + value_noise(metres * VEHICLE_FADE_CELLS * 2.7 + 3.1) * 0.35;
    var color = base * (1.0 + (fade - 0.5) * 2.0 * VEHICLE_FADE_VALUE) * panel;
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

    // Dust and mud thrown up by the tracks: a broken, splashed line.
    let dusty = smoothstep(0.24, 0.56, low + (grime - 0.5) * 0.35) * VEHICLE_DUST_STRENGTH;
    let muddy = smoothstep(0.6, 0.84, low + (grime - 0.5) * 0.45) * VEHICLE_MUD_STRENGTH;
    color = mix(color, VEHICLE_DUST * (0.8 + 0.4 * grime), dusty);
    color = mix(color, VEHICLE_MUD, muddy);
    let dirt = max(dusty, muddy);

    // Chips to bare steel along convex edges and chamfers (a few low on the hull),
    // where the chip field dips under the spot's exposure, with a darker rim of
    // lifted paint.
    let resolved = 1.0 - smoothstep(VEHICLE_CHIP_PIXEL_FROM, VEHICLE_CHIP_PIXEL_TO, pixel);
    let exposure = (edges.near_edge * 0.4 + chamfer * 0.05 + low * 0.05) * (1.0 - dirt * 0.6) * resolved;
    let chip = 1.0 - smoothstep(exposure - 0.03, exposure, wear);
    let chip_rim = (1.0 - smoothstep(exposure, exposure + 0.07, wear)) * (1.0 - chip);
    color *= 1.0 - chip_rim * 0.4;
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

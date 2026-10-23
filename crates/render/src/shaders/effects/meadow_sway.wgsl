// Pine Village meadow (`effects_scenery::MEADOW_SWAY`). One effect name serves
// three village materials, told apart by params[0].x; each mode is its own
// registered definition (`effects::registry`, `MODES`) whose hooks call this file's
// functions for that mode alone, so a pipeline holds only its mode's code:
//
// 0: the meadow tufts (`village_vegetation.rs`) sway in the wind after the
//    instance transform (tufts are instanced at world placements, so this is
//    world space), by their height above their own base. instance_data.xy is the
//    tuft's wind origin (world x/z). Extra texture 0 is the ground bake, whose
//    grass each tuft takes its colour from.
// 1: the village ground (`ground_surfaces.rs`): a meadow of lush and dry patches
//    worn into dirt roads with tank treads, packed lanes, puddles, pebbles and
//    wildflowers, a paved square, cottage gardens and the farmland outside the
//    wall, all in world space. Everything of it that neither moves nor changes
//    with the view (the lie of the land, the meadow's and the earth's colours,
//    where the turf gives way, gardens, fields and hedges) is rendered once into
//    the ground bake (`meadow_bake.wgsl`, `effects::bakes`); per pixel the shader
//    only cuts the turf's edge, the treads and puddles, the square's setts and the
//    near pebbles and blooms from it, the plan's tracks and the two tiles, and
//    lets the gusts roll over the long grass. The map is the grass tile at the
//    mesh UVs (world / 8 m), extra texture 0 the packed-dirt tile, extra texture 1
//    the baked ground plan (`village_ground.rs`, linear, unflipped), extra texture
//    2 the ground bake (`vg_bake_uv`; bound in the bump map's slot,
//    `gpu::resources` `texture_slots`). params[0] = (1, arena half size, unused,
//    unused); params[1].xyz the north-south roads' centre x; params[2].xyz the
//    east-west roads' centre z; params[3] = half widths (north-south outer,
//    north-south centre, east-west outer, east-west centre).
// 2: the mountain ranges (`village_landscape.rs`): ridged crags break the
//    slopes into buttresses, gullies and ledges, a canopy of single conifer
//    crowns thins into separate trees at a ragged treeline, and crisp snow
//    fills the gentler faces and gullies above the snowline, in world space;
//    the low meadows keep their vertex colours.
fn vg_tuft_world(w: ptr<function, EffectWorld>, v: EffectVertex) {
    if material.params[0].x > 0.5 {
        return;
    }
    let t = frame.camera_position.w;
    let o = v.instance_data.xy;
    var p = (*w).position;
    // Every blade of every tuft its own length: a hash of the tuft and the
    // blade's tip (its mesh position), so no two clumps share a silhouette.
    // Roots stay put; only the height above the tuft's base stretches.
    let blade = vg_hash(floor(v.position.xz * 37.0) + o * 1.618);
    let reach = mix(0.55, 1.4, blade * blade) * mix(0.85, 1.15, vg_hash(o + 3.7));
    let lift = p.y - v.instance_origin.y;
    p.y = v.instance_origin.y + lift * reach;
    let h = max(lift * reach, 0.0);
    (*w).position = p + vec3f(
        sin(t * 1.2 + o.x * 0.7 + o.y * 0.4) * h * 0.2,
        0.0,
        cos(t * 0.8 + o.y * 0.5) * h * 0.12,
    );
}

const VG_LUMA = vec3f(0.2126, 0.7152, 0.0722);

fn vg_linear(hex: u32) -> vec3f {
    let c = vec3f(f32((hex >> 16u) & 255u), f32((hex >> 8u) & 255u), f32(hex & 255u)) / 255.0;
    return select(pow((c + 0.055) / 1.055, vec3f(2.4)), c / 12.92, c <= vec3f(0.04045));
}

// Dave Hoskins' sine-free hashes: stable across GPUs at world-sized inputs.
fn vg_hash(p: vec2f) -> f32 {
    var p3 = fract(vec3f(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn vg_hash2(p: vec2f) -> vec2f {
    var p3 = fract(vec3f(p.x, p.y, p.x) * vec3f(0.1031, 0.1030, 0.0973));
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.xx + p3.yz) * p3.zy);
}

fn vg_noise(p: vec2f) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = vg_hash(i);
    let b = vg_hash(i + vec2f(1.0, 0.0));
    let c = vg_hash(i + vec2f(0.0, 1.0));
    let d = vg_hash(i + vec2f(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn vg_fbm(p: vec2f) -> f32 {
    return vg_noise(p) * 0.55 + vg_noise(p * 2.07 + 17.3) * 0.3 + vg_noise(p * 4.13 - 9.1) * 0.15;
}

// Surface-gradient bump of a world-space height field (metres), like the
// standard shader's bump but from a procedural height.
fn vg_bump(height: f32, world: vec3f, normal: vec3f, front: bool) -> vec3f {
    let slope = vec2f(dpdx(height), -dpdy(height));
    let sigma_x = normalize(dpdx(world));
    let sigma_y = normalize(-dpdy(world));
    let r1 = cross(sigma_y, normal);
    let r2 = cross(normal, sigma_x);
    let det = dot(sigma_x, r1) * select(-1.0, 1.0, front);
    let gradient = sign(det) * (slope.x * r1 + slope.y * r2);
    return normalize(abs(det) * normal - gradient);
}

// The slope of a world-space height field in metres per metre, as a vector in
// the surface's plane: unlike `vg_bump`, whose tilt is the height change per
// pixel, the land's real swells and banks light the same at every distance, so
// a mound reads as a mound from the overview and from a turret alike. The
// height must change smoothly over metres, never over a pixel or two.
fn vg_slope(height: f32, world: vec3f, normal: vec3f, front: bool) -> vec3f {
    let sigma_x = dpdx(world);
    let sigma_y = -dpdy(world);
    let r1 = cross(sigma_y, normal);
    let r2 = cross(normal, sigma_x);
    let det = dot(sigma_x, r1) * select(-1.0, 1.0, front);
    let gradient = sign(det) * (dpdx(height) * r1 - dpdy(height) * r2);
    return gradient / max(abs(det), 1e-10);
}

// Signed distance outside a rounded box at the origin (negative inside).
fn vg_box(p: vec2f, half: vec2f, radius: f32) -> f32 {
    let q = abs(p) - half + radius;
    return length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

// The village square (half extents in metres): the main street is paved through
// its barricades out to the cottages north and south, and the cross street out
// toward the cottages either side, so the heart of the village reads as a place,
// not more road. `village_roads::VILLAGE_SQUARE` is the same outline.
const VG_SQUARE_MAIN = vec2f(7.4, 26.0);
const VG_SQUARE_CROSS = vec2f(20.5, 4.4);
// One sett's footprint (m): along its row, and the rows' pitch.
const VG_SETT = vec2f(0.44, 0.29);
// The kerb: a course of long edge stones laid round the square, its width and
// each stone's length (m).
const VG_KERB_WIDTH = 0.36;
const VG_KERB_STONE = 0.68;
// How far the kerb stands proud of the road's earth (m), for the shade it casts.
const VG_KERB_RISE = 0.07;
// Radius of the rings of setts at the heart of the crossing (m).
const VG_ROSETTE = 4.1;
// Sett rows between the dark courses that panel the streets.
const VG_PANEL_ROWS = 17.0;

// The square's outline near `p`: x metres inside it (negative outside), y the
// coordinate along the nearest stretch of kerb, zw that stretch's outward
// direction. The two streets are set out square, so every stretch of kerb runs
// true and meets the next at a corner stone.
fn vg_square_edge(p: vec2f) -> vec4f {
    let a = abs(p);
    let s = select(vec2f(-1.0), vec2f(1.0), p >= vec2f(0.0));
    let in_main = a.x < VG_SQUARE_MAIN.x && a.y < VG_SQUARE_MAIN.y;
    let in_cross = a.x < VG_SQUARE_CROSS.x && a.y < VG_SQUARE_CROSS.y;
    if in_main && in_cross {
        // The crossing: only its four inner corners meet the outline.
        let inner = max(VG_SQUARE_MAIN.x - a.x, VG_SQUARE_CROSS.y - a.y);
        return vec4f(inner, 0.0, s * 0.7071);
    }
    if in_main {
        let side = VG_SQUARE_MAIN.x - a.x;
        let end = VG_SQUARE_MAIN.y - a.y;
        if max(side, end) < VG_KERB_WIDTH {
            return vec4f(min(side, end), 0.0, s * 0.7071);
        }
        return select(vec4f(end, p.x, 0.0, s.y), vec4f(side, p.y, s.x, 0.0), side < end);
    }
    if in_cross {
        let side = VG_SQUARE_CROSS.y - a.y;
        let end = VG_SQUARE_CROSS.x - a.x;
        if max(side, end) < VG_KERB_WIDTH {
            return vec4f(min(side, end), 0.0, s * 0.7071);
        }
        return select(vec4f(end, p.y, s.x, 0.0), vec4f(side, p.x, 0.0, s.y), side < end);
    }
    let outside = min(vg_box(p, VG_SQUARE_MAIN, 0.0), vg_box(p, VG_SQUARE_CROSS, 0.0));
    return vec4f(-outside, 0.0, 0.0, 0.0);
}

// Metres inside the square at `p` (negative outside).
fn vg_square(p: vec2f) -> f32 {
    return vg_square_edge(p).x;
}

// Metres in from the two kerbs that meet at the square's corner nearest `p`.
fn vg_corner_local(p: vec2f) -> vec2f {
    let a = abs(p);
    let in_main = a.x < VG_SQUARE_MAIN.x && a.y < VG_SQUARE_MAIN.y;
    let in_cross = a.x < VG_SQUARE_CROSS.x && a.y < VG_SQUARE_CROSS.y;
    if in_main && in_cross {
        return vec2f(VG_SQUARE_MAIN.x - a.x, VG_SQUARE_CROSS.y - a.y);
    }
    return select(VG_SQUARE_CROSS - a, VG_SQUARE_MAIN - a, in_main);
}

// One stone of the square's paving at `p`: a sett, or a kerb stone round the edge.
struct VgSett {
    // 1 on the stone, 0 in the joints.
    stone: f32,
    // Its surface slope (world x, z), m per m: a worn, domed top and each stone
    // set a little askew, so every sett catches the sun its own way.
    slope: vec2f,
    // 0 at its rim, 1 over its crown.
    crown: f32,
    // Random numbers for its stone and its wear.
    pick: vec2f,
    // How high the stone's top stands, 0 in the joints to 1 on the proudest
    // crowns: earth drifts over the low ones first.
    height: f32,
    // 1 on the kerb.
    kerb: f32,
    // 1 on the dark course ringing the square's heart.
    course: f32,
}

// A rounded stone's top over `local` (metres from its centre) inside `half`:
// coverage, crown and the outward slope of its worn rim.
fn vg_stone_top(local: vec2f, half: vec2f, radius: f32, joint_aa: f32) -> vec4f {
    let d = vg_box(local, half, radius);
    let stone = 1.0 - smoothstep(-joint_aa, joint_aa, d);
    let inner = abs(local) - half + radius;
    var outward = select(vec2f(0.0, sign(local.y)), vec2f(sign(local.x), 0.0), inner.x > inner.y);
    if inner.x > 0.0 && inner.y > 0.0 {
        outward = normalize(inner) * sign(local);
    }
    let rim = smoothstep(-0.05, 0.0, d);
    return vec4f(stone, 1.0 - rim, outward * rim);
}

// Setts in rows across each street, every row shifted its own way, so each
// street's courses run square to its traffic. The courses bow gently, as
// hand-laid rows do, some stones are half-length, some sit proud and some have
// sunk. A course of long kerb stones edges the square. `aa` is the pixel
// footprint in metres.
fn vg_setts(p: vec2f, aa: f32) -> VgSett {
    let joint_aa = max(aa * 0.8, 0.004);
    let edge = vg_square_edge(p);
    if edge.x < VG_KERB_WIDTH {
        // The kerb: long stones along the edge, a square one at each corner,
        // each settled a centimetre or two off true.
        let corner = abs(edge.z * edge.w) > 0.1;
        let run = edge.y + dot(edge.zw, vec2f(13.7, 29.3));
        let cell = floor(run / VG_KERB_STONE);
        var pick = vg_hash2(vec2f(cell, dot(edge.zw, vec2f(3.0, 7.0))) + 211.0);
        // Stone coordinates: along the kerb, and in from its outer face.
        var local = vec2f(run - (cell + 0.5) * VG_KERB_STONE, edge.x - VG_KERB_WIDTH * 0.5);
        var half = vec2f(VG_KERB_STONE * 0.5 - 0.012, VG_KERB_WIDTH * 0.5 - 0.012);
        var along_dir = vec2f(-edge.w, edge.z);
        var inward_dir = -edge.zw;
        if corner {
            // One square stone where two stretches meet.
            local = vg_corner_local(p) - VG_KERB_WIDTH * 0.5;
            half = vec2f(VG_KERB_WIDTH * 0.5 - 0.012);
            pick = vg_hash2(floor(p * 0.5) + 97.0);
            along_dir = vec2f(-sign(p.x), 0.0);
            inward_dir = vec2f(0.0, -sign(p.y));
        }
        local.y = local.y + (pick.y - 0.5) * 0.02;
        let top = vg_stone_top(local, half, 0.035, joint_aa);
        // Its rim, turned into the world; the stones lean a little outward
        // where the earth beside them has settled.
        let tilt = (pick - 0.5) * vec2f(0.06, 0.05) - vec2f(0.0, 0.035);
        let slope2 = top.zw + tilt;
        let slope = along_dir * slope2.x + inward_dir * slope2.y;
        return VgSett(top.x, slope, top.y, pick, top.x * mix(0.85, 1.0, top.y), 1.0, 0.0);
    }
    let r = length(p);
    if r < VG_ROSETTE {
        // The heart of the crossing: setts laid in rings round its middle,
        // edged by a course of long dark whinstones.
        let border = r > VG_ROSETTE - VG_KERB_WIDTH;
        let pitch = select(VG_SETT.y, VG_KERB_WIDTH, border);
        let ring = select(floor(r / VG_SETT.y), 1000.0, border);
        let centre_r = select((ring + 0.5) * VG_SETT.y, VG_ROSETTE - VG_KERB_WIDTH * 0.5, border);
        let length_wanted = select(VG_SETT.x, VG_KERB_STONE * 0.8, border);
        let count = max(floor(6.2832 * centre_r / length_wanted), 1.0);
        let turn = atan2(p.y, p.x) / 6.2832 + vg_hash(vec2f(ring, 3.0));
        let seg = floor(turn * count);
        var pick = vg_hash2(vec2f(seg, ring) + 401.0);
        let arc = (turn - (seg + 0.5) / count) * 6.2832 * centre_r;
        let local = vec2f(arc, r - centre_r);
        let half = vec2f(3.1416 * centre_r / count, pitch * 0.5) - vec2f(0.016, 0.014) - pick * vec2f(0.016, 0.012);
        var top = vg_stone_top(local, half, select(mix(0.04, 0.08, pick.y), 0.035, border), joint_aa);
        if r < VG_SETT.y {
            // One round stone at the very middle.
            top = vec4f(1.0 - smoothstep(VG_SETT.y - 0.03 - joint_aa, VG_SETT.y - 0.03 + joint_aa, r), 1.0 - smoothstep(0.15, 0.24, r), p * 0.4);
            pick = vec2f(0.3, 0.6);
        }
        let radial = p / max(r, 1e-4);
        let around = vec2f(-radial.y, radial.x);
        let proud = fract(pick.x * 3.7 + pick.y * 1.3);
        let tilt = (pick - 0.5) * 0.2;
        let slope2 = top.zw * mix(0.4, 0.7, proud) + local * mix(0.6, 1.0, proud) * select(1.0, 0.3, border) + tilt;
        let slope = around * slope2.x + radial * slope2.y;
        let height = top.x * mix(0.45, 1.0, select(proud, 0.9, border)) * mix(0.75, 1.0, top.y);
        return VgSett(top.x, slope, top.y, pick, height, 0.0, select(0.0, 1.0, border));
    }
    let arm = abs(p.x) > VG_SQUARE_MAIN.x;
    let straight = select(p, p.yx, arm);
    let q = straight + vec2f(
        (vg_noise(straight * 0.09 + 5.0) - 0.5) * 0.8,
        (vg_noise(straight * vec2f(0.11, 0.05)) - 0.5) * 1.2,
    );
    let row = floor(q.y / VG_SETT.y);
    let shift = vg_hash(vec2f(row, 7.0)) * VG_SETT.x;
    let column = floor((q.x + shift) / VG_SETT.x);
    let id = vec2f(column, row) + select(0.0, 503.0, arm);
    var centre = vec2f((column + 0.5) * VG_SETT.x - shift, (row + 0.5) * VG_SETT.y);
    var reach_x = VG_SETT.x * 0.5;
    var pick = vg_hash2(id + 5.0);
    // Some cells hold two shorter stones, cut a little off centre.
    if pick.y > 0.58 {
        let cut = (pick.x - 0.5) * 0.08;
        let right = q.x - centre.x > cut;
        centre.x = centre.x + select(cut * 0.5 - reach_x * 0.5, cut * 0.5 + reach_x * 0.5, right);
        reach_x = reach_x * 0.5 + select(cut * 0.5, -cut * 0.5, right);
        pick = vg_hash2(id + select(37.0, 71.0, right));
    }
    let jitter = vg_hash2(id + 23.0 + pick * 3.0);
    let local = q - centre - (jitter - 0.5) * 0.02;
    // Hand-laid: each stone its own size, the joints never one width.
    let half = vec2f(reach_x, VG_SETT.y * 0.5) - vec2f(0.016, 0.014) - jitter * vec2f(0.022, 0.02);
    let top = vg_stone_top(local, half, mix(0.04, 0.09, pick.y), joint_aa);
    // Worn round over its rim, a gentle crown over the rest, each stone set a
    // little askew; the proud ones crown more steeply, the sunk ones lie flat.
    let proud = fract(pick.x * 3.7 + pick.y * 1.3);
    let tilt = (pick - 0.5) * 0.22;
    var slope = top.zw * mix(0.4, 0.7, proud) + local * mix(0.6, 1.0, proud) + tilt;
    slope = select(slope, slope.yx, arm);
    let height = top.x * mix(0.45, 1.0, proud) * mix(0.75, 1.0, top.y);
    // Every few paces a double course of dark whinstone crosses the street,
    // bowing with the rows, so the paving reads in panels from any height
    // rather than as one slab.
    // Measured in rows from the course's middle, anti-aliased along its edge.
    let rows_out = q.y / VG_SETT.y - 1.0;
    let from_band = abs(rows_out - VG_PANEL_ROWS * round(rows_out / VG_PANEL_ROWS));
    let band_aa = max(joint_aa / VG_SETT.y, 0.02);
    // Half the rosette's border course in weight: a panel line, not a road marking.
    let band = (1.0 - smoothstep(1.0 - band_aa, 1.0 + band_aa, from_band)) * 0.45;
    return VgSett(top.x, slope, top.y, pick, height, 0.0, band);
}

// One road family's nearest strip: (signed metres inside it, offset across it,
// half width). `centres` are the strips' centres across, `along` the coordinate
// along them; strips stop a metre short of the arena edge `extent`.
fn vg_strip(across: f32, along: f32, centres: vec3f, outer: f32, centre: f32, extent: f32) -> vec3f {
    var best = vec3f(-1000.0, 0.0, 1.0);
    for (var i = 0; i < 3; i++) {
        let half = select(outer, centre, i == 1);
        let offset = across - centres[i];
        let inside = min(half - abs(offset), extent - 1.0 - abs(along));
        best = select(best, vec3f(inside, offset, half), inside > best.x);
    }
    return best;
}

// A disc on a jittered cell grid: x the coverage of this cell's disc (0 when the
// cell has none), y its dome height in metres, zw two random numbers for its look.
// Only a cell that holds a disc works out its shape.
fn vg_scatter(p: vec2f, density: f32, cells: f32, size: vec2f, aa: f32, seed: f32) -> vec4f {
    // No cell has a disc: callers weight everything else by the coverage.
    if density <= 0.0 {
        return vec4f(0.0);
    }
    let q = p * cells;
    let cell = floor(q);
    let jitter = vg_hash2(cell + seed);
    let pick = vg_hash(cell + seed * 1.37 + 5.1);
    var cover = 0.0;
    var dome = 0.0;
    if fract(pick * 13.7) < density {
        let centre = (cell + 0.2 + jitter * 0.6) / cells;
        let stretch = mix(0.65, 1.0, jitter.y);
        let angle = pick * 6.2832;
        let local = p - centre;
        let turned = vec2f(
            local.x * cos(angle) - local.y * sin(angle),
            (local.x * sin(angle) + local.y * cos(angle)) / stretch,
        );
        let base_radius = mix(size.x, size.y, fract(pick * 7.31));
        // A stone, a clod or a bloom is never a true ellipse: a few lobes
        // break its outline, each its own.
        let theta = atan2(turned.y, turned.x);
        let radius = base_radius * (1.0 + 0.15 * sin(theta * 3.0 + pick * 19.0) + 0.08 * sin(theta * 5.0 + jitter.x * 11.0));
        let distance = length(turned);
        cover = 1.0 - smoothstep(radius - aa, radius + aa, distance);
        dome = sqrt(max(1.0 - distance * distance / (radius * radius), 0.0)) * base_radius;
    }
    return vec4f(cover, dome, jitter.x, fract(pick * 3.71));
}

// A stone on `vg_scatter`'s jittered cell grid with the shade it casts on the earth
// and its sunlit side: `shade` and `light` are the offsets from `p` to where the
// sun's ray meets the stone, toward the sun. Both come from the stone of the cell
// under `p` (a stone keeps well inside its cell, so its neighbours' shades barely
// reach past their own), one cell's worth of hashes and trigonometry for all three.
// A share `big` of the stones are of the larger `big_size`, their outlines lobed.
struct VgStone {
    // Coverage of the stone and its dome height in metres.
    cover: f32,
    dome: f32,
    // Two random numbers for its look.
    pick: vec2f,
    // Its shade on the earth beside it.
    shade: f32,
    // 1 on its side toward the sun.
    lit: f32,
    // 1 for a big stone, 0 for a pebble.
    big: f32,
}

fn vg_stones(p: vec2f, density: f32, cells: f32, size: vec2f, big_size: vec2f, big: f32, aa: f32, seed: f32, shade: vec2f, light: vec2f) -> VgStone {
    var stone = VgStone(0.0, 0.0, vec2f(0.0), 0.0, 1.0, 0.0);
    if density <= 0.0 {
        return stone;
    }
    let cell = floor(p * cells);
    let jitter = vg_hash2(cell + seed);
    let pick = vg_hash(cell + seed * 1.37 + 5.1);
    stone.pick = vec2f(jitter.x, fract(pick * 3.71));
    if fract(pick * 13.7) >= density {
        return stone;
    }
    let centre = (cell + 0.2 + jitter * 0.6) / cells;
    let stretch = mix(0.65, 1.0, jitter.y);
    let angle = pick * 6.2832;
    let along = vec2f(cos(angle), -sin(angle));
    let across = vec2f(sin(angle), cos(angle)) / stretch;
    let local = p - centre;
    let turned = vec2f(dot(local, along), dot(local, across));
    let is_big = fract(pick * 5.3) < big;
    stone.big = select(0.0, 1.0, is_big);
    let range = select(size, big_size, is_big);
    let base_radius = mix(range.x, range.y, fract(pick * 7.31));
    // A stone is never a true ellipse: a few lobes break its outline, each its own
    // (too small to see on a pebble).
    var radius = base_radius;
    let shade_aa = aa * mix(1.6, 2.0, stone.big);
    if is_big {
        let theta = atan2(turned.y, turned.x);
        radius = base_radius * (1.0 + 0.15 * sin(theta * 3.0 + pick * 19.0) + 0.08 * sin(theta * 5.0 + jitter.x * 11.0));
    }
    let distance = length(turned);
    stone.cover = 1.0 - smoothstep(radius - aa, radius + aa, distance);
    stone.dome = sqrt(max(1.0 - distance * distance / (radius * radius), 0.0)) * base_radius;
    let shaded = local + shade * mix(1.0, 2.5, stone.big);
    let shade_distance = length(vec2f(dot(shaded, along), dot(shaded, across)));
    stone.shade = (1.0 - smoothstep(radius - shade_aa, radius + shade_aa, shade_distance)) * (1.0 - stone.cover);
    let toward = local + light * mix(1.0, 2.0, stone.big);
    stone.lit = smoothstep(radius - aa, radius + aa, length(vec2f(dot(toward, along), dot(toward, across))));
    return stone;
}

// The baked ground plan (`village_ground.rs`): three squares side by side, the
// arena, the valley and the arena's tracks, sampled at a world position.
// Coordinates stay half a texel inside each square.
const VG_ARENA_PLAN = 64.0;
const VG_VALLEY_PLAN = 170.0;
const VG_PLAN_WIDTH = 1536.0;
const VG_PLAN_HEIGHT = 512.0;
// The ground bake (`village_ground::VILLAGE_GROUND_BAKE`, `meadow_bake.wgsl`): six
// pages of VG_BAKE_PAGE texels, the arena's out to ±VG_ARENA_PLAN m in the first
// row and the valley's out to ±VG_VALLEY_PLAN m in the second, each row's columns
// the grass, the earth and the fields:
// - grass and earth: rgb the square root of the linear colour (so an 8-bit channel
//   keeps the dark greens), a the long grass (grass) and the damp field (earth,
//   `vg_damp_decode`);
// - fields: r where the turf gives way (`vg_cover_decode`, positive on the turf), gb
//   the land's slope along x and z (`vg_slope_decode`), a the wildflower meadows
//   (outside the village square) or the earth silted over its paving (inside).
// Rows run from the most negative z, like the plan's.
const VG_BAKE_PAGE = 512.0;
const VG_BAKE_SIZE = vec2f(1536.0, 1024.0);
const VG_BAKE_GRASS = 0.0;
const VG_BAKE_EARTH = 1.0;
const VG_BAKE_FIELDS = 2.0;
// The ranges the cover drive and the slopes are stored over (± each).
const VG_COVER_RANGE = 1.5;
const VG_SLOPE_RANGE = 1.0;
// The coarsest bake mip a colour lookup reads (16 texels: 4 m of the arena), so
// no page's mips bleed into its neighbour's but at its very edge.
const VG_BAKE_REACH = 16.0;
// The tracks' proximity ramp (`village_tracks::TREAD_REACH`) and a tread's half
// width, in metres.
const VG_TREAD_REACH = 1.0;
const VG_TREAD_HALF = 0.27;
// The wind over the meadow (unit x/z) and its gusts' speed in noise cells per second.
const VG_WIND = vec2f(0.8, 0.6);
const VG_GUST_SPEED = 0.55;
// How deep a fresh rut reads (m) for the shade its sunward wall casts.
const VG_RUT_DEPTH = 0.1;
// How high the turf stands over the worn earth (m), for the shade its rim casts.
const VG_TURF_LIP = 0.12;
// The mean luminance of the tufts' instance tints (`village_vegetation::TUFT_COLORS`).
const VG_TUFT_LUMA = 0.27;
// Earth seen between thinning blades at the turf's edge.
const VG_ROOT_EARTH = vec3f(0.135, 0.095, 0.06);
// Size of the hedged fields parcelling the valley, in metres.
const VG_ENCLOSURE = 42.0;
// How tall the valley's hedgerows stand (m), for the shade they cast.
const VG_HEDGE_HEIGHT = 1.5;
// Cloud shadows drifting over the valley and the ranges: noise cells per
// metre and their drift in cells per second (one rate for every point).
const VG_CLOUD_SCALE = 0.0055;
const VG_CLOUD_DRIFT = vec2f(0.012, 0.008);
// The ranges' painted woods (`vg_tips`): the spacing of the tips along a
// rank, the tallest tip above the rank's base, and the drop from one rank of
// crowns to the next below it, in metres.
const VG_TIP_SPACING = 2.6;
const VG_TIP_HEIGHT = 6.0;
const VG_RANK_STEP = 4.2;

// The bake's UV at `p` in page `column` of `row` (0 the arena, 1 the valley), half
// a texel inside the page.
fn vg_bake_uv(p: vec2f, row: f32, column: f32) -> vec2f {
    let half = select(VG_ARENA_PLAN, VG_VALLEY_PLAN, row > 0.5);
    let local = clamp(p / half * 0.5 + 0.5, vec2f(0.5 / VG_BAKE_PAGE), vec2f(1.0 - 0.5 / VG_BAKE_PAGE));
    return (vec2f(column, row) + local) * (VG_BAKE_PAGE / VG_BAKE_SIZE);
}

// The bake row that covers `p`: the arena's pages out to a texel inside their edge,
// where they still hold the arena's own blend into the valley.
fn vg_bake_row(p: vec2f) -> f32 {
    return select(1.0, 0.0, max(abs(p.x), abs(p.y)) < VG_ARENA_PLAN * (1.0 - 2.0 / VG_BAKE_PAGE));
}

fn vg_cover_encode(cover: f32) -> f32 {
    return clamp(cover / (2.0 * VG_COVER_RANGE) + 0.5, 0.0, 1.0);
}

fn vg_cover_decode(value: f32) -> f32 {
    return (value - 0.5) * (2.0 * VG_COVER_RANGE);
}

fn vg_slope_encode(slope: f32) -> f32 {
    return clamp(slope / (2.0 * VG_SLOPE_RANGE) + 0.5, 0.0, 1.0);
}

fn vg_slope_decode(value: f32) -> f32 {
    return (value - 0.5) * (2.0 * VG_SLOPE_RANGE);
}

// The damp field runs from -0.5 to 1.5; the hollows are damp above 0.58.
fn vg_damp_encode(damp: f32) -> f32 {
    return clamp((damp + 0.5) * 0.5, 0.0, 1.0);
}

fn vg_damp_decode(value: f32) -> f32 {
    return value * 2.0 - 0.5;
}

// Earth silted over the square's paving, stored over 0..1.2.
const VG_SILT_RANGE = 1.2;

// Whether `p` lies on or within half a metre of the village square, where the
// fields' alpha holds silted earth instead of the meadows.
fn vg_in_square(p: vec2f) -> bool {
    return abs(p.x) < 22.0 && abs(p.y) < 27.5 && vg_square(p) > -0.5;
}

// How much sunlight reaches `p` through the drifting clouds: 1 in the open,
// lower under a cloud's shadow, with soft edges a few tens of metres wide.
fn vg_cloud_light(p: vec2f) -> f32 {
    let q = p * VG_CLOUD_SCALE + VG_CLOUD_DRIFT * frame.camera_position.w;
    let cover = vg_noise(q) * 0.65 + vg_noise(q * 2.3 + 5.0) * 0.35;
    return 1.0 - smoothstep(0.5, 0.7, cover) * 0.3;
}

// Where long grass stands, positive in the stands; `village_vegetation::long_grass`
// is the same field.
fn vg_long_grass(p: vec2f) -> f32 {
    return sin(p.x * 0.083 + p.y * 0.041) * cos(p.y * 0.067 - p.x * 0.029) * 0.65
        + sin(p.x * 0.19 - p.y * 0.16 + 1.3) * 0.35;
}

// The lie of the land: low swells and hollows a few metres across, positive on
// the crests. The floor is flat for the simulation, so it lives only in the
// shading and in the colour of what grows there.
fn vg_swell(p: vec2f) -> f32 {
    return (vg_noise(p * 0.075 + 90.0) - 0.5) * 1.9
        + (vg_noise(vec2f(p.x * 0.17 - p.y * 0.12, p.y * 0.17 + p.x * 0.12) + 4.0) - 0.5) * 0.55;
}

// The meadow's broad colour at `p` (rgb) and how much of its turf has dried
// (w, 0 to 1). The land decides it: deep, dark grass where the water gathers in
// the hollows, fresh yellow-green on the open flats, sun-dried clumps on the
// crests, in drifts warped so neither a direction nor a grid shows. The turf's
// clumps and the tufts take their colour from it, so they read as part of the
// meadow they grow in rather than as confetti.
fn vg_lawn_tone(p: vec2f) -> vec4f {
    let warp = vec2f(vg_noise(p * 0.021 + 1.7), vg_noise(p * 0.021 + 8.3)) - 0.5;
    let q = p + warp * 26.0;
    let broad = vg_noise(q * 0.034);
    let turned = vec2f(q.x * 0.6 - q.y * 0.8, q.x * 0.8 + q.y * 0.6);
    let patches = vg_noise(turned * 0.13 + 5.0);
    let swell = vg_swell(p);
    // Swards a few strides to tens of metres across, each its own kind of
    // meadow: deep, damp grass, fresh green and patches gone to hay, with soft
    // edges, so the lawns read as patchwork meadow from every camera.
    let sward = vg_noise(q * 0.052 + 11.0) * 0.75 + patches * 0.25;
    let lush = clamp(max(smoothstep(0.36, 0.72, broad * 0.7 + patches * 0.3) * 0.6 + smoothstep(-0.05, -0.8, swell) * 0.45, smoothstep(0.4, 0.28, sward) * 0.9), 0.0, 1.0);
    let fresh = smoothstep(0.45, 0.78, patches) * (1.0 - lush);
    let dry = clamp(max(smoothstep(0.3, 0.95, swell) * 0.8 + smoothstep(0.62, 0.84, vg_noise(vec2f(q.y, -q.x) * 0.052 + 20.0)) * 0.3, smoothstep(0.6, 0.72, sward) * 0.85), 0.0, 1.0)
        * (1.0 - lush * 0.85);
    var tone = mix(vg_linear(0x5b8838), vg_linear(0x34612f), lush);
    tone = mix(tone, vg_linear(0x84a444), fresh * 0.55);
    // Sun-dried crests go olive and tawny, never lemon.
    tone = mix(tone, vg_linear(0x969052), dry * 0.62);
    tone = mix(tone, tone * vec3f(0.9, 0.99, 1.08), smoothstep(0.5, 0.8, patches) * 0.3 * (1.0 - dry));
    // A touch less saturated than a lawn, and lighter and darker over tens of
    // metres, so a meadow never reads as one flat green slab.
    tone = mix(vec3f(dot(tone, VG_LUMA)), tone, 0.7);
    return vec4f(tone * mix(0.82, 1.1, patches) * mix(0.84, 1.12, broad), dry);
}

// The hedgerows parcelling the valley into fields: coverage of a hedge at `p`,
// on gently bending lines a field apart, broken by gaps where gates and tracks
// pass and fading out toward the ranges. Their width swells and narrows bush by
// bush.
fn vg_enclosure_hedge(p: vec2f, aa: f32) -> f32 {
    let lean = vec2f(p.x * 0.96 - p.y * 0.28, p.x * 0.28 + p.y * 0.96);
    let bend = (vec2f(vg_noise(p * 0.011 + 3.0), vg_noise(p * 0.011 + 9.0)) - 0.5) * 22.0;
    let inner = fract((lean + bend) / VG_ENCLOSURE);
    let to_edge = min(min(inner.x, 1.0 - inner.x), min(inner.y, 1.0 - inner.y)) * VG_ENCLOSURE;
    let gap = smoothstep(0.24, 0.32, vg_noise(p * 0.07 + 13.0));
    let hedge_half = 1.1 + (vg_noise(p * 0.45 + 8.0) - 0.5) * 0.7;
    return (1.0 - smoothstep(hedge_half - aa, hedge_half + aa * 2.0 + 0.15, to_edge)) * gap
        * (1.0 - smoothstep(VG_VALLEY_PLAN - 22.0, VG_VALLEY_PLAN - 8.0, max(abs(p.x), abs(p.y))));
}

fn vg_plan_uv(p: vec2f, half: f32, square: f32) -> vec2f {
    let uv = clamp(p / half * 0.5 + 0.5, vec2f(0.5 / VG_PLAN_HEIGHT), vec2f(1.0 - 0.5 / VG_PLAN_HEIGHT));
    return vec2f((square + uv.x) / 3.0, uv.y);
}

// The arena plan's worn dirt at `p` through a cubic B-spline over its texels
// (four bilinear taps at level 0, so it may run in a branch): a road's edge is
// cut from it to a hand's width, and bilinear filtering would bend that cut at
// every texel into a staircase.
fn vg_arena_worn(p: vec2f) -> f32 {
    let texel = clamp(p / VG_ARENA_PLAN * 0.5 + 0.5, vec2f(0.0), vec2f(1.0)) * VG_PLAN_HEIGHT - 0.5;
    let i = floor(texel);
    let f = texel - i;
    let f2 = f * f;
    let f3 = f2 * f;
    let w0 = (1.0 - 3.0 * f + 3.0 * f2 - f3) / 6.0;
    let w1 = (4.0 - 6.0 * f2 + 3.0 * f3) / 6.0;
    let w2 = (1.0 + 3.0 * f + 3.0 * f2 - 3.0 * f3) / 6.0;
    let w3 = f3 / 6.0;
    let g0 = w0 + w1;
    let g1 = w2 + w3;
    let low = clamp(i - 0.5 + w1 / g0, vec2f(0.5), vec2f(VG_PLAN_HEIGHT - 0.5));
    let high = clamp(i + 1.5 + w3 / g1, vec2f(0.5), vec2f(VG_PLAN_HEIGHT - 0.5));
    let scale = vec2f(1.0 / VG_PLAN_WIDTH, 1.0 / VG_PLAN_HEIGHT);
    let a = textureSampleLevel(extra_texture1, extra_sampler1, vec2f(low.x, low.y) * scale, 0.0).r;
    let b = textureSampleLevel(extra_texture1, extra_sampler1, vec2f(high.x, low.y) * scale, 0.0).r;
    let c = textureSampleLevel(extra_texture1, extra_sampler1, vec2f(low.x, high.y) * scale, 0.0).r;
    let d = textureSampleLevel(extra_texture1, extra_sampler1, vec2f(high.x, high.y) * scale, 0.0).r;
    return g0.y * (g0.x * a + g1.x * b) + g1.y * (g0.x * c + g1.x * d);
}

// A crop field's look at `p`: rgb colour and a furrow height in metres. `code`
// is the plan's field byte: crop in the top two bits, row direction in eighths
// of a half turn below.
fn vg_crop(p: vec2f, code: f32, footprint: f32) -> vec4f {
    let crop = floor(code / 64.0);
    let bin = floor((code - crop * 64.0) / 8.0);
    let angle = bin * 0.39269908;
    let across = dot(p, vec2f(-sin(angle), cos(angle)));
    let along = dot(p, vec2f(cos(angle), sin(angle)));
    // Rows fade to their average once they shrink under a few pixels.
    let rows_visible = 1.0 - smoothstep(0.08, 0.22, footprint);
    let sway = vg_fbm(p * 0.07 + crop * 13.0);
    if (crop < 0.5) {
        // Ripe wheat: gold with wind-combed swirls and tractor tramlines.
        let row = sin(across * 9.0) * 0.5 + 0.5;
        var color = mix(vg_linear(0xa88a43), vg_linear(0xcdb46a), smoothstep(0.25, 0.8, sway));
        color = mix(color, vg_linear(0x8f8a46), smoothstep(0.6, 0.9, vg_noise(p * 0.3 + 4.0)) * 0.5);
        // Wind-combed ripples running with the rows.
        let ripple = vg_noise(vec2f(along * 0.12, across * 0.7)) * 0.6 + vg_noise(vec2f(along * 0.4, across * 2.1)) * 0.4;
        color = color * mix(0.86, 1.1, ripple);
        color = color * mix(1.0, mix(0.86, 1.06, row), rows_visible);
        let tram = 1.0 - smoothstep(0.25, 0.45, abs(fract(across / 12.0) - 0.5) * 12.0 - 0.0);
        color = mix(color, vg_linear(0x8f7a3e), tram * 0.55);
        return vec4f(color, row * 0.05 * rows_visible);
    } else if (crop < 1.5) {
        // Young crop in rows over brown soil.
        let row = sin(across * 8.4) * 0.5 + 0.5;
        let leaf = smoothstep(0.35, 0.7, row + (vg_noise(vec2f(across * 8.0, along * 3.0)) - 0.5) * 0.4);
        let soil = vg_linear(0x6e543b);
        let green = mix(vg_linear(0x4f7d2c), vg_linear(0x77a03c), sway);
        let average = mix(soil, green, 0.58);
        return vec4f(mix(average, mix(soil, green, leaf), rows_visible), leaf * 0.08 * rows_visible);
    } else if (crop < 2.5) {
        // Ploughed: dark furrows and sunlit ridges.
        let row = sin(across * 10.5) * 0.5 + 0.5;
        let clod = vg_noise(p * 6.0);
        var color = mix(vg_linear(0x504232), vg_linear(0x88724f), row * 0.7 + clod * 0.3);
        color = mix(mix(vg_linear(0x5b4a37), vg_linear(0x78644a), 0.5), color, rows_visible);
        return vec4f(color * mix(0.92, 1.05, sway), (row * 0.09 + clod * 0.02) * rows_visible);
    }
    // Mown hay: pale stubble with raked windrows.
    let windrow = 1.0 - smoothstep(0.35, 0.8, abs(fract(across / 4.5) - 0.5) * 4.5);
    var color = mix(vg_linear(0x9da35b), vg_linear(0xc2bb72), sway);
    color = mix(color, vg_linear(0xb79a4e), windrow * 0.8);
    let stubble = sin(across * 14.0) * 0.5 + 0.5;
    color = color * mix(1.0, mix(0.94, 1.04, stubble), rows_visible);
    return vec4f(color, windrow * 0.12);
}

// The ground bake: extra texture 2, which a material without a bump map binds in
// that map's slot (`gpu::resources` `texture_slots`).
fn vg_bake_level(uv: vec2f) -> vec4f {
    return textureSampleLevel(bump_texture, bump_sampler, uv, 0.0);
}

fn vg_bake_grad(uv: vec2f, ddx: vec2f, ddy: vec2f) -> vec4f {
    return textureSampleGrad(bump_texture, bump_sampler, uv, ddx, ddy);
}

// The ground is most of every frame's pixels, so the per-pixel work is the little
// that the bake cannot hold: the turf's edge cut to a hand's width, treads,
// puddles, the square's setts, the near pebbles and blooms, the tiles' grain and
// the gusts. Each runs only where it shows; texture samples and derivatives stay
// ahead of the branches, in uniform control flow.
fn village_ground(s: ptr<function, Surface>, f: EffectFragment) {
    let p = f.world.xz;
    let extent = material.params[0].y;
    let widths = material.params[3];
    // Pixel footprint in metres: sub-pixel detail fades instead of shimmering.
    // Mostly the pixel's area rather than its longer side, so the ground at a
    // low, grazing view keeps its pebbles and grain instead of smearing.
    let footprint_x = length(dpdx(f.world));
    let footprint_y = length(dpdy(f.world));
    let footprint = sqrt(footprint_x * footprint_y) * 0.7 + max(footprint_x, footprint_y) * 0.3;
    let aa = footprint * 0.75;
    // Graphics tiers without fine detail (`fine_detail`) draw the ground as a coarser
    // pixel would: no pebbles, blooms, cleats or tussock lift. With it, `detail` is 1.
    let detail = select(0.0, 1.0, fine_detail());
    let fine = (1.0 - smoothstep(0.02, 0.06, footprint)) * detail;
    // Detail a few centimetres across still reads from the gameplay camera.
    let mid = 1.0 - smoothstep(0.07, 0.16, footprint);
    // Detail a few decimetres across: the clumps of the turf's edge.
    let coarse = 1.0 - smoothstep(0.1, 0.24, footprint);
    let inside = 1.0 - smoothstep(extent + 2.0, extent + 3.0, max(abs(p.x), abs(p.y)));
    let sun_flat = normalize(frame.sun_direction.xz + vec2f(1e-4, 0.0));
    let sun_run = min(length(frame.sun_direction.xz) / max(frame.sun_direction.y, 0.2), 2.6);

    // ---- The bake: colours through their mips (anisotropic, at most
    // VG_BAKE_REACH texels wide), the fields at their finest level.
    let dp_x = dpdx(p);
    let dp_y = dpdy(p);
    let row = vg_bake_row(p);
    let to_uv = VG_BAKE_PAGE / (2.0 * select(VG_ARENA_PLAN, VG_VALLEY_PLAN, row > 0.5)) / VG_BAKE_SIZE;
    let reach = max(length(dp_x * to_uv * VG_BAKE_SIZE), length(dp_y * to_uv * VG_BAKE_SIZE));
    let narrow = min(1.0, VG_BAKE_REACH / max(reach, 1e-6));
    let ddx = dp_x * to_uv * narrow;
    let ddy = dp_y * to_uv * narrow;
    let grass_bake = vg_bake_grad(vg_bake_uv(p, row, VG_BAKE_GRASS), ddx, ddy);
    let earth_bake = vg_bake_grad(vg_bake_uv(p, row, VG_BAKE_EARTH), ddx, ddy);
    let fields = vg_bake_level(vg_bake_uv(p, row, VG_BAKE_FIELDS));
    // The turf's edge where the sun's ray over this point meets its rim.
    let lip_fields = vg_bake_level(vg_bake_uv(p + sun_flat * sun_run * VG_TURF_LIP, row, VG_BAKE_FIELDS));
    let long_grass = grass_bake.a;
    let damp_field = vg_damp_decode(earth_bake.a);
    let damp_aa = max(fwidth(damp_field), 0.004);
    let on_square = inside > 0.0 && vg_in_square(p);
    let meadow = select(fields.a, 0.0, on_square);

    // ---- Treads (`village_tracks.rs`): metres to the nearest tread's centre,
    // how fresh it is, the traffic that packed the ground and standing water.
    let tracks = textureSample(extra_texture1, extra_sampler1, vg_plan_uv(p, VG_ARENA_PLAN, 2.0));
    // The tracks where the sun's ray over a rut's floor crosses its rim.
    let tracks_sun = textureSample(extra_texture1, extra_sampler1, vg_plan_uv(p + sun_flat * sun_run * VG_RUT_DEPTH, VG_ARENA_PLAN, 2.0));
    let tread_aa = max(fwidth((1.0 - tracks.r) * VG_TREAD_REACH), aa);
    // The tread's run, from the slope of its proximity ramp across the screen.
    let ramp = vec2f(dpdx(tracks.r), dpdy(tracks.r));
    let jacobian = dp_x.x * dp_y.y - dp_x.y * dp_y.x;
    let tread_across = normalize(vec2f(ramp.x * dp_y.y - ramp.y * dp_x.y, ramp.y * dp_x.x - ramp.x * dp_y.x) * sign(jacobian) + vec2f(1e-6, 0.0));
    var tread_distance = (1.0 - tracks.r) * VG_TREAD_REACH;
    // Old treads survive in broken stretches; a tank sinks into soft stretches and
    // skims hard ones, so a rut widens and darkens and fades along its run.
    var tread_break = 1.0;
    var tread_bite = 1.0;
    if tracks.r > 0.3 {
        let soft_ground = vg_noise(p * 0.16 + 47.0);
        tread_distance -= (soft_ground - 0.5) * 0.1;
        tread_break = smoothstep(0.32, 0.6, vg_noise(p * 0.42 + 19.0) * 0.8 + 0.1 + tracks.g * 0.35);
        tread_bite = mix(0.35, 1.0, smoothstep(0.25, 0.7, soft_ground));
    }
    let fresh = tracks.g * inside;
    let traffic = tracks.b * inside;
    let standing = clamp((1.0 - tracks.a) / 0.6, 0.0, 1.0) * inside;
    let packed = smoothstep(0.2, 0.85, traffic);
    let tread_bed = (1.0 - smoothstep(VG_TREAD_HALF - tread_aa, VG_TREAD_HALF + tread_aa, tread_distance)) * tread_break;
    let tread_berm = smoothstep(VG_TREAD_HALF - 0.02, VG_TREAD_HALF + 0.08, tread_distance)
        * (1.0 - smoothstep(VG_TREAD_HALF + 0.1, VG_TREAD_HALF + 0.34, tread_distance)) * tread_break;
    // Old treads blur into the packed dirt; fresh ones keep sharp walls.
    let rut = tread_bed * smoothstep(0.05, 0.6, fresh) * tread_bite;
    let tread_sun = 1.0 - smoothstep(VG_TREAD_HALF - tread_aa, VG_TREAD_HALF + tread_aa, (1.0 - tracks_sun.r) * VG_TREAD_REACH);
    let rut_sun = tread_sun * smoothstep(0.05, 0.6, tracks_sun.g * inside);
    let berm = tread_berm * smoothstep(0.35, 0.9, fresh);
    // Cleat prints across a fresh tread, gone before they shrink to a few pixels.
    let tread_along = dot(p, vec2f(-tread_across.y, tread_across.x));
    let cleat_fade = (1.0 - smoothstep(0.012, 0.03, footprint)) * smoothstep(0.2, 0.6, fresh) * detail;
    let cleat = smoothstep(0.1, 0.6, cos(tread_along * 28.56)) * rut * cleat_fade;
    // Water stands in the deepest ruts and the junctions' potholes, in a ring of mud.
    let puddle = smoothstep(0.35, 0.65, standing);
    let mud_rim = smoothstep(0.02, 0.24, standing);
    let wet = max(standing * 0.85, rut * fresh * 0.35);

    // ---- Textures: two scales of each tile, turned against each other.
    let turned = vec2f(p.x * 0.6 + p.y * 0.8, p.y * 0.6 - p.x * 0.8);
    let grass_near = textureSample(map_texture, map_sampler, f.uv);
    let grass_far = textureSample(map_texture, map_sampler, turned / 21.0);
    let dirt_near = textureSample(extra_texture0, extra_sampler0, p / 3.1);
    let dirt_far = textureSample(extra_texture0, extra_sampler0, turned / 13.0);
    let grass_luma = dot(grass_near.rgb, VG_LUMA);
    // The tile's blade speckle flattens with distance, so it never sparkles.
    let grass_detail = mix(1.0, grass_luma / 0.3 * mix(1.0, dot(grass_far.rgb, VG_LUMA) / 0.3, 0.45), mix(0.55, 1.0, mid));
    let dirt_luma = dot(dirt_near.rgb, VG_LUMA);
    let dirt_detail = dirt_luma / 0.36 * mix(1.0, dot(dirt_far.rgb, VG_LUMA) / 0.36, 0.5);

    // ---- Where the grass wears through: the bake's cover drive (`meadow_bake.wgsl`),
    // frayed by a hand-scale lift of the tussocks and, close up, the tile's blades,
    // and cut over a hand or two, never under a pixel.
    let cover_static = vg_cover_decode(fields.r);
    var lift = 0.0;
    if detail > 0.0 && abs(cover_static) < 0.9 {
        lift = (vg_noise(p * 2.3 + 7.0) - 0.5) * 0.32 * coarse;
    }
    let fray = (lift + clamp(grass_detail - 1.0, -0.5, 0.5) * 0.2 * fine) * 0.5;
    let cover_drive = cover_static + fray;
    let cover_aa = max(fwidth(cover_drive) * 0.8, 0.012);
    let cover_soft = max(cover_aa * 1.5, 0.15);
    let far_cover = smoothstep(-cover_soft, cover_soft, cover_drive);
    // Treads cut the turf wherever they cross it, and mud rings every puddle.
    let dirt = max(max(1.0 - far_cover, rut), mud_rim);
    // The same over a few centimetres, for the relief.
    let soft_dirt = max(max(1.0 - smoothstep(-0.3, 0.3, cover_drive), rut), mud_rim);
    // The turf stands a hand high and shades the earth below its rim.
    let rim_shade = smoothstep(-cover_soft, cover_soft, vg_cover_decode(lip_fields.r) + fray) * (1.0 - far_cover);
    let clump_shade = rim_shade * 0.45 * dirt * (1.0 - rut);
    let damp = smoothstep(0.58 - damp_aa, 0.58 + damp_aa, damp_field) * dirt;

    // ---- Grass: the bake's meadow under the tile's blades, bowing in the gusts.
    var grass = vec3f(0.0);
    var blooms = vec4f(0.0);
    var bloom_color = vec3f(0.0);
    if dirt < 1.0 {
        grass = grass_bake.rgb * grass_bake.rgb;
        let blades = mix(0.84, 1.16, clamp((grass_detail - 0.55) * 0.9, 0.0, 1.0));
        grass = grass * blades;
        // Gusts roll over the long grass: soft bright swathes where the blades bow
        // and show their pale sides, downwind at one speed (the phase varies with
        // place, never the rate). Mown lawns stand too short to show them.
        if long_grass > 0.0 {
            let wind_along = dot(p, VG_WIND);
            let wind_across = dot(p, vec2f(-VG_WIND.y, VG_WIND.x));
            let gust = vg_noise(vec2f(wind_along * 0.085 - frame.camera_position.w * VG_GUST_SPEED, wind_across * 0.06));
            let sheen = smoothstep(0.55, 0.9, gust) * long_grass;
            grass = mix(grass, grass * vec3f(1.14, 1.14, 1.02) + vg_linear(0x2a2a12) * 0.15, sheen * 0.22);
        }
        // Flower drifts tint the meadow where single blooms are too small to draw.
        grass = mix(grass, vg_linear(0xe8d68a), meadow * (1.0 - fine) * 0.12);
        // Keep the bank tint and the far hills' vertex shading.
        grass = grass * f.vertex_color.rgb;
        // Wildflowers in the meadows: daisies with gold hearts, buttercups,
        // violets and campion.
        blooms = vg_scatter(p, (0.03 + meadow * 0.22) * fine, 1.9, vec2f(0.035, 0.06), aa, 27.0);
        if blooms.x > 0.0 {
            let bloom_pick = blooms.z;
            var petals = vg_linear(0xf3f0e4);
            petals = select(petals, vg_linear(0xf2c94a), bloom_pick > 0.55);
            petals = select(petals, vg_linear(0xb08fe0), bloom_pick > 0.78);
            petals = select(petals, vg_linear(0xec9fb8), bloom_pick > 0.92);
            let heart = smoothstep(0.55, 0.3, blooms.y / 0.05) * select(0.0, 1.0, bloom_pick <= 0.55);
            bloom_color = mix(petals, vg_linear(0xe7b52d), heart);
        }
    }

    // ---- The village square: setts laid over the heart of the village inside a
    // kerb of long edge stones, earth drifted in over them (the bake's silt), the
    // kerb shading the road beside it.
    var sett = VgSett(0.0, vec2f(0.0), 0.0, vec2f(0.0), 0.0, 0.0, 0.0);
    var paved = 0.0;
    var silted = 0.0;
    var kerb_shade = 0.0;
    var paved_soft = 0.0;
    // Single stones and joints settle to their average before they could shimmer.
    let joint_view = 1.0 - smoothstep(0.025, 0.075, footprint);
    if on_square {
        let inward = vg_square(p);
        if inward > -max(aa, 0.06) {
            paved = 1.0 - smoothstep(-aa, aa, -inward);
            paved_soft = 1.0 - smoothstep(-0.06, 0.06, -inward);
            sett = vg_setts(p, aa);
            let earth = fields.a * VG_SILT_RANGE;
            // The joints fill first and the proudest crowns last; far off the
            // stones' average height decides.
            let level = mix(0.5, sett.height, joint_view);
            silted = smoothstep(level - 0.1, level + 0.1, earth * 1.3 - 0.18) * (1.0 - sett.kerb * 0.45);
        }
        if inward < 0.0 && inward > -0.6 {
            let foot = 1.0 - smoothstep(0.0, 0.12, -inward);
            let shaded = select(0.0, 1.0, vg_square(p + sun_flat * sun_run * VG_KERB_RISE) > 0.0);
            kerb_shade = max(foot * 0.5, shaded);
        }
    }
    let unpaved = 1.0 - paved * 0.9;

    // ---- Dirt: the bake's earth with the tile's grain, combed along the busy
    // lines, damp in the hollows and cut by the treads.
    var soil = vec3f(0.0);
    if dirt > 0.0 || paved > 0.0 {
        soil = earth_bake.rgb * earth_bake.rgb;
        // Which way the road runs, for streaks combed along it.
        // Packed lanes are combed with streaks along the road's run.
        if packed > 0.0 {
            let along = vg_strip(p.x, p.y, material.params[1].xyz, widths.x, widths.y, extent);
            let across = vg_strip(p.y, p.x, material.params[2].xyz, widths.z, widths.w, extent);
            let lane = select(vec2f(across.y, p.x), vec2f(along.y, p.y), along.x > across.x);
            let streaks = vg_noise(vec2f(lane.x * 6.0, lane.y * 0.09));
            soil = soil * (1.0 + (streaks - 0.5) * 0.34 * smoothstep(0.0, 1.0, packed) * 0.9);
        }
        // The tile's grain at full strength, so the roads never read as a smear.
        let dirt_tex = mix(vec3f(dirt_luma), dirt_near.rgb, 0.45);
        soil = soil * clamp(1.0 + (dirt_detail / 0.8 - 1.0) * mix(1.5, 1.1, packed), 0.6, 1.4)
            * (dirt_tex / max(dirt_luma, 0.05) * 0.3 + 0.7);
        let damp_rim = (1.0 - smoothstep(0.0, 0.025, abs(damp_field - 0.58))) * dirt * (1.0 - damp);
        soil = mix(soil, soil * vec3f(0.8, 0.79, 0.8), damp);
        // The soaked mud round each puddle.
        soil = mix(soil, soil * vec3f(0.62, 0.6, 0.58), mud_rim * (1.0 - puddle) * 0.85);
        soil = mix(soil, soil * vec3f(0.9, 0.88, 0.86), damp_rim * 0.5);
        soil = mix(soil, soil * vec3f(1.14, 1.11, 1.05), berm * 0.75);
        // Ruts: a worn-out tread is only pressed a little paler into the dirt;
        // fresh ones cut deeper and darker, with cleat prints across them.
        soil = mix(soil, soil * vec3f(1.04, 1.035, 1.02), tread_bed * (1.0 - smoothstep(0.2, 0.6, fresh)) * 0.5);
        soil = mix(soil, soil * vec3f(0.66, 0.62, 0.6), rut * smoothstep(0.25, 0.85, fresh) * 0.8);
        soil = mix(soil, soil * vec3f(0.8, 0.78, 0.76), cleat * 0.5);
        soil = mix(soil, soil * vec3f(0.6, 0.6, 0.66), clump_shade * 0.65);
        soil = mix(soil, soil * vec3f(0.55, 0.53, 0.5), wet);
    }

    // The setts: warm-grey granite with the odd sandstone or dark whinstone, the
    // kerb a paler, finer granite, the lanes' crowns polished and their joints
    // packed with earth. Treads print only dust on the stone, never a rut.
    var paving = vec3f(0.0);
    var sett_view = 0.0;
    if paved > 0.0 {
        sett_view = 1.0 - smoothstep(0.03, 0.09, footprint);
        let pick = sett.pick;
        let kind = fract(pick.x * 13.1);
        var stone = mix(vg_linear(0x7e7870), vg_linear(0xa1988a), pick.y);
        stone = select(stone, mix(vg_linear(0x977f62), vg_linear(0xb09872), pick.y), kind > 0.76);
        stone = select(stone, vg_linear(0x86684f), kind > 0.94);
        stone = select(stone, mix(vg_linear(0x524e48), vg_linear(0x68625a), pick.y), fract(pick.x * 5.7) < 0.12);
        stone = mix(stone, mix(vg_linear(0x9b917f), vg_linear(0xafa48e), pick.x), sett.kerb);
        stone = mix(stone, mix(vg_linear(0x4f4b45), vg_linear(0x67615a), pick.x), sett.course);
        // Far off each stone settles into the paving's warm average.
        let average = mix(vg_linear(0x6c5f50), vg_linear(0x8d7c66), 0.5);
        stone = mix(mix(mix(average, vg_linear(0x9a907c), sett.kerb * 0.8), vg_linear(0x4f4a44), sett.course), stone, mix(0.15, 1.0, joint_view));
        stone = stone * mix(1.0, mix(0.9, 1.08, dirt_detail * 0.5), mid);
        let lane = smoothstep(0.15, 0.75, packed);
        stone = stone * mix(1.0, mix(0.88, 1.08 + lane * 0.06, sett.crown), sett_view);
        // Each stone weathered its own way, the lanes rubbed darker down their middles.
        stone = stone * mix(1.0, mix(0.72, 1.12, fract(pick.y * 7.9 + pick.x * 2.3)), joint_view);
        stone = mix(stone, stone * vec3f(0.84, 0.82, 0.8), lane * 0.45);
        let dust_tone = mix(vec3f(dot(soil, VG_LUMA)), soil, 0.6) * 1.15;
        let dust = clamp(0.1 + (1.0 - lane) * 0.2 + (1.0 - sett.crown) * 0.35, 0.0, 0.75) * (1.0 - lane * 0.5);
        stone = mix(stone, dust_tone, dust * 0.42);
        // From the gameplay camera the joints are a darker grain, not a grid.
        let joint = mix(stone * 0.78, mix(vg_linear(0x3b2f23), vg_linear(0x3c4a24), (1.0 - lane) * 0.4), joint_view);
        paving = mix(joint, stone, mix(0.8, sett.stone, joint_view));
        paving = mix(paving, mix(paving, soil * 1.2, 0.45), tread_bed * smoothstep(0.1, 0.6, fresh) * 0.4 * mix(0.5, 1.0, sett.crown));
        paving = mix(paving, mix(vec3f(dot(soil, VG_LUMA)), soil, 0.7) * mix(0.78, 0.95, silted), silted);
        paving = mix(paving, paving * vec3f(0.62, 0.6, 0.58), wet * 0.6);
    }

    // ---- Pebbles kicked out of the ruts onto the loose ground and the odd
    // fist-sized stone, thickest along the shoulders, each with its shade cast away
    // from the sun, in clusters with bare earth between: only where each spans a
    // few pixels.
    var stones = VgStone(0.0, 0.0, vec2f(0.0), 0.0, 1.0, 0.0);
    let loose_ground = dirt * (1.0 - rut) * unpaved * fine;
    if loose_ground > 0.0 {
        // Stones gather on the dry crests where the rain washed the topsoil off, and
        // along the ruts' berms; the damp hollows keep their earth.
        let cluster = smoothstep(0.45, 0.8, 0.85 - damp_field + berm * 0.25);
        // The road's shoulder: the earth just off the turf.
        let shoulder = smoothstep(-0.7, -0.35, cover_static) * (1.0 - smoothstep(-0.1, 0.1, cover_static));
        let sun_shift = sun_flat * min(sun_run, 1.6);
        let density = (0.12 + shoulder * 0.3 + (1.0 - packed) * 0.1) * mix(0.15, 2.2, cluster) * loose_ground;
        stones = vg_stones(p, density, 2.5, vec2f(0.025, 0.075), vec2f(0.06, 0.1), 0.12, aa, 3.0, sun_shift * 0.04, sun_flat * 0.025);
    }

    // ---- Combine colour, height and finish.
    var color = mix(grass, soil, dirt);
    // Where the turf thins into the earth the blades' bases and the damp soil
    // between them sit in their own shade.
    color = color * (1.0 - dirt * (1.0 - dirt) * 0.7);
    color = mix(color, paving, paved);
    color = mix(color, color * vec3f(0.56, 0.56, 0.62), kerb_shade * 0.55 * (1.0 - paved));
    color = mix(color, color * mix(vec3f(0.58, 0.58, 0.64), vec3f(0.5, 0.5, 0.57), stones.big), stones.shade * mix(0.7, 0.8, stones.big));
    // Pebbles half sunk in the earth, its colour with a little grey, lit on their
    // sunward crown; the big stones half buried, so they take the earth's warmth
    // and dust, lit from the dome's height with a dark foot.
    let pebble_crown = smoothstep(0.0, 0.85, clamp(stones.dome / 0.06, 0.0, 1.0));
    let pebble_tone = mix(vg_linear(0x5e5850), vg_linear(0x8e877a), stones.pick.x) * mix(0.62, 1.0, pebble_crown) * mix(0.85, 1.25, stones.lit);
    let pebble = mix(pebble_tone, soil * mix(0.62, 1.12, pebble_crown), 0.4 + stones.pick.y * 0.3);
    let crown = clamp(stones.dome / 0.09, 0.0, 1.0);
    let boulder = mix(soil * 0.8, mix(vg_linear(0x67625a), vg_linear(0x8f887a), stones.pick.x), 0.45) * mix(0.6, 1.0, smoothstep(0.0, 0.85, crown)) * mix(0.72, 1.3, stones.lit);
    color = mix(color, mix(pebble, boulder, stones.big), stones.cover);
    color = mix(color, bloom_color, blooms.x * (1.0 - dirt) * (1.0 - paved));
    // Puddles: muddy water, dark where it is deep, mirroring the pale sky
    // overhead and the hills at the horizon when the view grazes it.
    var mirror = vec3f(0.0);
    if puddle > 0.0 {
        let view = normalize(frame.camera_position.xyz - f.world);
        let depth = smoothstep(0.35, 1.0, standing);
        let water = mix(color * vec3f(0.36, 0.33, 0.29), vg_linear(0x221d16), depth * 0.75);
        let mirrored = mix(vg_linear(0x6d7d74), vg_linear(0xaebfcb), smoothstep(0.1, 0.55, view.y));
        let fresnel = 0.05 + 0.4 * pow(1.0 - clamp(view.y, 0.0, 1.0), 4.0);
        color = mix(color, water, puddle);
        // Its own light, so the reflection holds in shade too; the surface stays
        // matte to the lighting, which would mirror the whole bright sky as foil.
        mirror = mirrored * fresnel * 0.7 * puddle;
    }
    // The rut walls' shade on their floors: cool, since only the sky lights them.
    let rut_shadow = rut * (1.0 - rut_sun) * dirt * (1.0 - paved);
    color = mix(color, color * vec3f(0.55, 0.57, 0.66), rut_shadow * 0.45 * (1.0 - puddle));

    // Detail tuned for the gameplay camera tilts by its height per pixel, so
    // close up it would flatten out: nearer views keep their grain and lumps.
    let near_boost = clamp(0.045 / max(footprint, 1e-4), 1.0, 4.0);
    var relief = mix(grass_detail * 0.02 + 0.012 + VG_TURF_LIP * 0.25, dirt_detail * 0.02 - rut * 0.1 + berm * 0.025, soft_dirt) * (1.0 - paved_soft * (1.0 - silted * 0.5))
        + paved_soft * VG_KERB_RISE * 0.4
        + stones.dome * mix(0.6, 0.7, stones.big) + cleat * 0.012 * dirt * (1.0 - paved) + blooms.y * 0.3 * (1.0 - dirt);
    relief = relief * near_boost;
    let micro_normal = vg_bump(mix(relief, -0.08, puddle), f.world, (*s).normal, f.front_facing);
    // The lie of the land in true slopes (the bake's): swells and hollows over the
    // meadow, the banks where lawns stand a little proud of their worn roads, the
    // earth's mounds. Visual only; the floor stays flat. Each sett's worn crown and
    // its tilt come from its own shape (its slope runs downhill).
    let land = vec3f(vg_slope_decode(fields.g), 0.0, vg_slope_decode(fields.b));
    var tilt = land - (*s).normal * dot((*s).normal, land);
    tilt = tilt - vec3f(sett.slope.x, 0.0, sett.slope.y) * paved * sett_view * (1.0 - silted * 0.8) * inside;
    let normal = normalize(micro_normal - tilt * (1.0 - puddle));

    // Cloud shadows drift over the valley; the arena keeps the steady sun its
    // shadows are read against.
    if inside < 1.0 {
        color = color * mix(vg_cloud_light(p), 1.0, inside);
    }

    (*s).color = color * material.color.rgb * f.tint.rgb;
    (*s).normal = normalize(mix(normal, (*s).normal, puddle));
    (*s).emissive = (*s).emissive + mirror;
    (*s).roughness = mix(mix(mix(mix(0.95, mix(0.92, 0.82, packed), dirt), 0.68, wet * 0.6), 0.55, damp * 0.8), 0.62, puddle);
    // Worn setts take a dull sheen on their crowns.
    (*s).roughness = mix((*s).roughness, mix(0.8, 0.6, smoothstep(0.15, 0.75, packed) * sett.crown), paved * sett.stone * (1.0 - silted) * (1.0 - puddle));
    (*s).opacity = material.color.a * f.tint.a;
}

// Ridged relief on one plane: 0 in the creases, 1 on the ribs.
fn vg_ridged(q: vec2f) -> f32 {
    let r = 1.0 - abs(vg_noise(q) * 2.0 - 1.0);
    return r * r;
}

// One rank of conifer tips along a wood's edge, as a painted backdrop draws
// it: overlapping narrow triangles, each its own height, on a run `u` along
// the face (metres). x how far the tips rise above the rank's base (metres),
// y which side of its tree the point lies, -1 to 1 across it.
fn vg_tips(u: f32, seed: f32) -> vec2f {
    let cell = floor(u / VG_TIP_SPACING);
    var best = vec2f(0.0, 0.0);
    for (var j = -1; j <= 1; j++) {
        let c = cell + f32(j);
        let pick = vg_hash2(vec2f(c, seed));
        let centre = (c + 0.2 + pick.x * 0.6) * VG_TIP_SPACING;
        let tall = VG_TIP_HEIGHT * mix(0.55, 1.25, pick.y);
        let half = tall * 0.3;
        let offset = u - centre;
        let rise = tall * (1.0 - abs(offset) / half);
        if rise > best.x {
            best = vec2f(rise, clamp(offset / half, -1.0, 1.0));
        }
    }
    return best;
}

fn village_ranges(s: ptr<function, Surface>, f: EffectFragment) {
    let p = f.world;
    let base = (*s).color;
    let footprint = max(length(dpdx(p)), length(dpdy(p)));
    let smooth_normal = (*s).normal;
    let up = smooth_normal.y;
    // Rock relief: ribs and gullies running down the fall line of the steep
    // faces (the noise is stretched along y), broken outcrops on gentle ground.
    var weights = pow(abs(smooth_normal), vec3f(4.0));
    weights = weights / (weights.x + weights.y + weights.z);
    // The finer octave only where it shows, and with the tier's fine detail.
    let fine = (1.0 - smoothstep(0.35, 1.2, footprint)) * select(0.0, 1.0, fine_detail());
    var ribs_x = vg_ridged(vec2f(p.z * 0.085, p.y * 0.05)) * 0.75;
    var ribs_z = vg_ridged(vec2f(p.x * 0.085, p.y * 0.05) + 9.1) * 0.75;
    var outcrop = vg_ridged(p.xz * 0.06 + 1.3) * 0.75;
    if fine > 0.0 {
        ribs_x += vg_ridged(vec2f(p.z * 0.23, p.y * 0.14) + 3.7) * 0.25 * fine;
        ribs_z += vg_ridged(vec2f(p.x * 0.23, p.y * 0.14) + 5.3) * 0.25 * fine;
        outcrop += vg_ridged(p.xz * 0.17) * 0.25 * fine;
    }
    let crag = ribs_x * weights.x + outcrop * weights.y + ribs_z * weights.z;
    let relief_fade = 1.0 - smoothstep(1.5, 4.0, footprint);
    // A touch of the mesh's own planes keeps the rock crisp without showing
    // the grid as facets.
    var flat_normal = normalize(cross(dpdx(p), dpdy(p)));
    flat_normal = flat_normal * sign(dot(flat_normal, smooth_normal));
    let planar = normalize(mix(smooth_normal, flat_normal, 0.08));
    let crag_normal = vg_bump(crag * 1.6 * relief_fade, p, planar, f.front_facing);

    // Bands, broken by the relief and broad noise so no contour line shows.
    let band_noise = vg_fbm(p.xz * 0.016 + 30.0);
    let treeline = 22.0 + band_noise * 15.0;
    // The snow reaches lower down the shaded northern ranges and the big
    // massifs than on the sunny shoulders, so no one contour runs across them.
    let snowline = 41.0 + band_noise * 12.0 + (vg_noise(p.xz * 0.0055 + 12.0) - 0.5) * 18.0;
    let meadow = 1.0 - smoothstep(5.0, 11.0, p.y + (band_noise - 0.5) * 6.0);

    // Rock: light, warm ribs over cool, dark gullies, faint bedding across the
    // faces, mossy toward its foot.
    let tone = vg_fbm(p.xz * 0.04 + p.y * 0.015);
    let bedding = sin(p.y * 0.9 + vg_noise(p.xz * 0.05) * 6.0) * 0.5 + 0.5;
    // Painted value groups: the relief splits into crisp lit ribs and shaded
    // gullies, with a soft step between, rather than a smooth airbrush.
    let rock_value = crag * 0.9 + tone * 0.15;
    let rock_aa = max(fwidth(rock_value), 0.02);
    let rock_step = smoothstep(0.45 - rock_aa, 0.45 + rock_aa, rock_value) * 0.6 + smoothstep(0.7 - rock_aa, 0.7 + rock_aa, rock_value) * 0.4;
    // Mostly the light models the rock (`crag_normal`); the colour keeps only a
    // gentle value step between ribs and gullies, so the faces never break into
    // spots.
    var rock = mix(vg_linear(0x7b766b), vg_linear(0xa69d8b), mix(smoothstep(0.1, 0.8, rock_value), rock_step, 0.15));
    rock = rock * mix(0.96, 1.03, bedding * fine);
    rock = mix(rock, rock * vec3f(0.8, 0.84, 0.9), (1.0 - smoothstep(0.04, 0.4, crag)) * 0.25 * (1.0 - weights.y * 0.6));
    rock = mix(rock, vg_linear(0x5b684f), (1.0 - smoothstep(14.0, 30.0, p.y + crag * 6.0)) * 0.55);

    // Derivatives first, ahead of the woods' loop.
    // Snow lies by height and slope: whole fields on the gentle upper faces,
    // tongues down the gullies between the ribs (the relief runs down the fall
    // line on steep faces), the steepest rock and the ribs' crests blown bare.
    // No blotchy noise of its own, so it never spots the rock like a hide.
    let gully = (0.5 - crag) * (1.0 - weights.y * 0.7);
    let snow_drive = (p.y - snowline) / 3.5 + (up - 0.55) * 1.6 + gully * 1.1;
    let snow_aa = clamp(fwidth(snow_drive), 0.04, 0.35);

    // Forest: a painted matte of conifer woods. Each stand is a dark mass of
    // crowns under a serrated edge of tree tips, with more ranks of tips
    // stepping down inside it, lit along their tops and shading toward their
    // feet, so the wood reads as layered trees at any distance without a print
    // of single trees that would streak on a slope seen edge-on or shimmer far
    // off. The wood opens into glades, breaks up at a ragged treeline, keeps
    // off the cliffs and leaves grassy avalanche chutes down some gullies.
    // The wood clothes every face below the treeline but the cliffs, so the
    // ranges read in clean painted bands (wood, rock, snow) with a crisp edge
    // where a cliff breaks through, never a soft slope mask fogging the face.
    let slope_drive = up + crag * 0.1;
    let slope_aa = clamp(fwidth(slope_drive), 0.01, 0.08);
    let slope_ok = smoothstep(0.4 - slope_aa, 0.4 + slope_aa, slope_drive);
    // Avalanche chutes: a few clean grassy lanes down the deepest gullies,
    // crisp-edged like the wood they cut through.
    let chute_drive = vg_noise(vec2f(p.x * 0.02 + p.z * 0.02, 3.0)) - crag * 0.5;
    let chute_aa = clamp(fwidth(chute_drive), 0.01, 0.08);
    let chute = smoothstep(0.62 - chute_aa, 0.62 + chute_aa, chute_drive) * (1.0 - weights.y);
    let stand = slope_ok * (1.0 - chute * 0.9);
    let stands = vg_fbm(p.xz * 0.012 + 50.0);
    // Glades open only on gentle ground, where they hold grass: on a steep face
    // a gap in the wood would show bare rock as a pale blot.
    let glade = smoothstep(0.31, 0.24, vg_noise(p.xz * 0.022 + 3.1) * 0.75 + vg_noise(p.xz * 0.07 + 1.7) * 0.25
        + smoothstep(14.0, 40.0, p.y) * 0.07) * smoothstep(0.66, 0.78, up);
    // The tips run along whichever horizontal axis the face lies across.
    let face_dir = normalize(smooth_normal.xz + vec2f(1e-4, 0.0));
    let along_z = smoothstep(0.35, 0.65, abs(face_dir.x));
    let tip_aa = max(footprint * 0.7, 0.04);
    let edge = treeline + (vg_noise(p.xz * 0.03 + 7.0) - 0.5) * 9.0 - VG_TIP_HEIGHT;
    // The ranks repeat down the face, each lower one in front of the one above
    // it: the point belongs to the lowest rank whose tips rise over it, how
    // far below those tips it lies, and which side of its tree.
    var cover = 0.0;
    var depth = 0.0;
    var side = 0.0;
    var rank_tone = 1.0;
    let first_rank = max(floor((edge - p.y) / VG_RANK_STEP), 0.0);
    for (var j = 0; j < 3; j++) {
        let k = first_rank + f32(j);
        let seed = k * 17.0 + 3.0;
        let tips = mix(vg_tips(p.x, seed), vg_tips(p.z, seed + 5.0), along_z);
        let top = edge - k * VG_RANK_STEP + tips.x;
        let under = smoothstep(-tip_aa, tip_aa, top - p.y);
        depth = mix(depth, top - p.y, under);
        side = mix(side, tips.y, under);
        rank_tone = mix(rank_tone, mix(0.9, 1.0, vg_hash(vec2f(k, 41.0))), under);
        cover = max(cover, under);
    }
    let below = cover;
    let sun_along = mix(frame.sun_direction.x, frame.sun_direction.z, along_z);
    let sunward = clamp(side * sign(sun_along + 1e-4), -1.0, 1.0);
    let needles_dark = mix(vg_linear(0x15301f), vg_linear(0x203f28), smoothstep(0.3, 0.7, stands));
    let needles_lit = mix(vg_linear(0x34603a), vg_linear(0x4e7a3d), smoothstep(0.35, 0.75, stands));
    // Low on the slopes the woods are warmer and lighter, near the treeline
    // dark and blue; drifts of larch go yellow-green among the spruces.
    let height_tint = mix(vec3f(1.1, 1.08, 0.88), vec3f(0.84, 0.92, 1.06), smoothstep(4.0, treeline, p.y));
    // Each tree lit down its sunward flank from the tip, the crowns behind
    // and below it in shade; the deeper ranks a little darker.
    let tip_light = (1.0 - smoothstep(0.0, VG_RANK_STEP * 1.1, depth)) * mix(0.45, 1.0, smoothstep(-0.4, 0.6, sunward));
    var canopy = mix(needles_dark, needles_lit, tip_light);
    canopy = canopy * rank_tone * mix(0.94, 1.05, stands) * height_tint;
    let larch_drift = smoothstep(0.7, 0.86, vg_noise(p.xz * 0.06 + 3.0));
    canopy = mix(canopy, vg_linear(0x66803a) * height_tint * mix(0.7, 1.05, tip_light), larch_drift * 0.3);
    // Rock breaks through the wood only where the face turns to cliff
    // (`slope_ok`): the ridged relief's crest lines would cut pale worms
    // through the canopy.
    let forest = stand * below * (1.0 - glade);
    var color = rock;
    color = mix(color, canopy, forest);
    // Alpine grass above the wood, in its glades and down the chutes.
    let pasture = max(max(smoothstep(-6.0, 0.0, treeline - p.y) * slope_ok, chute * slope_ok * below), glade * stand * below)
        * (1.0 - forest);
    // Muted and tawny, so the high pastures never read as bright green
    // blotches on the rock.
    color = mix(color, vg_linear(0x6c7a4c) * mix(0.88, 1.06, tone), pasture * max(0.5 * smoothstep(0.6, 0.85, up), max(chute, glade) * 0.85));
    // The wood lights as a soft mass of crowns over the face.
    var normal = normalize(mix(crag_normal, normalize(smooth_normal + vec3f(0.0, 0.35, 0.0)), forest));

    // Snow: crisp fields above the snowline on the gentler faces, fingers of it
    // down the gullies, the steep ribs blown bare.
    let snow = smoothstep(-snow_aa, snow_aa, snow_drive) * smoothstep(max(snowline - 14.0, treeline + 6.0), max(snowline - 6.0, treeline + 13.0), p.y);
    let snow_light = mix(vg_linear(0xbfcde0), vg_linear(0xf5f7f5), smoothstep(0.3, 0.85, up));
    color = mix(color, snow_light, snow);
    normal = normalize(mix(normal, normalize(mix(crag_normal, smooth_normal, 0.5)), snow));
    // Low meadows keep their vertex colour, mottled.
    color = mix(color, base * mix(0.9, 1.08, vg_fbm(p.xz * 0.08)), meadow * 0.8);
    // Cloud shadows drift over the ranges as they do over the valley, kept
    // faint so they never read as stains on the rock.
    color = color * mix(1.0, vg_cloud_light(p.xz), 0.1);
    // Aerial perspective: the ranges lose their colour and pale into the air
    // with distance, so the farther slopes stand behind the nearer ones and
    // the backdrop never competes with the village for value. Part of the
    // haze is light of its own, so the shaded faces fade into it too.
    // Nearer ridges keep their colour and contrast, the farther ones layer
    // back into the air, so each range reads as its own silhouette.
    // The ramp shifts toward the sky's blue rather than greying, so the far
    // ranges stay clean and cool instead of muddy.
    let air = smoothstep(90.0, 520.0, length(p - frame.camera_position.xyz));
    let haze = vg_linear(0x9db5c9);
    color = mix(color, vec3f(dot(color, VG_LUMA)), air * 0.15);
    color = mix(color, haze * mix(0.82, 1.0, up), air * (0.35 + air * 0.25));
    (*s).emissive = (*s).emissive + haze * air * 0.1;

    (*s).color = color;
    (*s).normal = normalize(mix(normal, smooth_normal, meadow * 0.7));
    (*s).roughness = mix(0.94, 0.6, snow);
}

// The tufts' surface (mode 0).
fn vg_tuft_surface(s: ptr<function, Surface>, f: EffectFragment) {
    // Tufts light like the turf they grow from from either side of a blade:
    // a back face keeps its upward normal instead of turning black.
    (*s).normal = (*s).normal * select(1.0, -1.0, (*s).normal.y < 0.0);
    // Each tuft takes most of its colour from the meadow it stands in (the bake's
    // grass a couple of metres round it), keeping its own shade and the
    // root-to-tip ramp of its blades.
    let shade = dot(f.tint.rgb, VG_LUMA) / VG_TUFT_LUMA;
    let o = f.instance_data.xy;
    let meadow = textureSampleLevel(extra_texture0, extra_sampler0, vg_bake_uv(o, vg_bake_row(o), VG_BAKE_GRASS), 2.0).rgb;
    let turf = meadow * meadow * f.vertex_color.rgb * shade;
    var tuft = mix((*s).color, turf * material.color.rgb, 0.65);
    // Some clumps have gone to seed or dried at their tips, a few are a
    // fresh, bluer green: the tips (bright vertex colour) take it most.
    let pick = vg_hash(f.instance_data.xy * 0.731 + 11.0);
    let tip = smoothstep(0.7, 1.1, f.vertex_color.g);
    let seeded = smoothstep(0.62, 0.81, pick) * tip;
    tuft = mix(tuft, vg_linear(0x9c9160) * shade * f.vertex_color.rgb, seeded * 0.55);
    tuft = mix(tuft, tuft * vec3f(0.86, 0.98, 1.02), smoothstep(0.25, 0.08, pick) * 0.6);
    (*s).color = tuft;
}

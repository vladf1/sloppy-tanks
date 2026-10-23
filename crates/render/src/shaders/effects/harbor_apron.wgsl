// Harbor Havoc's surfaces worked out in world space (`effects_scenery::HARBOR_APRON`),
// so slabs, stains and wear never repeat with a texture tile. Extra texture 0 is
// the dock concrete tile, the grain of every mode; its broad blotches are divided
// out (a high-pass against a blurred sample) so they never stamp a pattern.
// params[0].x is the `ApronMode`: 0 concrete, 1 asphalt lane along z, 2 asphalt
// lane along x, 3 alpha-tested marking, 4 transparent label (its map is the
// lettering), 5 quay wall faces (the horizontal coordinate follows the face, y
// climbs out of the water), 6 ship hull plating (7, the far shore, is the
// `harbor_shore.wgsl` effect of its own), 8 a spawn pad's
// steel deck and its light ring (team paint in the vertex color, the pad's own
// coordinates in the uv, offset by `PAD_LIGHT_FLAG` on the ring), 10 a cover
// container's paint (standard, over its wear tile), shaded per box, 11 the sea
// wall's parapet round the apron.
//
// The modes fall into three kinds, one shader for all of them (`registry.rs`): the
// ground (`apron_ground`, concrete and asphalt), the paint blended over it
// (`apron_paint`) and the structures (`apron_structure`).
// The ground covers most of the screen, so it works out only its crisp detail per
// pixel (grain, slab joints and tones, tar, covers, the asphalt's aggregate, tyre
// streaks, sections and gutters); everything that varies slowly across the
// yard (grime, stains, oil, wheel tracks, polish, prints, damp and puddles, contact
// shade, the asphalt's oxidation and wheel paths) is baked once into the wear texture,
// extra texture 1 (`harbor_apron_wear.wgsl`): worked out per pixel, those layers cost
// the harbor's main view several milliseconds a frame.

const APRON_LUMA = vec3f(0.2126, 0.7152, 0.0722);
// Slab edge in metres; the joints are sawn on this grid and every third one is a
// sealed expansion joint.
const APRON_SLAB = 6.0;
const APRON_EXPANSION = 18.0;
// Half width of a sawn joint and of an expansion joint's sealant, metres.
const APRON_JOINT = 0.014;
const APRON_SEALANT = 0.022;
// Width of the worn arris either side of a joint, metres: it catches the sun on
// one side and falls into shade on the other.
const APRON_ARRIS = 0.07;
// Mean linear luminance of the dock tile, so its grain multiplies around 1.
const APRON_TILE_MEAN = 0.2;
// Mip bias of the blurred grain sample that the high-pass divides by.
const APRON_GRAIN_BLUR = 5.0;
// Oil spills, utility covers, the yard's old weathered stains, the rubber arcs of
// turning trucks and ghosts of removed paint are scattered on these grids, metres.
const APRON_STAIN_CELL = 7.0;
const APRON_COVER_CELL = 13.0;
const APRON_OLD_STAIN_CELL = 17.0;
const APRON_SCUFF_CELL = 15.0;
const APRON_GHOST_CELL = 11.0;
const APRON_TRACK_CELL = 9.0;
// Some cells of this grid keep the print of a container that stood there for a
// season: half its width and half the lengths of a 20 and a 40 foot box, metres.
const APRON_FOOTPRINT_CELL = 16.0;
const APRON_FOOTPRINT_HALF_WIDTH = 1.22;
const APRON_FOOTPRINT_HALF_LENGTHS = vec2f(3.03, 6.1);
// Linear color of the tar sealant run into the joints and over the worst cracks.
const APRON_TAR = vec3f(0.028, 0.027, 0.028);
// Length of a relaid stretch of asphalt lane, metres.
const APRON_SECTION = 23.0;
// Harbor water level (`harbor_water::HARBOR_WATER_HEIGHT`) and how far the tide
// stains the quay wall above it.
const APRON_SEA_LEVEL = -2.2;
const APRON_TIDE = 1.0;
// Spacing of the quay wall's vertical expansion joints, metres.
const APRON_WALL_JOINT = 12.0;
// Farthest a container's casting rust reaches from the casting, metres (with a
// margin over the 1.2 m its widest fade spans).
const APRON_CASTING_REACH = 1.25;
// Inner face of the boundary wall (`sim::data::ARENA`).
const APRON_ARENA = 60.0;
// How far into the bays the trucks' rubber polishes the slabs beside a lane, metres.
const APRON_POLISH_REACH = 9.0;
// Distance of a truck's wheel paths from the middle of its traffic half, metres.
const APRON_WHEEL_OFFSET = 0.9;
// Width of the drainage channel along a lane's open edges (outside its painted
// edge line, `harbor_scenery::EDGE_LINE_INSET`), and the pitch of its gratings.
const APRON_GUTTER = 0.26;
const APRON_GRATE_PITCH = 13.0;
// Traffic lanes as (centre, half width): along z (centre x), then along x (centre
// z), as `harbor_scenery::LANES_ALONG_Z` and `LANES_ALONG_X`, and how far each
// runs from the arena's centre along its length.
const APRON_LANES_Z = array<vec2f, 3>(vec2f(-52.0, 5.0), vec2f(0.0, 7.5), vec2f(52.0, 5.0));
const APRON_LANES_X = array<vec2f, 3>(vec2f(-45.0, 5.0), vec2f(0.0, 5.0), vec2f(45.0, 5.0));
const APRON_LANE_REACH = 59.0;
// Fixed cover standing on the apron as (centre x, centre z, half x, half z), as
// laid out in `harbor_layout`: the twelve indestructible containers, which rust
// where their castings stand, and six concrete barriers. Ground beside them
// darkens with contact occlusion and collects grime. The layout is symmetric, so
// each set is listed for one quadrant: the containers and the side barriers
// mirror across both axes (measured from |x|, |z|), the end barriers across the
// centre (measured from the point turned into the north half). Of a box and its
// mirror images the one in the point's own quadrant is the nearest whenever any
// is within 54 m, far beyond the 7 m where the ground stops taking their shade.
const APRON_CONTAINER_QUADRANT = array<vec4f, 3>(
    vec4f(30.0, 12.0, 3.0, 7.0),
    vec4f(30.0, 32.0, 3.0, 7.0),
    vec4f(13.0, 8.0, 6.0, 2.5),
);
const APRON_SIDE_BARRIER = vec4f(43.0, 0.0, 0.6, 5.0);
const APRON_END_BARRIERS = array<vec4f, 2>(
    vec4f(10.0, 55.0, 4.0, 0.55),
    vec4f(34.0, 55.0, 4.0, 0.55),
);
// The wear texture covers this square round the arena's centre, metres
// (`effects::bakes::HARBOR_APRON_WEAR`), and holds the ground's color multiplier
// over this range in rgb and its roughness in a.
const APRON_WEAR_EXTENT = 128.0;
const APRON_WEAR_RANGE = 2.0;
// A spawn pad's radius (`harbor_scenery::PAD_RADIUS`), the light ring's outer
// edge and the radius of its bolt circle, metres.
const PAD_RADIUS = 2.65;
const PAD_RING_OUTER = 2.3;
const PAD_BOLTS = 1.92;
// The uv.x offset of a pad's light ring (`harbor_scenery::PAD_LIGHT_FLAG`).
const PAD_LIGHT_FLAG = 10.0;
// How much of the team's saturation a pad's paint keeps, and its light ring.
const PAD_SATURATION = 0.42;
const PAD_LIGHT_SATURATION = 0.55;
// Cover containers each stand inside one cell of this grid (`harbor_layout`), so
// a box takes one shade of its line's color from its cell.
const CONTAINER_CELL = 20.0;
// Height of a cover container (`harbor_layout`), metres, and the linear color of
// the rust that weeps from its roof rail and blooms on its roof.
const CONTAINER_HEIGHT = 3.6;
const CONTAINER_RUST = vec3f(0.2, 0.072, 0.026);
// Pitch of the shallow corrugations pressed across a container's roof, metres.
const CONTAINER_ROOF_PITCH = 0.6;
// Share of the sunlight that the apron throws back up onto a container's walls.
const CONTAINER_BOUNCE = 0.045;

// The sea wall's parapet (`harbor_surfaces::harbor_parapet`, the boundary cover of
// `sim::harbor_layout`): its height and the depth of its capping beam, the length
// of a capping stone and of a precast panel, the kerb band's height, the line down
// the middle of the wall (metres from the arena's centre), the inner arris of its
// cap, and the pitch and reach of the bollards on its top (`harbor_scenery`), whose
// bases weep rust down both faces.
const PARAPET_TOP = 1.2;
const PARAPET_CAP = 0.3;
// How far the capping beam overhangs the panels, and the chamfer along its top
// arrises (`harbor_surfaces::PARAPET_CHAMFER`), metres.
const PARAPET_OVERHANG = 0.05;
const PARAPET_CHAMFER = 0.2;
// Length of a run of the wall repainted in one go, metres.
const PARAPET_PAINT_RUN = 9.0;
const PARAPET_STONE = 2.0;
const PARAPET_PANEL = 4.0;
const PARAPET_KERB = 0.32;
const PARAPET_MIDDLE = 61.0;
// Half the wall's thickness (`sim::arena::BOUNDARY_THICKNESS`), metres.
const PARAPET_HALF = 1.0;
const PARAPET_BOLLARDS = 24.0;
const PARAPET_BOLLARD_REACH = 48.0;
// Heights of the two rows of form-tie plugs down each panel, their pitch along it
// and their radius, metres.
const PARAPET_TIE_ROWS = vec2f(0.5, 0.84);
const PARAPET_TIE_PITCH = 1.0;
const PARAPET_TIE_RADIUS = 0.022;
// Period of the diagonal hazard stripes along the kerb band and along the inner
// chamfer, metres, and the linear hazard paints (`harbor_scenery::LINE_YELLOW` and a near
// black).
const PARAPET_KERB_STRIPE = 1.1;
const PARAPET_TOP_STRIPE = 1.2;
const HAZARD_YELLOW = vec3f(0.62, 0.405, 0.075);
const HAZARD_BLACK = vec3f(0.016, 0.017, 0.019);

fn apron_hash(cell: vec2f) -> vec4f {
    var p = fract(cell.xyxy * vec4f(0.1031, 0.1030, 0.0973, 0.1099));
    p = p + dot(p, p.wzxy + 33.33);
    return fract((p.xxyz + p.yzzw) * p.zywx);
}

// Four values in [0, 1) for a cell of whole coordinates, from one integer hash: about
// half the work of `apron_hash`, for the cells the ground looks up at every pixel
// (its wear bake looks up the same ones, so a cover's rust ring and a lane section's
// tone line up with what the pixel draws).
fn apron_cell_hash(cell: vec2f) -> vec4f {
    let c = vec2i(cell);
    var h = (bitcast<u32>(c.x) * 1597334677u) ^ (bitcast<u32>(c.y) * 3812015801u);
    h = (h ^ (h >> 16u)) * 2246822519u;
    h = (h ^ (h >> 13u)) * 3266489917u;
    h = h ^ (h >> 16u);
    return vec4f((vec4u(h) >> vec4u(0u, 8u, 16u, 24u)) & vec4u(255u)) * (1.0 / 256.0) + 0.5 / 256.0;
}

fn apron_noise(p: vec2f) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = apron_hash(i).x;
    let b = apron_hash(i + vec2f(1.0, 0.0)).x;
    let c = apron_hash(i + vec2f(0.0, 1.0)).x;
    let d = apron_hash(i + vec2f(1.0, 1.0)).x;
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// The same noise and hash per pixel, from the ground noise tile
// (`ground_noise.wgsl`): the wear bake keeps the arithmetic ones, the surfaces,
// drawn over most of the frame, look them up.
fn apron_pixel_noise(p: vec2f) -> f32 {
    return ground_noise(p).x;
}

fn apron_pixel_hash(cell: vec2f) -> vec4f {
    return vec4f(ground_hash(cell), ground_hash(cell + vec2f(113.0, 57.0)));
}

fn apron_box_distance(p: vec2f, b: vec4f) -> f32 {
    let q = abs(p - b.xy) - b.zw;
    return length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0);
}

// Wheel polish and the oil strip between the wheel paths, for a point `across`
// metres from the centre of a lane `half` metres wide.
fn apron_traffic(across: f32, half: f32) -> vec2f {
    let offset = abs(abs(across) - half * 0.5);
    let wheels = (offset - APRON_WHEEL_OFFSET) * (1.0 / 0.4);
    let strip = offset * (1.0 / 0.3);
    return vec2f(exp(-wheels * wheels), exp(-strip * strip)) * step(abs(across), half);
}

// Offset from the centre of the nearest lane along z (x) and along x (y), with
// those lanes' half widths (z, w).
fn apron_lane_offsets(p: vec2f) -> vec4f {
    var out = vec4f(1e3, 1e3, 1.0, 1.0);
    for (var i = 0; i < 3; i++) {
        let along_z = APRON_LANES_Z[i];
        if abs(p.x - along_z.x) < abs(out.x) {
            out.x = p.x - along_z.x;
            out.z = along_z.y;
        }
        let along_x = APRON_LANES_X[i];
        if abs(p.y - along_x.x) < abs(out.y) {
            out.y = p.y - along_x.x;
            out.w = along_x.y;
        }
    }
    return out;
}

// A blotch of `radius` metres around `at`, stretched `stretch` times along `dir`,
// its rim broken by `warp`: 0 at its centre, 1 on its rim.
fn apron_blotch(p: vec2f, at: vec2f, dir: vec2f, stretch: f32, radius: f32, warp: f32) -> f32 {
    let rel = p - at;
    let local = vec2f(dot(rel, dir) / stretch, dot(rel, vec2f(-dir.y, dir.x)));
    return length(local) / radius + warp;
}

// Tilt `n` by the world-space gradient of a height field whose screen derivatives
// are `dh` (Mikkelsen's surface gradient, as the standard bump).
fn apron_relief(n: vec3f, dpx: vec3f, dpy: vec3f, dh: vec2f) -> vec3f {
    let r1 = cross(dpy, n);
    let r2 = cross(n, dpx);
    let det = dot(dpx, r1);
    let gradient = sign(det) * (dh.x * r1 + dh.y * r2);
    return normalize(abs(det) * n - gradient);
}

// `c` turned `angle` radians round the grey axis: the same paint in another hue.
fn apron_hue(c: vec3f, angle: f32) -> vec3f {
    let k = vec3f(0.57735027);
    let cs = cos(angle);
    return max(c * cs + cross(k, c) * sin(angle) + k * dot(k, c) * (1.0 - cs), vec3f(0.0));
}

// The dock tile's grain at `p` with its broad blotches divided out, at two scales
// and angles so neither repeats.
fn apron_grain(p: vec2f) -> vec3f {
    let turned = vec2f(p.x * 0.8 - p.y * 0.6, p.x * 0.6 + p.y * 0.8);
    let grain_a = textureSample(extra_texture0, extra_sampler0, p / 4.7).rgb;
    let grain_b = textureSample(extra_texture0, extra_sampler0, turned / 9.3 + 0.31).rgb;
    let blur_a = textureSampleBias(extra_texture0, extra_sampler0, p / 4.7, APRON_GRAIN_BLUR).rgb;
    let blur_b = textureSampleBias(extra_texture0, extra_sampler0, turned / 9.3 + 0.31, APRON_GRAIN_BLUR).rgb;
    return grain_a / max(blur_a, vec3f(0.02)) * 0.6 + grain_b / max(blur_b, vec3f(0.02)) * 0.4;
}

// Metres from `p` on the yard to the nearest fixed cover or the boundary wall's
// inner face (x), and the rust round the containers' castings near it (y).
fn apron_fixture(p: vec2f, mottle: f32) -> vec2f {
    let outside = max(abs(p.x), abs(p.y)) - APRON_ARENA;
    var fixture = select(-outside, 1e3, outside > 0.0);
    var castings = 0.0;
    let mirrored = abs(p);
    let north = select(p, -p, p.y < 0.0);
    for (var i = 0; i < 3; i++) {
        let b = APRON_CONTAINER_QUADRANT[i];
        let distance = apron_box_distance(mirrored, b);
        fixture = min(fixture, distance);
        // A casting sits inside its container's footprint, so it is at least
        // `distance` away, and its rust reaches at most 1.2 m.
        if distance < APRON_CASTING_REACH {
            let corner = b.xy + sign(mirrored - b.xy) * (b.zw - 0.1);
            castings = max(castings, 1.0 - smoothstep(0.05, 0.6 + mottle * 0.6, length(mirrored - corner)));
        }
    }
    fixture = min(fixture, apron_box_distance(mirrored, APRON_SIDE_BARRIER));
    fixture = min(fixture, apron_box_distance(north, APRON_END_BARRIERS[0]));
    fixture = min(fixture, apron_box_distance(north, APRON_END_BARRIERS[1]));
    return vec2f(fixture, castings);
}

// Sunlight thrown back off the harbor's ripples, dancing on a face rising from
// the water: a moving web of bright lines, strongest just above the waterline.
fn apron_caustics(along: f32, y: f32, footprint: f32) -> f32 {
    let t = frame.camera_position.w;
    let height = y - APRON_SEA_LEVEL;
    let a = apron_pixel_noise(vec2f(along * 1.6 + t * 0.4, y * 3.0 - t * 0.25));
    let b = apron_pixel_noise(vec2f(along * 2.1 - t * 0.33 + 9.0, y * 3.6 + t * 0.22));
    let web = clamp(1.0 - abs(a - b) * 2.4, 0.0, 1.0);
    let web2 = web * web;
    // Faded where a pixel spans the web, so it never crawls at a distance.
    return web2 * web2 * web2 * smoothstep(0.08, 0.4, height) * (1.0 - smoothstep(0.4, 1.8, height))
        * (1.0 - smoothstep(0.03, 0.12, footprint));
}

// ------------------------------------------------------------------ the ground

// The ground's baked wear at `p`: its color multiplier (rgb) and roughness (a).
fn apron_wear(p: vec2f) -> vec4f {
    let wear = textureSample(extra_texture1, extra_sampler1, p * (1.0 / APRON_WEAR_EXTENT) + 0.5);
    return vec4f(wear.rgb * APRON_WEAR_RANGE, wear.a);
}

// Concrete slabs and asphalt lanes: the base color, grain and the crisp detail a
// pixel resolves from the gameplay camera, times the baked wear. A layer costs the
// whole apron even where it is skipped (the registers it holds slow every pixel),
// so detail that reads only up close (spalled corners, hairline and asphalt
// cracks, repair patches, the aggregate's chips) is left out.
fn apron_ground(s: ptr<function, Surface>, f: EffectFragment) {
    let mode = material.params[0].x;
    let asphalt = mode > 0.5;
    let along_z = mode < 1.5;
    let p = f.world.xz;
    // Every sample and derivative first, in control flow uniform across the draw.
    let grain = apron_grain(p);
    let grain_luma = dot(grain, APRON_LUMA);
    let wear = apron_wear(p);
    var fine = 1.0;
    var streak = 1.0;
    if asphalt {
        let turned = vec2f(p.x * 0.8 - p.y * 0.6, p.x * 0.6 + p.y * 0.8);
        fine = dot(textureSample(extra_texture0, extra_sampler0, turned / 1.3 + 0.17).rgb, APRON_LUMA) / APRON_TILE_MEAN;
        // The tile drawn out along the lane: tyre streaks.
        let streak_uv = select(vec2f(p.y / 1.9, p.x / 60.0) + 0.5, vec2f(p.x / 1.9, p.y / 60.0), along_z);
        streak = dot(textureSample(extra_texture0, extra_sampler0, streak_uv).rgb, APRON_LUMA) / APRON_TILE_MEAN;
    }
    let dpx = dpdx(f.world);
    let dpy = dpdy(f.world);
    // One pixel's footprint in metres anti-aliases every edge: the joints, the
    // covers and the gutters are all laid out in metres on the flat yard.
    let footprint = length(abs(dpx) + abs(dpy));
    let aa = max(footprint * 0.7, 1e-4);
    let slab_uv = p / APRON_SLAB;
    let to_edge = (0.5 - abs(fract(slab_uv) - 0.5)) * APRON_SLAB;
    let edge = min(to_edge.x, to_edge.y);
    let to_expansion = (0.5 - abs(fract(p / APRON_EXPANSION) - 0.5)) * APRON_EXPANSION;
    let expansion = min(to_expansion.x, to_expansion.y);
    // Relief: the sawn joints with their worn arrises, and the aggregate standing
    // proud of an asphalt lane's binder, fading where a pixel spans its detail.
    var relief = 0.0;
    if asphalt {
        relief = smoothstep(0.8, 1.9, fine) * 0.006 * (1.0 - smoothstep(0.02, 0.08, footprint));
    } else {
        relief = -(1.0 - smoothstep(0.0, APRON_ARRIS + footprint, edge)) * 0.022
            - (1.0 - smoothstep(0.0, 0.05 + footprint, expansion)) * 0.03;
    }
    let relief_dh = vec2f(dpdx(relief), dpdy(relief));

    var color = (*s).color;
    var roughness = wear.a;
    var tilt = vec3f(0.0);
    if !asphalt {
        // Concrete: each pour its own tone and hue with a trowel gradient, newer
        // replacement slabs, then the sawn joints with tar run into them, smeared
        // over a slab where the crew overbanded it, and over the worst cracks.
        let slab = apron_cell_hash(floor(slab_uv) + vec2f(17.0, 5.0));
        let patched = step(slab.y, 0.07);
        let tone = mix(0.74, 1.12, slab.x * slab.x * 0.4 + slab.x * 0.6) * mix(1.0, 1.14, patched);
        let hue = mix(vec3f(1.03, 1.0, 0.95), vec3f(0.96, 0.995, 1.035), slab.w);
        let pour = 1.0 + dot(fract(slab_uv) - 0.5, slab.zw - 0.5) * 0.2;
        color = material.color.rgb * f.tint.rgb * grain * tone * hue * pour;
        let groove = clamp((APRON_JOINT - edge) / aa + 0.5, 0.0, 1.0) * min(1.0, 2.0 * APRON_JOINT / aa);
        let sealant = clamp((APRON_SEALANT - expansion) / aa + 0.5, 0.0, 1.0) * min(1.0, 2.0 * APRON_SEALANT / aa);
        // Tar run over the cracks of some slabs, in broken lengths.
        var sealed = 0.0;
        if slab.z > 0.72 && slab.w > 0.45 && patched < 0.5 {
            let crack = abs(apron_pixel_noise(p / 3.1 + 13.0) - 0.5 + (grain_luma - 1.0) * 0.004);
            let crack_width = footprint * 0.4 + 0.014;
            sealed = (1.0 - smoothstep(0.0, crack_width, crack)) * smoothstep(0.55, 0.7, apron_pixel_noise(p / 1.7 + slab.xy * 9.0));
        }
        let overband = step(0.55, fract(slab.z * 7.1 + slab.x * 3.3));
        let tar_width = APRON_JOINT * 1.6 + overband * (0.03 + 0.08 * fract(slab.y * 13.7 + slab.w * 5.3));
        let tar = max((1.0 - smoothstep(tar_width - aa, tar_width + aa, edge + (grain_luma - 1.0) * 0.006)) * min(1.0, 2.0 * tar_width / aa), sealed)
            * mix(0.85, 0.45, smoothstep(0.08, 0.3, footprint));
        color = color * (1.0 - groove * 0.55) * (1.0 - sealant * 0.65);
        color = mix(color, APRON_TAR * mix(0.8, 1.3, grain_luma * 0.5), tar);
        roughness = mix(roughness, 0.7, tar);
        // No two slabs were laid dead level: each tilts a hair its own way and
        // catches the low sun a shade differently.
        tilt = vec3f(slab.z - 0.5, 0.0, slab.w - 0.5) * 0.1;
        // A cast-iron utility cover in some cells, clear of the lane edges.
        let cover_cell = floor(p / APRON_COVER_CELL);
        let cover_hash = apron_cell_hash(cover_cell + vec2f(41.0, 7.0));
        if cover_hash.w >= 0.72 {
            let cover_d = length(p - (cover_cell + 0.25 + 0.5 * cover_hash.xy) * APRON_COVER_CELL);
            let cover_radius = 0.34 + 0.12 * cover_hash.z;
            let offsets = apron_lane_offsets(p);
            let clear = step(0.9, min(abs(offsets.x) - offsets.z, abs(offsets.y) - offsets.w));
            let cover = clamp((cover_radius - cover_d) / aa + 0.5, 0.0, 1.0) * clear;
            let gap = clamp(0.5 - abs(cover_d - cover_radius - 0.02) / aa * 0.7, 0.0, 1.0) * clear;
            let rim = clamp((cover_d - cover_radius + 0.05) / aa + 0.5, 0.0, 1.0);
            // The iron is dark whatever the wear does to the concrete round it.
            color = mix(color, vec3f(0.05, 0.048, 0.046) * mix(1.0, 1.4, rim) / max(wear.rgb, vec3f(0.2)), cover) * (1.0 - gap * 0.7);
            roughness = mix(roughness, 0.5, cover);
        }
    } else {
        // Asphalt lanes: dark binder with pale aggregate standing proud of it,
        // ravelled toward the edges, tyre streaks along the wheel paths, the paving
        // joint down the middle and between relaid sections, and a precast drainage
        // channel along each edge the lane shares with the apron.
        let offsets = apron_lane_offsets(p);
        let across = select(offsets.y, offsets.x, along_z);
        let half = select(offsets.w, offsets.z, along_z);
        let lane_along = select(p.x, p.y, along_z);
        let traffic = apron_traffic(across, half);
        let aggregate = smoothstep(1.25, 2.0, fine) - smoothstep(0.6, 0.25, fine) * 0.6;
        let tyre = smoothstep(0.8, 1.35, streak) * 0.75;
        // Relaid a stretch at a time, each section meeting the next at a straight
        // transverse joint.
        let lane_centre = select(p.y - offsets.y, p.x - offsets.x, along_z);
        let lane_seed = apron_cell_hash(vec2f(lane_centre, 71.0));
        let section_at = lane_along / APRON_SECTION + lane_seed.x;
        let section = apron_cell_hash(vec2f(floor(section_at), lane_centre * 0.37 + 5.0));
        let section_edge = (0.5 - abs(fract(section_at) - 0.5)) * APRON_SECTION;
        let section_joint = (1.0 - smoothstep(0.01, 0.05 + footprint, section_edge)) * step(0.35, section.z);
        let centre_joint = (1.0 - smoothstep(0.015, 0.05, abs(across))) * step(0.5, section.x);
        let base = material.color.rgb * f.tint.rgb * mix(0.9, 1.06, grain_luma * 0.5) * mix(0.96, 1.04, section.y);
        color = base * (1.0 + aggregate * mix(0.32, 0.65, smoothstep(half - 1.1, half - 0.15, abs(across))));
        color = color * mix(1.0, 0.78, tyre * (0.35 + traffic.x * 0.65));
        color = color * (1.0 - centre_joint * 0.45) * (1.0 - section_joint * 0.4);
        // The channel along the open edges: a pale concrete gutter, dirt washed into
        // its slot, and a cast grating every so often. None across a crossing lane.
        let edge_d = half - abs(across);
        let open_edge = smoothstep(0.0, 0.25, abs(select(offsets.x, offsets.y, along_z)) - select(offsets.z, offsets.w, along_z));
        let gutter = (1.0 - smoothstep(APRON_GUTTER - aa, APRON_GUTTER + aa, edge_d)) * open_edge;
        if gutter > 0.0 {
            let slot = 1.0 - smoothstep(0.025, 0.025 + aa * 1.5, abs(edge_d - APRON_GUTTER * 0.6));
            let grate_at = fract(lane_along / APRON_GRATE_PITCH + lane_seed.y) * APRON_GRATE_PITCH - APRON_GRATE_PITCH * 0.5;
            let grate = step(abs(grate_at), 0.45) * step(edge_d, APRON_GUTTER - 0.04);
            var channel = vec3f(0.3, 0.29, 0.27) * mix(0.88, 1.06, grain_luma * 0.5 + 0.25) * (1.0 - slot * 0.75);
            channel = mix(channel, vec3f(0.03, 0.03, 0.032), grate);
            color = mix(color, channel, gutter);
        }
    }
    (*s).normal = normalize(apron_relief((*s).normal, dpx, dpy, relief_dh) + tilt);
    (*s).color = color * wear.rgb;
    (*s).roughness = roughness;
}


// ------------------------------------------------------------------ the paint

// Paint: flaked in old zones and where wheels pass, dirtied and rubbered by the
// traffic, its color faded by the sun and pitted by the aggregate it was rolled
// onto. Stencilled lettering is a thinner coat than the road lines: it wears
// through sooner, in fine flakes drawn out along the lane where the tyres run over
// it, so the words stay legible and the wear reads as tyre polish.
fn apron_paint(s: ptr<function, Surface>, f: EffectFragment) {
    let mode = material.params[0].x;
    let lettering = mode > 3.5;
    let p = f.world.xz;
    let grain = apron_grain(p);
    let grain_luma = dot(grain, APRON_LUMA);
    let turned = vec2f(p.x * 0.8 - p.y * 0.6, p.x * 0.6 + p.y * 0.8);
    let fine = dot(textureSample(extra_texture0, extra_sampler0, turned / 1.3 + 0.17).rgb, APRON_LUMA) / APRON_TILE_MEAN;
    let offsets = apron_lane_offsets(p);
    let traffic_z = apron_traffic(offsets.x, offsets.z);
    let traffic_x = apron_traffic(offsets.y, offsets.w);
    let traffic = max(max(traffic_z.x, traffic_x.x), max(traffic_z.y, traffic_x.y) * 0.6);
    let age = apron_pixel_noise(p / 13.0 + 31.0);
    var wear_flakes = 0.0;
    if lettering {
        wear_flakes = apron_pixel_noise(p * vec2f(3.1, 0.9)) * 0.8 + (grain_luma - 1.0) * 0.12 + 0.1;
    } else {
        wear_flakes = apron_pixel_noise(p * 0.7) * 0.85 + (grain_luma - 1.0) * 0.08;
    }
    let threshold = select(0.1 + smoothstep(0.55, 0.9, age) * 0.22 + traffic * 0.2, 0.06 + traffic * 0.42, lettering);
    let flake_aa = max(fwidth(wear_flakes), 0.01);
    let worn = smoothstep(threshold - flake_aa, threshold + flake_aa, wear_flakes);
    // Contact shade where paint runs up to fixed cover or the wall.
    var contact = 0.0;
    if max(abs(p.x), abs(p.y)) > APRON_ARENA - 8.0 || min(abs(abs(p.x) - 30.0), abs(abs(p.x) - 13.0)) < 9.0 {
        let fixture = apron_fixture(p, 0.5).x;
        contact = (1.0 - smoothstep(0.0, 0.8, fixture)) * 0.42 + (1.0 - smoothstep(0.0, 3.2, fixture)) * 0.16;
    }
    var color = (*s).color;
    if !lettering {
        color = material.color.rgb * f.vertex_color.rgb * f.tint.rgb;
    }
    let chroma = dot(color, APRON_LUMA);
    color = mix(color, vec3f(chroma) * vec3f(1.02, 1.0, 0.95), age * 0.25);
    let dirt = clamp(age * 0.75 + (fine - 1.0) * 0.12 + traffic * 0.4 - 0.2, 0.0, 1.0);
    color = mix(color, color * vec3f(0.6, 0.58, 0.53), dirt * 0.6);
    let rubber = traffic * smoothstep(0.4, 0.7, fine * 0.5);
    color = mix(color, color * vec3f(0.45, 0.44, 0.43), rubber * 0.5);
    color = color * mix(0.8, 1.04, clamp(fine - 0.3, 0.0, 1.0)) * 0.97;
    color = color * (1.0 - contact * 0.4);
    (*s).color = color;
    if lettering {
        (*s).opacity = (*s).opacity * mix(0.2, 0.9, worn) * mix(1.0, 0.85, age) * (1.0 - rubber * 0.3);
    } else {
        (*s).opacity = worn;
    }
}

// ------------------------------------------------------------------ the structures

// The sea wall's parapet: precast panels under a capping beam of shorter stones,
// set half a stone off the panels' joints; two rows of form-tie plugs down each
// panel, some weeping rust; rain-washed streaks running down from the cap and rust
// from under the bollards; road dirt splashed up the apron side and the black
// scuffs of tanks rubbing along it; and hazard paint in diagonal stripes on the
// kerb band and along the top, each panel painted in its own year and chipped to
// the concrete where hulls and boots wear it. Salt blooms on the sea side.
fn parapet_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let n = (*s).normal;
    let runs_z = abs(f.world.x) > abs(f.world.z);
    let along = select(f.world.x, f.world.z, runs_z);
    let outward = max(abs(f.world.x), abs(f.world.z));
    let y = f.world.y;
    let top = n.y > 0.5;
    let face = select(1.0, 0.0, top);
    let inner_side = select(0.0, 1.0, outward < PARAPET_MIDDLE);
    let inner = inner_side * face;
    // The chamfers along the cap's top arrises, and the flat top between them.
    let chamfer = select(0.0, 1.0, top && n.y < 0.95);
    let flat_top = select(0.0, 1.0, n.y >= 0.95);
    let outer = face - inner;
    // Which of the four walls: so no two walls wear alike.
    let wall = select(0.0, 2.0, runs_z) + select(0.0, 1.0, f.world.x + f.world.z > 0.0);
    // Every sample and derivative first, in control flow uniform across the draw.
    let p = select(vec2f(along, y), vec2f(along, outward), top);
    let grain = apron_grain(p);
    let grain_luma = dot(grain, APRON_LUMA);
    let footprint = length(fwidth(f.world));
    let panel_edge = (0.5 - abs(fract(along / PARAPET_PANEL) - 0.5)) * PARAPET_PANEL;
    let panel_aa = max(fwidth(panel_edge), 1e-4);
    let stone_edge = (0.5 - abs(fract(along / PARAPET_STONE + 0.5) - 0.5)) * PARAPET_STONE;
    let stone_aa = max(fwidth(stone_edge), 1e-4);
    let y_aa = max(fwidth(y), 1e-4);
    let kerb_u = (along + y) / PARAPET_KERB_STRIPE;
    let kerb_aa = max(fwidth(kerb_u), 1e-3);
    // Down the inner chamfer the stripes slant with the height; across the flat
    // top they slant across it, so the band reads as one coat over the arris.
    let top_u = (along + select((PARAPET_TOP - y) * 4.0, PARAPET_MIDDLE - outward + 0.4, n.y >= 0.95)) / PARAPET_TOP_STRIPE;
    let top_aa = max(fwidth(top_u), 1e-3);
    let tie_x = fract(along / PARAPET_TIE_PITCH) * PARAPET_TIE_PITCH - PARAPET_TIE_PITCH * 0.5;
    let tie_row = select(PARAPET_TIE_ROWS.x, PARAPET_TIE_ROWS.y, y > (PARAPET_TIE_ROWS.x + PARAPET_TIE_ROWS.y) * 0.5);
    let tie_d = length(vec2f(tie_x, y - tie_row));
    let tie_aa = max(fwidth(tie_d), 1e-4);
    // Paint chips: fine, so wear reads as scraped and flaked paint rather than
    // blocks of a mask; where it is worn is decided by `wear` below.
    let flakes = apron_pixel_noise(p * vec2f(3.7, 8.3) + wall * 17.0) * 0.8 + (grain_luma - 1.0) * 0.2;
    let flake_aa = max(fwidth(flakes), 0.02);

    // The cap: its top and the band of its faces above the precast body.
    let cap = max(1.0 - face, smoothstep(PARAPET_TOP - PARAPET_CAP - y_aa, PARAPET_TOP - PARAPET_CAP + y_aa, y));
    let body = 1.0 - cap;
    let panel = apron_pixel_hash(vec2f(floor(along / PARAPET_PANEL), wall * 7.0 + 3.0));
    let stone = apron_pixel_hash(vec2f(floor(along / PARAPET_STONE + 0.5), wall * 7.0 + 41.0));
    // Sawn joints: between the panels down the body, between the stones on the cap.
    let panel_joint = clamp((0.012 - panel_edge) / panel_aa + 0.5, 0.0, 1.0) * body;
    let stone_joint = clamp((0.008 - stone_edge) / stone_aa + 0.5, 0.0, 1.0) * cap;
    let joint_fringe = (1.0 - smoothstep(0.0, 0.12 + grain_luma * 0.05, select(panel_edge, stone_edge, cap > 0.5)))
        * (1.0 - smoothstep(0.03, 0.12, footprint));
    // Form-tie plugs, some weeping a thin run of rust.
    let tie = (1.0 - smoothstep(PARAPET_TIE_RADIUS - tie_aa, PARAPET_TIE_RADIUS + tie_aa, tie_d)) * body * face
        * (1.0 - smoothstep(0.02, 0.06, footprint));
    let tie_hash = apron_pixel_hash(vec2f(floor(along / PARAPET_TIE_PITCH), tie_row * 10.0 + wall * 5.0));
    let tie_run_x = tie_x / (0.01 + 0.018 * tie_hash.y);
    let tie_run = exp(-tie_run_x * tie_run_x) * step(y, tie_row)
        * exp(-(tie_row - y) / (0.08 + 0.3 * tie_hash.z)) * step(0.55, tie_hash.x) * body * face;
    // Rain running off the cap leaves dark streaks down the faces, rust weeps from
    // under each bollard, and road dirt is splashed up the apron side.
    let runs = apron_pixel_noise(vec2f(along * 2.6 + wall * 9.0, y * 0.25));
    let rain = smoothstep(0.46, 0.8, runs) * smoothstep(-0.4, 1.0, y) * body * face;
    let bollard_at = clamp(round(along / PARAPET_BOLLARDS) * PARAPET_BOLLARDS, -PARAPET_BOLLARD_REACH, PARAPET_BOLLARD_REACH);
    let bollard_off = (along - bollard_at + (grain_luma - 1.0) * 0.06) / (0.18 + 0.2 * runs);
    let bollard_rust = exp(-bollard_off * bollard_off) * smoothstep(0.1, 1.0, y) * face;
    let splash = (1.0 - smoothstep(0.0, 0.3 + runs * 0.35, y)) * inner;
    // Tanks rubbing along the apron side: black streaks of rubber and steel low on
    // the face, scraping the paint off as they go.
    let scuff = smoothstep(0.62, 0.86, apron_pixel_noise(vec2f(along * 0.42 + wall * 5.0, y * 6.0)) + (grain_luma - 1.0) * 0.1)
        * (1.0 - smoothstep(0.22, 0.7, y)) * inner;
    // Salt dried white down the sea side, thickest low where the spray lands.
    let salt = smoothstep(0.58, 0.82, runs + (grain_luma - 1.0) * 0.2)
        * (1.0 - smoothstep(0.3, 1.1, y)) * outer;

    // Hazard paint: a kerb band of broad diagonal stripes on the apron side and
    // a coping stripe along the cap's inner chamfer, the flat top left bare
    // concrete, so from the air the wall reads as a concrete quay edged in
    // safety paint rather than a painted frame. Laid in runs: most stretches
    // repainted in their own year, a few left to the ghost of the old coat.
    let run = apron_pixel_hash(vec2f(floor(along / PARAPET_PAINT_RUN + wall * 0.37), wall * 13.0 + 5.0));
    let bevel_coat = select(1.0, 0.0, run.z < 0.16);
    let kerb_coat = select(1.0, 0.0, run.w < 0.24);
    let kerb_zone = (1.0 - smoothstep(PARAPET_KERB - y_aa, PARAPET_KERB + y_aa, y)) * inner;
    let bevel_zone = chamfer * inner_side;
    let kerb = kerb_zone * kerb_coat;
    let top_band = bevel_zone * bevel_coat;
    let age = smoothstep(0.1, 0.95, select(panel.y, stone.y, top)) * mix(0.5, 1.0, run.x);
    // Wear goes where things hit the paint: the kerb's foot and the corners of the
    // panels, where tracks and hulls catch them; the bevel's arrises, where boots
    // step up onto the wall; round the bollards, where mooring lines chafe; and
    // the joints between the stones. The open middle of a stripe keeps its coat.
    let bevel_mid = PARAPET_TOP - PARAPET_CHAMFER * 0.5;
    let band_edge = smoothstep(PARAPET_CHAMFER * 0.25, PARAPET_CHAMFER * 0.5, abs(y - bevel_mid));
    let kerb_foot = 1.0 - smoothstep(0.0, 0.14, y);
    let corners = 1.0 - smoothstep(0.0, 0.35, select(panel_edge, stone_edge, top));
    let chafe = 1.0 - smoothstep(0.4, 1.6, abs(along - bollard_at));
    let wear = select(kerb_foot * 0.7 + corners * 0.35, band_edge * 0.25 * chamfer + corners * 0.2 + chafe * 0.4, top);
    let threshold = 0.02 + age * 0.06 + wear * 0.38 + scuff * 0.45;
    let painted = smoothstep(threshold - flake_aa, threshold + flake_aa, flakes) * max(kerb, top_band);
    // Ghost of the paint left in the scrapes and on the runs not repainted:
    // stained concrete, not bare white.
    let paint_ghost = max(kerb_zone * mix(0.45, 1.0, kerb_coat), bevel_zone * mix(0.45, 1.0, bevel_coat)) * (1.0 - painted);
    // Where a pixel spans a stripe the pair blends to its mean, so the band never
    // breaks into a crawling dotted line from the air.
    let stripe_raw = select(
        smoothstep(0.5 - kerb_aa, 0.5 + kerb_aa, fract(kerb_u)),
        smoothstep(0.5 - top_aa, 0.5 + top_aa, fract(top_u)),
        top,
    );
    let stripe = mix(stripe_raw, 0.45, smoothstep(0.2, 0.45, select(kerb_aa, top_aa, top)));
    let faded = mix(HAZARD_YELLOW, vec3f(dot(HAZARD_YELLOW, APRON_LUMA)) * vec3f(1.25, 1.1, 0.85), age * 0.35) * mix(1.0, 0.82, age);
    var hazard = mix(faded, HAZARD_BLACK + vec3f(0.01) * age, stripe) * mix(0.9, 1.06, grain_luma * 0.5 + 0.25);

    // Concrete: each panel and stone its own pour, the cap paler and smoother.
    let tone = mix(0.86, 1.08, panel.x) * mix(1.0, mix(1.04, 1.18, stone.x), cap);
    let hue = mix(vec3f(1.03, 1.0, 0.95), vec3f(0.97, 1.0, 1.03), panel.z);
    let mottle = apron_pixel_noise(p * vec2f(0.6, 1.4) + wall * 3.0);
    var color = material.color.rgb * f.tint.rgb * grain * tone * hue * mix(0.88, 1.06, mottle);
    color = mix(color, color * vec3f(0.6, 0.58, 0.53), rain * 0.7);
    color = mix(color, color * vec3f(0.78, 0.5, 0.3), clamp(bollard_rust * 0.7 + tie_run * 0.8, 0.0, 1.0));
    color = mix(color, color * vec3f(1.1, 1.1, 1.08) + vec3f(0.04), salt * 0.6);
    color = mix(color, color * vec3f(0.6, 0.55, 0.48), splash * 0.6);
    // Bare aggregate at the chipped edges of the joints.
    color = color * (1.0 - joint_fringe * 0.16);
    color = mix(color, color * vec3f(0.78, 0.74, 0.62), paint_ghost * 0.55);
    color = mix(color, hazard * mix(1.0, 0.7, splash), painted);
    color = mix(color, color * vec3f(0.3, 0.3, 0.31), scuff * 0.7);
    color = mix(color, color * 0.35, tie);
    color = color * (1.0 - panel_joint * 0.7) * (1.0 - stone_joint * 0.6);
    // The cap's arrises, rounded and worn smooth, catch the light along the top of
    // the wall: a bevel that draws the wall's edge, not a flat painted band.
    let cap_edge = select(PARAPET_TOP - PARAPET_CHAMFER - y, PARAPET_HALF + PARAPET_OVERHANG - PARAPET_CHAMFER - abs(outward - PARAPET_MIDDLE), top) + chamfer;
    let arris = (1.0 - smoothstep(0.0, 0.05 + footprint, cap_edge)) * (1.0 - painted);
    color = color * (1.0 + arris * 0.22);
    // The flat top: dirt trodden along its middle and lichen in rosettes on the
    // sea side, so the wall's top never reads as a clean painted band.
    let trodden = (1.0 - smoothstep(0.15, 0.55, abs(outward - PARAPET_MIDDLE + (mottle - 0.5) * 0.4)))
        * smoothstep(0.3, 0.7, mottle);
    let lichen = smoothstep(0.66, 0.78, flakes * 0.7 + (grain_luma - 1.0) * 0.3)
        * smoothstep(PARAPET_MIDDLE + 0.2, PARAPET_MIDDLE + 0.7, outward);
    color = color * mix(1.0, mix(0.64, 0.52, trodden) * mix(0.86, 1.06, mottle), flat_top * (1.0 - painted * 0.75));
    color = mix(color, color * vec3f(0.62, 0.66, 0.5), lichen * flat_top * (1.0 - painted) * 0.7);
    // Contact shade at the apron and under the cap's overhang.
    color = color * mix(1.0, mix(0.6, 1.0, smoothstep(0.0, 0.32, y)), inner);
    color = color * (1.0 - 0.22 * smoothstep(PARAPET_TOP - PARAPET_CAP - 0.14, PARAPET_TOP - PARAPET_CAP, y) * body * face);
    var roughness = mix(0.92, 0.84, cap);
    roughness = mix(roughness, 0.7, painted);
    roughness = mix(roughness, 0.62, scuff);
    // Relief: the joints and plugs sunk into the face, faded where a pixel spans them.
    let relief = -(panel_joint + stone_joint) * 0.01 - tie * 0.006 - (1.0 - smoothstep(0.0, 0.06, cap_edge)) * 0.012;
    let relief_dh = vec2f(dpdx(relief), dpdy(relief));
    (*s).normal = apron_relief(n, dpdx(f.world), dpdy(f.world), relief_dh);
    (*s).color = color;
    (*s).roughness = roughness;
    (*s).metalness = 0.0;
}

// Cover container: its line's color turned a little warmer or cooler, sun-bleached
// chalky on some boxes and darkened by grime on others; then the yard's weather,
// in world space so no two boxes wear alike: rust weeping down the walls from the
// roof rail, road dirt splashed up from the floor, and on the roofs rust blooms
// where the paint has gone, rain water dried into dark rings in the middle and rust
// eating in from the edges, where the spreaders and castings bite.
fn container_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let tier_detail = fine_detail();
    let variant = apron_pixel_hash(floor(f.world.xz / CONTAINER_CELL) + vec2f(5.0, 77.0));
    var paint = apron_hue((*s).color, (variant.x - 0.5) * 0.85);
    let fade = smoothstep(0.45, 1.0, variant.y);
    paint = mix(paint, vec3f(dot(paint, APRON_LUMA)) * 1.2 + vec3f(0.025), fade * 0.45);
    paint = paint * mix(0.82, 1.08, variant.z);
    var roughness = mix((*s).roughness, 0.78, fade);
    let n = (*s).normal;
    let y = f.world.y;
    let along = select(f.world.x, f.world.z, abs(n.x) > abs(n.z));
    let run_noise = apron_pixel_noise(vec2f(along * 2.3, 3.0 + variant.w * 40.0));
    let run_length = 0.5 + 2.6 * abs(sin(run_noise * 9.0 + variant.x * 6.0));
    var wobble = 0.175;
    if tier_detail {
        wobble = apron_pixel_noise(vec2f(along * 7.0, y * 1.7)) * 0.35;
    }
    let weep = smoothstep(0.58, 0.82, run_noise + wobble * 0.3)
        * smoothstep(CONTAINER_HEIGHT - 0.2 - run_length, CONTAINER_HEIGHT - 0.25, y + wobble)
        * (0.55 + 0.45 * variant.y);
    let splash = (1.0 - smoothstep(0.15, 0.7 + (1.0 - run_noise) * 0.7, y)) * (0.6 + 0.4 * wobble * 2.0);
    // Where this box's steel meets its frame: metres to the nearest edge of its
    // roof, to its ends, and to its nearest corner casting. Rust starts there,
    // where the paint is knocked by spreaders and twistlocks and water stands.
    var box_edge = 1e3;
    var box_end = 1e3;
    var corner = 1e3;
    var long_z = false;
    // The containers mirror across both axes, so the box is the nearest of
    // those in the point's own quadrant.
    let mirrored = abs(f.world.xz);
    for (var i = 0; i < 3; i++) {
        let b = APRON_CONTAINER_QUADRANT[i];
        let d = apron_box_distance(mirrored, b);
        if abs(d) < abs(box_edge) {
            box_edge = d;
            long_z = b.w > b.z;
            let q = abs(mirrored - b.xy) - b.zw;
            // The doors and the front wall are the short ends.
            box_end = select(-q.x, -q.y, b.w > b.z);
            corner = length(q);
        }
    }
    box_edge = abs(box_edge);
    // The roof is pressed in shallow corrugations across the box and dented
    // where spreaders and dropped lashing gear struck it; the relief fades
    // where a pixel spans a corrugation.
    let roof = select(0.0, 1.0, n.y > 0.5);
    let roof_footprint = length(fwidth(f.world));
    let corrugation = 0.5 + 0.5 * cos(select(mirrored.x, mirrored.y, long_z) * 6.2831853 / CONTAINER_ROOF_PITCH);
    let corrugation_fade = 1.0 - smoothstep(0.12, 0.3, roof_footprint);
    let broad = apron_pixel_noise(f.world.xz * 0.45 + variant.xy * 30.0 + vec2f(y * 0.3));
    let dents = smoothstep(0.58, 0.86, apron_pixel_noise(f.world.xz * 0.85 + variant.zw * 13.0));
    let roof_relief = roof * (corrugation * 0.03 * corrugation_fade - dents * 0.025);
    (*s).normal = apron_relief(n, dpdx(f.world), dpdy(f.world), vec2f(dpdx(roof_relief), dpdy(roof_relief)));
    // How far gone this box is: some lines keep their boxes painted, others
    // run them to rust.
    let age = smoothstep(0.15, 0.95, variant.w);
    var fine = 0.5;
    if tier_detail {
        fine = apron_pixel_noise(vec2f(along * 13.0, y * 13.0 + f.world.z * 7.0));
    }
    if n.y > 0.5 {
        // Roof: rust gathers where steel meets steel and water stands: a thin
        // line along the top rails, blooms round the corner castings, and in
        // the corrugations' valleys small dark ponding stains where rain dried
        // and the dirt it carried settled. The field between keeps its paint.
        let rim = 1.0 - smoothstep(0.04, 0.1 + 0.16 * broad * age, box_edge);
        let casting = 1.0 - smoothstep(0.12, 0.38 + 0.3 * broad * age, corner);
        let rust = clamp((rim * 0.85 + casting) * smoothstep(0.3, 0.6, fine + age * 0.35), 0.0, 1.0);
        let valley = 1.0 - corrugation;
        let ponding = smoothstep(0.62, 0.8, broad * 0.7 + fine * 0.3)
            * mix(0.35, 1.0, valley) * smoothstep(0.25, 0.8, box_edge);
        // Rust bleeding in a few short streaks from the rails along the valleys,
        // the way the rain ran off.
        let across_roof = select(mirrored.y, mirrored.x, long_z);
        let streak_hash = apron_pixel_hash(vec2f(floor(across_roof / CONTAINER_ROOF_PITCH), variant.w * 50.0 + 7.0));
        let streaks = (1.0 - smoothstep(0.05, 0.25 + streak_hash.y * 0.6 * age, box_edge)) * step(0.72, streak_hash.x)
            * valley * smoothstep(0.35, 0.65, fine);
        paint = mix(paint, paint * vec3f(1.1, 1.08, 1.04) + vec3f(0.02), 0.22);
        paint = mix(paint, paint * vec3f(0.62, 0.58, 0.52), ponding * 0.55);
        // Dirt settles in the corrugations' valleys and the dents; the top
        // rails' edges are knocked bright where the paint chips off them.
        paint = paint * mix(1.0, mix(0.7, 1.08, corrugation), corrugation_fade) * (1.0 - dents * 0.14);
        let rail = (1.0 - smoothstep(0.03, 0.1, box_edge)) * smoothstep(0.35, 0.6, fine);
        paint = mix(paint, paint * 1.45 + vec3f(0.035), rail * 0.5);
        // Rust bleeds a brown halo into the paint round it.
        let halo = clamp(rim * 1.2 + casting * 1.2 + streaks, 0.0, 1.0) * smoothstep(0.2, 0.5, fine + age * 0.3);
        paint = mix(paint, paint * vec3f(0.78, 0.64, 0.52), halo * 0.4);
        paint = mix(paint, CONTAINER_RUST * mix(0.65, 1.25, fine), clamp(rust + streaks * 0.6, 0.0, 1.0) * 0.85);
        roughness = mix(roughness, 0.92, rust);
    } else if abs(n.y) < 0.5 {
        // Walls: rust where the steel is knocked and water collects: the
        // bottom rail, the corner posts and the door end's hinges and locking
        // bars, weeping in runs down from the roof rail at the corners and
        // under a few dents, and road dirt splashed up from the floor. The
        // middle of a long wall keeps most of its paint.
        let end = 1.0 - smoothstep(0.08, 0.35 + broad * 0.4 * age, box_end);
        let hinge_rows = (1.0 - smoothstep(0.05, 0.12, abs(fract(y / 0.9 + 0.35) - 0.5) * 0.9))
            * (1.0 - smoothstep(0.3, 0.9, box_end));
        let foot = (1.0 - smoothstep(0.16, 0.24 + broad * 0.35 * age, y)) * smoothstep(0.3, 0.6, fine + age * 0.3);
        let creep = clamp((end + hinge_rows * 0.6) * smoothstep(0.45, 0.72, fine + age * 0.25) + foot, 0.0, 1.0);
        // Runs fall from the roof rail, mostly near the corners.
        let near_end = 1.0 - smoothstep(0.4, 2.2, box_end);
        let runs = weep * mix(0.25, 1.0, near_end) * smoothstep(0.66, 0.8, run_noise + near_end * 0.15);
        paint = mix(paint, CONTAINER_RUST * mix(0.8, 1.15, run_noise), runs * 0.6);
        paint = mix(paint, CONTAINER_RUST * mix(0.7, 1.2, fine), creep * 0.8);
        paint = mix(paint, paint * vec3f(0.55, 0.5, 0.44), splash * 0.6);
        roughness = mix(roughness, 0.9, max(max(runs, splash), creep) * 0.7);
        // Light thrown up off the sunlit apron fills the walls' shaded side, so
        // their corrugation never sinks into black: strongest low down.
        let bounce = (1.0 - smoothstep(0.0, CONTAINER_HEIGHT * 1.4, y)) * 0.7 + 0.3;
        (*s).emissive = (*s).emissive + paint * frame.sun_color.rgb * CONTAINER_BOUNCE * bounce;
    }
    (*s).color = paint;
    (*s).roughness = roughness;
}

// Spawn pad: a gunmetal tread plate polished where tanks turn on it, a hazard rim
// in the team's paint, team chevrons pointing in, a bolt circle and the hatch at
// its heart; the ring above it is a light strip that a pulse chases round. Team
// paint is knocked back toward grey, so from the air the pads mark the spawns
// without outshining the tanks on them.
fn pad_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let grain = apron_grain(f.world.xz);
    let grain_luma = dot(grain, APRON_LUMA);
    let footprint = length(fwidth(f.world));
    let pad_light = f.uv.x > PAD_LIGHT_FLAG * 0.5;
    let pad = (f.uv - vec2f(select(0.0, PAD_LIGHT_FLAG, pad_light), 0.0)) * PAD_RADIUS;
    let pad_r = length(pad);
    let pad_aa = max(fwidth(pad_r), 1e-4);
    // The lugs of its tread plate, which alternate their lie from one diamond to
    // the next.
    let diamond_q = vec2f(pad.x + pad.y, pad.x - pad.y) * 2.6;
    let diamond_cell = fract(diamond_q) - 0.5;
    let diamond_flip = step(0.5, fract((floor(diamond_q.x) + floor(diamond_q.y)) * 0.5));
    let lug_long = abs(diamond_cell.x) * 0.3 + abs(diamond_cell.y);
    let lug_cross = abs(diamond_cell.x) + abs(diamond_cell.y) * 0.3;
    let diamond_d = mix(lug_long, lug_cross, diamond_flip);
    let diamond_aa = max(fwidth(diamond_d), 1e-4);
    let lug = 1.0 - smoothstep(0.17 - diamond_aa, 0.17 + diamond_aa, diamond_d);
    let relief = lug * 0.012 * (1.0 - smoothstep(0.02, 0.06, footprint))
        - (1.0 - smoothstep(0.0, 0.05, abs(pad_r - 0.6))) * 0.01;
    let relief_dh = vec2f(dpdx(relief), dpdy(relief));
    let team = mix(vec3f(dot(f.vertex_color.rgb, APRON_LUMA)), f.vertex_color.rgb, PAD_SATURATION);
    let angle = atan2(pad.y, pad.x);
    let side = abs((*s).normal.y) < 0.5;
    let worn_noise = apron_pixel_noise(pad * 1.7 + 3.0);
    let worn = smoothstep(0.35, 0.8, worn_noise * 0.6 + (grain_luma - 1.0) * 0.3 + 0.35)
        * (1.0 - smoothstep(0.4, 1.8, pad_r));
    var deck = material.color.rgb * mix(0.9, 1.08, grain_luma * 0.5 + 0.25) * mix(1.0, 1.35, lug);
    deck = mix(deck, vec3f(0.36, 0.38, 0.39), worn * (0.4 + lug * 0.4));
    // The yard's dust tracked over the plate, thickest toward the rim where it
    // blows in off the concrete, so the pad sits in the yard rather than on it.
    let dust = smoothstep(0.3, 0.75, (1.0 - worn_noise) * 0.6 + smoothstep(0.9, 2.5, pad_r) * 0.5
        + (grain_luma - 1.0) * 0.2) * (1.0 - worn * 0.5);
    deck = mix(deck, vec3f(0.15, 0.142, 0.128) * mix(0.85, 1.15, grain_luma * 0.5), dust * 0.55);
    // Hazard rim: diagonal stripes of team paint and black round the bevel.
    let around = angle * pad_r;
    let stripe_aa = max(fwidth(around), 1e-3) * 0.9;
    let stripe = smoothstep(0.5 - stripe_aa, 0.5 + stripe_aa, fract((around + pad_r) * 0.9));
    let rim = smoothstep(PAD_RING_OUTER + 0.03 - pad_aa, PAD_RING_OUTER + 0.03 + pad_aa, pad_r);
    let rim_paint = mix(vec3f(0.035, 0.037, 0.04), team * 0.6, stripe) * mix(0.75, 1.0, worn_noise);
    // Chevrons: four Vs pointing at the centre, between 1.15 and 1.95 m out.
    let quarter = round(angle / 1.5707963) * 1.5707963;
    let radial = vec2f(cos(quarter), sin(quarter));
    let u = dot(pad, radial);
    let v = dot(pad, vec2f(-radial.y, radial.x));
    let chevron_d = abs(abs(v) * 0.9 - (u - 1.2)) - 0.12;
    let chevron = (1.0 - smoothstep(-pad_aa, pad_aa, chevron_d))
        * step(1.12, u) * step(u, 1.9) * step(abs(v), 0.66);
    let chip = smoothstep(0.3, 0.36, worn_noise * 0.6 + (grain_luma - 1.0) * 0.15 + 0.12);
    let paint = chevron * chip;
    // Bolt circle and the central hatch's seam.
    let bolt_angle = round(angle / 0.3926991) * 0.3926991;
    let bolt_d = length(pad - vec2f(cos(bolt_angle), sin(bolt_angle)) * PAD_BOLTS);
    let bolt = 1.0 - smoothstep(0.045 - pad_aa, 0.045 + pad_aa, bolt_d);
    let seam = 1.0 - smoothstep(0.0, 0.025 + pad_aa, abs(pad_r - 0.6));
    let hatch = 1.0 - smoothstep(0.6 - pad_aa, 0.6 + pad_aa, pad_r);
    var color = mix(deck, deck * vec3f(0.85, 0.9, 0.95) * 1.1, hatch);
    color = mix(color, team * mix(0.7, 0.95, grain_luma * 0.5), paint);
    color = mix(color, rim_paint * mix(0.85, 1.05, grain_luma * 0.5), rim * chip);
    color = mix(color, vec3f(0.5, 0.5, 0.48), bolt);
    color = color * (1.0 - seam * 0.6);
    // Dirt from the tracks gathers in the tread plate's valleys and at the rim.
    color = color * mix(0.78, 1.0, lug) * mix(1.0, 0.82, smoothstep(2.0, 2.6, pad_r) * (1.0 - stripe * 0.3));
    if side {
        let band = smoothstep(0.45, 0.55, fract(around * 0.9 + f.world.y * 3.0));
        color = mix(vec3f(0.04, 0.045, 0.05), team * 0.7, band);
    }
    (*s).color = color;
    (*s).roughness = mix(mix(mix(0.66, 0.42, worn), 0.9, dust), 0.66, max(paint, rim));
    (*s).metalness = mix(0.55, 0.0, max(paint, rim));
    (*s).normal = apron_relief((*s).normal, dpdx(f.world), dpdy(f.world), relief_dh);
    if pad_light {
        // The ring is a light strip; no early return, so the derivatives above
        // stay in uniform control flow.
        let chase = pow(0.5 + 0.5 * cos(angle * 3.0 - frame.camera_position.w * 2.2), 6.0);
        let light = mix(vec3f(dot(f.vertex_color.rgb, APRON_LUMA)), f.vertex_color.rgb, PAD_LIGHT_SATURATION);
        (*s).color = light * 0.35;
        // Calm while idle, so a dozen pads never out-shout the tanks on them.
        (*s).emissive = light * (0.22 + 0.7 * chase);
        (*s).roughness = 0.3;
        (*s).metalness = 0.0;
    }
}

// Ship hull plating: welded seams, rust running down from the scuppers and the
// deck edge, a slimy band at the waterline, fender scrapes and the blotchy fade of
// a hull painted in sections.
fn hull_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let faces_x = abs((*s).normal.x) > abs((*s).normal.z);
    let along = select(f.world.x, f.world.z, faces_x);
    let grain = apron_grain(vec2f(along, f.world.y));
    let grain_luma = dot(grain, APRON_LUMA);
    let footprint = length(fwidth(f.world));
    let plate = (0.5 - abs(fract(vec2f(along / 2.4, (f.world.y + 0.2) / 1.6)) - 0.5)) * vec2f(2.4, 1.6);
    let plate_aa = max(fwidth(plate.x) + fwidth(plate.y), 1e-4);
    let height = f.world.y - APRON_SEA_LEVEL;
    let mottle = apron_pixel_noise(vec2f(along, f.world.y) / 9.0);
    let seam = clamp((0.012 - min(plate.x, plate.y)) / plate_aa + 0.5, 0.0, 1.0);
    let run_cell = floor(along / 2.7);
    let run_hash = apron_pixel_hash(vec2f(run_cell, 13.0));
    let run_x = (run_cell + 0.2 + 0.6 * run_hash.x) * 2.7;
    let run_width = 0.05 + 0.14 * run_hash.y;
    let run_top = 5.0 - run_hash.w * 0.6;
    let run_length = 1.0 + 3.5 * run_hash.z;
    let run_d = (along - run_x + (mottle - 0.5) * 0.08) / run_width;
    let run = exp(-run_d * run_d) * smoothstep(run_top - run_length, run_top - 0.3, height) * step(0.3, run_hash.z);
    let bleed = smoothstep(0.55, 0.85, apron_pixel_noise(vec2f(along * 0.6, height * 2.2)) * mottle * 1.8);
    let slime = 1.0 - smoothstep(0.1, 0.55 + mottle * 0.4, height);
    let scrape = smoothstep(0.78, 0.9, apron_pixel_noise(vec2f(along * 0.35, height * 6.0))) * (1.0 - smoothstep(1.0, 3.2, height));
    let section = apron_pixel_hash(vec2f(floor(along / 9.0), floor(height / 2.5)) + 3.0).x;
    var color = (*s).color * mix(0.88, 1.06, section) * mix(0.92, 1.05, mottle) * mix(0.9, 1.04, grain_luma * 0.5 + 0.25);
    color = color * (1.0 - seam * 0.32);
    color = mix(color, color * vec3f(1.15, 1.12, 1.08) + vec3f(0.02), scrape * 0.5);
    color = mix(color, vec3f(0.17, 0.065, 0.025), clamp(run * 0.85 + bleed * 0.35, 0.0, 1.0));
    color = mix(color, vec3f(0.03, 0.045, 0.03), slime * 0.8);
    var roughness = mix((*s).roughness, 0.85, run);
    roughness = mix(roughness, 0.4, slime);
    (*s).color = color;
    (*s).roughness = roughness;
    (*s).emissive = (*s).emissive + frame.sun_color.rgb * apron_caustics(along, f.world.y, footprint) * 0.022;
}

// Quay wall: weathered concrete panels, a dark wet tide band crusted with barnacles
// and weed at the waterline, and rust and lime streaks running down from the top.
// A quay's top is ground like the apron: its grain lies in x/z, not stretched along
// a face's height.
fn quay_wall_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let faces_x = abs((*s).normal.x) > abs((*s).normal.z);
    let along = select(f.world.x, f.world.z, faces_x);
    let face = select(0.0, 1.0, abs((*s).normal.y) < 0.5);
    let p = select(f.world.xz, vec2f(along, f.world.y), face > 0.5);
    let grain = apron_grain(p);
    let grain_luma = dot(grain, APRON_LUMA);
    let footprint = length(fwidth(f.world));
    let wall_edge = (0.5 - abs(fract(along / APRON_WALL_JOINT) - 0.5)) * APRON_WALL_JOINT;
    let wall_aa = max(fwidth(wall_edge), 1e-4);
    let mottle = apron_pixel_noise(p / 9.0);
    let broad = apron_pixel_noise(p / 43.0 + 3.0);
    // Held within a few steps of value, so the stains read as wear.
    let grime = mix(0.88, 1.07, smoothstep(0.25, 0.75, mottle)) * mix(0.8, 1.14, smoothstep(0.25, 0.75, broad));
    let height = f.world.y - APRON_SEA_LEVEL;
    // Fine detail fades where a pixel spans it, so the tide line stays a soft
    // ragged edge from across the basin instead of breaking into speckle.
    let resolved = 1.0 - smoothstep(0.03, 0.12, footprint);
    let ragged = (mottle - 0.5) * 0.35 + (grain_luma - 1.0) * 0.08 * resolved;
    let wet = 1.0 - smoothstep(APRON_TIDE * 0.45, APRON_TIDE, height + ragged);
    let weed = 1.0 - smoothstep(0.05, 0.45, height + ragged * 0.6);
    let barnacles = smoothstep(0.62, 0.72, apron_pixel_noise(vec2f(along * 6.0, f.world.y * 6.0)))
        * (1.0 - smoothstep(0.25, 0.6, abs(height - 0.32 - ragged * 0.4))) * resolved;
    // Runs and joints belong to the faces; a quay's top only weathers.
    let runs = apron_pixel_noise(vec2f(along * 1.3, f.world.y * 0.08));
    let rust = smoothstep(0.62, 0.8, runs) * smoothstep(-2.5, -0.4, f.world.y) * 0.6 * face;
    let lime = smoothstep(0.7, 0.86, apron_pixel_noise(vec2f(along * 0.9 + 19.0, f.world.y * 0.1))) * 0.5 * face;
    let joint = clamp((0.03 - wall_edge) / wall_aa + 0.5, 0.0, 1.0) * face;
    var color = material.color.rgb * f.tint.rgb * grain * grime;
    color = mix(color, color * vec3f(1.1, 1.08, 1.02), lime * (1.0 - wet));
    color = mix(color, color * vec3f(0.62, 0.42, 0.28), rust * (1.0 - wet));
    color = mix(color, color * vec3f(0.3, 0.33, 0.31), wet);
    color = mix(color, vec3f(0.03, 0.055, 0.032) * mix(1.0, grain_luma, resolved), weed * 0.85);
    color = mix(color, vec3f(0.42, 0.42, 0.38) * grain_luma, barnacles * 0.7);
    color = color * (1.0 - joint * 0.55);
    (*s).color = color;
    (*s).roughness = mix((*s).roughness, 0.35, wet * (1.0 - barnacles));
    (*s).emissive = (*s).emissive + frame.sun_color.rgb * apron_caustics(along, f.world.y, footprint) * 0.03 * face;
}

// The structures round the yard, each mode its own function. `mode` is a material
// uniform, so the branches are uniform control flow and the samples and
// derivatives each mode needs run inside its own.
fn apron_structure(s: ptr<function, Surface>, f: EffectFragment) {
    let mode = material.params[0].x;
    if mode > 10.5 {
        parapet_surface(s, f);
    } else if mode > 9.5 {
        container_surface(s, f);
    } else if mode > 7.5 {
        pad_surface(s, f);
    } else if mode > 5.5 {
        hull_surface(s, f);
    } else {
        quay_wall_surface(s, f);
    }
}

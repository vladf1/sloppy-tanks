// Harbor Havoc's surfaces worked out in world space (`effects_scenery::HARBOR_APRON`),
// so slabs, stains and wear never repeat with a texture tile. Extra texture 0 is
// the dock concrete tile, the grain of every mode; its broad blotches are divided
// out (a high-pass against a blurred sample) so they never stamp a pattern.
// params[0].x is the `ApronMode`: 0 concrete, 1 asphalt lane along z, 2 asphalt
// lane along x, 3 alpha-tested marking, 4 transparent label (its map is the
// lettering), 5 quay wall faces (the horizontal coordinate follows the face, y
// climbs out of the water), 6 ship hull plating, 7 the far shore (lit and hazed
// here, emitted as its final color with the material's fog off), 8 a spawn pad's
// steel deck and its light ring (team paint in the vertex color, the pad's own
// coordinates in the uv, offset by `PAD_LIGHT_FLAG` on the ring), 10 a cover
// container's paint (standard, over its wear tile), shaded per box.

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
// Asphalt repair patches, oil spills, utility covers, the yard's old weathered
// stains, the rubber arcs of turning trucks and ghosts of removed paint are
// scattered on these grids, metres.
const APRON_PATCH_CELL = 9.0;
const APRON_STAIN_CELL = 7.0;
const APRON_COVER_CELL = 13.0;
const APRON_OLD_STAIN_CELL = 17.0;
const APRON_SCUFF_CELL = 15.0;
const APRON_GHOST_CELL = 11.0;
const APRON_TRACK_CELL = 9.0;
// Length of a relaid stretch of asphalt lane, metres.
const APRON_SECTION = 23.0;
// Harbor water level (`harbor_water::HARBOR_WATER_HEIGHT`) and how far the tide
// stains the quay wall above it.
const APRON_SEA_LEVEL = -2.2;
const APRON_TIDE = 0.75;
// Spacing of the quay wall's vertical expansion joints, metres.
const APRON_WALL_JOINT = 12.0;
// Farthest a container's casting rust reaches from the casting, metres (with a
// margin over the 1.2 m its widest fade spans).
const APRON_CASTING_REACH = 1.25;
// Inner face of the boundary wall (`sim::data::ARENA`).
const APRON_ARENA = 60.0;
// Distance of a truck's wheel paths from the middle of its traffic half, metres.
const APRON_WHEEL_OFFSET = 0.9;
// Traffic lanes as (centre, half width): along z (centre x), then along x (centre
// z), as `harbor_scenery::LANES_ALONG_Z` and `LANES_ALONG_X`.
const APRON_LANES_Z = array<vec2f, 3>(vec2f(-52.0, 5.0), vec2f(0.0, 7.5), vec2f(52.0, 5.0));
const APRON_LANES_X = array<vec2f, 3>(vec2f(-45.0, 5.0), vec2f(0.0, 5.0), vec2f(45.0, 5.0));
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
// Far shore haze: metres of air that hide half of a color beyond the first
// clear stretch, and how much less haze the high ground gets.
const SHORE_HAZE_DISTANCE = 340.0;
const SHORE_HAZE_CLEAR = 45.0;
const SHORE_HAZE_LIFT = 0.25;
// A far-shore vertex painted exactly this red is a warning light, and how
// brightly it shines.
const SHORE_LIGHT = vec3f(1.0, 0.0, 0.0);
const SHORE_LIGHT_GLOW = vec3f(2.4, 0.06, 0.03);
// Window pitch on the far city's towers: across the face and floor to floor, metres.
const SHORE_WINDOW = vec2f(2.6, 3.4);
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
const SHORE_FOREST = vec3f(0.024, 0.064, 0.022);
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
// A spawn pad's radius (`harbor_scenery::PAD_RADIUS`), the light ring's outer
// edge and the radius of its bolt circle, metres.
const PAD_RADIUS = 2.65;
const PAD_RING_OUTER = 2.3;
const PAD_BOLTS = 1.92;
// The uv.x offset of a pad's light ring (`harbor_scenery::PAD_LIGHT_FLAG`).
const PAD_LIGHT_FLAG = 10.0;
// How much of the team's saturation a pad's paint keeps.
const PAD_SATURATION = 0.7;
// Cover containers each stand inside one cell of this grid (`harbor_layout`), so
// a box takes one shade of its line's color from its cell.
const CONTAINER_CELL = 20.0;
// Height of a cover container (`harbor_layout`), metres, and the linear color of
// the rust that weeps from its roof rail and blooms on its roof.
const CONTAINER_HEIGHT = 3.6;
const CONTAINER_RUST = vec3f(0.2, 0.072, 0.026);
// Share of the sunlight that the apron throws back up onto a container's walls.
const CONTAINER_BOUNCE = 0.045;

fn apron_hash(cell: vec2f) -> vec4f {
    var p = fract(cell.xyxy * vec4f(0.1031, 0.1030, 0.0973, 0.1099));
    p = p + dot(p, p.wzxy + 33.33);
    return fract((p.xxyz + p.yzzw) * p.zywx);
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

fn apron_box_distance(p: vec2f, b: vec4f) -> f32 {
    let q = abs(p - b.xy) - b.zw;
    return length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0);
}

// Wheel polish and the oil strip between the wheel paths, for a point `across`
// metres from the centre of a lane `half` metres wide.
fn apron_traffic(across: f32, half: f32) -> vec2f {
    let offset = abs(abs(across) - half * 0.5);
    let wheels = exp(-pow((offset - APRON_WHEEL_OFFSET) / 0.4, 2.0));
    let strip = exp(-pow(offset / 0.3, 2.0));
    return vec2f(wheels, strip) * step(abs(across), half);
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

// Sunlight thrown back off the harbor's ripples, dancing on a face rising from
// the water: a moving web of bright lines, strongest just above the waterline.
fn apron_caustics(along: f32, y: f32, footprint: f32) -> f32 {
    let t = frame.camera_position.w;
    let height = y - APRON_SEA_LEVEL;
    let a = apron_noise(vec2f(along * 1.6 + t * 0.4, y * 3.0 - t * 0.25));
    let b = apron_noise(vec2f(along * 2.1 - t * 0.33 + 9.0, y * 3.6 + t * 0.22));
    let web = pow(clamp(1.0 - abs(a - b) * 2.4, 0.0, 1.0), 6.0);
    // Faded where a pixel spans the web, so it never crawls at a distance.
    return web * smoothstep(0.08, 0.4, height) * (1.0 - smoothstep(0.4, 1.8, height))
        * (1.0 - smoothstep(0.03, 0.12, footprint));
}

// Each layer below runs only where its weight can be non-zero, so a skipped pixel
// keeps exactly the value it computed before: the cover containers and the far
// shore need none of the yard's work, the ground's noise lookups wait for the
// joint, slab, spill, lane or cover that uses them, and container castings only
// near a container. `mode` is a material uniform, so branches on it are uniform
// control flow: the texture samples and derivatives a mode needs run inside its
// branch.
fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let mode = material.params[0].x;
    if mode > 9.5 {
        // Cover container: its line's color turned a little warmer or cooler,
        // sun-bleached chalky on some boxes and darkened by grime on others.
        let variant = apron_hash(floor(f.world.xz / CONTAINER_CELL) + vec2f(5.0, 77.0));
        var paint = apron_hue((*s).color, (variant.x - 0.5) * 0.85);
        let fade = smoothstep(0.45, 1.0, variant.y);
        paint = mix(paint, vec3f(dot(paint, APRON_LUMA)) * 1.2 + vec3f(0.025), fade * 0.45);
        paint = paint * mix(0.82, 1.08, variant.z);
        var roughness = mix((*s).roughness, 0.78, fade);
        // Then the yard's weather, in world space so no two boxes wear alike: rust
        // weeping down the walls from the roof rail in runs of their own length,
        // road dirt splashed up from the floor, and on the roofs rust blooms where
        // the paint has gone, rain water dried into dark rings in the middle and
        // rust eating in from the edges, where the spreaders and castings bite.
        let n = (*s).normal;
        let y = f.world.y;
        let along = select(f.world.x, f.world.z, abs(n.x) > abs(n.z));
        let run_noise = apron_noise(vec2f(along * 2.3, 3.0 + variant.w * 40.0));
        let run_length = 0.5 + 2.6 * apron_noise(vec2f(along * 0.9, 11.0 + variant.x * 20.0));
        let wobble = apron_noise(vec2f(along * 7.0, y * 1.7)) * 0.35;
        let weep = smoothstep(0.58, 0.82, run_noise + wobble * 0.3)
            * smoothstep(CONTAINER_HEIGHT - 0.2 - run_length, CONTAINER_HEIGHT - 0.25, y + wobble)
            * (0.55 + 0.45 * variant.y);
        let splash = (1.0 - smoothstep(0.15, 0.7 + apron_noise(vec2f(along * 1.1, 5.0)) * 0.7, y))
            * (0.6 + 0.4 * apron_noise(vec2f(along * 4.0, y * 6.0)));
        // Where this box's steel meets its frame: metres to the nearest edge of its
        // roof, to its ends, and to its nearest corner casting. Rust starts there,
        // where the paint is knocked by spreaders and twistlocks and water stands.
        var box_edge = 1e3;
        var box_end = 1e3;
        var corner = 1e3;
        // The containers mirror across both axes, so the box is the nearest of
        // those in the point's own quadrant.
        let mirrored = abs(f.world.xz);
        for (var i = 0; i < 3; i++) {
            let b = APRON_CONTAINER_QUADRANT[i];
            let d = apron_box_distance(mirrored, b);
            if abs(d) < abs(box_edge) {
                box_edge = d;
                let q = abs(mirrored - b.xy) - b.zw;
                // The doors and the front wall are the short ends.
                box_end = select(-q.x, -q.y, b.w > b.z);
                corner = length(q);
            }
        }
        box_edge = abs(box_edge);
        // How far gone this box is: some lines keep their boxes painted, others
        // run them to rust.
        let age = smoothstep(0.15, 0.95, variant.w);
        let fine = apron_noise(f.world.xz * 7.0 + vec2f(y * 5.0)) * 0.6 + apron_noise(vec2f(along * 13.0, y * 13.0)) * 0.4;
        let broad = apron_noise(f.world.xz * 0.45 + variant.xy * 30.0 + vec2f(y * 0.3));
        if n.y > 0.5 {
            // Roof: rust eating in from the rails and heaviest round the castings,
            // a few pits and blisters out in the field where the paint is thin,
            // rain dried into rings in the middle.
            let rim = 1.0 - smoothstep(0.05, 0.22 + 0.5 * broad * age, box_edge);
            let casting = 1.0 - smoothstep(0.15, 0.55 + 0.4 * broad, corner);
            let pits = smoothstep(0.8 - age * 0.12, 0.9, apron_noise(f.world.xz * 2.6 + variant.zw * 17.0) * 0.55 + fine * 0.45)
                * smoothstep(0.35, 0.75, broad);
            let rust = clamp((rim * 0.8 + casting) * smoothstep(0.3, 0.6, fine + age * 0.3) + pits, 0.0, 1.0);
            let pooled = apron_noise(f.world.xz * 0.38 + 9.0);
            let ring = smoothstep(0.5, 0.56, pooled) * (1.0 - smoothstep(0.6, 0.66, pooled));
            let dried = smoothstep(0.55, 0.62, pooled);
            paint = mix(paint, paint * vec3f(1.1, 1.08, 1.04) + vec3f(0.02), 0.25);
            paint = paint * (1.0 - ring * 0.14) * mix(1.0, 0.9, dried);
            // Rust bleeds a brown halo into the paint round it.
            let halo = clamp(rim * 1.4 + casting * 1.2 + pits * 0.6, 0.0, 1.0) * smoothstep(0.2, 0.5, fine + age * 0.3);
            paint = mix(paint, paint * vec3f(0.8, 0.68, 0.56), halo * 0.45);
            paint = mix(paint, CONTAINER_RUST * mix(0.65, 1.25, fine), rust * 0.85);
            roughness = mix(roughness, 0.92, rust);
        } else if abs(n.y) < 0.5 {
            // Walls: runs weeping from the roof rail, rust creeping up from the
            // bottom rail and bubbling along the door end, and road dirt.
            let end = 1.0 - smoothstep(0.1, 0.5 + broad * 0.6, box_end);
            let foot = (1.0 - smoothstep(0.22, 0.45 + broad * 0.4, y)) * smoothstep(0.35, 0.65, fine + age * 0.2);
            let creep = clamp(end * smoothstep(0.4, 0.7, fine + age * 0.25) + foot, 0.0, 1.0);
            paint = mix(paint, CONTAINER_RUST * mix(0.8, 1.15, run_noise), weep * 0.55);
            paint = mix(paint, CONTAINER_RUST * mix(0.7, 1.2, fine), creep * 0.75);
            paint = mix(paint, paint * vec3f(0.55, 0.5, 0.44), splash * 0.6);
            roughness = mix(roughness, 0.9, max(max(weep, splash), creep) * 0.7);
            // Light thrown up off the sunlit apron fills the walls' shaded side, so
            // their corrugation never sinks into black: strongest low down.
            let bounce = (1.0 - smoothstep(0.0, CONTAINER_HEIGHT * 1.4, y)) * 0.7 + 0.3;
            (*s).emissive = (*s).emissive + paint * frame.sun_color.rgb * CONTAINER_BOUNCE * bounce;
        }
        (*s).color = paint;
        (*s).roughness = roughness;
        return;
    }
    if mode > 6.5 && mode < 7.5 {
        // Far shore: lit by the sun and the sky fill, then hazed by the air between
        // it and the eye, emitted whole so the scene's fog never flattens the
        // layers. Painted warning lights shine through the haze, and the city's
        // towers (flagged by their uv) take floors of windows mirroring the sky.
        let n = (*s).normal;
        let across = select(f.world.x, f.world.z, abs(n.x) > abs(n.z));
        let window_cell = vec2f(across / SHORE_WINDOW.x, (f.world.y - APRON_SEA_LEVEL) / SHORE_WINDOW.y);
        let window_aa = fwidth(window_cell) * 0.75;
        let pane_at = fract(window_cell);
        let pane = smoothstep(0.2 - window_aa.x, 0.2 + window_aa.x, pane_at.x)
            * (1.0 - smoothstep(0.8 - window_aa.x, 0.8 + window_aa.x, pane_at.x))
            * smoothstep(0.32 - window_aa.y, 0.32 + window_aa.y, pane_at.y)
            * (1.0 - smoothstep(0.86 - window_aa.y, 0.86 + window_aa.y, pane_at.y));
        let walls = abs(n.y) < 0.5;
        let facade = select(0.0, 1.0, f.uv.y > 5.0 && f.uv.y < 15.0 && walls);
        let blinds = step(0.72, apron_hash(floor(window_cell) + vec2f(3.0, 9.0)).x);
        let glass = mix(frame.sky_color.rgb * 0.22 + vec3f(0.015, 0.025, 0.035), f.vertex_color.rgb * 0.7, blinds);
        var albedo = mix(f.vertex_color.rgb, glass, pane * facade);
        // Container stacks: every tier and 6 m bay its own box, in a line's paint,
        // with dark seams between them.
        let box_cell = floor(vec2f(across / 6.1, f.world.y / 2.6));
        let box_paint = SHORE_BOXES[u32(apron_hash(box_cell + vec2f(11.0, 5.0)).x * 6.0) % 6u];
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
        let terrain = select(0.0, 1.0, f.uv.y > 25.0)
            * smoothstep(0.004, 0.03, f.vertex_color.g - f.vertex_color.r);
        let shore_footprint = length(fwidth(f.world));
        let dry = apron_noise(ground / 61.0) * 0.6 + apron_noise(ground / 23.0 + 5.0) * 0.4;
        let woods = apron_noise(ground / 34.0 + 11.0) * 0.65 + apron_noise(ground / 11.0 + 3.0) * 0.35;
        // The woods' edge is ragged by the crowns along it, never a smooth blob.
        let crown_noise = apron_noise(ground / 4.2) * 0.6 + apron_noise(ground / 1.6 + 7.0) * 0.4;
        let woods_edge = woods + slope * 0.3 - 0.06 + (crown_noise - 0.5) * 0.07;
        let woods_aa = max(fwidth(woods_edge), 1e-4);
        let forest = smoothstep(0.5 - woods_aa, 0.5 + woods_aa, woods_edge);
        let crowns = mix(0.5, crown_noise, 1.0 - smoothstep(1.5, 5.0, shore_footprint));
        var land = f.vertex_color.rgb * mix(vec3f(0.88, 1.0, 0.86), vec3f(1.2, 1.1, 0.72), smoothstep(0.45, 0.8, dry));
        let field_at = vec2f(ground.x * 0.83 - ground.y * 0.56, ground.x * 0.56 + ground.y * 0.83) / SHORE_FIELD;
        let field = apron_hash(floor(field_at) + vec2f(7.0, 19.0));
        let field_to_edge = (0.5 - abs(fract(field_at) - 0.5)) * SHORE_FIELD;
        let field_edge = min(field_to_edge.x, field_to_edge.y);
        let fields = step(0.3, field.x) * smoothstep(0.16, 0.1, slope) * (1.0 - forest);
        let crop = SHORE_CROPS[u32(field.y * 4.0) % 4u] * mix(0.85, 1.12, field.z)
            * mix(0.92, 1.06, apron_noise(field_at * vec2f(40.0, 3.0)));
        let hedge = 1.0 - smoothstep(0.6, 1.6 + shore_footprint, field_edge);
        land = mix(land, crop, fields * 0.75);
        land = mix(land, SHORE_FOREST * 1.3, hedge * fields);
        // Crowns: deep shade between them, and sunlit tops a warmer green.
        let crown_lit = smoothstep(0.25, 0.75, crowns);
        land = mix(land, SHORE_FOREST * mix(vec3f(0.45), vec3f(2.1, 2.0, 1.5), crown_lit), forest);
        let rock = smoothstep(0.3, 0.46, slope + (apron_noise(ground / 7.0) - 0.5) * 0.25) * (1.0 - forest * 0.6);
        let strata = 0.82 + 0.18 * sin(f.world.y * 1.6 + apron_noise(ground / 9.0) * 3.0);
        land = mix(land, SHORE_ROCK * strata, rock);
        // Grass seen across a bay is never lime: its green greys a little.
        land = mix(vec3f(dot(land, APRON_LUMA)), land, SHORE_GRASS_SATURATION);
        albedo = mix(albedo, land, terrain);
        let hemisphere = mix(frame.ground_color.rgb, frame.sky_color.rgb, n.y * 0.5 + 0.5);
        let sun = max(dot(n, frame.sun_direction.xyz), 0.0) * frame.sun_color.rgb;
        let lit = albedo * (sun + hemisphere) * 0.3183099;
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
        if atmosphere_on() {
            // The scene's low sea mist: thick at the waterline and thinning up the
            // slopes, so the headlands rise out of it with clear green crests.
            let ray = (f.world - frame.camera_position.xyz) / max(distance, 1e-4);
            let mist = frame.haze_far.w * exp(-max(f.world.y, 0.0) / MIST_HEIGHT);
            hazed = mix(hazed, haze_tint(ray, mist), mist);
        }
        // The painted warning lights are flagged by their vertex colour, since the
        // albedo above is repainted for the countryside and containers.
        let light = select(0.0, 1.0, all(abs(f.vertex_color.rgb - SHORE_LIGHT) < vec3f(0.01)));
        (*s).emissive = mix(hazed, SHORE_LIGHT_GLOW, light);
        (*s).color = vec3f(0.0);
        (*s).roughness = 1.0;
        (*s).metalness = 0.0;
        return;
    }
    let wall = mode > 4.5 && mode < 6.5;
    // Concrete, the asphalt lanes and the paint lie on the yard.
    let ground = mode < 4.5;
    // Concrete and asphalt take the joints, cracks and lane streaks.
    let paved = mode < 2.5;
    // A wall face runs along x or z; its plane coordinates are (along, height).
    let faces_x = abs((*s).normal.x) > abs((*s).normal.z);
    let along = select(f.world.x, f.world.z, faces_x);
    let p = select(f.world.xz, vec2f(along, f.world.y), wall);
    // Every sample and derivative first, in control flow uniform across the draw.
    let turned = vec2f(p.x * 0.8 - p.y * 0.6, p.x * 0.6 + p.y * 0.8);
    let grain_a = textureSample(extra_texture0, extra_sampler0, p / 4.7).rgb;
    let grain_b = textureSample(extra_texture0, extra_sampler0, turned / 9.3 + 0.31).rgb;
    let blur_a = textureSampleBias(extra_texture0, extra_sampler0, p / 4.7, APRON_GRAIN_BLUR).rgb;
    let blur_b = textureSampleBias(extra_texture0, extra_sampler0, turned / 9.3 + 0.31, APRON_GRAIN_BLUR).rgb;
    var fine = 0.0;
    if (mode > 0.5 && mode < 4.5) || mode > 7.5 {
        fine = dot(textureSample(extra_texture0, extra_sampler0, turned / 1.3 + 0.17).rgb, APRON_LUMA) / APRON_TILE_MEAN;
    }
    var chips = 0.0;
    var streak_z = 0.0;
    var streak_x = 0.0;
    var crack_noise = 0.0;
    var crack_width = 0.0;
    var craze = 0.0;
    var craze_width = 0.0;
    if paved {
        chips = dot(textureSample(extra_texture0, extra_sampler0, p / 0.61 + 0.43).rgb, APRON_LUMA) / APRON_TILE_MEAN;
        streak_z = dot(textureSample(extra_texture0, extra_sampler0, vec2f(p.x / 1.9, p.y / 60.0)).rgb, APRON_LUMA);
        streak_x = dot(textureSample(extra_texture0, extra_sampler0, vec2f(p.y / 1.9, p.x / 60.0) + 0.5).rgb, APRON_LUMA);
        crack_noise = apron_noise(p / 3.1 + 13.0) + apron_noise(p * 0.9) * 0.18;
        crack_width = max(fwidth(crack_noise), 1e-4);
        // Only the asphalt lanes craze.
        if mode > 0.5 {
            craze = min(abs(apron_noise(p * 2.3 + 5.0) - 0.5), abs(apron_noise(turned * 2.9 + 9.0) - 0.5));
            craze_width = max(fwidth(craze), 1e-4);
        }
    }
    let slab_uv = p / APRON_SLAB;
    let cell = floor(slab_uv);
    let to_edge = (0.5 - abs(fract(slab_uv) - 0.5)) * APRON_SLAB;
    let edge = min(to_edge.x, to_edge.y);
    let aa = max(fwidth(edge), 1e-4);
    let to_expansion = (0.5 - abs(fract(p / APRON_EXPANSION) - 0.5)) * APRON_EXPANSION;
    let expansion = min(to_expansion.x, to_expansion.y);
    let expansion_aa = max(fwidth(expansion), 1e-4);
    let patch_uv = p / APRON_PATCH_CELL;
    let patch_cell = floor(patch_uv);
    let patch_local = fract(patch_uv) * APRON_PATCH_CELL;
    let patch_aa = max(fwidth(patch_local.x) + fwidth(patch_local.y), 1e-4);
    let wall_edge = (0.5 - abs(fract(along / APRON_WALL_JOINT) - 0.5)) * APRON_WALL_JOINT;
    let wall_aa = max(fwidth(wall_edge), 1e-4);
    // A utility cover in some cells: distance to its centre.
    let cover_cell = floor(p / APRON_COVER_CELL);
    let cover_hash = apron_hash(cover_cell + vec2f(41.0, 7.0));
    let cover_at = (cover_cell + 0.25 + 0.5 * cover_hash.xy) * APRON_COVER_CELL;
    let cover_d = length(p - cover_at);
    let cover_aa = max(fwidth(cover_d), 1e-4);
    // Hull plating: welded seams on a 2.4 x 1.6 m grid.
    let plate = (0.5 - abs(fract(vec2f(along / 2.4, (f.world.y + 0.2) / 1.6)) - 0.5)) * vec2f(2.4, 1.6);
    let plate_aa = max(fwidth(plate.x) + fwidth(plate.y), 1e-4);
    // Spawn pad coordinates, metres from its centre, and the lugs of its tread
    // plate, which alternate their lie from one diamond to the next.
    let pad_light = f.uv.x > PAD_LIGHT_FLAG * 0.5;
    let pad = (f.uv - vec2f(select(0.0, PAD_LIGHT_FLAG, pad_light), 0.0)) * PAD_RADIUS;
    let pad_r = length(pad);
    let pad_aa = max(fwidth(pad_r), 1e-4);
    let diamond_q = vec2f(pad.x + pad.y, pad.x - pad.y) * 2.6;
    let diamond_cell = fract(diamond_q) - 0.5;
    let diamond_flip = step(0.0, sin(floor(diamond_q.x) * 1.7 + floor(diamond_q.y) * 2.3));
    let lug_long = abs(diamond_cell.x) * 0.3 + abs(diamond_cell.y);
    let lug_cross = abs(diamond_cell.x) + abs(diamond_cell.y) * 0.3;
    let diamond_d = mix(lug_long, lug_cross, diamond_flip);
    let diamond_aa = max(fwidth(diamond_d), 1e-4);
    let lug = 1.0 - smoothstep(0.17 - diamond_aa, 0.17 + diamond_aa, diamond_d);

    // Relief of the ground modes: the sawn joints with their worn arrises, the
    // aggregate standing proud of an asphalt lane's binder and the lugs of a pad's
    // tread plate. It tilts the normal, fading out where a pixel spans its detail.
    let footprint = length(fwidth(f.world));
    let concrete_relief = -(1.0 - smoothstep(0.0, APRON_ARRIS + footprint, edge)) * 0.05
        - (1.0 - smoothstep(0.0, 0.05 + footprint, expansion)) * 0.03;
    let asphalt_relief = (smoothstep(0.8, 1.9, fine) * 0.006 + smoothstep(0.7, 1.8, chips) * 0.004)
        * (1.0 - smoothstep(0.02, 0.08, footprint));
    let pad_relief = lug * 0.012 * (1.0 - smoothstep(0.02, 0.06, footprint))
        - (1.0 - smoothstep(0.0, 0.05, abs(pad_r - 0.6))) * 0.01;
    var relief = 0.0;
    if mode < 0.5 {
        relief = concrete_relief;
    } else if mode < 2.5 {
        relief = asphalt_relief;
    } else if mode > 7.5 {
        relief = pad_relief;
    }
    let relief_dh = vec2f(dpdx(relief), dpdy(relief));
    let dpx = dpdx(f.world);
    let dpy = dpdy(f.world);

    // The tile's grain with its broad blotches divided out.
    let grain = grain_a / max(blur_a, vec3f(0.02)) * 0.6 + grain_b / max(blur_b, vec3f(0.02)) * 0.4;
    let grain_luma = dot(grain, APRON_LUMA);
    // Broad mottling, and the macro variation that keeps the yard from reading as
    // one tile from the air (the paint, the pads and the hull take no grime).
    let mottle = apron_noise(p / 9.0) * 0.6 + apron_noise(p / 2.7 + 7.0) * 0.4;
    var grime = 0.0;
    if paved || wall {
        let broad = apron_noise(p / 43.0 + 3.0) * 0.65 + apron_noise(p / 15.0 + 8.0) * 0.35;
        // Two noises summed bunch round one half, so they are stretched back out to
        // read across the yard from the far end of a quay.
        grime = mix(0.84, 1.07, smoothstep(0.25, 0.75, mottle)) * mix(0.76, 1.1, smoothstep(0.28, 0.72, broad));
    }

    // Damp hollows that hold water after a shower; the deepest are puddles. The
    // second octave adds at most 0.3, so below 0.6 the first leaves the hollow
    // short of the damp threshold (0.77) whatever the second would add.
    var damp = 0.0;
    var puddle = 0.0;
    if ground {
        let hollow_broad = apron_noise(p / 7.0 + 3.3);
        if hollow_broad >= 0.6 {
            let hollow = hollow_broad * 0.7 + apron_noise(p / 2.1) * 0.3;
            damp = smoothstep(0.77, 0.81, hollow);
            puddle = smoothstep(0.825, 0.84, hollow);
        }
    }

    var color = (*s).color;
    var roughness = (*s).roughness;
    if mode > 7.5 {
        // Spawn pad: a gunmetal tread plate polished where tanks turn on it, a
        // hazard rim in the team's paint, team chevrons pointing in, a bolt circle
        // and the hatch at its heart; the ring above it is a light strip that a
        // pulse chases round.
        // Team paint knocked back toward grey, so from the air the pads mark the
        // spawns without outshining the tanks on them.
        let team = mix(vec3f(dot(f.vertex_color.rgb, APRON_LUMA)), f.vertex_color.rgb, PAD_SATURATION);
        let angle = atan2(pad.y, pad.x);
        let side = abs((*s).normal.y) < 0.5;
        let worn = smoothstep(0.35, 0.8, apron_noise(pad * 1.7 + 3.0) * 0.6 + (fine - 1.0) * 0.3 + 0.35)
            * (1.0 - smoothstep(0.4, 1.8, pad_r));
        var deck = material.color.rgb * mix(0.9, 1.08, grain_luma * 0.5 + 0.25) * mix(1.0, 1.35, lug);
        deck = mix(deck, vec3f(0.36, 0.38, 0.39), worn * (0.4 + lug * 0.4));
        // Hazard rim: diagonal stripes of team paint and black round the bevel.
        let around = angle * pad_r;
        let stripe_aa = max(fwidth(around), 1e-3) * 0.9;
        let stripe = smoothstep(0.5 - stripe_aa, 0.5 + stripe_aa, fract((around + pad_r) * 0.9));
        let rim = smoothstep(PAD_RING_OUTER + 0.03 - pad_aa, PAD_RING_OUTER + 0.03 + pad_aa, pad_r);
        let rim_paint = mix(vec3f(0.035, 0.037, 0.04), team * 0.85, stripe);
        // Chevrons: four Vs pointing at the centre, between 1.15 and 1.95 m out.
        let quarter = round(angle / 1.5707963) * 1.5707963;
        let radial = vec2f(cos(quarter), sin(quarter));
        let u = dot(pad, radial);
        let v = dot(pad, vec2f(-radial.y, radial.x));
        let chevron_d = abs(abs(v) * 0.9 - (u - 1.2)) - 0.12;
        let chevron = (1.0 - smoothstep(-pad_aa, pad_aa, chevron_d))
            * step(1.12, u) * step(u, 1.9) * step(abs(v), 0.66);
        let chip = smoothstep(0.3, 0.36, apron_noise(pad * 3.0 + 1.0) * 0.6 + (grain_luma - 1.0) * 0.15 + 0.12);
        let paint = chevron * chip;
        // Bolt circle and the central hatch's seam.
        let bolt_angle = round(angle / 0.3926991) * 0.3926991;
        let bolt_d = length(pad - vec2f(cos(bolt_angle), sin(bolt_angle)) * PAD_BOLTS);
        let bolt = 1.0 - smoothstep(0.045 - pad_aa, 0.045 + pad_aa, bolt_d);
        let seam = 1.0 - smoothstep(0.0, 0.025 + pad_aa, abs(pad_r - 0.6));
        let hatch = 1.0 - smoothstep(0.6 - pad_aa, 0.6 + pad_aa, pad_r);
        color = mix(deck, deck * vec3f(0.85, 0.9, 0.95) * 1.1, hatch);
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
        (*s).roughness = mix(mix(0.55, 0.32, worn), 0.6, max(paint, rim));
        (*s).metalness = mix(0.55, 0.0, max(paint, rim));
        (*s).normal = apron_relief((*s).normal, dpx, dpy, relief_dh);
        if pad_light {
            // The ring is a light strip; no early return, so the derivatives
            // above stay in uniform control flow.
            let chase = pow(0.5 + 0.5 * cos(angle * 3.0 - frame.camera_position.w * 2.2), 6.0);
            (*s).color = team * 0.35;
            (*s).emissive = team * (0.4 + 1.2 * chase);
            (*s).roughness = 0.3;
            (*s).metalness = 0.0;
        }
        return;
    }
    if mode > 5.5 {
        // Ship hull plating: welded seams, rust running down from the scuppers and
        // the deck edge, a slimy band at the waterline, fender scrapes and the
        // blotchy fade of a hull painted in sections.
        let height = f.world.y - APRON_SEA_LEVEL;
        let seam = clamp((0.012 - min(plate.x, plate.y)) / plate_aa + 0.5, 0.0, 1.0);
        let run_cell = floor(along / 2.7);
        let run_hash = apron_hash(vec2f(run_cell, 13.0));
        let run_x = (run_cell + 0.2 + 0.6 * run_hash.x) * 2.7;
        let run_width = 0.05 + 0.14 * run_hash.y;
        let run_wobble = (apron_noise(vec2f(along * 3.0, f.world.y * 0.7)) - 0.5) * 0.08;
        let run_top = 5.0 - run_hash.w * 0.6;
        let run_length = 1.0 + 3.5 * run_hash.z;
        let run = exp(-pow((along - run_x + run_wobble) / run_width, 2.0))
            * smoothstep(run_top - run_length, run_top - 0.3, height) * step(0.3, run_hash.z);
        let bleed = smoothstep(0.55, 0.85, apron_noise(vec2f(along * 0.6, height * 2.2)) * apron_noise(vec2f(along / 5.0, 3.0)) * 1.8);
        let slime = 1.0 - smoothstep(0.1, 0.55 + mottle * 0.4, height);
        let scrape = smoothstep(0.78, 0.9, apron_noise(vec2f(along * 0.35, height * 6.0)))
            * (1.0 - smoothstep(1.0, 3.2, height));
        let section = apron_hash(vec2f(floor(along / 9.0), floor(height / 2.5)) + 3.0).x;
        color = color * mix(0.88, 1.06, section) * mix(0.92, 1.05, mottle) * mix(0.9, 1.04, grain_luma * 0.5 + 0.25);
        color = color * (1.0 - seam * 0.32);
        color = mix(color, color * vec3f(1.15, 1.12, 1.08) + vec3f(0.02), scrape * 0.5);
        color = mix(color, vec3f(0.17, 0.065, 0.025), clamp(run * 0.85 + bleed * 0.35, 0.0, 1.0));
        color = mix(color, vec3f(0.03, 0.045, 0.03), slime * 0.8);
        roughness = mix(roughness, 0.85, run);
        roughness = mix(roughness, 0.4, slime);
        (*s).color = color;
        (*s).roughness = roughness;
        (*s).emissive = (*s).emissive + frame.sun_color.rgb * apron_caustics(along, f.world.y, footprint) * 0.022;
        return;
    }
    // Ground modes: contact occlusion and grime against fixed cover and the
    // boundary wall, and rust where containers stand on their castings. Paint on
    // the wall tops, outside the apron, takes none.
    let outside = max(abs(p.x), abs(p.y)) - APRON_ARENA;
    var fixture = select(-outside, 1e3, outside > 0.0);
    var castings = 0.0;
    if !wall {
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
    }
    let contact = (1.0 - smoothstep(0.0, 0.8, fixture)) * 0.42 + (1.0 - smoothstep(0.0, 3.2, fixture)) * 0.16;
    let banked = (1.0 - smoothstep(0.0, 1.6 + mottle * 1.4, fixture)) * smoothstep(0.35, 0.6, mottle);
    let offsets = apron_lane_offsets(p);
    let lane_edge = min(abs(offsets.x) - offsets.z, abs(offsets.y) - offsets.w);
    // A cast-iron utility cover in some cells, clear of the lane edges and cover.
    let cover_radius = 0.34 + 0.12 * cover_hash.z;
    let has_cover = step(0.72, cover_hash.w) * step(0.9, abs(lane_edge)) * step(1.5, fixture);
    let cover = clamp((cover_radius - cover_d) / cover_aa + 0.5, 0.0, 1.0) * has_cover;
    let cover_gap = clamp(0.5 - abs(cover_d - cover_radius - 0.02) / cover_aa * 0.7, 0.0, 1.0) * has_cover;
    var iron = vec3f(0.0);
    if has_cover > 0.0 {
        let checker = step(0.0, sin(p.x * 34.0) * sin(p.y * 34.0));
        let cover_rim = clamp((cover_d - cover_radius + 0.05) / cover_aa + 0.5, 0.0, 1.0);
        iron = vec3f(0.05, 0.048, 0.046) * mix(0.8, 1.25, checker) * mix(1.0, 1.4, cover_rim);
    }
    // Rust bleeding out round a cover's frame.
    let cover_rust = (1.0 - smoothstep(cover_radius, cover_radius + 0.25 + mottle * 0.3, cover_d))
        * (1.0 - cover) * has_cover * smoothstep(0.3, 0.6, mottle);

    if wall {
        // Quay wall: weathered concrete panels, a dark wet tide band crusted with
        // barnacles and weed at the waterline, and rust and lime streaks running
        // down from the top.
        let height = f.world.y - APRON_SEA_LEVEL;
        let ragged = (apron_noise(vec2f(along * 0.7, 0.0)) - 0.5) * 0.35 + (grain_luma - 1.0) * 0.08;
        let wet = 1.0 - smoothstep(APRON_TIDE * 0.6, APRON_TIDE, height + ragged);
        let weed = 1.0 - smoothstep(0.05, 0.38, height + ragged * 0.6);
        let barnacles = smoothstep(0.62, 0.72, apron_noise(vec2f(along * 6.0, f.world.y * 6.0)))
            * (1.0 - smoothstep(0.25, 0.6, abs(height - 0.32 - ragged * 0.4)));
        let runs = apron_noise(vec2f(along * 1.3, f.world.y * 0.08));
        let rust = smoothstep(0.62, 0.8, runs) * smoothstep(-2.5, -0.4, f.world.y) * 0.6;
        let lime = smoothstep(0.7, 0.86, apron_noise(vec2f(along * 0.9 + 19.0, f.world.y * 0.1))) * 0.5;
        let joint = clamp((0.03 - wall_edge) / wall_aa + 0.5, 0.0, 1.0);
        color = material.color.rgb * f.tint.rgb * grain * grime;
        color = mix(color, color * vec3f(1.1, 1.08, 1.02), lime * (1.0 - wet));
        color = mix(color, color * vec3f(0.62, 0.42, 0.28), rust * (1.0 - wet));
        color = mix(color, color * vec3f(0.36, 0.38, 0.36), wet);
        color = mix(color, vec3f(0.035, 0.06, 0.035) * grain_luma, weed * 0.85);
        color = mix(color, vec3f(0.42, 0.42, 0.38) * grain_luma, barnacles * 0.7);
        color = color * (1.0 - joint * 0.55);
        roughness = mix(roughness, 0.35, wet * (1.0 - barnacles));
        (*s).emissive = (*s).emissive + frame.sun_color.rgb * apron_caustics(along, f.world.y, footprint) * 0.03;
    } else if mode < 0.5 {
        // Concrete slabs, in layers: each pour's own tone and hue with a trowel
        // gradient, newer replacement slabs, worn arrises and spalled corners at
        // the joints, sealed expansion joints, hairline cracks and ghosts of
        // removed bay paint; then what the yard does to it: grime washed into the
        // joints and banked against cover, old stains weathered pale inside a dark
        // tide line, rubber arcs where straddle carriers turn, tyre dirt cut across
        // the lane corners, rusty casting prints, fresh oil with a wet core and
        // salt bloom along the foot of the wall.
        let slab = apron_hash(cell + vec2f(17.0, 5.0));
        let patched = step(slab.y, 0.07);
        let tone = mix(0.8, 1.1, slab.x) * mix(1.0, 1.12, patched);
        let hue = mix(vec3f(1.03, 1.0, 0.95), vec3f(0.96, 0.995, 1.035), slab.w);
        let pour = 1.0 + dot(fract(slab_uv) - 0.5, slab.zw - 0.5) * 0.2;
        let groove = clamp((APRON_JOINT - edge) / aa + 0.5, 0.0, 1.0) * min(1.0, 2.0 * APRON_JOINT / aa);
        let sealant = clamp((APRON_SEALANT - expansion) / expansion_aa + 0.5, 0.0, 1.0)
            * min(1.0, 2.0 * APRON_SEALANT / expansion_aa);
        let fringe = (1.0 - smoothstep(0.0, 0.3 + mottle * 0.3, edge)) * (0.4 + mottle);
        // Spalled corners: a chunk broken out where four slabs meet.
        let corner_cell = round(slab_uv);
        let corner_hash = apron_hash(corner_cell + vec2f(61.0, 3.0));
        // The noise moves a break's rim at most 0.06 m, so a corner without a break,
        // or farther than that past its rim, has none.
        let corner_reach = length((slab_uv - corner_cell) * APRON_SLAB);
        let spall_radius = 0.08 + 0.3 * corner_hash.x;
        var spall = 0.0;
        if corner_hash.y >= 0.55 && corner_reach - 0.07 < spall_radius + aa {
            let corner_d = corner_reach + (apron_noise(p * 5.0) - 0.5) * 0.12;
            spall = (1.0 - smoothstep(spall_radius - aa, spall_radius + aa, corner_d)) * step(0.55, corner_hash.y);
        }
        // Hairline shrinkage cracks across some slabs, darker where dirt fills them.
        var hairline = (1.0 - smoothstep(0.0, crack_width * 1.2 + 0.003, abs(crack_noise - 0.5)))
            * (1.0 - smoothstep(0.03, 0.09, footprint))
            * step(0.72, slab.z) * (1.0 - patched);
        if hairline > 0.0 {
            hairline = hairline * smoothstep(0.55, 0.7, apron_noise(p / 1.7 + slab.xy * 9.0));
        }
        var chipped = 1.0 - smoothstep(APRON_JOINT, APRON_JOINT * 3.5, edge);
        if chipped > 0.0 {
            chipped = chipped * step(0.6, grain_luma * apron_noise(p * 6.0) * 1.4);
        }
        let streak = select(streak_x, streak_z, abs(offsets.x) - offsets.z < abs(offsets.y) - offsets.w) / APRON_TILE_MEAN;
        let corner_dirt = (1.0 - smoothstep(0.0, 2.4, lane_edge)) * (0.45 + 0.55 * smoothstep(0.7, 1.3, streak));

        // Ghosts of bay lines ground off when the yard was re-marked.
        let ghost_cell = floor(p / APRON_GHOST_CELL);
        let ghost_hash = apron_hash(ghost_cell + vec2f(23.0, 51.0));
        let ghost_local = p - (ghost_cell + 0.5) * APRON_GHOST_CELL;
        let ghost_along_x = ghost_hash.x > 0.5;
        let ghost_run = select(ghost_local.y, ghost_local.x, ghost_along_x);
        let ghost_across = select(ghost_local.x, ghost_local.y, ghost_along_x) - (ghost_hash.y - 0.5) * 6.0;
        // Each layer below takes its noise only where the rest of its weight is
        // non-zero, multiplied in the same order, so a skipped pixel keeps the zero
        // it had.
        let ghost_line = (1.0 - smoothstep(0.07, 0.09, abs(ghost_across))) * step(abs(ghost_run), 1.5 + 3.0 * ghost_hash.z)
            * step(0.62, ghost_hash.w);
        var ghost = 0.0;
        if ghost_line != 0.0 {
            ghost = ghost_line * smoothstep(0.3, 0.6, apron_noise(p * 1.3 + 4.0) + grain_luma * 0.2);
        }

        // Old stains, weathered pale at their heart and dark at their tide line.
        let old_cell = floor(p / APRON_OLD_STAIN_CELL);
        let old_hash = apron_hash(old_cell + vec2f(71.0, 13.0));
        let old_at = (old_cell + 0.25 + 0.5 * old_hash.xy) * APRON_OLD_STAIN_CELL;
        let old_dir = vec2f(cos(old_hash.w * 6.283), sin(old_hash.w * 6.283));
        let has_old = step(fract(old_hash.x * 7.3 + old_hash.y * 3.1), 0.55);
        var old_stain = 0.0;
        var old_line = 0.0;
        if has_old != 0.0 {
            let old_shape = apron_blotch(p, old_at, old_dir, 1.6, 1.4 + 2.6 * old_hash.z,
                (apron_noise(p * 0.7 + old_hash.xy * 30.0) - 0.5) * 0.9 + (grain_luma - 1.0) * 0.15);
            old_stain = (1.0 - smoothstep(0.55, 1.0, old_shape)) * has_old;
            old_line = smoothstep(0.75, 0.92, old_shape) * (1.0 - smoothstep(0.92, 1.05, old_shape)) * has_old;
        }

        // Rubber arcs left by turning straddle carriers: a pair of wheel tracks on
        // a circle, fading in and out along it.
        let scuff_cell = floor(p / APRON_SCUFF_CELL);
        let scuff_hash = apron_hash(scuff_cell + vec2f(91.0, 37.0));
        let scuff_at = (scuff_cell + 0.5 + (scuff_hash.xy - 0.5) * 0.15) * APRON_SCUFF_CELL;
        let scuff_radius = 3.0 + 2.6 * scuff_hash.z;
        let scuff_rel = p - scuff_at;
        let scuff_d = length(scuff_rel);
        let scuff_dir = vec2f(cos(scuff_hash.w * 6.283), sin(scuff_hash.w * 6.283));
        let scuff_span = smoothstep(0.45, 0.9, dot(scuff_rel / max(scuff_d, 1e-3), scuff_dir));
        let scuff_track = exp(-pow((scuff_d - scuff_radius) / 0.13, 2.0)) + exp(-pow((scuff_d - scuff_radius - 1.7) / 0.13, 2.0));
        let scuff_arc = scuff_track * scuff_span * step(0.62, fract(scuff_hash.z * 5.7));
        var scuff = 0.0;
        if scuff_arc != 0.0 {
            scuff = scuff_arc
                * smoothstep(0.3, 0.7, apron_noise(vec2f(scuff_d * 4.0, atan2(scuff_rel.y, scuff_rel.x) * 9.0)) + grain_luma * 0.3);
        }

        // Fresh oil: a spill in some cells (more of them beside the containers,
        // where handlers park), dark and still wet at its core with a thin-film
        // sheen, and a drip trailing off it.
        let stain_cell = floor(p / APRON_STAIN_CELL);
        let spill = apron_hash(stain_cell + vec2f(5.0, 29.0));
        let near_cover = 1.0 - smoothstep(1.0, 7.0, fixture);
        let has_spill = step(fract(spill.x * 13.7 + spill.y * 7.1), 0.04 + 0.34 * near_cover);
        var oil = 0.0;
        var oil_core = 0.0;
        var film = 0.0;
        var film_tint = vec3f(0.0);
        if has_spill > 0.0 {
            let spill_at = (stain_cell + 0.2 + 0.6 * spill.xy) * APRON_STAIN_CELL;
            let spill_dir = vec2f(cos(spill.w * 6.283), sin(spill.w * 6.283));
            let spill_radius = 0.25 + 0.9 * spill.z * spill.z;
            let shape = apron_blotch(p, spill_at, spill_dir, 1.8, spill_radius,
                (apron_noise(p * 1.9 + spill.xy * 40.0) - 0.5) * 0.7 + (grain_luma - 1.0) * 0.2);
            let drip_at = spill_at + spill_dir * spill_radius * (1.6 + spill.y);
            let drip = apron_blotch(p, drip_at, spill_dir, 1.0, 0.12 + 0.12 * spill.x, (apron_noise(p * 6.0) - 0.5) * 0.5);
            oil = max(1.0 - smoothstep(0.55, 1.0, shape), (1.0 - smoothstep(0.6, 1.0, drip)) * 0.8) * has_spill;
            oil_core = (1.0 - smoothstep(0.05, 0.5, shape)) * has_spill;
            film = smoothstep(0.2, 0.42, shape) * (1.0 - smoothstep(0.42, 0.62, shape)) * has_spill;
            film_tint = 0.6 + 0.4 * cos(6.283 * (shape * 2.6 + vec3f(0.0, 0.33, 0.67)));
        }
        // Straddle carriers working the bays: pairs of broad wheel paths running
        // with the nearest lane, polished dark and grimy, in and out along their
        // runs, so the yard reads as worked even from the far end of the quay.
        let track_along_z = abs(offsets.x) - offsets.z < abs(offsets.y) - offsets.w;
        let track_across = select(p.y, p.x, track_along_z);
        let track_run = select(p.x, p.y, track_along_z);
        let track_cell = floor(track_across / APRON_TRACK_CELL);
        let track_hash = apron_hash(vec2f(track_cell, select(3.0, 7.0, track_along_z)));
        let track_centre = (track_cell + 0.3 + 0.4 * track_hash.x) * APRON_TRACK_CELL;
        let track_off = abs(abs(track_across - track_centre) - (1.5 + 0.5 * track_hash.y));
        let track_path = exp(-pow(track_off / (0.3 + 0.25 * track_hash.z), 2.0));
        let track_clear = smoothstep(1.0, 2.5, fixture);
        var tracks = 0.0;
        if track_path != 0.0 && track_hash.w >= 0.25 && track_clear != 0.0 {
            let track_run_noise = apron_noise(vec2f(track_run / 13.0, track_cell * 3.1 + 0.5));
            tracks = track_path * smoothstep(0.3, 0.6, track_run_noise) * step(0.25, track_hash.w)
                * (0.55 + 0.45 * smoothstep(0.7, 1.4, streak)) * track_clear;
        }
        // Salt bloom from the spray along the foot of the boundary wall.
        let salt_reach = 1.0 - smoothstep(0.0, 2.2 + mottle * 2.0, -outside - 0.8);
        var salt = 0.0;
        if salt_reach != 0.0 {
            salt = salt_reach * smoothstep(0.45, 0.7, apron_noise(p * 0.9 + 17.0) + (grain_luma - 1.0) * 0.3);
        }

        color = material.color.rgb * f.tint.rgb * grain * tone * hue * pour * grime;
        color = mix(color, color * vec3f(1.08, 1.07, 1.03) + vec3f(0.012), ghost * 0.55);
        color = color * (1.0 - fringe * 0.22) * mix(1.0, 1.1, chipped) * (1.0 - hairline * 0.4);
        color = mix(color, color * vec3f(0.92, 0.9, 0.86) * mix(0.8, 1.3, chips * 0.5), spall);
        color = mix(color, color * vec3f(1.06, 1.04, 1.0), old_stain * 0.35);
        color = mix(color, color * vec3f(0.66, 0.62, 0.57), old_line * 0.6 + old_stain * 0.2);
        color = mix(color, color * vec3f(0.62, 0.58, 0.53), corner_dirt * 0.45);
        color = mix(color, color * vec3f(0.7, 0.68, 0.64), tracks * 0.5);
        color = mix(color, color * vec3f(0.66, 0.6, 0.52), banked * 0.5);
        color = mix(color, color * vec3f(0.85, 0.55, 0.34), castings * 0.75);
        color = mix(color, color * vec3f(0.38, 0.37, 0.36), clamp(scuff, 0.0, 1.0) * 0.4);
        color = mix(color, color * vec3f(0.24, 0.23, 0.22), oil * mix(0.55, 0.8, spill.z));
        color = mix(color, color * film_tint * 1.4, film * 0.35);
        color = color * (1.0 - oil_core * 0.35);
        color = mix(color, color * vec3f(1.12, 1.12, 1.1) + vec3f(0.03), salt * 0.45);
        color = color * (1.0 - groove * 0.55) * (1.0 - sealant * 0.65);
        color = color * (1.0 - contact);
        color = color * mix(1.0, 0.8, damp) * mix(1.0, 0.72, puddle);
        roughness = mix(roughness, 0.98, spall);
        roughness = mix(roughness, 0.72, clamp(scuff, 0.0, 1.0));
        roughness = mix(roughness, 0.62, tracks * 0.6);
        roughness = mix(roughness, 0.5, oil);
        roughness = mix(roughness, 0.14, oil_core);
        roughness = mix(roughness, 0.42, damp);
        roughness = mix(roughness, 0.12, puddle);
        // Cast-iron cover: a raised checker plate in a dark gap, rust round it.
        color = mix(color, color * vec3f(0.8, 0.55, 0.38), cover_rust * 0.6);
        color = mix(color, iron, cover);
        color = color * (1.0 - cover_gap * 0.7);
        roughness = mix(roughness, 0.5, cover);
        (*s).metalness = mix((*s).metalness, 0.55, cover);
        (*s).normal = apron_relief((*s).normal, dpx, dpy, relief_dh * (1.0 - puddle));
    } else if mode < 2.5 {
        // Asphalt lanes: dark binder with pale aggregate standing proud of it,
        // oxidised paler and browner where the traffic thins and ravelled at the
        // edges, rubber-polished wheel paths with skid streaks and an oil strip
        // between them, crazing where the wheels have tired the surface, the paving
        // joint down the middle, thin tar-sealed cracks, ragged repair patches of
        // fresher, blacker mix and cast-iron covers.
        let along_z = abs(mode - 1.0) < 0.5;
        let across = select(offsets.y, offsets.x, along_z);
        let half = select(offsets.w, offsets.z, along_z);
        let streak = select(streak_x, streak_z, along_z) / APRON_TILE_MEAN;
        let traffic = apron_traffic(across, half);
        let aggregate = smoothstep(1.25, 2.0, fine) - smoothstep(0.6, 0.25, fine) * 0.6;
        let stones = smoothstep(1.3, 2.1, chips) * 0.8 - smoothstep(0.7, 0.35, chips) * 0.4;
        let worn = apron_noise(p / 11.0 + 5.0);
        let ravel = smoothstep(half - 1.1, half - 0.15, abs(across)) * (0.5 + 0.5 * mottle);
        let tyre = smoothstep(0.8, 1.35, streak) * mix(0.5, 1.0, mottle);
        let lane_along = select(p.x, p.y, along_z);
        let skid = smoothstep(0.58, 0.8, apron_noise(vec2f(across * 3.2, lane_along * 0.09))
            * 0.75 + apron_noise(vec2f(across * 9.0, lane_along * 0.6)) * 0.25) * traffic.x;
        var drips = 0.0;
        if traffic.y > 0.0 {
            drips = smoothstep(0.5, 0.7, apron_noise(vec2f(across * 2.2, lane_along * 0.31))
                * 0.7 + apron_noise(vec2f(across * 5.0, lane_along * 1.7)) * 0.3) * traffic.y;
        }
        let patch_hash = apron_hash(patch_cell + vec2f(3.0, 11.0));
        let patch_size = vec2f(2.0, 1.4) + patch_hash.zw * vec2f(3.5, 3.0);
        let patch_at = vec2f(0.6) + patch_hash.xy * (vec2f(APRON_PATCH_CELL) - patch_size - 1.2);
        let has_patch = step(0.93, patch_hash.x * 0.6 + patch_hash.w * 0.6);
        // A cell without a patch needs neither its edge nor its seal.
        var patched = 0.0;
        var seal = 0.0;
        if has_patch != 0.0 {
            // Saw-cut square, so only a little raggedness where the tar seal ran.
            let ragged = (apron_noise(p * 4.1 + 7.0) - 0.5) * 0.07;
            let inside = min(
                min(patch_local.x - patch_at.x, patch_at.x + patch_size.x - patch_local.x),
                min(patch_local.y - patch_at.y, patch_at.y + patch_size.y - patch_local.y),
            ) + ragged;
            patched = clamp(inside / patch_aa + 0.5, 0.0, 1.0) * has_patch;
            seal = (1.0 - smoothstep(0.015, 0.06 + footprint, abs(inside))) * has_patch
                * mix(0.3, 1.0, apron_noise(p * 1.1 + 3.0)) * (1.0 - smoothstep(0.05, 0.25, footprint));
        }
        let hairline_fade = 1.0 - smoothstep(0.03, 0.09, footprint);
        let crack_line = (1.0 - smoothstep(0.0, crack_width * 1.1 + 0.004, abs(crack_noise - 0.5))) * (1.0 - patched);
        var crack = 0.0;
        if crack_line > 0.0 {
            crack = crack_line * smoothstep(0.7, 0.8, apron_noise(p / 6.0 + 2.0)) * mix(0.35, 1.0, hairline_fade);
        }
        let craze_line = 1.0 - smoothstep(0.0, craze_width + 0.006, craze);
        var crazing = 0.0;
        if craze_line != 0.0 && traffic.x != 0.0 && patched != 1.0 && hairline_fade != 0.0 {
            crazing = craze_line * smoothstep(0.66, 0.78, apron_noise(p / 5.0 + 41.0)) * traffic.x * (1.0 - patched) * hairline_fade;
        }
        // The joint's wobble moves it at most 0.03 m, so beyond 0.085 m of the lane's
        // centre it is gone.
        var centre_joint = 0.0;
        if abs(across) < 0.085 {
            centre_joint = (1.0 - smoothstep(0.015, 0.05, abs(across + (apron_noise(p * 0.8) - 0.5) * 0.06)))
                * smoothstep(0.35, 0.6, apron_noise(p / 4.0 + 17.0));
        }
        // Relaid a stretch at a time: each section its own age, meeting the next
        // at a straight transverse joint.
        let lane_centre = select(p.y - offsets.y, p.x - offsets.x, along_z);
        let lane_seed = apron_hash(vec2f(lane_centre, 71.0));
        let section_at = lane_along / APRON_SECTION + lane_seed.x;
        let section = apron_hash(vec2f(floor(section_at), lane_centre * 0.37 + 5.0));
        let section_edge = (0.5 - abs(fract(section_at) - 0.5)) * APRON_SECTION;
        let section_joint = (1.0 - smoothstep(0.01, 0.05 + footprint, section_edge)) * step(0.35, section.z);
        let oxidised = clamp(worn * (1.0 - traffic.x * 0.7) + (section.y - 0.5) * 0.6, 0.0, 1.0);
        var base = material.color.rgb * f.tint.rgb * mix(0.9, 1.06, grain_luma * 0.5) * mix(0.88, 1.1, section.x)
            * mix(vec3f(0.92), vec3f(1.3, 1.26, 1.2), oxidised) * mix(1.0, 1.2, ravel);
        // A patch is fresh mix laid later: blacker, unoxidised binder that still
        // takes the lane's grime and wheel wear, edged by its tar seal.
        // Only a few shades blacker than the lane round it, with its own fine
        // aggregate, so it reads as a repair rather than a hole in the texture.
        let fresh_weight = patched * mix(0.6, 1.0, patch_hash.z);
        if fresh_weight != 0.0 {
            let fresh = base * vec3f(0.9, 0.9, 0.91) * mix(0.95, 1.04, apron_noise(p * 0.7 + 5.0));
            base = mix(base, fresh, fresh_weight);
        }
        let stone_weight = mix(mix(0.32, 0.65, ravel), 0.12, patched);
        color = base * (1.0 + aggregate * stone_weight + stones * stone_weight * 0.7) * grime;
        color = color * mix(1.0, 0.8, tyre * (0.35 + traffic.x * 0.65));
        color = color * mix(1.0, 0.84, traffic.x);
        color = mix(color, color * vec3f(0.42, 0.41, 0.4), skid * 0.55);
        color = mix(color, color * vec3f(0.5, 0.49, 0.48), drips * 0.6);
        color = color * (1.0 - seal * 0.28) * (1.0 - crack * 0.45) * (1.0 - crazing * 0.35) * (1.0 - centre_joint * 0.45)
            * (1.0 - section_joint * 0.4);
        color = mix(color, color * vec3f(0.85, 0.55, 0.34), castings * 0.5);
        color = color * (1.0 - contact * 0.8);
        color = color * mix(1.0, 0.88, damp) * mix(1.0, 0.8, puddle);
        roughness = mix(0.9, 0.82, oxidised) - stones * 0.05;
        roughness = mix(roughness, 0.6, traffic.x * 0.8);
        // Fresh mix is matte: its binder has not been polished by the wheels yet.
        roughness = mix(roughness, 0.9, patched);
        roughness = mix(roughness, 0.5, skid);
        roughness = mix(roughness, 0.5, drips);
        roughness = mix(roughness, 0.28, crack);
        roughness = mix(roughness, 0.62, seal);
        roughness = mix(roughness, 0.5, damp);
        roughness = mix(roughness, 0.22, puddle);
        color = mix(color, color * vec3f(0.8, 0.55, 0.38), cover_rust * 0.5);
        color = mix(color, iron, cover);
        color = color * (1.0 - cover_gap * 0.7);
        roughness = mix(roughness, 0.5, cover);
        (*s).metalness = mix((*s).metalness, 0.55, cover);
        (*s).normal = apron_relief((*s).normal, dpx, dpy, relief_dh * (1.0 - puddle));
    } else {
        // Paint: flaked in old zones and where wheels pass, dirtied and rubbered by
        // the traffic, its color faded by the sun and pitted by the aggregate it
        // was rolled onto.
        let traffic_z = apron_traffic(offsets.x, offsets.z);
        let traffic_x = apron_traffic(offsets.y, offsets.w);
        let traffic = max(max(traffic_z.x, traffic_x.x), max(traffic_z.y, traffic_x.y) * 0.6);
        let age = apron_noise(p / 13.0 + 31.0);
        let flakes = apron_noise(p * 1.1) * 0.55 + apron_noise(p * 0.45 + 3.0) * 0.35 + (grain_luma - 1.0) * 0.08;
        let on_apron = 1.0 - step(0.0, outside);
        // The wall tops see no wheels, but boots, mooring lines and spray wear
        // their paint nearly as hard.
        let threshold = (0.1 + smoothstep(0.55, 0.9, age) * 0.22 + traffic * 0.2) * mix(0.8, 1.0, on_apron);
        let flake_aa = max(fwidth(flakes), 0.01);
        let worn = smoothstep(threshold - flake_aa, threshold + flake_aa, flakes);
        if mode < 3.5 {
            color = material.color.rgb * f.vertex_color.rgb * f.tint.rgb;
        }
        let chroma = dot(color, APRON_LUMA);
        color = mix(color, vec3f(chroma) * vec3f(1.02, 1.0, 0.95), age * 0.25);
        let dirt = clamp(apron_noise(p / 4.0 + 2.0) * 0.6 + age * 0.4 + traffic * 0.4 - 0.25, 0.0, 1.0);
        color = mix(color, color * vec3f(0.6, 0.58, 0.53), dirt * 0.6);
        if traffic != 0.0 {
            let rubber = traffic * smoothstep(0.4, 0.7, apron_noise(p * vec2f(0.4, 3.0) + 9.0));
            color = mix(color, color * vec3f(0.45, 0.44, 0.43), rubber * 0.5);
        }
        color = color * mix(0.8, 1.04, clamp(fine - 0.3, 0.0, 1.0)) * mix(0.93, 1.02, mottle);
        // Rain lies on the apron's paint, not on the wall tops above it.
        color = color * (1.0 - contact * 0.4);
        color = color * mix(1.0, 0.85, damp * on_apron);
        roughness = mix(roughness, 0.35, puddle * on_apron);
        if mode < 3.5 {
            (*s).opacity = worn;
        } else {
            (*s).opacity = (*s).opacity * mix(0.2, 1.0, worn) * mix(1.0, 0.75, age);
        }
    }
    (*s).color = color;
    (*s).roughness = roughness;
}

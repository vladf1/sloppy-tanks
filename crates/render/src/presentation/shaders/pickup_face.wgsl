// A pickup's kind faces are mapped to the atlas's first tile; instance_data.xy
// moves them into the kind's tile, so every kind shares one material and a kind's
// pallets and items each draw as one instanced call. Faces mapped to the shared
// cells (brass, steel, rubber, the pallet's planks) lie outside the first tile and
// keep their UVs. A worn pallet (its wear past PALLET_WORN) has lost the deck
// board whose UVs lie above params[0].x.
// The pallet's pennant is the only geometry whose UVs lie inside the first tile's
// rows params[1].xy: it is lowered down its pole by params[0].w times one less
// the hoist (1 while stocked, the restock's progress while empty, so it is
// hoisted as the supply returns), and ripples away from the pole at x params[0].y
// over its length params[0].z, at one rate for every pennant, each at its own
// phase (frame.camera_position.w is the clock). The pole is the only geometry
// whose UVs lie inside params[2]; a hull driving over it bends pole and pennant
// down about its foot at (params[0].y, 0, params[1].z), in under the hull, by up
// to params[1].w.
// instance_data.z packs the hoist plus twice the bend's step (out of
// BEND_STEPS); instance_data.w packs the drop's wear (its fraction: sun-greyed or
// darker boards, fresher or dirtier paint) and the bend's bearing (its whole
// part, out of BEARINGS round the pallet's own frame). Items carry z 1: no bend
// and a full hoist.
const PALLET_WORN: f32 = 0.66;
const FLAG_RATE: f32 = 4.2;
const FLAG_WAVES: f32 = 7.0;
const FLAG_SWING: f32 = 0.07;
const FLAG_SAG: f32 = 0.05;
const BEND_STEPS: f32 = 15.0;
const BEARINGS: f32 = 16.0;
const TAU: f32 = 6.2831853;

// Tip a point of the pole or pennant about the pole's foot by `angle` toward the
// level unit direction `toward` (x, z).
fn bend_about_foot(p: vec3f, foot: vec3f, toward: vec2f, angle: f32) -> vec3f {
    let d = p - foot;
    let along = dot(d.xz, toward);
    let c = cos(angle);
    let s = sin(angle);
    let tipped = along * c + d.y * s;
    let xz = d.xz + toward * (tipped - along);
    return foot + vec3f(xz.x, d.y * c - along * s, xz.y);
}

fn effect_vertex(v: ptr<function, EffectVertex>) {
    let uv = (*v).uv;
    let data = (*v).instance_data;
    let params = material.params[0];
    let rows = material.params[1];
    let pole = material.params[2];
    let step = floor(data.z * 0.5);
    let hoist = data.z - 2.0 * step;
    let bend = rows.w * step / BEND_STEPS;
    let bearing = floor(data.w) / BEARINGS * TAU;
    let toward = vec2f(cos(bearing), sin(bearing));
    let wear = fract(data.w);
    let foot = vec3f(params.y, 0.0, rows.z);
    if uv.x > pole.x && uv.x < pole.z && uv.y > pole.y && uv.y < pole.w {
        (*v).position = bend_about_foot((*v).position, foot, toward, bend);
    } else if uv.x < 0.25 && uv.y > 0.75 {
        (*v).uv = uv + data.xy;
        if uv.y > rows.x && uv.y < rows.y {
            let along = clamp(((*v).position.x - params.y) / params.z, 0.0, 1.0);
            let origin = (*v).instance_origin;
            let phase = origin.x * 0.37 + origin.z * 0.23;
            let t = frame.camera_position.w;
            let wave = sin(t * FLAG_RATE + phase - along * FLAG_WAVES);
            (*v).position.z += along * FLAG_SWING * wave;
            (*v).position.y -= along * along * FLAG_SAG + params.w * (1.0 - hoist);
            (*v).position = bend_about_foot((*v).position, foot, toward, bend);
            (*v).normal = normalize((*v).normal + vec3f(0.0, 0.0, 0.4 * wave * along));
        }
    } else if uv.x > 0.75 && uv.y > 0.75 {
        if uv.y > params.x && wear > PALLET_WORN {
            (*v).position = vec3f(0.0);
        }
    }
}

fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let w = fract(f.instance_data.w);
    let wood = f.uv.x > 0.75 && f.uv.y > 0.75;
    if wood {
        // Boards from a different stack: lighter and sun-greyed, or darker.
        let grey = vec3f(dot((*s).color, vec3f(0.2126, 0.7152, 0.0722)));
        let aged = mix((*s).color, grey, 0.35 * fract(w * 7.31));
        (*s).color = aged * (0.78 + 0.34 * w);
    } else {
        (*s).color = (*s).color * (0.9 + 0.14 * fract(w * 3.7));
    }
}

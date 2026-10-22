//! Port of `quarry-machinery.ts`: the parked excavator and haul truck on the
//! quarry apron, in sun-faded industrial yellow with weathered accents. All
//! assemblies are static, merged into a handful of material batches.

use std::f64::consts::{FRAC_PI_2, PI};
use std::sync::Arc;

use glam::DVec3;

use crate::scene::{Material, Node};

use super::batching::batch;
use super::harbor_surfaces::{steel_beam, steel_box};
use super::model_primitives::{Cache, box_part, cylinder_part, material, put, rotated};
use super::tank_surfaces::armor_wear_texture;
use crate::geometry::math::{hex_to_linear, normalize, quat_from_unit_vectors, smoothstep};

// Sun-faded industrial yellow, dusty steel, rusty red and one muted teal accent.
pub(super) const PAINT: u32 = 0xc69a4b;
const STEEL: u32 = 0x5a564c;
const RUBBER: u32 = 0x343431;
const GLASS: u32 = 0x526c72;
const RUST: u32 = 0x8a4f2e;
const TEAL: u32 = 0x4e7d7c;
const DUST: u32 = 0xc9b78d;
/// The cab roof's pale paint, weathered like the fleet yellow.
const CAB_ROOF: u32 = 0xd2c6a2;
/// Caked quarry dust baked over the lower machine and its upward faces.
const GRIME: u32 = 0x9c8566;

/// `accent(w, h, d, color, metal, rough)`: a weathered accent box (rust eats light,
/// dust film kills reflections).
fn accent(w: f64, h: f64, d: f64, color: u32, metal: f64, rough: f64) -> Node {
    let mut mesh = box_part(w, h, d, color, 0.0);
    if let Some(drawable) = &mut mesh.drawable {
        drawable.material = material(color, metal, rough);
    }
    mesh
}

static FLEET_PAINT: Cache<u32, Material> = Cache::new();

/// The scratched panel atlas over the fleet colors, with its own material cache
/// so the fleet's paint can weather independently of harbor props.
fn fleet_paint(color: u32) -> Arc<Material> {
    FLEET_PAINT.get_or_insert(color, || {
        let wear = armor_wear_texture();
        Material {
            map: Some(wear.clone()),
            bump_map: Some(wear),
            bump_scale: 0.045,
            ..Material::standard(if color == PAINT { 0xd3a33f } else { color }, 0.18, 0.84)
        }
    })
}

/// `weatherPaint(group)`: every standard-shaded fleet-yellow or cab-roof part in
/// the assembly (nested groups included) takes the weathered paint.
pub(super) fn weather_paint(node: &mut Node) {
    if let Some(drawable) = &mut node.drawable {
        let color = drawable.material.color.0;
        if drawable.material.effect == crate::scene::Effect::None
            && drawable.material.shading == crate::scene::Shading::Standard
            && (color == PAINT || color == CAB_ROOF)
        {
            drawable.material = fleet_paint(color);
        }
    }
    for child in &mut node.children {
        weather_paint(child);
    }
}

/// `piston(group, a, b)`: a hydraulic ram (barrel and polished rod) from a to b.
fn piston(group: &mut Node, a: [f64; 3], b: [f64; 3]) {
    let from = DVec3::from_array(a);
    let direction = DVec3::from_array(b) - from;
    let length = direction.length();
    for (fraction, radius, color) in [(0.65, 0.21, STEEL), (1.0, 0.105, 0xb8b8af)] {
        let mut mesh = cylinder_part(radius, length * fraction, color, 10);
        mesh.position = from + direction * (fraction / 2.0);
        mesh.rotation = quat_from_unit_vectors(DVec3::Y, normalize(direction));
        group.children.push(mesh);
    }
}

/// `quarryExcavator()` (`quarry-excavator`): steel tracks, glazed cab, articulated
/// boom and bucket teeth. The bucket stays a nested (separately batched) group.
pub fn quarry_excavator() -> Node {
    let mut group = Node::group("quarry-excavator");
    for z in [-2.25, 2.25] {
        put(
            &mut group,
            box_part(7.8, 1.55, 1.4, RUBBER, 0.3),
            0.0,
            0.8,
            z,
        );
        for x in [-2.8, -1.4, 0.0, 1.4, 2.8] {
            let wheel = rotated(cylinder_part(0.62, 1.48, STEEL, 12), FRAC_PI_2, 0.0, 0.0);
            put(&mut group, wheel, x, 0.82, z);
            let hub = rotated(cylinder_part(0.2, 1.52, 0x7b786c, 10), FRAC_PI_2, 0.0, 0.0);
            put(&mut group, hub, x, 0.82, z);
        }
        let mut x = -3.4;
        while x < 3.6 {
            for y in [0.13, 1.52] {
                put(&mut group, steel_box(0.16, 0.1, 1.5, STEEL), x, y, z);
            }
            x += 0.48;
        }
    }
    put(
        &mut group,
        cylinder_part(1.7, 0.45, STEEL, 16),
        0.0,
        1.72,
        0.0,
    );
    put(&mut group, steel_box(6.9, 1.65, 4.3, PAINT), -0.7, 2.6, 0.0);
    put(
        &mut group,
        steel_box(2.1, 1.15, 4.1, PAINT),
        -3.0,
        3.75,
        0.0,
    );
    // A dark radiator with a few broad fins reads at the gameplay camera distance.
    put(
        &mut group,
        box_part(0.06, 0.85, 2.8, RUBBER, 0.0),
        -4.08,
        3.75,
        0.0,
    );
    let mut z = -1.2;
    while z <= 1.2 {
        put(
            &mut group,
            box_part(0.09, 0.83, 0.05, STEEL, 0.0),
            -4.12,
            3.75,
            z,
        );
        z += 0.3;
    }
    // Rusty exhaust stack, counterweight wear stripe and dust-caked track frames.
    let mut stack = cylinder_part(0.13, 1.3, RUST, 8);
    if let Some(drawable) = &mut stack.drawable {
        drawable.material = material(RUST, 0.3, 0.9);
    }
    put(&mut group, stack, -2.9, 4.65, 1.45);
    put(
        &mut group,
        accent(0.08, 0.5, 3.6, RUST, 0.3, 0.9),
        -4.16,
        2.9,
        0.0,
    );
    for z in [-2.25, 2.25] {
        put(
            &mut group,
            accent(7.6, 0.14, 1.5, DUST, 0.0, 1.0),
            0.0,
            1.62,
            z,
        );
    }
    // A muted teal toolbox breaks the yellow without shouting.
    put(
        &mut group,
        accent(1.3, 0.8, 0.6, TEAL, 0.3, 0.7),
        1.8,
        2.05,
        2.3,
    );
    put(
        &mut group,
        steel_box(2.55, 2.65, 2.25, PAINT),
        0.65,
        4.55,
        -1.05,
    );
    put(
        &mut group,
        box_part(2.18, 1.85, 0.04, GLASS, 0.0),
        0.65,
        4.78,
        -2.19,
    );
    put(
        &mut group,
        box_part(0.04, 1.9, 1.88, GLASS, 0.0),
        1.94,
        4.78,
        -1.05,
    );
    for x in [-0.47, 0.7, 1.78] {
        put(&mut group, steel_box(0.1, 2.1, 0.1, STEEL), x, 4.72, -2.24);
    }
    put(
        &mut group,
        steel_box(2.85, 0.2, 2.55, CAB_ROOF),
        0.65,
        5.97,
        -1.05,
    );
    put(
        &mut group,
        box_part(1.5, 0.18, 0.55, STEEL, 0.0),
        0.65,
        2.5,
        -2.6,
    );
    put(
        &mut group,
        cylinder_part(0.17, 0.3, 0xd18b35, 8),
        -0.15,
        6.22,
        -1.1,
    );
    // Two boom plates enclose visible pins and paired hydraulic rams.
    for z in [0.45, 1.35] {
        steel_beam(&mut group, [1.2, 3.4, z], [6.0, 8.8, z], 0.68, PAINT);
        steel_beam(&mut group, [6.0, 8.8, z], [10.8, 3.3, z], 0.53, PAINT);
        piston(&mut group, [1.6, 3.6, z + 0.15], [4.6, 7.3, z + 0.15]);
        piston(&mut group, [5.8, 8.5, z - 0.12], [9.35, 5.45, z - 0.12]);
        steel_beam(&mut group, [10.8, 3.3, z], [11.7, 1.4, z], 0.27, STEEL);
    }
    for [x, y] in [[1.2, 3.4], [6.0, 8.8], [10.8, 3.3]] {
        let pin = rotated(cylinder_part(0.32, 1.6, STEEL, 12), FRAC_PI_2, 0.0, 0.0);
        put(&mut group, pin, x, y, 0.9);
    }
    let mut bucket = Node::group("");
    put(&mut bucket, steel_box(2.5, 0.16, 2.4, STEEL), 0.0, 0.0, 0.0);
    put(
        &mut bucket,
        steel_box(0.18, 1.45, 2.4, STEEL),
        -1.16,
        0.65,
        0.0,
    );
    for z in [-1.12, 1.12] {
        put(&mut bucket, steel_box(2.5, 1.25, 0.16, STEEL), 0.0, 0.57, z);
    }
    for z in [-0.9, -0.45, 0.0, 0.45, 0.9] {
        put(
            &mut bucket,
            steel_box(0.65, 0.18, 0.23, 0xaaa493),
            1.48,
            0.0,
            z,
        );
    }
    let bucket = rotated(bucket, 0.0, 0.0, -0.22);
    put(&mut group, bucket, 11.5, 0.6, 0.9);
    excavator_details(&mut group);
    weather_paint(&mut group);
    // The bucket group sits before the details appended after it.
    if let Some(bucket) = group.children.iter_mut().find(|c| c.drawable.is_none()) {
        batch(bucket);
    }
    batch(&mut group);
    grime(&mut group, 2.2);
    group
}

/// Handrails round the house, counterweight hazard chevrons, track rollers,
/// boarding steps, mirrors and work lights for the parked excavator.
fn excavator_details(group: &mut Node) {
    let top = 3.43;
    let rail = top + 0.95;
    for (a, b) in [
        ([-4.0, rail, -2.05], [-0.7, rail, -2.05]),
        ([-4.0, rail, 2.05], [2.6, rail, 2.05]),
        ([-4.0, rail, -2.05], [-4.0, rail, 2.05]),
    ] {
        steel_beam(group, a, b, 0.06, PAINT);
    }
    for [x, z] in [
        [-4.0, -2.05],
        [-2.3, -2.05],
        [-0.7, -2.05],
        [-4.0, 2.05],
        [-2.3, 2.05],
        [-0.6, 2.05],
        [1.0, 2.05],
        [2.6, 2.05],
        [-4.0, 0.0],
    ] {
        steel_beam(group, [x, top, z], [x, rail, z], 0.05, PAINT);
    }
    // Black and yellow chevrons across the counterweight's rear face.
    for i in 0..8 {
        let color = if i % 2 == 0 { 0xd2a336 } else { 0x2a2a26 };
        let mut stripe = box_part(0.03, 0.22, 0.75, color, 0.0);
        stripe.set_rotation_euler(0.7, 0.0, 0.0);
        put(group, stripe, -4.2, 2.55, -1.75 + f64::from(i) * 0.5);
    }
    // Carrier rollers on the track frames and a boarding step.
    for z in [-2.25, 2.25] {
        for x in [-1.9, 0.0, 1.9] {
            let roller = rotated(cylinder_part(0.18, 0.5, STEEL, 8), FRAC_PI_2, 0.0, 0.0);
            put(group, roller, x, 1.72, z * 0.82);
        }
    }
    for (i, y) in [0.75, 1.3].into_iter().enumerate() {
        put(
            group,
            steel_box(0.9, 0.06, 0.45, STEEL),
            1.2 - i as f64 * 0.1,
            y,
            -3.2,
        );
    }
    steel_beam(group, [1.65, 0.6, -3.2], [1.65, 1.75, -2.95], 0.05, STEEL);
    // Mirror arm off the cab and two work lights on the boom foot.
    steel_beam(group, [1.95, 5.3, -2.2], [2.45, 5.5, -2.55], 0.05, STEEL);
    put(
        group,
        box_part(0.05, 0.45, 0.3, 0x2d302e, 0.0),
        2.45,
        5.35,
        -2.6,
    );
    for z in [0.3, 1.5] {
        let lamp = rotated(steel_box(0.3, 0.26, 0.34, 0x353e3c), 0.0, 0.0, -0.4);
        put(group, lamp, 2.05, 4.35, z);
        let lens = rotated(box_part(0.02, 0.2, 0.26, 0xf0e6c4, 0.0), 0.0, 0.0, -0.4);
        put(group, lens, 2.22, 4.38, z);
    }
}

/// `quarryDumpTruck()` (`quarry-haul-truck`): the parked haul truck. Presentation
/// loads its bed with rocks after batching.
pub fn quarry_dump_truck() -> Node {
    let mut group = Node::group("quarry-haul-truck");
    put(&mut group, steel_box(11.6, 0.6, 4.7, STEEL), 0.0, 1.8, 0.0);
    for z in [-2.6, 2.6] {
        for x in [-3.8, 1.5, 3.6] {
            let tire = rotated(cylinder_part(1.4, 1.15, RUBBER, 16), FRAC_PI_2, 0.0, 0.0);
            put(&mut group, tire, x, 1.4, z);
            let hub = rotated(cylinder_part(0.65, 1.2, PAINT, 12), FRAC_PI_2, 0.0, 0.0);
            put(&mut group, hub, x, 1.4, z);
        }
    }
    put(&mut group, steel_box(3.3, 3.3, 4.45, PAINT), -3.9, 3.7, 0.0);
    put(
        &mut group,
        box_part(0.05, 1.45, 3.85, GLASS, 0.0),
        -5.58,
        4.45,
        0.0,
    );
    for side in [-1.0, 1.0] {
        put(
            &mut group,
            box_part(2.6, 1.45, 0.05, GLASS, 0.0),
            -3.9,
            4.45,
            side * 2.25,
        );
        put(
            &mut group,
            steel_box(7.7, 2.6, 0.25, PAINT),
            1.45,
            4.2,
            side * 2.62,
        );
        let mut x = -1.5;
        while x <= 4.8 {
            put(
                &mut group,
                steel_box(0.18, 2.7, 0.16, PAINT),
                x,
                4.2,
                side * 2.8,
            );
            x += 1.5;
        }
        put(
            &mut group,
            box_part(0.12, 0.35, 0.55, 0xdfdac4, 0.0),
            -5.68,
            3.0,
            side * 1.65,
        );
    }
    put(&mut group, steel_box(7.7, 0.3, 5.3, STEEL), 1.45, 2.85, 0.0);
    put(&mut group, steel_box(0.25, 2.6, 5.3, PAINT), 5.2, 4.2, 0.0);
    put(&mut group, steel_box(0.25, 3.2, 5.3, PAINT), -2.3, 4.5, 0.0);
    put(&mut group, steel_box(3.5, 0.2, 5.3, PAINT), -3.95, 6.0, 0.0);
    put(
        &mut group,
        steel_box(0.25, 0.6, 5.1, STEEL),
        -5.7,
        2.25,
        0.0,
    );
    // Dusty bed floor, rust-eaten rim and teal mudflaps behind the rear wheels.
    put(
        &mut group,
        accent(7.5, 0.12, 5.0, 0x4a4238, 0.1, 1.0),
        1.45,
        2.98,
        0.0,
    );
    for side in [-1.0, 1.0] {
        put(
            &mut group,
            accent(7.7, 0.28, 0.14, RUST, 0.3, 0.9),
            1.45,
            5.45,
            side * 2.62,
        );
        put(
            &mut group,
            accent(0.08, 0.8, 0.6, TEAL, 0.2, 0.8),
            4.62,
            0.8,
            side * 2.6,
        );
    }
    put(
        &mut group,
        accent(3.3, 0.08, 4.3, DUST, 0.0, 1.0),
        -3.9,
        5.42,
        0.0,
    );
    truck_details(&mut group);
    // Same sun-faded fleet yellow as the excavator, not steel-tinted brown.
    weather_paint(&mut group);
    batch(&mut group);
    grime(&mut group, 2.4);
    group
}

/// Tread lugs, rims, grille, deck rails, ladder, stacks, mirrors, beacon, fuel
/// tank and hazard paint: the details that sell a 100-tonne haul truck.
fn truck_details(group: &mut Node) {
    for z in [-2.6, 2.6] {
        for x in [-3.8, 1.5, 3.6] {
            // Chevron lugs around the tread.
            for i in 0..16 {
                let angle = f64::from(i) * PI / 8.0;
                let yaw = if i % 2 == 0 { 0.22 } else { -0.22 };
                let mut lug = box_part(0.34, 0.1, 1.05, RUBBER, 0.0);
                lug.set_rotation_euler(0.0, yaw, angle + FRAC_PI_2);
                put(
                    group,
                    lug,
                    x + angle.cos() * 1.42,
                    1.4 + angle.sin() * 1.42,
                    z,
                );
            }
            // Dark rim with a ring of wheel nuts on the outer face.
            let rim = rotated(cylinder_part(1.0, 1.18, 0x3b3a35, 16), FRAC_PI_2, 0.0, 0.0);
            put(group, rim, x, 1.4, z);
            let face = z.signum() * 0.61;
            for k in 0..8 {
                let angle = f64::from(k) * PI / 4.0;
                let nut = rotated(cylinder_part(0.06, 0.08, 0x8f8a7c, 6), FRAC_PI_2, 0.0, 0.0);
                put(
                    group,
                    nut,
                    x + angle.cos() * 0.48,
                    1.4 + angle.sin() * 0.48,
                    z + face,
                );
            }
        }
        // Fenders over the front wheels carry the walkway.
        put(group, steel_box(3.1, 0.14, 1.35, PAINT), -3.8, 2.95, z);
    }
    // Radiator grille with heavy horizontal bars and hazard-striped bumper.
    put(
        group,
        box_part(0.06, 1.25, 2.7, 0x26261f, 0.0),
        -5.6,
        2.95,
        0.0,
    );
    for i in 0..6 {
        put(
            group,
            steel_box(0.1, 0.07, 2.7, STEEL),
            -5.63,
            2.4 + f64::from(i) * 0.22,
            0.0,
        );
    }
    for i in 0..9 {
        let color = if i % 2 == 0 { 0xd2a336 } else { 0x2a2a26 };
        put(
            group,
            box_part(0.03, 0.45, 0.55, color, 0.0),
            -5.84,
            2.25,
            -2.2 + f64::from(i) * 0.55,
        );
    }
    // Walkway handrail across the front deck, and a boarding ladder.
    for z in [-2.4, -1.2, 0.0, 1.2, 2.4] {
        steel_beam(group, [-5.45, 3.05, z], [-5.45, 4.0, z], 0.06, PAINT);
    }
    steel_beam(group, [-5.45, 4.0, -2.4], [-5.45, 4.0, 2.4], 0.07, PAINT);
    steel_beam(group, [-5.45, 3.55, -2.4], [-5.45, 3.55, 2.4], 0.05, PAINT);
    for side in [-1.35, -1.85] {
        steel_beam(group, [-6.25, 0.45, side], [-5.75, 2.55, side], 0.07, STEEL);
    }
    for i in 0..5 {
        let t = f64::from(i) / 4.0;
        steel_beam(
            group,
            [-6.2 + t * 0.42, 0.65 + t * 1.75, -1.35],
            [-6.2 + t * 0.42, 0.65 + t * 1.75, -1.85],
            0.05,
            STEEL,
        );
    }
    // Twin exhaust stacks, mirrors on arms and an amber beacon on the canopy.
    for z in [-1.7, -2.05] {
        let stack = cylinder_part(0.12, 1.5, 0x4a453c, 8);
        put(group, stack, -2.35, 5.85, z);
    }
    put(
        group,
        cylinder_part(0.15, 0.08, 0x2a2622, 8),
        -2.35,
        6.62,
        -1.7,
    );
    for side in [-1.0, 1.0] {
        steel_beam(
            group,
            [-5.5, 4.9, side * 2.2],
            [-6.15, 5.15, side * 2.75],
            0.05,
            STEEL,
        );
        put(
            group,
            box_part(0.06, 0.55, 0.32, 0x2d302e, 0.0),
            -6.15,
            5.0,
            side * 2.8,
        );
    }
    put(
        group,
        cylinder_part(0.16, 0.26, 0xe0912e, 10),
        -4.8,
        6.23,
        1.9,
    );
    // Fuel tank slung between the axles, with straps.
    let tank = rotated(cylinder_part(0.5, 2.0, 0x8c8676, 12), 0.0, 0.0, FRAC_PI_2);
    put(group, tank, -1.15, 1.35, -2.3);
    for x in [-1.75, -0.55] {
        let strap = rotated(cylinder_part(0.53, 0.08, 0x3b3a35, 12), 0.0, 0.0, FRAC_PI_2);
        put(group, strap, x, 1.35, -2.3);
    }
}

/// Bake dust and grime into a batched assembly's vertex paint: caked dust low on
/// the machine, a dust film on every upward face. `soil_line` is the height the
/// splashed dirt reaches, in the assembly's frame.
fn grime(group: &mut Node, soil_line: f64) {
    let dust = hex_to_linear(GRIME);
    for child in &mut group.children {
        let Some(drawable) = &mut child.drawable else {
            continue;
        };
        if drawable.mesh.colors.is_empty() {
            continue;
        }
        let mut mesh = (*drawable.mesh).clone();
        for ((color, p), n) in mesh
            .colors
            .iter_mut()
            .zip(&mesh.positions)
            .zip(&mesh.normals)
        {
            let y = f64::from(p[1]);
            let low = 1.0 - smoothstep(y, 0.0, soil_line);
            let up = smoothstep(f64::from(n[1]), 0.55, 0.95);
            let amount = (low * 0.7 + up * 0.32).min(0.8);
            for k in 0..3 {
                color[k] = (f64::from(color[k]) * (1.0 - amount) + dust[k] * amount) as f32;
            }
        }
        drawable.mesh = Arc::new(mesh);
    }
}

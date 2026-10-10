//! Port of `quarry-machinery.ts`: the parked excavator and haul truck on the
//! quarry apron, in sun-faded industrial yellow with weathered accents. All
//! assemblies are static, merged into a handful of material batches.

use std::f64::consts::FRAC_PI_2;
use std::sync::Arc;

use glam::DVec3;

use crate::scene::{Material, Node};

use super::batching::batch;
use super::harbor_surfaces::{steel_beam, steel_box};
use super::model_primitives::{Cache, box_part, cylinder_part, material, put, rotated};
use super::tank_surfaces::armor_wear_texture;
use crate::geometry::math::{normalize, quat_from_unit_vectors};

// Sun-faded industrial yellow, dusty steel, rusty red and one muted teal accent.
const PAINT: u32 = 0xc69a4b;
const STEEL: u32 = 0x5a564c;
const RUBBER: u32 = 0x343431;
const GLASS: u32 = 0x526c72;
const RUST: u32 = 0x8a4f2e;
const TEAL: u32 = 0x4e7d7c;
const DUST: u32 = 0xc9b78d;
/// The cab roof's pale paint, weathered like the fleet yellow.
const CAB_ROOF: u32 = 0xd2c6a2;

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
fn weather_paint(node: &mut Node) {
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
    weather_paint(&mut group);
    if let Some(bucket) = group.children.last_mut() {
        batch(bucket);
    }
    batch(&mut group);
    group
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
    // Same sun-faded fleet yellow as the excavator, not steel-tinted brown.
    weather_paint(&mut group);
    batch(&mut group);
    group
}

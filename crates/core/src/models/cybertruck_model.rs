//! The Cybertruck easter egg: a stainless-steel wedge of a pickup that roams
//! debug single-player arenas as a harmless target. Low poly by nature: its body
//! is one extruded side profile (a wedge nose, the roof apex, the long vault over
//! the bed and trapezoid wheel arches), with tinted glass, light bars across the
//! nose and tail, and, after the production truck, chunky tyres in thick black
//! arch flares, black rocker cladding and bumpers, and a dark tonneau cover between
//! the steel sail panels, which is what reads from the game's high camera.
//!
//! Part names follow the tanks (`hull`, `track-group`, an empty `turret` and
//! `barrel`, a `muzzle` marker at the nose) so presentation poses it like the
//! Humvee; each wheel is a joint of its own, so a truck that lost one can leave
//! it out. Its paint is the same for every team: it belongs to none.

use std::f64::consts::FRAC_PI_2;
use std::sync::{Arc, OnceLock};

use glam::{DVec2, DVec3};

use super::model_primitives::{material, put, shadowed};
use super::part;
use crate::geometry::{
    ExtrudeOptions, Mesh, Shape, cylinder_geometry, extrude_geometry, shape_geometry,
};
use crate::scene::{Color, Material, Node};

/// Overall length and width; the body's height comes from its profile.
const LENGTH: f64 = 5.7;
const WIDTH: f64 = 2.0;
/// Wheels: radius, tread width, track half-width (the tread stands a little proud of
/// the flank) and axle stations (a 3.8 m wheelbase).
pub const CYBERTRUCK_WHEEL_RADIUS: f64 = 0.45;
pub const CYBERTRUCK_WHEEL_WIDTH: f64 = 0.36;
const WHEEL_X: f64 = 0.85;
const AXLES: [f64; 2] = [-1.9, 1.9];
/// Each wheel's hub as (across, along) the truck before the vehicle scale, in the
/// order of `CYBERTRUCK_WHEEL_JOINTS`.
pub const CYBERTRUCK_WHEELS: [[f64; 2]; 4] = [
    [-WHEEL_X, AXLES[0]],
    [-WHEEL_X, AXLES[1]],
    [WHEEL_X, AXLES[0]],
    [WHEEL_X, AXLES[1]],
];
pub const CYBERTRUCK_WHEEL_JOINTS: [&str; 4] = ["wheel-0", "wheel-1", "wheel-2", "wheel-3"];
/// The roof apex and the window line along the body's flanks.
const APEX: [f64; 2] = [0.3, 1.8];
const NOSE: [f64; 2] = [2.84, 1.0];
const TAIL: [f64; 2] = [-2.85, 1.22];
const BELTLINE: f64 = 1.2;
/// The sill under the doors, and the steel sails either side of the tonneau cover.
const SILL: f64 = 0.5;
const SAIL: f64 = 0.2;
/// Glass and trim lie this far off the steel they cover.
const SKIN: f64 = 0.004;
/// The ground line in the model: vehicles are drawn 0.25 m above the ground (the
/// physics body's height less the view's model drop), here at the 0.95 vehicle scale.
const GROUND_Y: f64 = -0.26;

/// Brushed stainless steel, a mid grey.
const STAINLESS: u32 = 0xaeb3b7;
/// The gold-foil wrap: bright, smooth metal with a warm glow so it reads as gold
/// rather than olive in shade.
const GOLD_FOIL: u32 = 0xf6c445;
const GOLD_GLOW: u32 = 0x7a5200;
/// Tinted glass, the black cladding and tonneau, tyres and dark grey rims.
const GLASS: u32 = 0x26313a;
const TRIM: u32 = 0x232527;
const RUBBER: u32 = 0x18191a;
const RIM: u32 = 0x4b5056;
const HEADLIGHT: u32 = 0xf2f5ff;
const TAILLIGHT: u32 = 0xd3241c;

/// The body's side profile, as (along the truck, up) points front to back over the
/// top: nose, the straight rise to the apex, the vault down to the tailgate, then
/// back along the sill under both arches.
fn profile() -> Vec<DVec2> {
    let half = LENGTH / 2.0;
    let mut points = vec![[half, 0.6], NOSE, APEX, TAIL, [-half, 0.6]];
    // The sill runs back to front under both arches.
    for center in AXLES {
        points.extend(arch(center, 0.0));
    }
    points.iter().map(|&[z, y]| DVec2::new(z, y)).collect()
}

/// A wheel arch over the axle at `center`, rear to front, `grow` larger all round.
fn arch(center: f64, grow: f64) -> [[f64; 2]; 4] {
    [
        [center - 0.74 - grow, SILL],
        [center - 0.44 - grow * 0.6, 1.02 + grow * 0.7],
        [center + 0.44 + grow * 0.6, 1.02 + grow * 0.7],
        [center + 0.74 + grow, SILL],
    ]
}

/// Height of the hood-and-windshield line and of the vault line at `z`.
fn roof(z: f64) -> f64 {
    let line = |a: [f64; 2], b: [f64; 2]| a[1] + (z - a[0]) * (b[1] - a[1]) / (b[0] - a[0]);
    if z >= APEX[0] {
        line(APEX, NOSE)
    } else {
        line(TAIL, APEX)
    }
}

/// A flat quad from four corners, wound to face along `normal`.
fn quad(corners: [DVec3; 4], normal: DVec3) -> Mesh {
    let [a, b, c, d] = corners;
    let facing = (b - a).cross(c - a).dot(normal) > 0.0;
    let order = if facing {
        [a, b, c, a, c, d]
    } else {
        [a, c, b, a, d, c]
    };
    let positions: Vec<f64> = order.iter().flat_map(|p| [p.x, p.y, p.z]).collect();
    let normals: Vec<f64> = order
        .iter()
        .flat_map(|_| [normal.x, normal.y, normal.z])
        .collect();
    Mesh::from_f64(&positions, &normals, &[0.0; 12], None)
}

/// A flat outline on the body's flank: `points` are (along, up), placed on the
/// right flank (`side` 1) or the left (-1), facing out.
fn flank(points: &[[f64; 2]], side: f64) -> Mesh {
    // Seen from outside the right flank, forward runs to the left.
    let outline: Vec<DVec2> = points
        .iter()
        .map(|&[z, y]| DVec2::new(-side * z, y))
        .collect();
    let mut mesh = shape_geometry(&[Shape::from_points(&outline)]).to_non_indexed();
    mesh.rotate_y(side * FRAC_PI_2);
    mesh.translate(side * (WIDTH / 2.0 + SKIN), 0.0, 0.0);
    mesh
}

/// Geometry every Cybertruck shares, as (material, mesh) pairs.
struct Kit {
    body: Vec<(Arc<Material>, Arc<Mesh>)>,
    wheel: Vec<(Arc<Material>, Arc<Mesh>)>,
}

fn glowing(color: u32) -> Arc<Material> {
    Arc::new(Material {
        emissive: Color(color),
        emissive_intensity: 1.4,
        ..Material::standard(color, 0.0, 0.5)
    })
}

/// The shared geometry in stainless steel, or wrapped in gold foil.
fn kit(gold: bool) -> &'static Kit {
    static KITS: [OnceLock<Kit>; 2] = [OnceLock::new(), OnceLock::new()];
    KITS[usize::from(gold)].get_or_init(|| {
        let steel = if gold {
            Arc::new(Material {
                emissive: Color(GOLD_GLOW),
                emissive_intensity: 0.5,
                ..Material::standard(GOLD_FOIL, 0.6, 0.22)
            })
        } else {
            material(STAINLESS, 0.75, 0.36)
        };
        let glass = material(GLASS, 0.3, 0.1);
        let trim = material(TRIM, 0.0, 0.8);
        let mut body = extrude_geometry(
            &[Shape::from_points(&profile())],
            &ExtrudeOptions {
                depth: WIDTH,
                bevel_enabled: false,
                ..ExtrudeOptions::default()
            },
        );
        // The profile's first axis runs along the truck, the extrusion across it.
        body.translate(0.0, 0.0, -WIDTH / 2.0);
        body.rotate_y(-FRAC_PI_2);
        // Windshield and the small rear window lie on the roof lines, the side
        // windows on the flanks above the beltline.
        let front_glass = 1.6;
        let rear_glass = -0.55;
        let across = WIDTH / 2.0 - 0.07;
        let on_roof = |z: f64, x: f64| DVec3::new(x, roof(z) + SKIN, z);
        let windshield = quad(
            [
                on_roof(front_glass, -across),
                on_roof(front_glass, across),
                on_roof(APEX[0] + 0.05, across),
                on_roof(APEX[0] + 0.05, -across),
            ],
            DVec3::new(0.0, NOSE[0] - APEX[0], APEX[1] - NOSE[1]).normalize(),
        );
        let rear_window = quad(
            [
                on_roof(APEX[0] - 0.05, -across),
                on_roof(APEX[0] - 0.05, across),
                on_roof(rear_glass, across),
                on_roof(rear_glass, -across),
            ],
            DVec3::new(0.0, APEX[0] - TAIL[0], TAIL[1] - APEX[1]).normalize(),
        );
        let window_back = -0.95;
        let side_window = [
            [front_glass, BELTLINE],
            [APEX[0], APEX[1] - 0.07],
            [window_back, roof(window_back) - 0.06],
            [window_back, BELTLINE],
        ];
        let mut glass_parts = vec![windshield, rear_window];
        // The tonneau over the bed, between the steel sails.
        let sail = WIDTH / 2.0 - SAIL;
        let tonneau_front = rear_glass - 0.02;
        let tonneau_back = TAIL[0] + 0.04;
        let mut trim_parts = vec![quad(
            [
                on_roof(tonneau_front, -sail),
                on_roof(tonneau_front, sail),
                on_roof(tonneau_back, sail),
                on_roof(tonneau_back, -sail),
            ],
            DVec3::new(0.0, APEX[0] - TAIL[0], TAIL[1] - APEX[1]).normalize(),
        )];
        // Thick flares round each arch and the rocker cladding between them.
        let rocker = [AXLES[0] + 0.74, AXLES[1] - 0.74];
        for side in [-1.0, 1.0] {
            glass_parts.push(flank(&side_window, side));
            for center in AXLES {
                // A flare around the arch: its outline out, then the opening back.
                let mut band: Vec<[f64; 2]> = arch(center, 0.17).to_vec();
                band.extend(arch(center, 0.0).iter().rev());
                trim_parts.push(flank(&band, side));
            }
            trim_parts.push(flank(
                &[
                    [rocker[0], SILL - 0.08],
                    [rocker[1], SILL - 0.08],
                    [rocker[1], SILL + 0.18],
                    [rocker[0], SILL + 0.18],
                ],
                side,
            ));
        }
        // Black bumpers under the nose, flush with it, and a chunkier one standing
        // out under the tailgate.
        let half = LENGTH / 2.0;
        for (z, height, depth) in [(half - 0.15, 0.26, 0.3), (-half + 0.18, 0.32, 0.44)] {
            let mut bumper = crate::geometry::box_geometry(WIDTH + 0.02, height, depth);
            bumper.translate(0.0, 0.58 - height / 2.0, z);
            trim_parts.push(bumper.to_non_indexed());
        }
        let merge = |parts: &[Mesh]| {
            Arc::new(
                crate::geometry::merge_geometries(&parts.iter().collect::<Vec<_>>())
                    .expect("parts share attributes"),
            )
        };
        let bar = |y: f64, z: f64| {
            let mut mesh = crate::geometry::box_geometry(WIDTH - 0.16, 0.035, 0.02);
            mesh.translate(0.0, y, z);
            mesh
        };
        let mut wheel_tire = cylinder_geometry(
            CYBERTRUCK_WHEEL_RADIUS,
            CYBERTRUCK_WHEEL_RADIUS,
            CYBERTRUCK_WHEEL_WIDTH,
            16,
        );
        wheel_tire.rotate_z(FRAC_PI_2);
        let mut rim = cylinder_geometry(
            CYBERTRUCK_WHEEL_RADIUS * 0.66,
            CYBERTRUCK_WHEEL_RADIUS * 0.66,
            CYBERTRUCK_WHEEL_WIDTH + 0.012,
            12,
        );
        rim.rotate_z(FRAC_PI_2);
        Kit {
            body: vec![
                (steel, Arc::new(body)),
                (glass, merge(&glass_parts)),
                (trim, merge(&trim_parts)),
                (
                    glowing(HEADLIGHT),
                    Arc::new(bar(NOSE[1] - 0.02, NOSE[0] + 0.012)),
                ),
                (
                    glowing(TAILLIGHT),
                    Arc::new(bar(TAIL[1] - 0.03, TAIL[0] - 0.012)),
                ),
            ],
            wheel: vec![
                (material(RUBBER, 0.0, 0.92), Arc::new(wheel_tire)),
                (material(RIM, 0.5, 0.45), Arc::new(rim)),
            ],
        }
    })
}

/// The Cybertruck (in gold foil when `gold`), before the vehicle scale.
pub(super) fn cybertruck_model(name: &str, gold: bool) -> Node {
    let kit = kit(gold);
    let parts = |pairs: &[(Arc<Material>, Arc<Mesh>)]| {
        pairs
            .iter()
            .map(|(material, mesh)| shadowed(mesh.clone(), material.clone()))
            .collect::<Vec<_>>()
    };
    let mut root = Node::group(name);
    let mut hull = Node::group(part::HULL);
    hull.position.y = GROUND_Y;
    let mut wheels = Node::group(part::TRACK_GROUP);
    for (joint, [x, z]) in CYBERTRUCK_WHEEL_JOINTS.into_iter().zip(CYBERTRUCK_WHEELS) {
        let mut wheel = Node::group(joint);
        wheel.children.extend(parts(&kit.wheel));
        put(&mut wheels, wheel, x, CYBERTRUCK_WHEEL_RADIUS, z);
    }
    hull.children.push(wheels);
    hull.children.extend(parts(&kit.body));
    root.children.push(hull);
    // No weapon: the turret and barrel are empty, the muzzle sits at the nose.
    let mut turret = Node::group(part::TURRET);
    let mut barrel = Node::group(part::BARREL);
    put(
        &mut barrel,
        Node::group(part::MUZZLE),
        0.0,
        NOSE[1] + GROUND_Y,
        LENGTH / 2.0,
    );
    turret.children.push(barrel);
    root.children.push(turret);
    root
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use glam::DMat4;

    use super::*;
    use crate::models::{Team, VehicleKind, WreckPart, tank_model, wreck_model};

    /// Triangles allowed for a whole truck: one extruded body, glass, trim, two light
    /// bars and four wheels.
    const TRIANGLE_BUDGET: usize = 1_500;
    /// Distinct meshes: the body parts, and one tyre and cover for all four wheels.
    const MESH_BUDGET: usize = 7;

    fn meshes(model: &Node) -> (usize, HashSet<*const Mesh>) {
        let mut triangles = 0;
        let mut meshes = HashSet::new();
        model.traverse(DMat4::IDENTITY, &mut |part, _| {
            if let Some(drawable) = &part.drawable {
                triangles += drawable.mesh.triangle_count();
                meshes.insert(Arc::as_ptr(&drawable.mesh));
            }
        });
        (triangles, meshes)
    }

    #[test]
    fn trucks_stay_within_their_triangle_and_mesh_budget() {
        for kind in [VehicleKind::Cybertruck, VehicleKind::GoldCybertruck] {
            let (triangles, meshes) = meshes(&tank_model(kind, Team::Blue));
            assert!(triangles < TRIANGLE_BUDGET, "{triangles} triangles");
            assert!(meshes.len() <= MESH_BUDGET, "{} meshes", meshes.len());
        }
    }

    #[test]
    fn every_wheel_is_a_joint_in_the_truck_and_its_wreck() {
        let truck = tank_model(VehicleKind::Cybertruck, Team::Red);
        let wreck = wreck_model(VehicleKind::Cybertruck, Team::Red, WreckPart::Intact);
        for joint in CYBERTRUCK_WHEEL_JOINTS {
            assert!(truck.find(joint).is_some(), "truck {joint}");
            assert!(wreck.find(joint).is_some(), "wreck {joint}");
        }
    }

    #[test]
    fn a_loose_wheel_turns_its_axle_along_y_about_its_hub() {
        let wheel = wreck_model(VehicleKind::Cybertruck, Team::Red, WreckPart::Wheel);
        let bounds = crate::geometry::node_bounds(&wheel, DMat4::IDENTITY);
        let size = bounds.size();
        assert!(bounds.center().length() < 1e-6, "{:?}", bounds.center());
        assert!(
            (size.x - 2.0 * CYBERTRUCK_WHEEL_RADIUS).abs() < 0.01,
            "{size:?}"
        );
        assert!(
            (size.z - 2.0 * CYBERTRUCK_WHEEL_RADIUS).abs() < 0.01,
            "{size:?}"
        );
        assert!(size.y < CYBERTRUCK_WHEEL_WIDTH + 0.02, "{size:?}");
    }
}

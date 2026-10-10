//! The Cybertruck easter egg: a stainless-steel wedge of a pickup that roams debug
//! single-player arenas as a harmless target, modelled after the production truck
//! with the Humvee's parts kit. `body` builds the faceted shell and its fittings,
//! `wheel` the all-terrain wheel; each is merged once into one shared mesh per
//! material, like the Humvee's assemblies.
//!
//! Part names follow the tanks (`hull`, `track-group`, an empty `turret` and
//! `barrel`, a `muzzle` marker at the nose) so presentation poses it like the
//! Humvee; each wheel is a joint of its own, so a truck that lost one can leave
//! it out. Its paint is the same for every team: it belongs to none.

use std::sync::{Arc, OnceLock};

use super::model_primitives::{material, put, shadowed};
use super::part;
use super::parts_kit::Role;
use super::tank_surfaces::armor_wear_texture;
use crate::geometry::Mesh;
use crate::scene::{Color, Material, Node};

mod body;
mod wheel;

/// Body extremes along the truck and its half-width (the flanks).
const NOSE: f64 = 2.84;
const TAIL: f64 = -2.84;
const HALF_WIDTH: f64 = 1.0;
/// Wheels: radius, tread width, track half-width (the tread stands a little proud of
/// the flank) and axle stations (a 3.8 m wheelbase).
pub const CYBERTRUCK_WHEEL_RADIUS: f64 = 0.45;
pub const CYBERTRUCK_WHEEL_WIDTH: f64 = 0.304;
const WHEEL_X: f64 = 0.86;
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
/// The ground line in the model: vehicles are drawn 0.25 m above the ground (the
/// physics body's height less the view's model drop), here at the 0.95 vehicle scale.
const GROUND_Y: f64 = -0.26;
/// The light bar's height on the nose, where the muzzle marker sits.
const MUZZLE_Y: f64 = 0.97;

/// Brushed stainless steel, a mid grey, smudged and scratched by the tanks' wear
/// texture.
const STAINLESS: u32 = 0xc6cbcf;
const STAINLESS_METALNESS: f32 = 0.55;
const STAINLESS_ROUGHNESS: f32 = 0.32;
/// The gold-foil wrap: bright, smooth metal with a warm glow so it reads as gold
/// rather than olive in shade.
const GOLD_FOIL: u32 = 0xf6c445;
const GOLD_GLOW: u32 = 0x7a5200;
/// The wear texture's relief on the steel: faint, it is not battle-worn armor.
const WEAR_BUMP: f32 = 0.01;
/// Black cladding, flares and bumpers; near-black seams, liners and fittings; the
/// tonneau's slats; tires; the aero covers' dark metal; tinted glass.
const CLADDING: u32 = 0x2b2e31;
const GAP: u32 = 0x0e1012;
const SLAT: u32 = 0x24272a;
/// Rubber light enough for the tread and sidewall to show, like the Humvee's.
const RUBBER: u32 = 0x2a2e30;
const AERO: u32 = 0x6a7076;
const GLASS: u32 = 0x1b2730;
/// Lamps glow.
const HEADLAMP: u32 = 0xf2f5ff;
const AMBER: u32 = 0xf09a2a;
const TAILLIGHT: u32 = 0xd3241c;

/// Geometry every Cybertruck shares, one mesh per role and assembly.
struct Assemblies {
    body: Vec<(Role, Arc<Mesh>)>,
    wheel: Vec<(Role, Arc<Mesh>)>,
}

fn assemblies() -> &'static Assemblies {
    static ASSEMBLIES: OnceLock<Assemblies> = OnceLock::new();
    ASSEMBLIES.get_or_init(|| Assemblies {
        body: body::body_kit().finish(),
        wheel: wheel::wheel_kit().finish(),
    })
}

/// A lamp lens that glows.
fn glowing(color: u32) -> Arc<Material> {
    Arc::new(Material {
        emissive: Color(color),
        emissive_intensity: 1.4,
        ..Material::standard(color, 0.0, 0.5)
    })
}

/// The materials of one finish, stainless or gold, by role.
struct Finish {
    body: Arc<Material>,
    lamps: [Arc<Material>; 3],
}

fn finish(gold: bool) -> &'static Finish {
    static FINISHES: [OnceLock<Finish>; 2] = [OnceLock::new(), OnceLock::new()];
    FINISHES[usize::from(gold)].get_or_init(|| {
        let texture = armor_wear_texture();
        let worn = |material: Material| Material {
            map: Some(texture.clone()),
            bump_map: Some(texture.clone()),
            bump_scale: WEAR_BUMP,
            ..material
        };
        let body = if gold {
            worn(Material {
                emissive: Color(GOLD_GLOW),
                emissive_intensity: 0.5,
                ..Material::standard(GOLD_FOIL, 0.6, 0.22)
            })
        } else {
            worn(Material::standard(
                STAINLESS,
                STAINLESS_METALNESS,
                STAINLESS_ROUGHNESS,
            ))
        };
        Finish {
            body: Arc::new(body),
            lamps: [glowing(HEADLAMP), glowing(AMBER), glowing(TAILLIGHT)],
        }
    })
}

impl Finish {
    fn material(&self, role: Role) -> Arc<Material> {
        match role {
            Role::Paint => self.body.clone(),
            Role::Shade => material(CLADDING, 0.0, 0.7),
            Role::Gap => material(GAP, 0.0, 0.9),
            Role::Canvas => material(SLAT, 0.2, 0.55),
            Role::Rubber => material(RUBBER, 0.0, 0.92),
            Role::Steel => material(AERO, 0.6, 0.4),
            Role::Glass => material(GLASS, 0.5, 0.06),
            Role::Headlamp => self.lamps[0].clone(),
            Role::Amber => self.lamps[1].clone(),
            Role::Red => self.lamps[2].clone(),
        }
    }

    /// One shadowed part per role of an assembly.
    fn parts(&self, meshes: &[(Role, Arc<Mesh>)]) -> impl Iterator<Item = Node> {
        meshes
            .iter()
            .map(|(role, mesh)| shadowed(mesh.clone(), self.material(*role)))
    }
}

/// The Cybertruck (in gold foil when `gold`), before the vehicle scale.
pub(super) fn cybertruck_model(name: &str, gold: bool) -> Node {
    let finish = finish(gold);
    let shared = assemblies();
    let mut root = Node::group(name);
    let mut hull = Node::group(part::HULL);
    hull.position.y = GROUND_Y;
    let mut wheels = Node::group(part::TRACK_GROUP);
    for (joint, [x, z]) in CYBERTRUCK_WHEEL_JOINTS.into_iter().zip(CYBERTRUCK_WHEELS) {
        // The left wheels are the right ones turned half a revolution.
        let mut wheel = Node::group(joint);
        if x < 0.0 {
            wheel.rotation = glam::DQuat::from_rotation_y(std::f64::consts::PI);
        }
        wheel.children.extend(finish.parts(&shared.wheel));
        put(&mut wheels, wheel, x, CYBERTRUCK_WHEEL_RADIUS, z);
    }
    hull.children.push(wheels);
    hull.children.extend(finish.parts(&shared.body));
    root.children.push(hull);
    // No weapon: the turret and barrel are empty, the muzzle sits at the nose.
    let mut turret = Node::group(part::TURRET);
    let mut barrel = Node::group(part::BARREL);
    put(
        &mut barrel,
        Node::group(part::MUZZLE),
        0.0,
        MUZZLE_Y + GROUND_Y,
        NOSE,
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

    /// Triangles allowed for a whole truck, within the Humvee's own budget: the
    /// faceted shell and its fittings, and four treaded wheels.
    const TRIANGLE_BUDGET: usize = 9_000;
    /// Distinct meshes: one per material of the body, and of a wheel for all four.
    const MESH_BUDGET: usize = 14;

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
        assert!(
            bounds.center().x.hypot(bounds.center().z) < 0.02,
            "{:?}",
            bounds.center()
        );
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

    #[test]
    fn the_tires_touch_the_ground_line() {
        let truck = tank_model(VehicleKind::Cybertruck, Team::Red);
        let (wheels, parent) = truck
            .find_with_parent_world(part::TRACK_GROUP, DMat4::IDENTITY)
            .expect("wheels");
        let bottom = crate::geometry::node_bounds(wheels, parent).min.y;
        let ground = GROUND_Y * crate::sim::data::vehicle(VehicleKind::Cybertruck).scale;
        assert!((bottom - ground).abs() < 0.01, "{bottom} vs {ground}");
    }
}

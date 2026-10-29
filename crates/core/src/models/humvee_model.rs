//! The wheeled launcher vehicle: an up-armored four-door HMMWV with a TOW
//! launcher on its roof ring (originally a port of `humvee-model.ts`).
//!
//! Same part names as the tanks: `hull` (stretched along z), `track-group` (the
//! wheels, un-stretched so they stay round), `turret`, `barrel` and an empty
//! `muzzle` marker at the launcher mouth.
//!
//! Each assembly is merged once into one shared mesh per paint role (`kit`), so
//! the body, a wheel and the launcher cost a few meshes each; the livery then
//! bakes each team's weathered paint into its own copy, the four wheels sharing
//! one.

use std::sync::{Arc, OnceLock};

use glam::{DVec2, DVec3};

use super::model_primitives::{material, paint, put, shadowed};
use super::tank_model::{LENS_ROUGHNESS, MARKING_WHITE};
use super::tank_surfaces::{LAMP_LENS, vehicle_glass};
use super::{Team, VehicleKind, part};
use crate::geometry::Mesh;
use crate::scene::{Material, Node};

mod body;
mod kit;
mod turret;
mod wheel;

use body::hood_insignia;
use kit::Role;

/// The body shell and wheelbase are stretched by this factor along z.
pub const HUMVEE_BODY_LENGTH_SCALE: f64 = 1.16;
/// Height of the roof launcher tube's axis.
const LAUNCHER_Y: f64 = 2.13;
/// Wheel centres: track half-width, axle height and (unstretched) axle stations,
/// which the body's wheel openings follow.
const WHEEL_X: f64 = 1.0;
const AXLE_Y: f64 = 0.25;
const REAR_AXLE: f64 = -1.32;
const FRONT_AXLE: f64 = 1.3;
/// The launcher mouth, where missiles appear.
const MUZZLE_Z: f64 = 1.7;

/// Tire rubber: dark grey, light enough for the sidewall and lugs to show.
const RUBBER: u32 = 0x2d3031;
/// Armored glass: a deep blue-green tint that mirrors the sky toward grazing
/// angles (`vehicle_glass`), so the windows read as glass rather than as painted
/// panels.
const GLASS: u32 = 0x1f3a40;
const GAP: u32 = 0x101416;
const CANVAS: u32 = 0x4f4a38;
const AMBER: u32 = 0xdf923b;
const RED_LAMP: u32 = 0x931e16;
/// Rubber, seals and canvas are matte.
const MATTE_ROUGHNESS: f64 = 0.92;

/// Geometry shared by every Humvee, one mesh per role and assembly.
struct Assemblies {
    body: Vec<(Role, Arc<Mesh>)>,
    wheel: Vec<(Role, Arc<Mesh>)>,
    turret: Vec<(Role, Arc<Mesh>)>,
    barrel: Vec<(Role, Arc<Mesh>)>,
}

fn assemblies() -> &'static Assemblies {
    static ASSEMBLIES: OnceLock<Assemblies> = OnceLock::new();
    ASSEMBLIES.get_or_init(|| Assemblies {
        body: body::body_kit().finish(),
        wheel: wheel::wheel_kit().finish(),
        turret: turret::turret_kit().finish(),
        barrel: turret::barrel_kit().finish(),
    })
}

/// Paint for one Humvee: its team's paint, shade and steel.
struct Palette {
    base: u32,
    shade: u32,
    steel: u32,
}

impl Palette {
    fn material(&self, role: Role) -> Arc<Material> {
        match role {
            Role::Paint => paint(self.base),
            Role::Shade => paint(self.shade),
            Role::Steel => paint(self.steel),
            Role::Rubber => material(RUBBER, 0.0, MATTE_ROUGHNESS),
            Role::Gap => material(GAP, 0.0, MATTE_ROUGHNESS),
            Role::Canvas => material(CANVAS, 0.0, MATTE_ROUGHNESS),
            Role::Glass => vehicle_glass(GLASS),
            // The marker lamps draw with the matte fittings; the livery paints the
            // headlamp lenses as lamps (warm glass, a hot core falling off to the
            // reflector) in the paint's own draw.
            Role::Headlamp => material(LAMP_LENS, 0.0, LENS_ROUGHNESS),
            Role::Amber => material(AMBER, 0.0, LENS_ROUGHNESS),
            Role::Red => material(RED_LAMP, 0.0, LENS_ROUGHNESS),
        }
    }

    /// One shadowed part per role of an assembly.
    fn parts(&self, meshes: &[(Role, Arc<Mesh>)]) -> impl Iterator<Item = Node> {
        meshes
            .iter()
            .map(|(role, mesh)| shadowed(mesh.clone(), self.material(*role)))
    }
}

/// The openings a burning Humvee wreck's fire comes out of, in the vehicle's frame
/// before its scale, each as the centre of its top and its half size in plan: the
/// engine bay under the hood and the gunner's ring in the roof.
pub(super) fn wreck_openings() -> [(DVec3, DVec2); 2] {
    let (engine, hood) = body::engine_bay();
    let stretch = DVec3::new(1.0, 1.0, HUMVEE_BODY_LENGTH_SCALE);
    [
        (
            engine * stretch,
            hood * DVec2::new(1.0, HUMVEE_BODY_LENGTH_SCALE),
        ),
        (
            DVec3::new(0.0, turret::RING_BASE, 0.0),
            DVec2::splat(turret::RING_OPENING),
        ),
    ]
}

/// `humveeModel(team, false)` in `colors` (paint, shade, steel), before the
/// vehicle scale and worn finish, with the team's insignia on the hood.
pub(super) fn humvee_model(team: Team, colors: [u32; 3]) -> Node {
    let [base, shade, steel] = colors;
    let palette = Palette { base, shade, steel };
    let shared = assemblies();
    let mut root = Node::group(VehicleKind::Humvee.as_str());
    let mut hull = Node::group(part::HULL);
    hull.scale.z = HUMVEE_BODY_LENGTH_SCALE;
    let mut track_group = Node::group(part::TRACK_GROUP);
    // Wheels retain their circular section while the shell and wheelbase lengthen.
    track_group.scale.z = 1.0 / HUMVEE_BODY_LENGTH_SCALE;
    for side in [-1.0, 1.0] {
        for z in [REAR_AXLE, FRONT_AXLE] {
            // The left wheels are the right ones turned half a revolution.
            let mut wheel = Node::group("");
            if side < 0.0 {
                wheel.rotation = glam::DQuat::from_rotation_y(std::f64::consts::PI);
            }
            wheel.children.extend(palette.parts(&shared.wheel));
            put(
                &mut track_group,
                wheel,
                side * WHEEL_X,
                AXLE_Y,
                z * HUMVEE_BODY_LENGTH_SCALE,
            );
        }
    }
    hull.children.push(track_group);
    hull.children.extend(palette.parts(&shared.body));
    hull.children.push(shadowed(
        Arc::new(hood_insignia(team)),
        material(MARKING_WHITE, 0.0, MATTE_ROUGHNESS),
    ));
    root.children.push(hull);
    root.children.push(turret(&palette));
    root
}

/// Shield ring, sight and launcher; the empty `muzzle` marker sits at the tube
/// mouth.
fn turret(palette: &Palette) -> Node {
    let shared = assemblies();
    let mut turret = Node::group(part::TURRET);
    turret.children.extend(palette.parts(&shared.turret));
    let mut barrel = Node::group(part::BARREL);
    barrel.children.extend(palette.parts(&shared.barrel));
    put(
        &mut barrel,
        Node::group(part::MUZZLE),
        0.0,
        LAUNCHER_Y,
        MUZZLE_Z,
    );
    turret.children.push(barrel);
    turret
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use glam::DMat4;

    use super::*;
    use crate::models::{Team, tank_model};

    /// Triangles allowed for a whole Humvee: body, four wheels, shields, launcher.
    const TRIANGLE_BUDGET: usize = 15_000;
    /// Distinct meshes: one per livery finish and assembly, one wheel for all four.
    const MESH_BUDGET: usize = 24;

    fn meshes(model: &Node) -> (usize, Vec<*const Mesh>) {
        let mut triangles = 0;
        let mut meshes = Vec::new();
        model.traverse(DMat4::IDENTITY, &mut |part, _| {
            if let Some(drawable) = &part.drawable {
                triangles += drawable.mesh.triangle_count();
                meshes.push(Arc::as_ptr(&drawable.mesh));
            }
        });
        (triangles, meshes)
    }

    #[test]
    fn humvee_stays_within_its_triangle_and_mesh_budget() {
        let (triangles, meshes) = meshes(&tank_model(VehicleKind::Humvee, Team::Blue));
        assert!(triangles < TRIANGLE_BUDGET, "{triangles} triangles");
        let distinct: HashSet<_> = meshes.into_iter().collect();
        assert!(distinct.len() <= MESH_BUDGET, "{} meshes", distinct.len());
    }

    /// The simulation measures every vehicle's hit box and muzzle from its blue
    /// model, so a red Humvee must draw the same parts in the same places: only the
    /// hood insignia's shape and the paint material may tell the teams apart.
    #[test]
    fn teams_differ_only_in_paint_and_insignia() {
        let parts = |team| {
            let mut parts = Vec::new();
            tank_model(VehicleKind::Humvee, team).traverse(DMat4::IDENTITY, &mut |part, world| {
                if let Some(drawn) = &part.drawable {
                    parts.push((world, drawn.clone()));
                }
            });
            parts
        };
        let (blue, red) = (parts(Team::Blue), parts(Team::Red));
        assert_eq!(blue.len(), red.len(), "a part is drawn for one team only");
        let insignia = |team| hood_insignia(team).triangle_count();
        let (mut reshaped, mut paints) = (0, Vec::new());
        for ((place, b), (red_place, r)) in blue.iter().zip(&red) {
            assert_eq!(place, red_place, "a part moved for one team");
            if b.mesh != r.mesh || b.instances != r.instances {
                assert_eq!(
                    (b.mesh.triangle_count(), r.mesh.triangle_count()),
                    (insignia(Team::Blue), insignia(Team::Red)),
                    "only the insignia changes shape"
                );
                reshaped += 1;
            }
            if b.material != r.material && !paints.contains(&(&b.material, &r.material)) {
                paints.push((&b.material, &r.material));
            }
        }
        assert_eq!(reshaped, 1, "only the insignia changes shape");
        assert_eq!(paints.len(), 1, "only the paint material differs");
    }
}

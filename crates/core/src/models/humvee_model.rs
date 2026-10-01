//! The wheeled launcher vehicle: an up-armored four-door HMMWV with a TOW
//! launcher on its roof ring (originally a port of `humvee-model.ts`).
//!
//! Same part names as the tanks: `hull` (stretched along z), `track-group` (the
//! wheels, un-stretched so they stay round), `turret`, `barrel` and an empty
//! `muzzle` marker at the launcher mouth.
//!
//! Each assembly is merged once into one shared mesh per paint role (`kit`), so
//! the body, a wheel and the launcher cost a few meshes each; every team and the
//! wreck reuse the same geometry with their own palette.

use std::sync::{Arc, OnceLock};

use glam::DVec3;

use super::model_primitives::{material, paint, put, shadowed};
use super::tank_model::{DARK, STEEL, WRECK_PAINT, WRECK_STEEL, shade_of};
use super::tank_surfaces::{Finish, apply_tank_surface, vehicle_paint};
use super::{Team, VehicleKind, part};
use crate::geometry::Mesh;
use crate::scene::{Material, Node};

mod body;
mod kit;
mod turret;
mod wheel;

use kit::Role;

/// The body shell and wheelbase are stretched by this factor along z.
pub const HUMVEE_BODY_LENGTH_SCALE: f64 = 1.16;
/// Height of the roof launcher tube's axis.
const LAUNCHER_Y: f64 = 2.13;
/// Wheel centres: track half-width, axle height and (unstretched) axle stations.
const WHEEL_X: f64 = 1.0;
const AXLE_Y: f64 = 0.25;
const AXLES: [f64; 2] = [-1.32, 1.3];
/// The launcher mouth, where missiles appear.
const MUZZLE_Z: f64 = 1.7;

const WRECK_RUBBER: u32 = 0x1d252b;
/// Tire rubber: dark grey, light enough for the sidewall's shape to show.
const RUBBER: u32 = 0x25292a;
const WRECK_GLASS: u32 = 0x1a2328;
const GLASS: u32 = 0x29444b;
const GAP: u32 = 0x101416;
const CANVAS: u32 = 0x4f4a38;
const HEADLAMP: u32 = 0xe5ddbc;
const AMBER: u32 = 0xdf923b;
const RED_LAMP: u32 = 0x931e16;
/// Rubber, seals and canvas are matte; glass and lenses are glossy.
const MATTE_ROUGHNESS: f64 = 0.92;
const GLASS_METALNESS: f64 = 0.5;
const GLASS_ROUGHNESS: f64 = 0.08;

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

/// Paint for one Humvee: team or burnt colors.
struct Palette {
    base: u32,
    shade: u32,
    steel: u32,
    rubber: u32,
    glass: u32,
    wreck: bool,
}

impl Palette {
    /// Lamps go dark and canvas burns on a wreck.
    fn lit(&self, color: u32) -> u32 {
        if self.wreck { self.shade } else { color }
    }

    fn material(&self, role: Role) -> Arc<Material> {
        let glossy = |color| material(color, GLASS_METALNESS, GLASS_ROUGHNESS);
        match role {
            Role::Paint => paint(self.base),
            Role::Shade => paint(self.shade),
            Role::Steel => paint(self.steel),
            Role::Rubber => material(self.rubber, 0.0, MATTE_ROUGHNESS),
            Role::Gap => material(GAP, 0.0, MATTE_ROUGHNESS),
            Role::Canvas => material(self.lit(CANVAS), 0.0, MATTE_ROUGHNESS),
            Role::Glass => glossy(self.glass),
            Role::Headlamp => glossy(self.lit(HEADLAMP)),
            Role::Amber => glossy(self.lit(AMBER)),
            Role::Red => glossy(self.lit(RED_LAMP)),
        }
    }

    /// One shadowed part per role of an assembly.
    fn parts(&self, meshes: &[(Role, Arc<Mesh>)]) -> impl Iterator<Item = Node> {
        meshes
            .iter()
            .map(|(role, mesh)| shadowed(mesh.clone(), self.material(*role)))
    }
}

/// `humveeModel(team, wreck)`.
pub fn humvee_model(team: Team, wreck: bool) -> Node {
    let base = if wreck {
        WRECK_PAINT
    } else {
        vehicle_paint(team)
    };
    let palette = Palette {
        base,
        shade: if wreck { DARK } else { shade_of(base) },
        steel: if wreck { WRECK_STEEL } else { STEEL },
        rubber: if wreck { WRECK_RUBBER } else { RUBBER },
        glass: if wreck { WRECK_GLASS } else { GLASS },
        wreck,
    };
    let shared = assemblies();
    let mut root = Node::group(VehicleKind::Humvee.name());
    let mut hull = Node::group(part::HULL);
    hull.scale.z = HUMVEE_BODY_LENGTH_SCALE;
    let mut track_group = Node::group(part::TRACK_GROUP);
    // Wheels retain their circular section while the shell and wheelbase lengthen.
    track_group.scale.z = 1.0 / HUMVEE_BODY_LENGTH_SCALE;
    for side in [-1.0, 1.0] {
        for z in AXLES {
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
    root.children.push(hull);
    root.children.push(turret(&palette));
    root.scale = DVec3::splat(VehicleKind::Humvee.scale());
    let finish = if wreck {
        Finish::Wrecked
    } else {
        Finish::Fresh {
            steel: palette.steel,
        }
    };
    apply_tank_surface(
        &mut root,
        &[palette.base, palette.shade, palette.steel],
        finish,
    );
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

    /// Triangles allowed for a whole Humvee: body, four wheels, shields, launcher.
    const TRIANGLE_BUDGET: usize = 15_000;
    /// Distinct meshes: one per paint role and assembly, one wheel for all four.
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
        let (triangles, meshes) = meshes(&humvee_model(Team::Blue, false));
        assert!(triangles < TRIANGLE_BUDGET, "{triangles} triangles");
        let distinct: HashSet<_> = meshes.into_iter().collect();
        assert!(distinct.len() <= MESH_BUDGET, "{} meshes", distinct.len());
    }

    #[test]
    fn teams_and_wrecks_share_one_geometry() {
        let blue = meshes(&humvee_model(Team::Blue, false));
        assert_eq!(blue, meshes(&humvee_model(Team::Red, false)));
        assert_eq!(blue, meshes(&humvee_model(Team::Red, true)));
    }
}

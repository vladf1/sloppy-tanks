//! Port of the model half of `flags.ts`: team flag poles and cloth behind each
//! team's spawn line. The cloth waves in the vertex shader
//! ([`super::effects_props::FLAG_CLOTH`]); the wind state machine that feeds its
//! uniforms (random gusts easing every 3–7 s) belongs to presentation.

use std::sync::Arc;

use glam::{DMat4, DVec3};

use super::batching::paint_mesh;
use super::effects_props::FLAG_CLOTH;
use super::model_primitives::{TEAM_COLORS, cylinder_part};
use super::{Team, model_primitives::Cache};
use crate::geometry::plane_geometry_segments;
use crate::scene::{Color, Effect, Instance, Material, Node, Side};
use crate::sim::arena::{BOUNDARY_THICKNESS, spawn_positions};
use crate::sim::data::ARENA;

/// Node names of the flag instanced meshes.
pub const FLAG_POLE: &str = "flag-pole";
pub const FLAG_CLOTH_NODE: &str = "flag-cloth";

/// Flags stand on the middle of the boundary wall on each team's side, one per spawn row.
const FLAG_X: f64 = ARENA + BOUNDARY_THICKNESS / 2.0;
const POLE_COLOR: u32 = 0x59656a;
const POLE_Y: f64 = 2.4;
const CLOTH_Y: f64 = 4.6;
/// Radius of a sphere that contains every allowed gust of the deformed cloth
/// around its instance origin (the TypeScript's fixed bounding sphere).
pub const FLAG_CLOTH_BOUNDS_RADIUS: f64 = 2.0;

/// The x and z of each of a team's flags (TS `spawnPositions(team)` with x = ±`FLAG_X`).
pub fn flag_positions(team: Team) -> [(f64, f64); 5] {
    let x = if team == Team::Blue { -FLAG_X } else { FLAG_X };
    spawn_positions(team, 1.0).map(|spawn| (x, spawn.z))
}

/// A flag's ripple phase (TS attribute `flagPhase`): the cloth shader derives it from
/// the instance translation with this formula.
pub fn flag_phase(x: f64, z: f64) -> f64 {
    z * 0.12 + x * 0.04
}

static CLOTH_MATERIALS: Cache<Team, Material> = Cache::new();

fn cloth_material(team: Team) -> Arc<Material> {
    CLOTH_MATERIALS.get_or_insert(team, || Material {
        color: Color(TEAM_COLORS[team.index()]),
        roughness: 1.0,
        side: Side::Double,
        effect: Effect::Custom {
            name: FLAG_CLOTH,
            params: Vec::new(),
        },
        ..Material::default()
    })
}

fn instances(y: f64, positions: impl IntoIterator<Item = (f64, f64)>) -> Vec<Instance> {
    positions
        .into_iter()
        .map(|(x, z)| Instance {
            matrix: DMat4::from_translation(DVec3::new(x, y, z)),
            color: None,
        })
        .collect()
}

/// The `Flags` group: one instanced pole mesh (team 0 then team 1) and one
/// instanced cloth mesh per team. The cloth is not frustum culled here; a
/// renderer that culls must use [`FLAG_CLOTH_BOUNDS_RADIUS`] around each instance.
pub fn flags_model() -> Node {
    let mut group = Node::group("flags");
    let mut pole = cylinder_part(0.055, 4.8, POLE_COLOR, 8);
    paint_mesh(&mut pole);
    pole.name = FLAG_POLE.to_string();
    if let Some(drawable) = &mut pole.drawable {
        drawable.instances = Some(instances(
            POLE_Y,
            flag_positions(Team::Blue)
                .into_iter()
                .chain(flag_positions(Team::Red)),
        ));
    }
    group.children.push(pole);
    let plane = Arc::new(plane_geometry_segments(1.4, 0.9, 16, 6));
    for team in [Team::Blue, Team::Red] {
        let mut cloth = Node::mesh(plane.clone(), cloth_material(team));
        cloth.name = FLAG_CLOTH_NODE.to_string();
        if let Some(drawable) = &mut cloth.drawable {
            drawable.cast_shadow = true;
            drawable.receive_shadow = true;
            drawable.frustum_culled = false;
            drawable.instances = Some(instances(CLOTH_Y, flag_positions(team)));
        }
        group.children.push(cloth);
    }
    group
}

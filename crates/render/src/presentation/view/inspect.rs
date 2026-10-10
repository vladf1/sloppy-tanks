//! Read-only diagnostics of the presentation for the browser checks and fixtures
//! (`Game.debug_view_json`): what each entity's view shows after the last frame.
//! Nothing here changes drawing state.

use glam::{Mat4, Quat, Vec3};

use super::{FragmentLook, Presentation};
use crate::gpu::InstanceState;

/// The reticle: which of its three rings shows, and its placement.
#[derive(Clone, Debug, Default)]
pub struct ReticleInspection {
    pub visible: bool,
    pub confirmed: bool,
    pub ready: bool,
    pub reloading: bool,
    pub scale: f32,
    pub position: Vec3,
}

#[derive(Clone, Debug)]
pub struct TankInspection {
    pub id: u32,
    pub shown: bool,
    pub bar_shown: bool,
    /// Rank chevrons shown on the health bar.
    pub chevrons: usize,
    pub position: Vec3,
    pub bar_position: Vec3,
    /// Render-only suspension angles (radians).
    pub pitch: f64,
    pub roll: f64,
    /// Distance between the hull's and the turret's up vectors: 0 when the turret
    /// rides the hull's tilt.
    pub turret_tilt_error: f32,
    /// The turret's forward in hull space; `(sin, 0, cos)` of the aim relative to
    /// the heading when the turret turns independently of the hull.
    pub turret_forward_in_hull: Vec3,
}

#[derive(Clone, Debug)]
pub struct CoverInspection {
    pub id: u32,
    pub shown: bool,
    pub stage: u32,
    /// A joint of the combined static-cover model rather than its own instance.
    pub combined: bool,
    pub model_key: String,
    /// Trees: whether the crown and the cut stump surface show.
    pub crown: Option<bool>,
    pub cut: Option<bool>,
    /// Joints drawn in its model instance (a combined cover counts its own span).
    pub visible_joints: usize,
}

#[derive(Clone, Debug)]
pub struct PickupInspection {
    pub id: u32,
    pub base_shown: bool,
    pub gem: bool,
    pub ring: bool,
    pub ring_dim: bool,
    pub refill: bool,
    /// Refill arc segments drawn.
    pub segments: f32,
}

#[derive(Clone, Debug)]
pub struct FragmentInspection {
    pub id: u32,
    /// "piece", "wreck" or "owned" (timber members and falling crowns).
    pub look: &'static str,
    pub shown: bool,
    pub opacity: f32,
    pub position: Vec3,
    pub scale: Vec3,
    /// Scars on a loose timber member (`None` for other debris).
    pub timber_marks: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct ViewInspection {
    pub reticle: ReticleInspection,
    /// The theme whose scenery shows.
    pub theme: Option<&'static str>,
    pub player_ring: bool,
    pub tanks: Vec<TankInspection>,
    pub covers: Vec<CoverInspection>,
    pub pickups: Vec<PickupInspection>,
    pub fragments: Vec<FragmentInspection>,
    pub mines: usize,
    pub branches: usize,
    pub pickup_effects: usize,
    pub laser_lenses: usize,
    pub laser_cores: usize,
    pub laser_beams: usize,
}

fn joint_shown(state: &InstanceState, joint: usize) -> bool {
    state.node_visible.get(joint).copied().unwrap_or(false)
}

fn rotation(matrix: Option<Mat4>) -> Quat {
    matrix.map_or(Quat::IDENTITY, |matrix| {
        matrix.to_scale_rotation_translation().1
    })
}

impl Presentation {
    /// Every entity view's state after the last frame, sorted by id.
    pub fn inspect(&self) -> ViewInspection {
        let renderer = &self.renderer;
        let mut inspection = ViewInspection::default();
        if let Some(state) = renderer.instance_state(self.reticle.instance) {
            let (scale, _, position) = state.world.to_scale_rotation_translation();
            inspection.reticle = ReticleInspection {
                visible: state.visible,
                confirmed: joint_shown(&state, self.reticle.confirmed),
                ready: joint_shown(&state, self.reticle.ready),
                reloading: joint_shown(&state, self.reticle.reloading),
                scale: scale.x,
                position,
            };
        }
        inspection.player_ring = renderer
            .instance_state(self.player_ring)
            .is_some_and(|state| state.visible);
        for (&id, view) in &self.tanks {
            let (Some(model), Some(bar)) = (
                renderer.instance_state(view.instance),
                renderer.instance_state(view.bar),
            ) else {
                continue;
            };
            let library = &self.library;
            let chevrons = library.bars.get(&view.team).map_or(0, |bar_model| {
                bar_model
                    .ranks
                    .iter()
                    .filter(|&&joint| joint_shown(&bar, joint))
                    .count()
            });
            let (hull, turret) = library.tanks.get(&(view.kind, view.team)).map_or(
                (Quat::IDENTITY, Quat::IDENTITY),
                |tank| {
                    (
                        rotation(model.overrides.get(tank.hull.index).copied().flatten()),
                        rotation(model.overrides.get(tank.turret.index).copied().flatten()),
                    )
                },
            );
            let suspension = view.suspension.unwrap_or_default();
            inspection.tanks.push(TankInspection {
                id,
                shown: model.visible && view.world.w_axis.w != 0.0,
                bar_shown: bar.visible,
                chevrons,
                position: model.world.w_axis.truncate(),
                bar_position: bar.world.w_axis.truncate(),
                pitch: suspension.pitch.angle,
                roll: suspension.roll.angle,
                turret_tilt_error: (hull * Vec3::Y).distance(turret * Vec3::Y),
                turret_forward_in_hull: hull.inverse() * turret * Vec3::Z,
            });
        }
        for (&id, view) in &self.covers {
            let Some(state) = renderer.instance_state(view.instance) else {
                continue;
            };
            let shown = match view.joint {
                Some(joint) => state.visible && joint_shown(&state, joint),
                None => state.visible,
            };
            let span = match view.joint {
                Some(joint) => {
                    let nodes = self
                        .cover_models
                        .get(&view.key)
                        .map_or(&[][..], |entry| renderer.model_nodes(entry.model));
                    let end = nodes[joint + 1..]
                        .iter()
                        .position(|node| node.name.starts_with(super::COMBINED_JOINT))
                        .map_or(nodes.len(), |offset| joint + 1 + offset);
                    joint..end
                }
                None => 0..state.node_visible.len(),
            };
            inspection.covers.push(CoverInspection {
                id,
                shown,
                stage: view.stage,
                combined: view.joint.is_some(),
                model_key: view.key.clone(),
                crown: view
                    .tree
                    .as_ref()
                    .map(|tree| joint_shown(&state, tree.crown)),
                cut: view.tree.as_ref().map(|tree| joint_shown(&state, tree.cut)),
                visible_joints: span.filter(|&joint| joint_shown(&state, joint)).count(),
            });
        }
        for (&id, view) in &self.pickups {
            let (Some(base), Some(gem), Some(model)) = (
                renderer.instance_state(view.base),
                renderer.instance_state(view.gem),
                self.library.pickups.get(&view.kind),
            ) else {
                continue;
            };
            inspection.pickups.push(PickupInspection {
                id,
                base_shown: base.visible,
                gem: gem.visible,
                ring: joint_shown(&base, model.ring),
                ring_dim: joint_shown(&base, model.ring_dim),
                refill: joint_shown(&base, model.refill),
                segments: base.data[0],
            });
        }
        for (&id, view) in &self.fragments {
            let Some(state) = renderer.instance_state(view.instance) else {
                continue;
            };
            let (scale, _, position) = state.world.to_scale_rotation_translation();
            inspection.fragments.push(FragmentInspection {
                id,
                look: match view.look {
                    FragmentLook::Piece => "piece",
                    FragmentLook::Wreck(..) => "wreck",
                    FragmentLook::Owned(..) => "owned",
                },
                shown: state.visible,
                opacity: state.opacity,
                position,
                scale,
                timber_marks: view.timber.as_ref().map(|part| part.marks.len()),
            });
        }
        inspection.tanks.sort_by_key(|tank| tank.id);
        inspection.covers.sort_by_key(|cover| cover.id);
        inspection.pickups.sort_by_key(|pickup| pickup.id);
        inspection.fragments.sort_by_key(|fragment| fragment.id);
        inspection.theme = self.theme.map(|theme| theme.name());
        inspection.mines = self.mines.len();
        inspection.branches = self.branches.len();
        inspection.pickup_effects = self.pickup_effects.len();
        let laser = &self.effects.systems.laser;
        inspection.laser_lenses = laser.lens.len();
        inspection.laser_cores = laser.core.len();
        inspection.laser_beams = laser.beams();
        inspection
    }
}

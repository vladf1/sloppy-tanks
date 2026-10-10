//! Read-only diagnostics of placed instances for the browser checks and fixtures
//! (`Game.debug_view_json`): what an instance will draw, without touching the GPU.

use glam::Mat4;

use super::{InstanceId, Renderer};

/// An instance's current placement and visibility.
#[derive(Clone, Debug, PartialEq)]
pub struct InstanceState {
    pub visible: bool,
    pub opacity: f32,
    pub data: [f32; 4],
    pub world: Mat4,
    /// Per joint: whether it draws.
    pub node_visible: Vec<bool>,
    /// Per joint: the local transform replacing its rest pose, if any.
    pub overrides: Vec<Option<Mat4>>,
}

impl Renderer {
    /// The current water's settings, if the scene has water.
    pub fn water_settings(&self) -> Option<&super::WaterSettings> {
        self.water.as_ref().map(|water| &water.settings)
    }

    /// The state of a live instance, or `None` once it was removed.
    pub fn instance_state(&self, id: InstanceId) -> Option<InstanceState> {
        let instance = self.instances.get(id.index, id.generation)?;
        Some(InstanceState {
            visible: instance.visible,
            opacity: instance.opacity,
            data: instance.data,
            world: instance.world,
            node_visible: instance.node_visible.clone(),
            overrides: instance.overrides.clone(),
        })
    }
}

//! Read-only diagnostics of placed instances for the browser checks and fixtures
//! (`Game.debug_view_json`): what an instance will draw, without touching the GPU, and
//! a digest of what a frame drew.

use glam::Mat4;

use super::{InstanceId, Renderer};
use crate::draw_list::Draw;

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

    /// A hash of what the last frame handed the GPU beyond the effect pools' records:
    /// its instance records (static, dynamic and merged), every view's draw list, the
    /// merged shadow draws and bases, and the view, output, occlusion and water
    /// uniforms. Manual before/after evidence (`render_frame_benchmark`) that a
    /// change kept the picture; class numbers are part of it, so a change in the order
    /// classes are made changes it too.
    pub fn frame_digest(&self) -> u64 {
        use std::hash::{DefaultHasher, Hasher};
        let mut hash = DefaultHasher::new();
        let draws = |hash: &mut DefaultHasher, list: &[Draw]| {
            hash.write_usize(list.len());
            for draw in list {
                for value in [
                    draw.class,
                    draw.first_instance,
                    draw.instance_count,
                    draw.first_index,
                    draw.index_count,
                ] {
                    hash.write_u32(value);
                }
            }
        };
        for records in [
            &self.static_records,
            &self.builder.records,
            &self.merged_records,
        ] {
            hash.write_usize(records.len());
            hash.write(bytemuck::cast_slice(records));
        }
        for view in &self.views {
            draws(&mut hash, &view.opaque);
            draws(&mut hash, &view.transparent);
        }
        draws(&mut hash, &self.static_shadow_draws);
        for list in [&self.merged_draws, &self.static_merged_draws] {
            hash.write_usize(list.len());
            for draw in list {
                for value in [draw.model, draw.group, draw.first, draw.count] {
                    hash.write_u32(value);
                }
            }
        }
        hash.write(bytemuck::cast_slice(&self.shadow_bases));
        hash.write(bytemuck::cast_slice(&self.view_uniforms()));
        hash.write(bytemuck::bytes_of(&self.output_uniform()));
        if self.occlusion_on() {
            hash.write(bytemuck::bytes_of(&self.occlusion_uniform()));
        }
        if let Some(water) = self.water_uniform() {
            hash.write(bytemuck::bytes_of(&water));
        }
        hash.finish()
    }
}

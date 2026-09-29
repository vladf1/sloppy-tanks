//! `EffectsLab`: the runtime effects driven from `tools/effects-lab.ts`. The page
//! scripts a small scene (tanks driving, shots in flight, one event of every
//! kind) and feeds the same state and events to this renderer and to the former
//! Three.js effect classes for a side-by-side comparison.

use std::collections::HashMap;

use glam::{Mat4, Vec3};
use serde::Deserialize;
use sloppy_core::geometry::plane_geometry;
use sloppy_core::models::tank_model;
use sloppy_core::scene::{Material, Node};
use sloppy_core::sim::maps::GroundKind;
use sloppy_core::sim::render_state::{RenderShot, RenderTank};
use sloppy_core::sim::{MatchPhase, Point3, RenderState, SimEvent, Team, Vec2, VehicleKind};
use sloppy_render::camera::PerspectiveCamera;
use sloppy_render::effects::Effects;
use sloppy_render::gpu::{InstanceId, Lifetime, ModelId, Renderer, RendererOptions};
use wasm_bindgen::prelude::*;

/// Tanks rest 0.4 m below their body origin (`presentation.ts` `updateTanks`).
const HULL_DROP: f32 = 0.4;
const GROUND_SIZE: f64 = 160.0;
const GROUND_COLOR: u32 = 0x9a8260;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LabMatch {
    phase: MatchPhase,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LabTank {
    id: u32,
    kind: VehicleKind,
    team: Team,
    alive: bool,
    heading: f64,
    #[serde(default)]
    laser: f64,
    previous: Vec2,
    position: Point3,
    velocity: Point3,
}

/// The subset of the TS `RenderState` the effects read.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LabState {
    elapsed: f64,
    #[serde(rename = "match")]
    match_state: LabMatch,
    map_theme: String,
    #[serde(default)]
    map_floor: Option<GroundKind>,
    tanks: Vec<LabTank>,
    shots: Vec<RenderShot>,
}

#[wasm_bindgen]
pub struct EffectsLab {
    renderer: Renderer,
    effects: Effects,
    state: RenderState,
    models: HashMap<(VehicleKind, Team), (ModelId, Mat4)>,
    tanks: HashMap<u32, (InstanceId, Mat4)>,
}

fn js_error(message: impl Into<String>) -> JsValue {
    js_sys::Error::new(&message.into()).into()
}

#[wasm_bindgen]
impl EffectsLab {
    pub async fn create(
        canvas: web_sys::HtmlCanvasElement,
        asset_base: String,
    ) -> Result<EffectsLab, JsValue> {
        console_error_panic_hook::set_once();
        let mut renderer = Renderer::new(canvas, RendererOptions { asset_base })
            .await
            .map_err(js_error)?;
        let mut ground = plane_geometry(GROUND_SIZE, GROUND_SIZE);
        ground.rotate_x(-std::f64::consts::FRAC_PI_2);
        let mut node = Node::mesh(
            std::sync::Arc::new(ground),
            std::sync::Arc::new(Material::standard(GROUND_COLOR, 0.0, 1.0)),
        );
        if let Some(drawable) = &mut node.drawable {
            drawable.receive_shadow = true;
        }
        renderer.add_scenery(&node, Lifetime::Shared);
        let effects = Effects::new(&mut renderer);
        let state = RenderState {
            map_theme: "village".into(),
            ..RenderState::default()
        };
        Ok(EffectsLab {
            renderer,
            effects,
            state,
            models: HashMap::new(),
            tanks: HashMap::new(),
        })
    }

    pub fn set_seed(&mut self, seed: u32) {
        self.effects.set_seed(u64::from(seed));
    }

    /// Replace the effects' view of the match (TS `RenderState` subset, JSON).
    pub fn set_state(&mut self, json: &str) -> Result<(), JsValue> {
        let lab: LabState =
            serde_json::from_str(json).map_err(|error| js_error(error.to_string()))?;
        let state = &mut self.state;
        state.elapsed = lab.elapsed;
        state.match_state.phase = lab.match_state.phase;
        state.map_theme = lab.map_theme;
        state.map_floor = lab.map_floor;
        state.shots = lab.shots;
        state.tanks.clear();
        for tank in &lab.tanks {
            state.tanks.push(RenderTank {
                id: tank.id,
                kind: tank.kind,
                team: tank.team,
                alive: tank.alive,
                heading: tank.heading,
                laser: tank.laser,
                previous: tank.previous,
                position: tank.position,
                velocity: tank.velocity,
                ..RenderTank::default()
            });
            if !self.tanks.contains_key(&tank.id) {
                let (model, root) =
                    *self
                        .models
                        .entry((tank.kind, tank.team))
                        .or_insert_with(|| {
                            let node = tank_model(tank.kind, tank.team);
                            // Instances place the root; keep its own transform (the
                            // vehicle scale) to apply under that placement.
                            let root = node.local_matrix().as_mat4();
                            (self.renderer.add_model(&node, Lifetime::Shared), root)
                        });
                if let Some(instance) =
                    self.renderer
                        .add_instance(model, Mat4::IDENTITY, Lifetime::Shared)
                {
                    self.tanks.insert(tank.id, (instance, root));
                }
            }
        }
        Ok(())
    }

    /// One simulation event (TS `SimEvent` JSON); `player_hit` as in the game.
    pub fn event(&mut self, json: &str, player_hit: bool) -> Result<(), JsValue> {
        let event: SimEvent =
            serde_json::from_str(json).map_err(|error| js_error(error.to_string()))?;
        self.effects
            .event(&mut self.renderer, &self.state, &event, player_hit);
        Ok(())
    }

    pub fn reset(&mut self) {
        self.effects.reset(&mut self.renderer, &self.state);
    }

    /// Advance effects by `dt` and draw at `time` (seconds).
    pub fn frame(&mut self, alpha: f32, dt: f32, time: f64) -> Result<(), JsValue> {
        for tank in &self.state.tanks {
            let Some(&(instance, root)) = self.tanks.get(&tank.id) else {
                continue;
            };
            let a = f64::from(alpha);
            let x = tank.previous.x + (tank.position.x - tank.previous.x) * a;
            let z = tank.previous.z + (tank.position.z - tank.previous.z) * a;
            let world = Mat4::from_translation(Vec3::new(
                x as f32,
                tank.position.y as f32 - HULL_DROP,
                z as f32,
            )) * Mat4::from_rotation_y(tank.heading as f32)
                * root;
            self.renderer.set_transform(instance, world);
            self.renderer.set_visible(instance, tank.alive);
        }
        self.effects
            .update(&mut self.renderer, &self.state, alpha, dt, time);
        self.renderer.render(time as f32).map_err(js_error)
    }

    /// Compile up to `budget` pipelines; returns `[compiled, remaining]`.
    pub fn prepare_step(&mut self, budget: u32) -> Vec<u32> {
        self.effects.warm_up_samples(&mut self.renderer);
        let progress = self.renderer.prepare_step(budget);
        vec![progress.compiled, progress.remaining]
    }

    pub fn warm_up(&mut self) -> Result<(), JsValue> {
        self.renderer.warm_up().map_err(js_error)
    }

    pub fn textures_pending(&self) -> u32 {
        self.renderer.textures_pending() as u32
    }

    pub fn set_camera(&mut self, position: Vec<f32>, target: Vec<f32>) {
        let mut camera = PerspectiveCamera::new(43.0, 0.1, 320.0);
        camera.look_at(Vec3::from_slice(&position), Vec3::from_slice(&target));
        self.renderer.set_camera(camera);
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.renderer.resize(width, height);
    }

    /// Renderer counters, effect counts and per-pool instances as JSON.
    pub fn stats(&mut self) -> String {
        let s = self.renderer.stats();
        let e = self.effects.stats();
        let pools: Vec<String> = self
            .renderer
            .pool_summary()
            .iter()
            .map(|(label, count)| format!("[\"{label}\",{count}]"))
            .collect();
        format!(
            concat!(
                "{{\"drawCalls\":{},\"triangles\":{},\"pipelines\":{},\"latePipelines\":{},",
                "\"pools\":{},\"poolInstances\":{},\"gpuBytes\":{},\"effects\":{{",
                "\"particles\":{},\"blasts\":{},\"puffs\":{},\"blastRings\":{},\"trackMarks\":{},",
                "\"trackDust\":{},\"gravel\":{},\"quarryDust\":{},\"projectiles\":{},",
                "\"laserBeams\":{},\"pickupEffects\":{},\"instances\":{}}},\"poolList\":[{}]}}"
            ),
            s.draw_calls,
            s.triangles,
            s.pipelines,
            s.late_pipelines,
            s.pools,
            s.pool_instances,
            s.gpu_bytes,
            e.particles,
            e.blasts,
            e.puffs,
            e.blast_rings,
            e.track_marks,
            e.track_dust,
            e.gravel,
            e.quarry_dust,
            e.projectiles,
            e.laser_beams,
            e.pickup_effects,
            e.instances,
            pools.join(","),
        )
    }

    pub fn error(&self) -> Option<String> {
        self.renderer.error()
    }
}

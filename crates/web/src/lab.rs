//! `RenderLab`: the renderer driven from `tools/render-lab.ts` for calibration
//! against Three.js, warm-up, reset and resource checks.

use std::sync::Arc;

use glam::{Mat4, Vec2, Vec3};
use sloppy_core::models::tank_model;
use sloppy_core::scene::Node;
use sloppy_core::sim::{Team, VehicleKind};
use sloppy_render::camera::{PerspectiveCamera, ShadowCamera};
use sloppy_render::gpu::{
    Environment, Fog, InstanceId, Lifetime, ModelId, PointLight, Renderer, RendererOptions,
    SunShadow, WaterSettings,
};
use wasm_bindgen::prelude::*;

use crate::lab_scene::{SceneSpec, object_node, water_mesh};

#[wasm_bindgen]
pub struct RenderLab {
    renderer: Renderer,
    objects: Vec<(String, InstanceId)>,
    models: Vec<(InstanceId, ModelId)>,
}

fn js_error(message: impl Into<String>) -> JsValue {
    js_sys::Error::new(&message.into()).into()
}

#[wasm_bindgen]
impl RenderLab {
    /// Create the renderer on a canvas. `asset_base` prefixes texture paths.
    pub async fn create(
        canvas: web_sys::HtmlCanvasElement,
        asset_base: String,
    ) -> Result<RenderLab, JsValue> {
        console_error_panic_hook::set_once();
        let renderer = Renderer::new(canvas, RendererOptions { asset_base })
            .await
            .map_err(js_error)?;
        Ok(RenderLab {
            renderer,
            objects: Vec::new(),
            models: Vec::new(),
        })
    }

    /// Replace the scene: round resources are released first, like a new round.
    pub fn load_scene(&mut self, json: &str) -> Result<(), JsValue> {
        let spec: SceneSpec =
            serde_json::from_str(json).map_err(|error| js_error(error.to_string()))?;
        self.renderer.reset_round();
        self.objects.clear();
        self.models.clear();
        self.renderer.set_environment(Environment {
            background: spec.background,
            fog: spec.fog.as_ref().map(|fog| Fog {
                color: fog.color,
                near: fog.near,
                far: fog.far,
            }),
            sky_color: spec.hemisphere.sky,
            ground_color: spec.hemisphere.ground,
            hemisphere_intensity: spec.hemisphere.intensity,
            sun_color: spec.sun.color,
            sun_intensity: spec.sun.intensity,
            sun_position: Vec3::from(spec.sun.position),
            sun_target: Vec3::from(spec.sun.target),
            exposure: spec.exposure,
            reflections: spec.reflections,
        });
        let sun = Vec3::from(spec.sun.position);
        self.renderer.set_sun_shadow(SunShadow {
            enabled: spec.shadow.enabled,
            map_size: spec.shadow.map_size,
            camera: ShadowCamera::square(
                sun,
                Vec3::from(spec.sun.target),
                spec.shadow.half,
                spec.shadow.near,
                spec.shadow.depth,
            ),
            bias: spec.shadow.bias,
            normal_bias: spec.shadow.normal_bias,
            radius: 1.0,
            receiver_floor: f32::NEG_INFINITY,
        });
        self.renderer.set_point_light(
            0,
            spec.point_light.as_ref().map(|light| PointLight {
                position: Vec3::from(light.position),
                color: light.color,
                intensity: light.intensity,
                distance: light.distance,
                decay: light.decay,
            }),
        );
        let mut camera = PerspectiveCamera::new(spec.camera.fov, spec.camera.near, spec.camera.far);
        camera.look_at(
            Vec3::from(spec.camera.position),
            Vec3::from(spec.camera.target),
        );
        self.renderer.set_camera(camera);
        self.renderer
            .set_water(spec.water.as_ref().map(|water| WaterSettings {
                height: water.height,
                ..WaterSettings::harbor(Arc::new(water_mesh(water)))
            }));
        let mut scenery = Node::group("lab scenery");
        for object in &spec.objects {
            let node = object_node(object);
            if object.is_static {
                scenery.children.push(node);
                continue;
            }
            // The object node becomes the model root; its transform is the
            // instance's world matrix.
            let world = Mat4::from_scale_rotation_translation(
                node.scale.as_vec3(),
                node.rotation.as_quat(),
                node.position.as_vec3(),
            );
            let mut root = node;
            root.position = glam::DVec3::ZERO;
            root.rotation = glam::DQuat::IDENTITY;
            root.scale = glam::DVec3::ONE;
            let model = self.renderer.add_model(&root, Lifetime::Round);
            let copies = std::iter::once([0.0; 3]).chain(object.copies.iter().copied());
            for offset in copies {
                let placed = Mat4::from_translation(glam::DVec3::from(offset).as_vec3()) * world;
                if let Some(instance) = self.renderer.add_instance(model, placed, Lifetime::Round) {
                    self.objects.push((object.name.clone(), instance));
                    self.models.push((instance, model));
                }
            }
        }
        if !scenery.children.is_empty() {
            let id = self.renderer.add_scenery(&scenery, Lifetime::Round);
            self.objects.push(("scenery".into(), id));
        }
        Ok(())
    }

    /// RGBA8 pixels for `TextureSource::Generated(name)`, rows top to bottom.
    pub fn set_generated_texture(
        &mut self,
        name: &str,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> Result<(), JsValue> {
        let name: &'static str = Box::leak(name.to_owned().into_boxed_str());
        let image = sloppy_render::gpu::image_data(width, height, &rgba)?;
        self.renderer.set_generated_texture(name, image);
        Ok(())
    }

    pub fn textures_pending(&self) -> u32 {
        self.renderer.textures_pending() as u32
    }

    pub fn texture_failures(&self) -> Vec<String> {
        self.renderer.texture_failures().to_vec()
    }

    /// Create up to `budget` pipelines compiled in the background; returns
    /// `[compiled, remaining, compiling]` (`compiling`: still in background compiles).
    pub fn prepare_step(&mut self, budget: u32) -> Vec<u32> {
        let progress = self.renderer.prepare_step(budget);
        vec![progress.compiled, progress.remaining, progress.compiling]
    }

    pub fn warm_up(&mut self) -> Result<(), JsValue> {
        self.renderer.warm_up().map_err(js_error)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.renderer.resize(width, height);
    }

    pub fn set_camera(&mut self, position: Vec<f32>, target: Vec<f32>) {
        let mut camera = self.renderer.camera();
        camera.look_at(Vec3::from_slice(&position), Vec3::from_slice(&target));
        self.renderer.set_camera(camera);
    }

    /// Fade or restore a named object (below 1 it draws blended).
    pub fn set_opacity(&mut self, name: &str, opacity: f32) {
        for (object, id) in &self.objects {
            if object == name {
                self.renderer.set_opacity(*id, opacity);
            }
        }
    }

    /// Turn a joint of the `copy`-th instance of an object about Y by `yaw`
    /// radians on top of its rest pose (a turret traverse).
    pub fn pose_joint(&mut self, name: &str, copy: usize, joint: &str, yaw: f32) -> bool {
        let Some((_, id)) = self
            .objects
            .iter()
            .filter(|(object, _)| object == name)
            .nth(copy)
        else {
            return false;
        };
        let Some(&(_, model)) = self.models.iter().find(|(instance, _)| instance == id) else {
            return false;
        };
        let Some(node) = self.renderer.model_node(model, joint) else {
            return false;
        };
        let rest = self.renderer.model_nodes(model)[node].rest;
        self.renderer
            .set_node_transform(*id, node, Some(rest * Mat4::from_rotation_y(yaw)));
        true
    }

    /// Place the game's model of a vehicle (`kind` "scout", "balanced", "heavy" or
    /// "humvee"; `team` 0 blue, 1 red) as the object `name`, turned `yaw` about Y.
    /// It joins the scene before `prepare_step`, like the spec's objects.
    #[allow(clippy::too_many_arguments)]
    pub fn add_vehicle(
        &mut self,
        name: &str,
        kind: &str,
        team: u8,
        x: f32,
        y: f32,
        z: f32,
        yaw: f32,
    ) -> Result<(), JsValue> {
        let kind: VehicleKind = serde_json::from_value(serde_json::Value::from(kind))
            .map_err(|error| js_error(error.to_string()))?;
        // The model root's own transform (the chassis scale) moves to the instance,
        // as for the spec's objects.
        let mut root = (*tank_model(kind, Team::from_index(usize::from(team)))).clone();
        let world =
            Mat4::from_rotation_translation(glam::Quat::from_rotation_y(yaw), Vec3::new(x, y, z))
                * root.local_matrix().as_mat4();
        root.position = glam::DVec3::ZERO;
        root.rotation = glam::DQuat::IDENTITY;
        root.scale = glam::DVec3::ONE;
        let model = self.renderer.add_model(&root, Lifetime::Round);
        let instance = self
            .renderer
            .add_instance(model, world, Lifetime::Round)
            .ok_or_else(|| js_error("vehicle model"))?;
        self.objects.push((name.to_owned(), instance));
        self.models.push((instance, model));
        Ok(())
    }

    /// Replace the clear color without reloading the scene.
    pub fn set_background(&mut self, color: u32) {
        let mut environment = *self.renderer.environment();
        environment.background = color;
        self.renderer.set_environment(environment);
    }

    pub fn set_visible(&mut self, name: &str, visible: bool) {
        for (object, id) in &self.objects {
            if object == name {
                self.renderer.set_visible(*id, visible);
            }
        }
    }

    pub fn frame(&mut self, time: f32) -> Result<(), JsValue> {
        self.renderer.render(time).map_err(js_error)
    }

    /// Renderer counters as JSON.
    pub fn stats(&self) -> String {
        let s = self.renderer.stats();
        format!(
            concat!(
                "{{\"drawCalls\":{},\"triangles\":{},\"shadowDrawCalls\":{},\"reflectionDrawCalls\":{},",
                "\"shadowTriangles\":{},\"reflectionTriangles\":{},\"mainTriangles\":{},",
                "\"instanceRecords\":{},\"pipelines\":{},\"shaderModules\":{},\"latePipelines\":{},",
                "\"meshes\":{},\"materials\":{},\"textures\":{},\"texturesPending\":{},\"buffers\":{},",
                "\"models\":{},\"instances\":{},\"drawClasses\":{},\"gpuBytes\":{}}}"
            ),
            s.draw_calls,
            s.triangles,
            s.shadow_draw_calls,
            s.reflection_draw_calls,
            s.shadow_triangles,
            s.reflection_triangles,
            s.main_triangles,
            s.instance_records,
            s.pipelines,
            s.shader_modules,
            s.late_pipelines,
            s.meshes,
            s.materials,
            s.textures,
            s.textures_pending,
            s.buffers,
            s.models,
            s.instances,
            s.draw_classes,
            s.gpu_bytes,
        )
    }

    /// Ground point under a canvas pixel at `height`, or empty.
    pub fn pick(&self, x: f32, y: f32, height: f32) -> Vec<f32> {
        self.renderer
            .pick_ground(Vec2::new(x, y), height)
            .map_or(Vec::new(), |point| point.to_array().to_vec())
    }

    pub fn error(&self) -> Option<String> {
        self.renderer.error()
    }
}

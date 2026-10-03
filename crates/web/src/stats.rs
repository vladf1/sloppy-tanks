//! Renderer and presentation rows of Stats for nerds, shared by the page's game types.

use serde_json::{Map, Value, json};
use sloppy_render::presentation::Presentation;

/// Draw calls, triangles, GPU resources and entity views, with `Game.stats_json()`'s
/// field names.
pub fn presentation_stats(view: &Presentation) -> Map<String, Value> {
    let render = view.renderer.stats();
    let counts = view.stats();
    let value = json!({
        "drawCalls": render.draw_calls,
        "triangles": render.triangles,
        "shadowDrawCalls": render.shadow_draw_calls,
        "reflectionDrawCalls": render.reflection_draw_calls,
        "shadowTriangles": render.shadow_triangles,
        "reflectionTriangles": render.reflection_triangles,
        "mainTriangles": render.main_triangles,
        "instanceRecords": render.instance_records,
        "pipelines": render.pipelines,
        "shaderModules": render.shader_modules,
        "latePipelines": render.late_pipelines,
        "meshes": render.meshes,
        "unusedMeshes": render.unused_meshes,
        "materials": render.materials,
        "textures": render.textures,
        "texturesPending": render.textures_pending,
        "buffers": render.buffers,
        "models": render.models,
        "instances": render.instances,
        "drawClasses": render.draw_classes,
        "gpuBytes": render.gpu_bytes,
        "meshSlackBytes": render.mesh_slack_bytes,
        "view": {
            "tanks": counts.tanks,
            "covers": counts.covers,
            "coverModels": counts.cover_models,
            "fragments": counts.fragments,
            "pickups": counts.pickups,
            "mines": counts.mines,
            "pickupEffects": counts.pickup_effects,
            "branches": counts.branches,
        },
    });
    match value {
        Value::Object(fields) => fields,
        _ => Map::new(),
    }
}

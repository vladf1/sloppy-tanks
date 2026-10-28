//! Renderer, presentation and frame-timing rows of Stats for nerds, shared by the page's
//! game types and the render lab: `stats_json`'s records and the readings both panels'
//! rows are written from ([`crate::nerd_rows`]).

use serde_json::{Map, Value, json};
use sloppy_render::gpu::Renderer;
use sloppy_render::presentation::Presentation;

use crate::frame_clock::FrameTimes;
use crate::nerd_rows::{FrameStats, RendererStats};
use crate::page::Graphics;

/// Draw calls, triangles and GPU resources.
pub fn renderer_stats(renderer: &Renderer) -> Map<String, Value> {
    let render = renderer.stats();
    let value = json!({
        "graphicsApi": sloppy_render::GRAPHICS_API,
        "drawCalls": render.draw_calls,
        "triangles": render.triangles,
        "shadowDrawCalls": render.shadow_draw_calls,
        "reflectionDrawCalls": render.reflection_draw_calls,
        "shadowTriangles": render.shadow_triangles,
        "reflectionTriangles": render.reflection_triangles,
        "occlusionDrawCalls": render.occlusion_draw_calls,
        "occlusionTriangles": render.occlusion_triangles,
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
    });
    match value {
        Value::Object(fields) => fields,
        _ => Map::new(),
    }
}

/// The renderer rows, the entity views and the last frame's timings.
pub fn presentation_stats(view: &Presentation, times: &FrameTimes) -> Map<String, Value> {
    let mut stats = renderer_stats(&view.renderer);
    let counts = view.stats();
    stats.insert(
        "view".into(),
        json!({
            "tanks": counts.tanks,
            "covers": counts.covers,
            "coverModels": counts.cover_models,
            "fragments": counts.fragments,
            "pickups": counts.pickups,
            "mines": counts.mines,
            "pickupEffects": counts.pickup_effects,
            "branches": counts.branches,
            "libraryPending": counts.library_pending,
        }),
    );
    stats.insert("frameMs".into(), times.frame_ms.into());
    stats.insert("fps".into(), times.fps().into());
    stats.insert("simMs".into(), times.sim_ms.into());
    stats.insert("renderMs".into(), times.render_ms.into());
    stats
}

/// The frame clock's averages both panels lead with: every row comes from the engine's
/// clock, so the page and the desktop app measure frames alike.
pub fn frame_readings(times: &FrameTimes) -> FrameStats {
    FrameStats {
        fps: times.fps(),
        sim_ms: times.average_sim_ms,
        render_ms: times.average_render_ms,
    }
}

/// The renderer's counts, quality and resolution for both panels' Render rows.
pub fn renderer_readings(
    renderer: &Renderer,
    graphics: &Graphics,
    pixel_ratio: f64,
) -> RendererStats {
    let render = renderer.stats();
    RendererStats {
        graphics_api: sloppy_render::GRAPHICS_API.into(),
        quality: graphics.quality_stats(renderer, pixel_ratio),
        draw_calls: render.draw_calls.into(),
        triangles: render.triangles,
        shadow_triangles: render.shadow_triangles,
        reflection_triangles: render.reflection_triangles,
        main_triangles: render.main_triangles,
        meshes: render.meshes.into(),
        textures: render.textures.into(),
        gpu_bytes: render.gpu_bytes as f64,
        mesh_slack_bytes: render.mesh_slack_bytes as f64,
        pixel_ratio,
    }
}

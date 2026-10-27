//! Stats for nerds (`src/game/nerd-stats.ts`): the engine's `stats_json`, shown while
//! Settings allows it and N has opened it.

use egui::{Align2, RichText};

use super::UiState;
use super::theme::{TEXT_DIM, TEXT_LABEL};

/// The rows shown, in order: label and `stats_json` key.
const ROWS: &[(&str, &str)] = &[
    ("FPS", "fps"),
    ("Frame ms", "frameMs"),
    ("Sim ms", "simMs"),
    ("Render ms", "renderMs"),
    ("Draw calls", "drawCalls"),
    ("Shadow draws", "shadowDrawCalls"),
    ("Reflection draws", "reflectionDrawCalls"),
    ("Triangles", "triangles"),
    ("Pipelines", "pipelines"),
    ("Late pipelines", "latePipelines"),
    ("Meshes", "meshes"),
    ("Materials", "materials"),
    ("Textures", "textures"),
    ("GPU MB", "gpuBytes"),
    ("Bodies", "bodies"),
    ("Sleeping", "sleepingBodies"),
    ("Tanks alive", "tanksAlive"),
    ("Shots", "shots"),
    ("Fragments", "fragments"),
    ("Particles", "particles"),
    ("Pixel ratio", "pixelRatio"),
];

pub fn panel(context: &egui::Context, state: &UiState) {
    let Some(stats) = &state.stats else { return };
    egui::Area::new(egui::Id::unique("nerd stats"))
        .anchor(Align2::LEFT_TOP, [12.0, 64.0])
        .interactable(false)
        .show(context, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label(RichText::new("STATS FOR NERDS").strong().color(TEXT_LABEL));
                egui::Grid::new("nerd stats rows")
                    .num_columns(2)
                    .show(ui, |ui| {
                        let quality = &stats["quality"];
                        ui.label(RichText::new("Graphics").color(TEXT_DIM));
                        ui.label(format!(
                            "{} · {} · scale {:.2}",
                            stats["graphicsApi"].as_str().unwrap_or("?"),
                            quality["tier"].as_str().unwrap_or("?"),
                            quality["scale"].as_f64().unwrap_or(1.0)
                        ));
                        ui.end_row();
                        if let Some(gpu) = quality["gpuMs"].as_f64() {
                            ui.label(RichText::new("GPU ms").color(TEXT_DIM));
                            ui.label(format!("{gpu:.2}"));
                            ui.end_row();
                        }
                        for (label, key) in ROWS {
                            let Some(value) = stats[key].as_f64() else {
                                continue;
                            };
                            ui.label(RichText::new(*label).color(TEXT_DIM));
                            ui.label(if *key == "gpuBytes" {
                                format!("{:.1}", value / 1_048_576.0)
                            } else if value.fract() == 0.0 {
                                format!("{value}")
                            } else {
                                format!("{value:.2}")
                            });
                            ui.end_row();
                        }
                    });
            });
        });
}

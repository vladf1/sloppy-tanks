//! The pause menu, the results (`round-recap.ts`) and the Settings dialog
//! (`settings-dialog.ts`).

use egui::{Align2, RichText};

use super::theme::{GOLD, RED, TEXT_DIM, TEXT_LABEL, display_font, team_color};
use super::{Action, UiState, View};
use crate::preferences::Preferences;

fn centered_window(title: &str) -> egui::Window<'_> {
    egui::Window::new(title)
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
}

pub fn pause(
    context: &egui::Context,
    state: &mut UiState,
    _view: &View,
    actions: &mut Vec<Action>,
) {
    let Some(hud) = &state.hud else { return };
    centered_window("pause")
        .fixed_size([420.0, 0.0])
        .show(context, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("PAUSED").font(display_font(40.0)).color(GOLD));
                ui.label(
                    RichText::new(format!("{} · {}", hud.map_name, hud.difficulty)).color(TEXT_DIM),
                );
                ui.label(format!(
                    "{} · Kills {} · Deaths {}",
                    hud.human.vehicle_name, hud.human.kills, hud.human.deaths
                ));
                ui.add_space(8.0);
                if ui
                    .button(RichText::new("RESUME").font(display_font(24.0)))
                    .clicked()
                {
                    actions.push(Action::Resume);
                }
                if ui.button("SETTINGS").clicked() {
                    actions.push(Action::OpenSettings);
                }
                if ui.button("END BATTLE").clicked() {
                    actions.push(Action::EndBattle);
                }
                ui.add_space(8.0);
                ui.label(RichText::new("CONTROLS").color(TEXT_LABEL));
                egui::Grid::new("controls").show(ui, |ui| {
                    for (keys, what) in CONTROLS {
                        ui.label(RichText::new(*keys).strong());
                        ui.label(RichText::new(*what).color(TEXT_DIM));
                        ui.end_row();
                    }
                });
            });
        });
}

/// The keyboard reference in the pause menu.
pub const CONTROLS: &[(&str, &str)] = &[
    ("W A S D / arrows", "Drive"),
    ("Mouse", "Aim"),
    ("Left button / Space", "Fire"),
    ("Right button", "Drop mine"),
    ("1-5, Q / E, wheel", "Ammunition"),
    ("Shift + wheel", "Zoom"),
    ("V", "First person"),
    ("F11", "Fullscreen"),
    ("Esc", "Pause"),
];

pub fn results(
    context: &egui::Context,
    state: &mut UiState,
    _view: &View,
    actions: &mut Vec<Action>,
) {
    let Some(hud) = &state.hud else { return };
    let match_state = &hud.match_state;
    let solo = hud.game_mode == "solo";
    let title = match (solo, match_state.winner) {
        (true, _) if hud.human.alive => "SURVIVED",
        (true, _) => "TANK DESTROYED",
        (false, Some(team)) if team == hud.human_team => "VICTORY",
        (false, Some(_)) => "DEFEAT",
        (false, None) => "DRAW",
    };
    centered_window("results")
        .fixed_size([520.0, 0.0])
        .show(context, |ui| {
            ui.vertical_centered(|ui| {
                let color = if title == "DEFEAT" || title == "TANK DESTROYED" {
                    RED
                } else {
                    GOLD
                };
                ui.label(RichText::new(title).font(display_font(48.0)).color(color));
                if !solo {
                    ui.label(
                        RichText::new(format!(
                            "{} — {}",
                            match_state.scores[0], match_state.scores[1]
                        ))
                        .font(display_font(28.0)),
                    );
                }
                if let Some(recap) = &hud.recap {
                    for feat in &recap.feats {
                        ui.label(RichText::new(&feat.title).color(GOLD));
                        ui.label(RichText::new(&feat.detail).size(11.0).color(TEXT_DIM));
                    }
                    let stat = |key: &str| recap.stats.get(key).copied().unwrap_or(0.0);
                    ui.label(format!(
                        "Kills {} · Damage {:.0} · Accuracy {:.0}%",
                        stat("kills"),
                        stat("damage"),
                        if recap.shots > 0.0 {
                            recap.direct_hits / recap.shots * 100.0
                        } else {
                            0.0
                        }
                    ));
                    if !recap.improved.is_empty() {
                        ui.label(RichText::new("New personal best!").color(GOLD));
                    }
                }
                egui::Grid::new("scoreboard").striped(true).show(ui, |ui| {
                    ui.label(RichText::new("TANK").color(TEXT_LABEL));
                    ui.label(RichText::new("K").color(TEXT_LABEL));
                    ui.label(RichText::new("D").color(TEXT_LABEL));
                    ui.end_row();
                    let mut rows = hud.scoreboard.clone();
                    rows.sort_by(|a, b| b.kills.total_cmp(&a.kills));
                    for row in &rows {
                        let name = RichText::new(&row.name).color(team_color(row.team));
                        ui.label(if row.id == hud.human.id {
                            name.strong()
                        } else {
                            name
                        });
                        ui.label(format!("{}", row.kills));
                        ui.label(format!("{}", row.deaths));
                        ui.end_row();
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .button(RichText::new("PLAY AGAIN").font(display_font(24.0)))
                        .clicked()
                    {
                        actions.push(Action::Start);
                    }
                    if ui
                        .button(RichText::new("BATTLE SETUP").font(display_font(24.0)))
                        .clicked()
                    {
                        actions.push(Action::BattleSetup);
                    }
                });
            });
        });
}

/// Graphics choices: value, label.
const GRAPHICS: [(&str, &str); 4] = [
    ("auto", "Auto"),
    ("high", "High"),
    ("medium", "Medium"),
    ("low", "Low"),
];

pub fn settings(
    context: &egui::Context,
    state: &mut UiState,
    _view: &View,
    actions: &mut Vec<Action>,
) {
    let Some(draft) = &mut state.settings_draft else {
        return;
    };
    let mut close = None;
    centered_window("settings")
        .fixed_size([380.0, 0.0])
        .show(context, |ui| {
            ui.label(
                RichText::new("SETTINGS")
                    .font(display_font(28.0))
                    .color(GOLD),
            );
            ui.add(egui::Slider::new(&mut draft.volume, 0.0..=1.0).text("Sound"));
            ui.horizontal(|ui| {
                ui.label("Graphics");
                for (value, label) in GRAPHICS {
                    if ui
                        .selectable_label(draft.graphics == value, label)
                        .clicked()
                    {
                        draft.graphics = value.into();
                    }
                }
            });
            {
                ui.add(egui::Slider::new(&mut draft.tank_speed, 0.5..=2.0).text("Tank speed"));
                ui.add(
                    egui::Slider::new(&mut draft.bullet_speed, 0.5..=2.0).text("Projectile speed"),
                );
                if ui.button("Reset speeds").clicked() {
                    draft.tank_speed = 1.0;
                    draft.bullet_speed = 1.0;
                }
            }
            ui.checkbox(&mut draft.fullscreen, "Fullscreen");
            ui.checkbox(&mut draft.nerd_stats, "Stats for nerds (N in battle)");
            ui.horizontal(|ui| {
                if ui.button(RichText::new("SAVE").strong()).clicked() {
                    close = Some(true);
                }
                if ui.button("CANCEL").clicked()
                    || ui.input(|input| input.key_pressed(egui::Key::Escape))
                {
                    close = Some(false);
                }
            });
        });
    match close {
        Some(true) => {
            if let Some(draft) = state.settings_draft.take() {
                actions.push(Action::SettingsSaved(Box::new(draft)));
            }
        }
        Some(false) => state.settings_draft = None,
        None => {}
    }
}

/// A fresh draft of the saved settings.
pub fn open_settings(state: &mut UiState, prefs: &Preferences) {
    state.settings_draft = Some(prefs.clone());
}

/// The engine stopped: say so instead of drawing on.
pub fn failure(context: &egui::Context, message: &str) {
    centered_window("failure")
        .fixed_size([420.0, 0.0])
        .show(context, |ui| {
            ui.label(
                RichText::new("THE RENDERER STOPPED")
                    .font(display_font(28.0))
                    .color(RED),
            );
            ui.label(RichText::new(message).color(TEXT_DIM));
            ui.label("Restart Sloppy Tanks to play again.");
        });
}

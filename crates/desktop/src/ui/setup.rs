//! Battle Setup (`src/game/battle-setup.html`, `start-menu.ts`): tank, map, battle
//! format and difficulty, and GO.

use egui::{Align2, RichText};

use super::theme::{self, GOLD, TEXT_DIM, display_font};
use super::{Action, UiState, View};

/// The tanks a player drives: id, name, role.
pub const TANKS: [(&str, &str, &str); 3] = [
    ("scout", "Skipper", "Scout · 80 HP · 40 km/h"),
    ("balanced", "Bruiser", "Balanced · 100 HP · 32 km/h"),
    ("heavy", "Big Rig", "Heavy · 140 HP · 24 km/h"),
];
/// The standard maps: id, name.
pub const MAPS: [(&str, &str); 3] = [
    ("village", "Pine Village"),
    ("harbor", "Harbor Havoc"),
    ("quarry", "Dusty Dig"),
];
/// The extra levels a `--debug` run offers.
pub const EXTRA_MAPS: [(&str, &str); 2] = [
    ("stress-test", "Stress Grid"),
    ("superstress", "Scrap Yard"),
];
pub const BATTLES: [(&str, &str, &str); 2] = [
    ("team", "Team Battle", "6 vs 6 · Respawns · First to 100"),
    ("solo", "Solo Assault", "One life · Survive the waves"),
];
pub const DIFFICULTIES: [(&str, &str); 3] =
    [("easy", "Easy"), ("normal", "Normal"), ("hard", "Hard")];

pub fn battle_setup(
    context: &egui::Context,
    state: &mut UiState,
    view: &View,
    actions: &mut Vec<Action>,
) {
    let mut prefs = view.prefs.clone();
    egui::Window::new("battle setup")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .fixed_size([560.0, 0.0])
        .show(context, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("SLOPPY TANKS")
                        .font(display_font(56.0))
                        .color(GOLD),
                );
                ui.label(
                    RichText::new("BATTLE SETUP")
                        .font(display_font(20.0))
                        .color(TEXT_DIM),
                );
            });
            ui.add_space(8.0);
            let multiplayer = super::rooms::play_tabs(ui, &mut state.rooms);
            ui.label(
                RichText::new("TANK")
                    .font(theme::bold_font(13.0))
                    .color(theme::TEXT_LABEL),
            );
            ui.horizontal(|ui| {
                for (id, name, role) in TANKS {
                    let selected = prefs.tank == id;
                    let text = RichText::new(format!("{name}\n{role}")).size(13.0);
                    if ui
                        .add_sized([170.0, 52.0], egui::Button::selectable(selected, text))
                        .clicked()
                    {
                        prefs.tank = id.into();
                    }
                }
            });
            ui.label(
                RichText::new("MAP")
                    .font(theme::bold_font(13.0))
                    .color(theme::TEXT_LABEL),
            );
            ui.horizontal_wrapped(|ui| {
                let extra = if view.debug { &EXTRA_MAPS[..] } else { &[] };
                for (id, name) in MAPS.iter().chain(extra) {
                    if ui.selectable_label(prefs.map == *id, *name).clicked() {
                        prefs.map = (*id).into();
                    }
                }
            });
            if multiplayer {
                super::rooms::multiplayer(ui, &mut state.rooms, view, &prefs, actions);
                footer(ui, view, actions);
                return;
            }
            ui.label(
                RichText::new("BATTLE")
                    .font(theme::bold_font(13.0))
                    .color(theme::TEXT_LABEL),
            );
            ui.horizontal(|ui| {
                for (id, name, detail) in BATTLES {
                    let text = RichText::new(format!("{name}\n{detail}")).size(13.0);
                    if ui
                        .add_sized(
                            [260.0, 44.0],
                            egui::Button::selectable(prefs.game_mode == id, text),
                        )
                        .clicked()
                    {
                        prefs.game_mode = id.into();
                    }
                }
            });
            ui.label(
                RichText::new("DIFFICULTY")
                    .font(theme::bold_font(13.0))
                    .color(theme::TEXT_LABEL),
            );
            ui.horizontal(|ui| {
                for (id, name) in DIFFICULTIES {
                    if ui.selectable_label(prefs.difficulty == id, name).clicked() {
                        prefs.difficulty = id.into();
                    }
                }
            });
            ui.add_space(10.0);
            ui.vertical_centered(|ui| {
                let label = if state.starting { "LOADING…" } else { "GO!" };
                let go = egui::Button::new(
                    RichText::new(label)
                        .font(display_font(30.0))
                        .color(theme::INK_0),
                )
                .fill(GOLD)
                .min_size([200.0, 48.0].into());
                if ui.add_enabled(!state.starting, go).clicked() {
                    actions.push(Action::Start);
                }
                ui.label(RichText::new(&state.status).color(TEXT_DIM));
                ui.horizontal(|ui| {
                    if ui.button("Settings").clicked() {
                        actions.push(Action::OpenSettings);
                    }
                    if ui.button("Quit").clicked() {
                        actions.push(Action::Quit);
                    }
                });
                ui.label(RichText::new(view.build).size(11.0).color(TEXT_DIM));
            });
        });
    if prefs != *view.prefs {
        actions.push(Action::SettingsSaved(Box::new(prefs)));
        actions.push(Action::OptionsChanged);
    }
}

/// Settings, Quit and the build line under the Multiplayer tab.
fn footer(ui: &mut egui::Ui, view: &View, actions: &mut Vec<Action>) {
    ui.vertical_centered(|ui| {
        ui.horizontal(|ui| {
            if ui.button("Settings").clicked() {
                actions.push(Action::OpenSettings);
            }
            if ui.button("Quit").clicked() {
                actions.push(Action::Quit);
            }
        });
        ui.label(RichText::new(view.build).size(11.0).color(TEXT_DIM));
    });
}

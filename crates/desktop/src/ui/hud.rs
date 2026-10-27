//! The battle HUD (`hudMarkup` in `ui-markup.ts`, `hud-feedback.ts`): the score bar
//! and clock, the tank panel with its hull bar, the ammo rack, the mine dial, effect
//! chips, the kill feed, toasts, the damage ring and hit flash, and the corner buttons.

use egui::{Align2, Color32, FontId, Pos2, RichText, Stroke, Vec2};

use super::theme::{self, GOLD, HULL_DAMAGED, RED, TEXT, TEXT_DIM, display_font, team_color};
use super::{Action, DAMAGE_RING_SECONDS, HIT_FLASH_SECONDS, UiState, View};
use crate::engine_api::{HudState, Phase};

/// Ammunition names on the rack, by `weapon`.
fn weapon_name(weapon: &str) -> &str {
    match weapon {
        "standard" => "Standard",
        "spread" => "Spread",
        "rocket" => "Rocket",
        "ricochet" => "Ricochet",
        "piercing" => "Piercing",
        "tow" => "TOW",
        "mine" => "Mine",
        other => other,
    }
}

/// `m:ss`.
fn clock(seconds: f64) -> String {
    let seconds = seconds.max(0.0).ceil() as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

pub fn hud(context: &egui::Context, state: &mut UiState, view: &View, actions: &mut Vec<Action>) {
    let Some(hud) = state.hud.clone() else { return };
    score_bar(context, &hud);
    if view.phase == Phase::Playing || view.phase == Phase::Ready {
        tank_panel(context, &hud, actions);
        kill_feed(context, state);
        toast(context, state);
        hit_feedback(context, state);
        // A room draws its own respawn card: tanks are chosen between battles there.
        if !hud.human.alive && !view.room {
            respawn(context, &hud, view, actions);
        }
    }
    corner_buttons(context, view, actions);
    if state.cockpit {
        compass(context, state.hull_angle);
    }
}

fn score_bar(context: &egui::Context, hud: &HudState) {
    egui::Area::new(egui::Id::unique("score bar"))
        .anchor(Align2::CENTER_TOP, [0.0, 10.0])
        .interactable(false)
        .show(context, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let match_state = &hud.match_state;
                    if hud.game_mode == "solo" {
                        ui.label(
                            RichText::new(format!("KILLS {}", hud.human.kills))
                                .font(display_font(26.0)),
                        );
                    } else {
                        for team in 0..2 {
                            let score = match_state.scores[team];
                            let name = &hud.team_names[team];
                            let you = if hud.human_team as usize == team {
                                " · YOU"
                            } else {
                                ""
                            };
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(format!("{name}{you}"))
                                        .size(11.0)
                                        .color(team_color(team as u8)),
                                );
                                ui.label(
                                    RichText::new(format!("{score}"))
                                        .font(display_font(30.0))
                                        .color(TEXT),
                                );
                                let fraction = if hud.score_limit > 0.0 {
                                    (score / hud.score_limit) as f32
                                } else {
                                    0.0
                                };
                                ui.add(
                                    egui::ProgressBar::new(fraction.clamp(0.0, 1.0))
                                        .desired_width(110.0)
                                        .desired_height(4.0)
                                        .fill(team_color(team as u8)),
                                );
                            });
                            if team == 0 {
                                ui.vertical(|ui| {
                                    let time = if match_state.overtime {
                                        "NEXT KILL".to_owned()
                                    } else {
                                        clock(match_state.time)
                                    };
                                    ui.label(
                                        RichText::new(time).font(display_font(30.0)).color(GOLD),
                                    );
                                    let objective = if hud.endless_match {
                                        "ENDLESS STRESS".to_owned()
                                    } else {
                                        format!("FIRST TO {}", hud.score_limit)
                                    };
                                    ui.label(RichText::new(objective).size(11.0).color(TEXT_DIM));
                                });
                            }
                        }
                    }
                });
            });
        });
}

/// The hull's colour by health: healthy, damaged, critical.
fn hull_color(ratio: f64) -> Color32 {
    if ratio > 0.6 {
        Color32::from_rgb(0x5d, 0xe0, 0x8a)
    } else if ratio > 0.3 {
        HULL_DAMAGED
    } else {
        RED
    }
}

fn tank_panel(context: &egui::Context, hud: &HudState, actions: &mut Vec<Action>) {
    let human = &hud.human;
    egui::Area::new(egui::Id::unique("tank panel"))
        .anchor(Align2::CENTER_BOTTOM, [0.0, -12.0])
        .show(context, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&human.vehicle_name).font(display_font(20.0)));
                        ui.label(RichText::new(&human.rank_name).size(11.0).color(TEXT_DIM));
                        ui.label(
                            RichText::new(format!(
                                "{:.0} / {:.0}",
                                human.hp.max(0.0),
                                human.max_hp
                            ))
                            .color(hull_color(human.health_ratio)),
                        );
                        ui.add(
                            egui::ProgressBar::new(human.health_ratio.clamp(0.0, 1.0) as f32)
                                .desired_width(180.0)
                                .desired_height(10.0)
                                .fill(hull_color(human.health_ratio)),
                        );
                    });
                    ui.separator();
                    for (index, slot) in human.ammo.iter().enumerate() {
                        let count = slot
                            .count
                            .map_or_else(|| "∞".to_owned(), |count| format!("{count}"));
                        let text = RichText::new(format!(
                            "{} {}\n{}",
                            index + 1,
                            weapon_name(&slot.weapon),
                            count
                        ))
                        .size(12.0)
                        .color(if slot.available {
                            TEXT
                        } else {
                            TEXT_DIM
                        });
                        let button = egui::Button::selectable(slot.selected, text);
                        if ui.add_enabled(slot.available, button).clicked() {
                            actions.push(Action::SelectAmmo(index as u8 + 1));
                        }
                    }
                    ui.separator();
                    let mine = if human.mine_cooldown > 0.0 {
                        format!("MINE\n{:.0}s", human.mine_cooldown.ceil())
                    } else {
                        "MINE\nREADY".to_owned()
                    };
                    ui.label(RichText::new(mine).size(12.0));
                    for (label, seconds) in [
                        ("SPAWN SHIELD", human.protection),
                        ("SHIELD", human.shield),
                        ("RAPID", human.rapid),
                        ("BOOST", human.speed),
                        ("LASER", human.laser),
                    ] {
                        if seconds > 0.0 {
                            ui.label(
                                RichText::new(format!("{label}\n{:.0}s", seconds.ceil()))
                                    .size(11.0)
                                    .color(GOLD),
                            );
                        }
                    }
                });
            });
        });
}

fn kill_feed(context: &egui::Context, state: &UiState) {
    if state.kill_feed.is_empty() {
        return;
    }
    egui::Area::new(egui::Id::unique("kill feed"))
        .anchor(Align2::RIGHT_TOP, [-12.0, 70.0])
        .interactable(false)
        .show(context, |ui| {
            for entry in &state.kill_feed {
                egui::Frame::popup(ui.style())
                    .stroke(Stroke::new(
                        1.0,
                        if entry.own { GOLD } else { theme::PLATE_EDGE },
                    ))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let color = |team: Option<u8>| team.map_or(TEXT, team_color);
                            ui.label(RichText::new(&entry.killer).color(color(entry.killer_team)));
                            ui.label(
                                RichText::new(weapon_name(&entry.weapon))
                                    .size(11.0)
                                    .color(TEXT_DIM),
                            );
                            ui.label(RichText::new(&entry.victim).color(color(entry.victim_team)));
                        });
                    });
            }
        });
}

fn toast(context: &egui::Context, state: &UiState) {
    let Some((text, _)) = &state.toast else {
        return;
    };
    egui::Area::new(egui::Id::unique("toast"))
        .anchor(Align2::CENTER_CENTER, [0.0, 120.0])
        .interactable(false)
        .show(context, |ui| {
            ui.label(RichText::new(text).font(display_font(26.0)).color(GOLD));
        });
}

/// The red edge flash on a hit, and the ring around the tank toward the damage.
fn hit_feedback(context: &egui::Context, state: &UiState) {
    let painter = context.layer_painter(egui::LayerId::background());
    let screen = context.content_rect();
    if let Some(age) = state.hit_flash {
        let alpha = (1.0 - age / HIT_FLASH_SECONDS).clamp(0.0, 1.0);
        let color = Color32::from_rgba_unmultiplied(0xff, 0x30, 0x40, (alpha * 90.0) as u8);
        painter.rect_stroke(
            screen.shrink(12.0),
            0.0,
            Stroke::new(24.0, color),
            egui::StrokeKind::Middle,
        );
    }
    if let (Some((angle, age)), Some((x, y))) = (state.damage_ring, state.viewer) {
        let center = Pos2::new(
            screen.center().x + x * screen.width() / 2.0,
            screen.center().y - y * screen.height() / 2.0,
        );
        let alpha = (1.0 - age / DAMAGE_RING_SECONDS).clamp(0.0, 1.0);
        let color = Color32::from_rgba_unmultiplied(0xff, 0x4d, 0x5e, (alpha * 230.0) as u8);
        let radius = 46.0;
        let direction = Vec2::angled(angle as f32 - std::f32::consts::FRAC_PI_2);
        let tip = center + direction * radius;
        let side = direction.rot90() * 10.0;
        painter.add(egui::Shape::convex_polygon(
            vec![tip + direction * 14.0, tip + side, tip - side],
            color,
            Stroke::NONE,
        ));
    }
}

fn respawn(context: &egui::Context, hud: &HudState, view: &View, actions: &mut Vec<Action>) {
    egui::Area::new(egui::Id::unique("respawn"))
        .anchor(Align2::CENTER_CENTER, [0.0, -40.0])
        .show(context, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("DESTROYED")
                            .font(display_font(32.0))
                            .color(RED),
                    );
                    if hud.human.respawn > 0.0 {
                        ui.label(format!("Back in {:.0}s", hud.human.respawn.ceil()));
                    }
                    ui.label(RichText::new("Next tank").size(11.0).color(TEXT_DIM));
                    ui.horizontal(|ui| {
                        for (id, name, _) in super::setup::TANKS {
                            if ui.selectable_label(view.prefs.tank == id, name).clicked() {
                                actions.push(Action::NextTank(id.into()));
                            }
                        }
                    });
                });
            });
        });
}

fn corner_buttons(context: &egui::Context, view: &View, actions: &mut Vec<Action>) {
    egui::Area::new(egui::Id::unique("corner buttons"))
        .anchor(Align2::RIGHT_TOP, [-12.0, 12.0])
        .show(context, |ui| {
            ui.horizontal(|ui| {
                if ui.button("👁").on_hover_text("First person (V)").clicked() {
                    actions.push(Action::ToggleView);
                }
                if ui.button("⛶").on_hover_text("Fullscreen (F11)").clicked() {
                    actions.push(Action::ToggleFullscreen);
                }
                if view.phase == Phase::Playing
                    && ui.button("⏸").on_hover_text("Pause (Esc)").clicked()
                {
                    actions.push(Action::Pause);
                }
                if ui.button("⚙").on_hover_text("Settings").clicked() {
                    actions.push(Action::OpenSettings);
                }
            });
        });
}

/// The cockpit's hull compass: the hull's heading against the turret's straight up.
fn compass(context: &egui::Context, hull_angle: f64) {
    let painter = context.layer_painter(egui::LayerId::background());
    let screen = context.content_rect();
    let center = Pos2::new(screen.center().x, screen.max.y - 120.0);
    painter.circle_stroke(center, 28.0, Stroke::new(2.0, theme::PLATE_EDGE));
    let direction = Vec2::angled(hull_angle as f32 - std::f32::consts::FRAC_PI_2);
    painter.line_segment(
        [center - direction * 20.0, center + direction * 20.0],
        Stroke::new(4.0, GOLD),
    );
    painter.text(
        Pos2::new(center.x, center.y + 40.0),
        Align2::CENTER_TOP,
        "V",
        FontId::proportional(12.0),
        TEXT_DIM,
    );
}

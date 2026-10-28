//! The battle HUD (`hudMarkup` in `ui-markup.ts`, `ui.ts`, `hud-feedback.ts`,
//! `cockpit.ts`): the score bar and clock, the corner toolbar, the kill feed, toasts,
//! the tank panel with its hull, ammunition rack, mine socket and effect tabs, the
//! cockpit compass, and the hit feedback (edge flash, damage ring, critical pulse).

use egui::{
    Align2, Color32, CornerRadius, Id, LayerId, Mesh, Order, Painter, Pos2, Rect, Sense, Shape,
    Stroke, StrokeKind, Vec2, pos2, vec2,
};

use super::icons::Art;
use super::paint::{self, fade, hex, hexa, mix};
use super::theme::{self, GOLD, GOLD_HOT, bold_font, display_font};
use super::{Action, DAMAGE_RING_SECONDS, HIT_FLASH_SECONDS, ToastTone, UiState, View};
use sloppy_core::sim::{GameMode, MatchPhase, Team};
use sloppy_web::catalog::AMMO_OPTIONS;
use sloppy_web::feedback::cause_icon;
use sloppy_web::hud::{EffectKind, Hud, HudHuman, HullTone};

/// Below this share of its hull the tank's HUD warns.
const CRITICAL_HEALTH: f64 = 0.25;
/// The screen edge the HUD keeps clear.
const EDGE: f32 = 16.0;

/// The painter under every HUD area: shades, feed, toast and hit feedback.
fn backdrop(context: &egui::Context) -> Painter {
    context.layer_painter(LayerId::new(Order::Background, Id::unique("hud backdrop")))
}

pub fn hud(
    context: &egui::Context,
    state: &mut UiState,
    art: &Art,
    view: &View,
    actions: &mut Vec<Action>,
) {
    let Some(hud) = state.hud.clone() else { return };
    let playing = view.phase == MatchPhase::Playing;
    let screen = context.content_rect();
    let painter = backdrop(context);
    let time = context.input(|input| input.time);
    let human = &hud.human;
    let critical = playing && human.alive && human.health_ratio < CRITICAL_HEALTH;
    if critical {
        // A slow red pulse at the screen's edges while the hull is critical.
        let beat = ((time / 1.6 * std::f64::consts::TAU).cos() * 0.5 + 0.5) as f32;
        paint::vignette(
            &painter,
            screen,
            vec2(0.8, 0.75),
            &[
                (0.58, Color32::TRANSPARENT),
                (0.86, hexa(0xd01025, 0.33)),
                (1.0, hexa(0x8a0014, 0.7)),
            ],
            0.45 + 0.55 * beat,
        );
    }
    if let Some(age) = state.hit_flash.filter(|_| playing) {
        // A hit flashes the screen's rim, never the middle where the fight is.
        paint::vignette(
            &painter,
            screen,
            vec2(0.85, 0.8),
            &[
                (0.7, Color32::TRANSPARENT),
                (0.86, hexa(0xff1f3d, 0.13)),
                (1.0, hexa(0xb00020, 0.5)),
            ],
            (1.0 - age / HIT_FLASH_SECONDS) as f32,
        );
    }
    if state.cockpit {
        paint::vignette_at(
            &painter,
            screen,
            pos2(screen.center().x, screen.top() + screen.height() * 0.46),
            vec2(0.8, 0.76),
            &[
                (0.66, Color32::TRANSPARENT),
                (0.9, hexa(0x0a121a, 0.6)),
                (1.0, hexa(0x05090d, 0.8)),
            ],
            1.0,
        );
    }
    // A soft shade along the top edge keeps the score, buttons and feed legible.
    painter.add(paint::vgradient(
        Rect::from_min_size(screen.min, vec2(screen.width(), 120.0)),
        &[
            (0.0, hexa(0x030a18, 0.45)),
            (0.55, hexa(0x030a18, 0.15)),
            (1.0, Color32::TRANSPARENT),
        ],
    ));
    score_bar(&painter, screen, &hud, view.room);
    kill_feed(&painter, art, screen, state);
    if playing {
        toast(&painter, screen, state);
        toolbar(context, art, screen, state, view, actions);
        if human.alive {
            damage_ring(&painter, screen, state);
        }
    }
    // The respawn card rises into place over a third of a second.
    let rise = context.animate_bool_with_time(Id::unique("respawn rise"), !human.alive, 0.32);
    if human.alive {
        tank_panel(context, art, screen, state, &hud, playing, time, actions);
    } else if playing && !view.room {
        // A room draws its own respawn card: tanks are chosen between battles there.
        super::respawn::card(context, art, screen, state, &hud, view, rise, actions);
    }
}

/// One team's plate on the score bar: its slanted outline toward the clock.
fn team_plate(x: f32, width: f32, height: f32, left_side: bool) -> Vec<Pos2> {
    let slant = 16.0;
    if left_side {
        vec![
            pos2(x, 0.0),
            pos2(x + width, 0.0),
            pos2(x + width, height),
            pos2(x + slant, height),
        ]
    } else {
        vec![
            pos2(x, 0.0),
            pos2(x + width, 0.0),
            pos2(x + width - slant, height),
            pos2(x, height),
        ]
    }
}

/// The score hangs from the top edge: two team plates cut toward the clock between
/// them, each with a bar filling toward the score limit; your side carries gold.
fn score_bar(painter: &Painter, screen: Rect, hud: &Hud, room: bool) {
    let solo = hud.game_mode == GameMode::Solo;
    let match_state = &hud.match_state;
    let no_limit = room || solo || hud.endless_match;
    let height = if room { 55.0 } else { 62.0 };
    let top = screen.top();
    let label_font = bold_font(14.0);
    let score_font = display_font(38.0);
    let plate_width = |label: &str, score: &str| {
        let label = paint::text_width(painter, label, label_font.clone(), 1.6);
        let score = paint::text_width(painter, score, score_font.clone(), 0.0);
        (label + score + 10.0 + 42.0).max(156.0)
    };
    let labels = if solo {
        ["KILLS".to_owned(), String::new()]
    } else {
        hud.team_names.map(str::to_uppercase)
    };
    let scores = if solo {
        [format!("{}", hud.human.kills), String::new()]
    } else {
        match_state.scores.map(|score| format!("{score}"))
    };
    let pill = &hud.score_pill;
    let overtime = pill.overtime;
    let time = pill.clock().to_string();
    let objective = pill
        .objective
        .map(|objective| objective.to_string())
        .unwrap_or_default();
    let objective_width = paint::text_width(painter, &objective, bold_font(13.0), 1.2);
    let clock_width = (objective_width + 24.0).max(136.0) + if solo { 12.0 } else { 0.0 };
    let widths = [
        plate_width(&labels[0], &scores[0]),
        if solo {
            0.0
        } else {
            plate_width(&labels[1], &scores[1])
        },
    ];
    let total = widths[0] + clock_width + widths[1];
    let left = screen.center().x - total / 2.0;
    let shift = vec2(0.0, top);
    for team in 0..if solo { 1 } else { 2 } {
        let width = widths[team];
        let x = if team == 0 {
            left
        } else {
            left + widths[0] + clock_width
        };
        let mut outline = team_plate(x, width, height, team == 0);
        outline.iter_mut().for_each(|point| *point += shift);
        let (light, deep) = if team == 0 {
            (hex(0x2f8ff2), hex(0x1252b0))
        } else {
            (hex(0xff5367), hex(0xb01b33))
        };
        painter.add(paint::gradient_polygon(&outline, light, deep));
        // The glossy top half.
        let mut gloss = team_plate(x, width, height * 0.46, team == 0);
        if team == 0 {
            gloss[3].x = x + 16.0 * 0.46;
        } else {
            gloss[2].x = x + width - 16.0 * 0.46;
        }
        gloss.iter_mut().for_each(|point| *point += shift);
        painter.add(paint::gradient_polygon(
            &gloss,
            hexa(0xffffff, 0.2),
            hexa(0xffffff, 0.0),
        ));
        let yours = !room && !solo && team == hud.human_team.index();
        if yours {
            painter.add(paint::gradient_polygon(
                &[
                    pos2(x, top + 3.0),
                    pos2(x + width, top + 3.0),
                    pos2(x + width, top + 16.0),
                    pos2(x, top + 16.0),
                ],
                hexa(0xffcf38, 0.4),
                hexa(0xffcf38, 0.0),
            ));
            painter.rect_filled(Rect::from_min_size(pos2(x, top), vec2(width, 3.0)), 0, GOLD);
        }
        let middle = top + height / 2.0 - 2.0;
        let (label_pos, label_align, score_pos, score_align) = if team == 0 {
            (
                pos2(x + 24.0, middle),
                Align2::LEFT_CENTER,
                pos2(x + width - 18.0, middle),
                Align2::RIGHT_CENTER,
            )
        } else {
            (
                pos2(x + width - 24.0, middle),
                Align2::RIGHT_CENTER,
                pos2(x + 18.0, middle),
                Align2::LEFT_CENTER,
            )
        };
        paint::shadowed(
            painter,
            label_pos,
            label_align,
            &labels[team],
            label_font.clone(),
            Color32::WHITE,
            1.6,
        );
        paint::shadowed(
            painter,
            score_pos,
            score_align,
            &scores[team],
            score_font.clone(),
            Color32::WHITE,
            0.0,
        );
        if !no_limit {
            // The race bar fills from the clock's side.
            let track = if team == 0 {
                Rect::from_min_max(
                    pos2(x + 22.0, top + height - 7.0),
                    pos2(x + width - 8.0, top + height - 4.0),
                )
            } else {
                Rect::from_min_max(
                    pos2(x + 8.0, top + height - 7.0),
                    pos2(x + width - 22.0, top + height - 4.0),
                )
            };
            let share = (f64::from(match_state.scores[team]) / f64::from(hud.score_limit.max(1)))
                .clamp(0.0, 1.0) as f32;
            painter.rect_filled(
                track,
                2,
                if yours {
                    hexa(0x1a1200, 0.54)
                } else {
                    hexa(0x0a1428, 0.33)
                },
            );
            let fill_width = track.width() * share;
            let fill = if team == 0 {
                Rect::from_min_max(pos2(track.right() - fill_width, track.top()), track.max)
            } else {
                Rect::from_min_max(track.min, pos2(track.left() + fill_width, track.bottom()))
            };
            let color = if yours { GOLD_HOT } else { Color32::WHITE };
            painter.rect_filled(fill.expand(1.5), 3, fade(color, 0.3));
            painter.rect_filled(fill, 2, color);
        }
        if yours {
            // A gold tab under the plate names it YOUR TEAM.
            let label = "YOUR TEAM";
            let tab_width = paint::text_width(painter, label, bold_font(12.0), 2.0) + 32.0;
            let center = x + width / 2.0;
            let tab =
                Rect::from_center_size(pos2(center, top + height + 9.0), vec2(tab_width, 19.0));
            painter.add(paint::gradient_polygon(
                &[
                    tab.left_top(),
                    tab.right_top(),
                    pos2(tab.right() - 8.0, tab.bottom()),
                    pos2(tab.left() + 8.0, tab.bottom()),
                ],
                GOLD_HOT,
                GOLD,
            ));
            paint::text(
                painter,
                tab.center(),
                Align2::CENTER_CENTER,
                label,
                bold_font(12.0),
                hex(0x10213a),
                2.0,
            );
        }
    }
    // The clock between the plates (a solo run's takes the missing plate's slant).
    let x = left + widths[0];
    let clock_outline = if solo {
        team_plate(x, clock_width, height, false)
    } else {
        vec![
            pos2(x, 0.0),
            pos2(x + clock_width, 0.0),
            pos2(x + clock_width, height),
            pos2(x, height),
        ]
    }
    .into_iter()
    .map(|point| point + shift)
    .collect::<Vec<_>>();
    painter.add(paint::gradient_polygon(
        &clock_outline,
        hex(0x0f2140),
        hex(0x07111f),
    ));
    painter.line_segment(
        [
            pos2(x, top + height - 0.5),
            pos2(
                x + clock_width - if solo { 16.0 } else { 0.0 },
                top + height - 0.5,
            ),
        ],
        Stroke::new(1.0, hexa(0x9fd0ff, 0.2)),
    );
    let center = x + clock_width / 2.0 - if solo { 6.0 } else { 0.0 };
    let (time_font, time_color) = if overtime {
        (display_font(22.0), hex(0xff8a96))
    } else {
        (display_font(30.0), Color32::WHITE)
    };
    paint::shadowed(
        painter,
        pos2(center, top + if room { height / 2.0 } else { 20.0 }),
        Align2::CENTER_CENTER,
        &time,
        time_font,
        time_color,
        1.0,
    );
    paint::text(
        painter,
        pos2(center, top + 42.0),
        Align2::CENTER_CENTER,
        &objective,
        bold_font(13.0),
        GOLD_HOT,
        1.2,
    );
}

/// The corner toolbar: first person, fullscreen, pause and settings on one plate.
fn toolbar(
    context: &egui::Context,
    art: &Art,
    screen: Rect,
    state: &mut UiState,
    view: &View,
    actions: &mut Vec<Action>,
) {
    if state.settings_draft.is_some() {
        return;
    }
    let button = 38.0;
    let buttons = [
        ("eye", "First-person view (V)"),
        (
            if view.prefs.fullscreen {
                "collapse"
            } else {
                "expand"
            },
            if view.prefs.fullscreen {
                if cfg!(target_os = "macos") {
                    "Exit fullscreen (Cmd+F)"
                } else {
                    "Exit fullscreen (F11)"
                }
            } else {
                if cfg!(target_os = "macos") {
                    "Enter fullscreen (Cmd+F)"
                } else {
                    "Enter fullscreen (F11)"
                }
            },
        ),
        ("pause", "Pause (Esc)"),
        ("gear", "Settings"),
    ];
    let size = vec2(button * buttons.len() as f32 + 6.0, button + 6.0);
    let origin = pos2(screen.right() - EDGE - size.x, screen.top() + 14.0);
    egui::Area::new(Id::unique("hud toolbar"))
        .fixed_pos(origin)
        .order(Order::Middle)
        .show(context, |ui| {
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            let painter = ui.painter().clone();
            paint::plate(&painter, rect, 9.0, None);
            for (index, (icon, tip)) in buttons.iter().enumerate() {
                let cell = Rect::from_min_size(
                    rect.min + vec2(3.0 + index as f32 * button, 3.0),
                    Vec2::splat(button),
                );
                let response = ui
                    .interact(cell, Id::unique(("hud tool", index)), Sense::click())
                    .on_hover_text(*tip);
                let lit = index == 0 && state.cockpit;
                if lit {
                    painter.add(paint::rounded_vgradient(
                        cell.shrink(2.0),
                        [7.0; 4],
                        &[(0.0, GOLD_HOT), (1.0, GOLD)],
                    ));
                } else if response.hovered() {
                    painter.rect_filled(cell.shrink(1.0), 7, hexa(0xffffff, 0.08));
                }
                if index > 0 {
                    painter.line_segment(
                        [
                            pos2(cell.left(), cell.top() + 9.0),
                            pos2(cell.left(), cell.bottom() - 9.0),
                        ],
                        Stroke::new(1.0, hexa(0x9fd0ff, 0.14)),
                    );
                }
                let tint = if lit {
                    hex(0x10233c)
                } else if response.hovered() {
                    Color32::WHITE
                } else {
                    hex(0xe9f2fb)
                };
                art.icon(
                    &painter,
                    icon,
                    Rect::from_center_size(cell.center(), Vec2::splat(20.0)),
                    tint,
                );
                if response.clicked() {
                    actions.push(match index {
                        0 => Action::ToggleView,
                        1 => Action::ToggleFullscreen,
                        2 => Action::Pause,
                        _ => {
                            state.resume_after_settings = true;
                            Action::OpenSettings
                        }
                    });
                }
            }
        });
}

/// The kill feed: killer, weapon, victim, newest on top, one fixed-width steel plate
/// per row with each side's team as a bright edge at its end.
fn kill_feed(painter: &Painter, art: &Art, screen: Rect, state: &UiState) {
    let width = 336.0_f32.min(screen.width() * 0.42);
    let row_height = 30.0;
    let right = screen.right() - EDGE;
    let lines = if state.cockpit {
        2
    } else {
        super::KILL_FEED_LINES
    };
    for (index, entry) in state.kill_feed.iter().take(lines).enumerate() {
        let top = screen.top() + 74.0 + index as f32 * (row_height + 4.0);
        let rect = Rect::from_min_size(pos2(right - width, top), vec2(width, row_height));
        let side = |team: Option<Team>| match team {
            Some(Team::Blue) => hex(0x3d9cff),
            Some(Team::Red) => hex(0xff4d63),
            None => hex(0xc9b994),
        };
        let from = side(entry.killer_team);
        let to = side(entry.victim_team);
        let own = entry.own_kill || entry.own_death;
        let frame = if entry.own_death { hex(0xff5d70) } else { GOLD };
        if own {
            painter.rect_filled(rect.expand(3.0), 5, fade(frame, 0.18));
        }
        let top_color = if own {
            mix(hex(0x1f2735), frame, 0.24)
        } else {
            hex(0x1f2735)
        };
        painter.add(paint::vgradient(
            rect,
            &[
                (0.0, top_color),
                (if own { 0.85 } else { 1.0 }, hex(0x11161f)),
                (1.0, hex(0x11161f)),
            ],
        ));
        painter.add(paint::hgradient(
            rect,
            &[(0.0, fade(from, 0.3)), (0.4, Color32::TRANSPARENT)],
        ));
        painter.add(paint::hgradient(
            rect,
            &[(0.6, Color32::TRANSPARENT), (1.0, fade(to, 0.3))],
        ));
        let edge = if own { 7.0 } else { 5.0 };
        painter.rect_filled(
            Rect::from_min_size(rect.min, vec2(edge, row_height)),
            0,
            from,
        );
        painter.rect_filled(
            Rect::from_min_max(pos2(rect.right() - edge, rect.top()), rect.max),
            0,
            to,
        );
        painter.line_segment(
            [
                rect.left_top() + vec2(edge, 0.5),
                rect.right_top() + vec2(-edge, 0.5),
            ],
            Stroke::new(1.0, hexa(0xffffff, 0.15)),
        );
        if own {
            painter.rect_stroke(rect, 3, Stroke::new(2.0, frame), StrokeKind::Inside);
        }
        // The newest row flashes in, then stays a little brighter for a few seconds.
        if index == 0 {
            let flash =
                (1.0 - entry.age / 0.35).max(0.0) * 0.35 + if entry.age < 1.4 { 0.06 } else { 0.0 };
            if flash > 0.0 {
                painter.rect_filled(rect, 3, hexa(0xffffff, flash as f32));
            }
        }
        let opacity = if own {
            1.0
        } else {
            [1.0, 0.92, 0.82][index.min(2)]
        };
        let center = rect.center();
        let name_width = (width - 32.0 - 26.0 - 16.0) / 2.0;
        let font = bold_font(13.5);
        let killer_color = if entry.own_kill {
            GOLD_HOT
        } else {
            Color32::WHITE
        };
        let victim_color = if entry.own_death {
            hex(0xff9aa5)
        } else {
            Color32::WHITE
        };
        for (name, color, pos, align) in [
            (
                &entry.killer,
                killer_color,
                pos2(center.x - 21.0, center.y),
                Align2::RIGHT_CENTER,
            ),
            (
                &entry.victim,
                victim_color,
                pos2(center.x + 21.0, center.y),
                Align2::LEFT_CENTER,
            ),
        ] {
            let name = name.to_uppercase();
            let shadow = paint::galley(painter, &name, font.clone(), hexa(0, 0.8), 0.7, name_width);
            let at = align.anchor_size(pos, shadow.size());
            painter.galley(at.min + vec2(0.0, 1.0), shadow, Color32::BLACK);
            paint::text_fit(
                painter,
                pos,
                align,
                &name,
                font.clone(),
                fade(color, opacity),
                0.7,
                name_width,
            );
        }
        let icon_tint = if own {
            Color32::WHITE
        } else {
            fade(hex(0xb9cbe0), opacity)
        };
        art.icon(
            painter,
            cause_icon(entry.cause),
            Rect::from_center_size(center, Vec2::splat(22.0)),
            icon_tint,
        );
    }
}

/// Notices (pickups in cyan, promotions in gold, the viewer's death in red) in a
/// fixed slot under the score, on a band that fades out at both ends.
fn toast(painter: &Painter, screen: Rect, state: &UiState) {
    let Some(toast) = &state.toast else { return };
    if toast.text.is_empty() {
        return;
    }
    let (rule, color, size) = match toast.tone {
        ToastTone::Pickup => (hexa(0x6fe6ff, 0.47), hex(0xbff6ff), 14.0),
        ToastTone::Promotion => (hexa(0xffcf38, 0.4), GOLD_HOT, 17.0),
        ToastTone::Death => (hexa(0xff4d5e, 0.53), hex(0xffd6da), 14.0),
    };
    let font = bold_font(size);
    let max = (screen.width() - 40.0).min(720.0) - 88.0;
    let galley = paint::galley(painter, &toast.text, font.clone(), color, 1.4, max);
    // The entrance settles from a slightly smaller band.
    let grow = 0.92 + 0.08 * ((toast.age / 0.25).min(1.0) as f32);
    let size = vec2(galley.size().x + 88.0, galley.size().y + 16.0) * vec2(grow, 1.0);
    let rect = Rect::from_center_size(
        pos2(screen.center().x, screen.top() + 150.0 + size.y / 2.0),
        size,
    );
    let band = hexa(0x08142a, 0.91);
    painter.add(paint::hgradient(
        rect,
        &[
            (0.0, Color32::TRANSPARENT),
            (0.16, band),
            (0.84, band),
            (1.0, Color32::TRANSPARENT),
        ],
    ));
    for y in [rect.top(), rect.bottom()] {
        painter.add(paint::hgradient(
            Rect::from_min_max(pos2(rect.left(), y - 0.5), pos2(rect.right(), y + 0.5)),
            &[
                (0.0, Color32::TRANSPARENT),
                (0.16, rule),
                (0.84, rule),
                (1.0, Color32::TRANSPARENT),
            ],
        ));
    }
    let shadow = paint::galley(painter, &toast.text, font, hexa(0, 0.6), 1.4, max);
    let at = Align2::CENTER_CENTER.anchor_size(rect.center(), galley.size());
    painter.galley(at.min + vec2(0.0, 2.0), shadow, Color32::BLACK);
    painter.galley(at.min, galley, color);
}

/// Each effect's icon and colour on its tab.
fn effect_art(kind: EffectKind) -> (&'static str, u32) {
    match kind {
        EffectKind::Spawn | EffectKind::Shield => ("shield", 0x6fe6ff),
        EffectKind::Rapid => ("rapid", 0xffb547),
        EffectKind::Boost => ("boost", 0x9dff6a),
        EffectKind::Laser => ("target", 0xff6fd8),
        EffectKind::Repair => ("repair", 0x8ff0b0),
    }
}

const STATUS_WIDTH: f32 = 128.0;
const SLOT_WIDTH: f32 = 36.0;
const SELECTED_SLOT_WIDTH: f32 = 100.0;
const SLOT_GAP: f32 = 4.0;
const MINE_WIDTH: f32 = 42.0;
const COMPASS_WIDTH: f32 = 68.0;
const PANEL_HEIGHT: f32 = 65.4;

/// The tank panel: hull on the left, the ammunition rack and the mine socket on the
/// right, on an armour plate tinted by your team; power-up tabs stand on its top
/// edge, the ammunition notice above its left end, and in first person the hull
/// compass joins its right edge.
#[allow(clippy::too_many_arguments)]
fn tank_panel(
    context: &egui::Context,
    art: &Art,
    screen: Rect,
    state: &UiState,
    hud: &Hud,
    playing: bool,
    time: f64,
    actions: &mut Vec<Action>,
) {
    let human = &hud.human;
    let strip_width = AMMO_OPTIONS
        .iter()
        .map(|option| {
            let selected = human
                .ammo
                .iter()
                .any(|slot| slot.weapon == option.weapon && slot.selected);
            if selected {
                SELECTED_SLOT_WIDTH
            } else {
                SLOT_WIDTH
            }
        })
        .sum::<f32>()
        + SLOT_GAP * (AMMO_OPTIONS.len() - 1) as f32;
    let inner = vec2(
        STATUS_WIDTH + 10.0 + 10.0 + strip_width + 10.0 + 10.0 + MINE_WIDTH,
        PANEL_HEIGHT,
    );
    let padding = [14.0, 10.0, 11.0, 10.0];
    let plate_size = vec2(
        inner.x + padding[0] + padding[2],
        inner.y + padding[1] + padding[3],
    );
    let compass = state.cockpit;
    let size = plate_size + vec2(if compass { COMPASS_WIDTH } else { 0.0 }, 0.0);
    let origin = pos2(screen.left() + EDGE, screen.bottom() - EDGE - size.y);
    egui::Area::new(Id::unique("tank panel"))
        .fixed_pos(origin)
        .order(Order::Middle)
        .show(context, |ui| {
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            let painter = ui.painter().clone();
            // A soft shade in the corner behind the panel.
            let team_tint = if hud.game_mode == GameMode::Solo {
                None
            } else {
                Some(theme::team_color(hud.human_team))
            };
            let tint = match human.hull_tone {
                HullTone::Critical => Some(hex(0xff2a40)),
                HullTone::Damaged => Some(hex(0xff8a2a)),
                HullTone::Healthy => team_tint,
            };
            let plate_rect = Rect::from_min_size(rect.min, plate_size);
            if compass {
                // The dial's segment continues the panel: the panel keeps only its top-left
                // cut and the segment the bottom-right one.
                let whole = rect;
                let outline = paint::cut_corners(whole, 12.0);
                painter.add(paint::gradient_polygon(
                    &outline,
                    hexa(paint::PLATE_TOP, 0.97),
                    hexa(paint::PLATE_BOTTOM, 0.99),
                ));
                painter.add(Shape::closed_line(
                    outline,
                    Stroke::new(1.0, theme::PLATE_EDGE),
                ));
                painter.line_segment(
                    [
                        pos2(plate_rect.right(), rect.top() + 10.0),
                        pos2(plate_rect.right(), rect.bottom() - 10.0),
                    ],
                    Stroke::new(1.0, hexa(0xffffff, 0.09)),
                );
                compass_dial(
                    &painter,
                    Rect::from_min_max(pos2(plate_rect.right(), rect.top()), rect.max),
                    state.hull_angle,
                );
            } else {
                paint::plate(&painter, plate_rect, 12.0, tint);
            }
            let content = Rect::from_min_size(plate_rect.min + vec2(padding[0], padding[1]), inner);
            hull_status(ui, &painter, art, content, state, human, playing, time);
            let mut x = content.left() + STATUS_WIDTH + 10.0;
            painter.line_segment(
                [pos2(x, content.top()), pos2(x, content.bottom())],
                Stroke::new(1.0, hexa(0xffffff, 0.09)),
            );
            x += 10.0;
            let strip = Rect::from_min_size(pos2(x, content.top()), vec2(strip_width, inner.y));
            ammo_rack(ui, &painter, art, strip, human, playing, actions);
            x = strip.right() + 10.0;
            painter.line_segment(
                [pos2(x, content.top()), pos2(x, content.bottom())],
                Stroke::new(1.0, hexa(0xffffff, 0.09)),
            );
            x += 10.0;
            mine_socket(
                &painter,
                art,
                Rect::from_min_size(pos2(x, content.top()), vec2(MINE_WIDTH, inner.y)),
                state,
                human,
            );
            effect_tabs(&painter, art, plate_rect, human);
            if let Some((notice, _)) = &state.ammo_notice {
                ammo_notice(&painter, plate_rect, notice, screen);
            }
        });
}

/// The status column: tank and rank, the hull number and the segmented bar.
#[allow(clippy::too_many_arguments)]
fn hull_status(
    ui: &mut egui::Ui,
    painter: &Painter,
    art: &Art,
    rect: Rect,
    state: &UiState,
    human: &HudHuman,
    playing: bool,
    time: f64,
) {
    let critical = playing && human.alive && human.health_ratio < CRITICAL_HEALTH;
    if critical {
        let beat = (0.5 - 0.5 * (time / 1.6 * std::f64::consts::TAU).cos()) as f32;
        painter.rect_filled(
            rect.expand2(vec2(5.0, 5.0)).intersect(rect.expand(5.0)),
            8,
            hexa(0xff3b4a, 0.06 + 0.14 * beat),
        );
    }
    let ramp = hex(human.hull_ramp);
    let tone = human.hull_tone;
    // The tank's name and its rank.
    let name = human.vehicle_name.to_uppercase();
    let name_rect = paint::text_fit(
        painter,
        rect.left_top(),
        Align2::LEFT_TOP,
        &name,
        bold_font(13.0),
        Color32::WHITE,
        1.4,
        STATUS_WIDTH - 40.0,
    );
    let rank = human.rank_name.to_uppercase();
    let rank_width = paint::text_width(painter, &rank, bold_font(12.0), 0.8) + 12.0;
    let rank_rect = Rect::from_min_size(
        pos2(name_rect.right() + 7.0, rect.top() - 1.0),
        vec2(
            rank_width.min(rect.right() - name_rect.right() - 7.0 + 30.0),
            17.0,
        ),
    );
    let (rank_fill, rank_ink) = if human.rank == 0 {
        (hexa(0xffffff, 0.11), hex(0xc3d4e6))
    } else {
        (GOLD, hex(0x1a2433))
    };
    painter.rect_filled(rank_rect, 4, rank_fill);
    if human.rank > 0 {
        painter.add(paint::vgradient(
            rank_rect.shrink2(vec2(0.0, 3.0)),
            &[(0.0, GOLD_HOT), (1.0, GOLD)],
        ));
    }
    paint::text_fit(
        painter,
        rank_rect.center(),
        Align2::CENTER_CENTER,
        &rank,
        bold_font(12.0),
        rank_ink,
        0.8,
        rank_rect.width() - 6.0,
    );
    ui.interact(rank_rect, Id::unique("rank"), Sense::hover())
        .on_hover_text(human.rank_title);
    // The hull readout: its icon, the number in the ramp's colour and the maximum.
    let readout_y = rect.top() + 36.0;
    art.icon(
        painter,
        "hull",
        Rect::from_center_size(pos2(rect.left() + 10.0, readout_y), Vec2::splat(20.0)),
        ramp,
    );
    let mut number_color = if tone == HullTone::Healthy {
        Color32::WHITE
    } else {
        mix(Color32::WHITE, ramp, 0.72)
    };
    if tone == HullTone::Critical && playing {
        let flare = (0.5 - 0.5 * (time / 0.9 * std::f64::consts::TAU).cos()) as f32;
        number_color = mix(number_color, Color32::WHITE, flare);
    }
    let hp = format!("{}", human.hp.max(0.0).ceil());
    let number = paint::shadowed(
        painter,
        pos2(rect.left() + 24.0, readout_y + 13.0),
        Align2::LEFT_BOTTOM,
        &hp,
        display_font(32.0),
        number_color,
        0.0,
    );
    paint::shadowed(
        painter,
        pos2(number.right() + 5.0, readout_y + 11.0),
        Align2::LEFT_BOTTOM,
        &format!("/ {}", human.max_hp.round()),
        display_font(21.0),
        hex(0xb9cadd),
        0.5,
    );
    // The segmented bar: the fill drops at once; a ghost trails it to show the chunk.
    let bar = Rect::from_min_size(
        pos2(rect.left(), rect.bottom() - 9.0),
        vec2(STATUS_WIDTH, 9.0),
    );
    painter.rect_filled(bar, 3, hex(0x030913));
    let share = human.health_ratio.clamp(0.0, 1.0) as f32;
    let ghost = state.hull_ghost.clamp(0.0, 1.0) as f32;
    if ghost > share {
        painter.rect_filled(
            Rect::from_min_size(bar.min, vec2(bar.width() * ghost, bar.height())),
            3,
            hexa(0xffffff, 0.22),
        );
    }
    let fill = Rect::from_min_size(bar.min, vec2(bar.width() * share, bar.height()));
    if share > 0.0 {
        painter.rect_filled(fill.expand(2.0), 4, fade(ramp, 0.25));
        painter.add(paint::vgradient(
            fill,
            &[
                (0.0, mix(Color32::WHITE, ramp, 0.4)),
                (0.45, ramp),
                (0.7, ramp),
                (1.0, mix(ramp, Color32::BLACK, 0.3)),
            ],
        ));
    }
    for segment in 1..10 {
        let x = bar.left() + bar.width() * segment as f32 / 10.0;
        painter.rect_filled(
            Rect::from_min_max(pos2(x - 2.0, bar.top()), pos2(x, bar.bottom())),
            0,
            hex(0x030913),
        );
    }
}

/// The rack: every round keeps its slot and key; the loaded round's slot is wide and
/// names it, a round not carried is a dim ghost in the channel.
fn ammo_rack(
    ui: &mut egui::Ui,
    painter: &Painter,
    art: &Art,
    strip: Rect,
    human: &HudHuman,
    playing: bool,
    actions: &mut Vec<Action>,
) {
    painter.rect_filled(strip, 8, hexa(0x020812, 0.35));
    let mut x = strip.left();
    for (index, option) in AMMO_OPTIONS.iter().enumerate() {
        let (weapon, label, name) = (option.weapon.as_str(), option.label(), option.name);
        let slot = human.ammo.iter().find(|slot| slot.weapon == option.weapon);
        let selected = slot.is_some_and(|slot| slot.selected);
        let available = slot.is_some_and(|slot| slot.available);
        let count = slot.and_then(|slot| slot.count);
        let width = if selected {
            SELECTED_SLOT_WIDTH
        } else {
            SLOT_WIDTH
        };
        let rect = Rect::from_min_size(pos2(x, strip.top()), vec2(width, strip.height()));
        x += width + SLOT_GAP;
        let color = hex(option.color());
        let count_text = match count {
            None => "unlimited".to_owned(),
            Some(count) => format!("{count} remaining"),
        };
        let response = ui
            .interact(rect, Id::unique(("ammo slot", index)), Sense::click())
            .on_hover_text(format!("{}: {name} · {count_text}", index + 1));
        let usable = playing && human.alive && available;
        if !available {
            // A ghost of what a crate would load.
            art.icon(
                painter,
                weapon,
                Rect::from_center_size(rect.center(), Vec2::splat(20.0)),
                fade(hex(0x8195ad), 0.38),
            );
            continue;
        }
        let radius = CornerRadius::same(8);
        painter.rect_filled(rect, radius, hex(0x081427));
        if selected {
            painter.add(paint::vgradient(
                rect.shrink2(vec2(0.0, 4.0)),
                &[
                    (0.0, mix(hex(0x1a3863), color, 0.3)),
                    (0.6, hex(0x10264a)),
                    (1.0, hex(0x0a1830)),
                ],
            ));
            painter.rect_filled(rect.expand(3.0), 10, fade(color, 0.12));
            painter.rect_stroke(rect, radius, Stroke::new(2.0, color), StrokeKind::Inside);
        } else {
            painter.add(paint::vgradient(
                rect.shrink2(vec2(0.0, 4.0)),
                &[(0.0, hex(0x11284a)), (1.0, hex(0x081427))],
            ));
            let rim = if usable && response.hovered() {
                hexa(0xffffff, 0.25)
            } else {
                hexa(0xffffff, 0.11)
            };
            painter.rect_stroke(rect, radius, Stroke::new(1.0, rim), StrokeKind::Inside);
        }
        // The round's colour along the slot's top.
        let stripe = Rect::from_min_size(
            rect.min + vec2(3.0, 1.0),
            vec2(rect.width() - 6.0, if selected { 3.0 } else { 2.0 }),
        );
        painter.rect_filled(stripe, 1, if selected { color } else { fade(color, 0.7) });
        let count_font_size = if selected { 20.0 } else { 14.0 };
        if selected {
            art.icon(
                painter,
                weapon,
                Rect::from_min_size(rect.min + vec2(9.0, 14.0), Vec2::splat(24.0)),
                color,
            );
            let count_pos = pos2(rect.right() - 9.0, rect.top() + 26.0);
            match count {
                None => art.icon(
                    painter,
                    "infinity",
                    Rect::from_center_size(count_pos - vec2(12.0, 0.0), Vec2::splat(24.0)),
                    Color32::WHITE,
                ),
                Some(count) => {
                    paint::shadowed(
                        painter,
                        count_pos,
                        Align2::RIGHT_CENTER,
                        &format!("{count}"),
                        display_font(count_font_size),
                        Color32::WHITE,
                        0.0,
                    );
                }
            }
            paint::text_fit(
                painter,
                pos2(rect.left() + 9.0, rect.bottom() - 6.0),
                Align2::LEFT_BOTTOM,
                label,
                bold_font(12.0),
                color,
                0.9,
                rect.width() - 14.0,
            );
        } else {
            // The key at the top, the icon and the count under it.
            let key = format!("{}", index + 1);
            let key_rect =
                Rect::from_center_size(pos2(rect.center().x, rect.top() + 11.0), vec2(16.0, 15.0));
            painter.rect_filled(key_rect, 3, hexa(0xffffff, 0.12));
            paint::text(
                painter,
                key_rect.center(),
                Align2::CENTER_CENTER,
                &key,
                bold_font(12.0),
                hex(0xf2f7fc),
                0.0,
            );
            art.icon(
                painter,
                weapon,
                Rect::from_center_size(pos2(rect.center().x, rect.top() + 33.0), Vec2::splat(20.0)),
                color,
            );
            let count_pos = pos2(rect.center().x, rect.bottom() - 10.0);
            match count {
                None => art.icon(
                    painter,
                    "infinity",
                    Rect::from_center_size(count_pos, Vec2::splat(18.0)),
                    Color32::WHITE,
                ),
                Some(count) => {
                    paint::shadowed(
                        painter,
                        count_pos,
                        Align2::CENTER_CENTER,
                        &format!("{count}"),
                        display_font(count_font_size),
                        theme::TEXT,
                        0.0,
                    );
                }
            }
        }
        if usable && response.clicked() {
            actions.push(Action::SelectAmmo(index as u8 + 1));
        }
    }
}

/// The mine socket: armed, or a dial that refills while the seconds count down.
fn mine_socket(painter: &Painter, art: &Art, rect: Rect, state: &UiState, human: &HudHuman) {
    let cooling = human.mine_cooldown > 0.0;
    let total = state.mine_total.max(human.mine_cooldown);
    let armed = if cooling && total > 0.0 {
        (1.0 - human.mine_cooldown / total) as f32
    } else {
        1.0
    };
    let radius = CornerRadius::same(8);
    if cooling {
        painter.rect_filled(rect, radius, hex(0x050d1c));
    } else {
        painter.rect_filled(rect, radius, hex(0x081427));
        painter.add(paint::vgradient(
            rect.shrink2(vec2(0.0, 4.0)),
            &[(0.0, hex(0x11284a)), (1.0, hex(0x081427))],
        ));
    }
    let fill_height = rect.height() * armed;
    painter.rect_filled(
        Rect::from_min_max(pos2(rect.left(), rect.bottom() - fill_height), rect.max),
        radius,
        hexa(0xffcf38, if cooling { 0.15 } else { 0.12 }),
    );
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(1.0, hexa(0xffffff, 0.11)),
        StrokeKind::Inside,
    );
    let middle = pos2(rect.center().x, rect.top() + (rect.height() - 16.0) / 2.0);
    if cooling {
        paint::shadowed(
            painter,
            middle,
            Align2::CENTER_CENTER,
            &format!("{}", human.mine_cooldown.ceil()),
            display_font(18.0),
            hex(0xe6eef8),
            0.0,
        );
    } else {
        painter.rect_filled(
            Rect::from_min_size(rect.min + vec2(3.0, 1.0), vec2(rect.width() - 6.0, 2.0)),
            1,
            fade(GOLD, 0.7),
        );
        art.icon(
            painter,
            "mine",
            Rect::from_center_size(middle, Vec2::splat(22.0)),
            GOLD_HOT,
        );
    }
    let key = paint::kbd(
        painter,
        pos2(rect.center().x, rect.bottom() - 11.0),
        "RMB",
        11.0,
    );
    if cooling {
        painter.rect_filled(key, 4, hexa(0x050d1c, 0.45));
    }
}

/// Active power-ups: tabs docked on the panel's top edge over the rack, growing
/// leftward, each a timer ring round its icon, its name and what is left.
fn effect_tabs(painter: &Painter, art: &Art, plate: Rect, human: &HudHuman) {
    let mut right = plate.right() - 12.0;
    let height = 32.0;
    let font = bold_font(13.0);
    for effect in human.effects.iter().rev() {
        let (icon, color) = effect_art(effect.kind);
        let color = hex(color);
        let detail = effect.detail.to_string();
        let name_width = paint::text_width(painter, effect.name, font.clone(), 1.0);
        let detail_width = if detail.is_empty() {
            0.0
        } else {
            paint::text_width(painter, &detail, font.clone(), 0.4) + 5.0
        };
        let width = 8.0 + 26.0 + 7.0 + name_width + detail_width + 16.0;
        let rect = Rect::from_min_max(
            pos2(right - width, plate.top() - height),
            pos2(right, plate.top() + 1.0),
        );
        right -= width + 3.0;
        let radius = CornerRadius {
            nw: 9,
            ne: 9,
            sw: 0,
            se: 0,
        };
        painter.rect_filled(rect, radius, hex(0x12284b));
        painter.add(paint::vgradient(
            rect.shrink2(vec2(0.0, 0.0)).with_min_y(rect.top() + 6.0),
            &[(0.0, fade(color, 0.2)), (0.85, Color32::TRANSPARENT)],
        ));
        painter.rect_filled(
            Rect::from_min_size(rect.min + vec2(5.0, 0.0), vec2(rect.width() - 10.0, 2.0)),
            1,
            fade(color, 0.85),
        );
        for x in [rect.left() + 0.5, rect.right() - 0.5] {
            painter.line_segment(
                [pos2(x, rect.top() + 8.0), pos2(x, rect.bottom() - 1.0)],
                Stroke::new(1.0, theme::PLATE_EDGE),
            );
        }
        let ring = pos2(rect.left() + 8.0 + 13.0, rect.center().y - 1.0);
        painter.circle_filled(ring, 13.0, hex(0x071228));
        painter.circle_stroke(ring, 13.0, Stroke::new(1.0, hexa(0xffffff, 0.12)));
        paint::timer_ring(painter, ring, 10.5, 3.0, effect.share as f32, color);
        art.icon(
            painter,
            icon,
            Rect::from_center_size(ring, Vec2::splat(13.0)),
            color,
        );
        let text_x = ring.x + 13.0 + 7.0;
        let name = paint::text(
            painter,
            pos2(text_x, ring.y),
            Align2::LEFT_CENTER,
            effect.name,
            font.clone(),
            color,
            1.0,
        );
        if !detail.is_empty() {
            paint::text(
                painter,
                pos2(name.right() + 5.0, ring.y),
                Align2::LEFT_CENTER,
                &detail,
                font.clone(),
                Color32::WHITE,
                0.4,
            );
        }
        // A bar along the tab's foot repeats the ring at a glance.
        let foot = Rect::from_min_max(
            pos2(rect.left() + 9.0, rect.bottom() - 5.0),
            pos2(rect.right() - 9.0, rect.bottom() - 3.0),
        );
        painter.rect_filled(foot, 1, hexa(0xffffff, 0.1));
        painter.rect_filled(
            Rect::from_min_size(foot.min, vec2(foot.width() * effect.share as f32, 2.0)),
            1,
            color,
        );
    }
}

/// The ammunition notice above the panel's left end.
fn ammo_notice(painter: &Painter, plate: Rect, notice: &str, screen: Rect) {
    let font = bold_font(12.0);
    let max = (screen.width() - 2.0 * EDGE).min(600.0) - 24.0;
    let galley = paint::galley(painter, notice, font, GOLD_HOT, 0.0, max);
    let size = galley.size() + vec2(23.0, 14.0);
    let rect = Rect::from_min_size(pos2(plate.left(), plate.top() - 42.0 - size.y), size);
    painter.rect_filled(rect.translate(vec2(0.0, 4.0)), 8, hexa(0, 0.3));
    painter.add(paint::vgradient(
        rect,
        &[(0.0, hexa(0x11264a, 0.94)), (1.0, hexa(0x08142a, 0.95))],
    ));
    painter.rect_stroke(
        rect,
        8,
        Stroke::new(1.0, hexa(0xffdf91, 0.33)),
        StrokeKind::Inside,
    );
    painter.rect_filled(
        Rect::from_min_size(rect.min, vec2(3.0, rect.height())),
        CornerRadius {
            nw: 8,
            sw: 8,
            ne: 0,
            se: 0,
        },
        GOLD,
    );
    painter.galley(rect.min + vec2(12.0, 7.0), galley, GOLD_HOT);
}

/// The cockpit's hull compass: a top-down hull turned against the turret's straight up,
/// with the key back to the overhead view at its foot.
fn compass_dial(painter: &Painter, rect: Rect, hull_angle: f64) {
    let center = pos2(rect.center().x - 2.0, rect.top() + 8.0 + 20.0);
    painter.circle_filled(center, 20.0, hexa(0x050d1c, 0.6));
    painter.circle_stroke(center, 13.5, Stroke::new(1.0, hexa(0x9fd0ff, 0.19)));
    let angle = hull_angle as f32;
    let (sin, cos) = angle.sin_cos();
    let turn = |offset: Vec2| {
        center
            + vec2(
                offset.x * cos - offset.y * sin,
                offset.x * sin + offset.y * cos,
            )
    };
    let quad = |left: f32, top: f32, right: f32, bottom: f32, color: Color32| {
        Shape::convex_polygon(
            vec![
                turn(vec2(left, top)),
                turn(vec2(right, top)),
                turn(vec2(right, bottom)),
                turn(vec2(left, bottom)),
            ],
            color,
            Stroke::NONE,
        )
    };
    painter.add(quad(-9.0, -12.5, 9.0, 12.5, hex(0x2a4466)));
    painter.add(quad(-9.0, -12.5, -4.0, 12.5, hex(0x9fb4c9)));
    painter.add(quad(4.0, -12.5, 9.0, 12.5, hex(0x9fb4c9)));
    painter.add(quad(-9.0, -12.5, 9.0, -9.5, hex(0xe6eef8)));
    // The turret and gun always point up: the way the view looks.
    painter.rect_filled(
        Rect::from_min_max(
            pos2(center.x - 1.5, center.y - 19.0),
            pos2(center.x + 1.5, center.y),
        ),
        1,
        GOLD,
    );
    painter.circle_filled(center, 7.5, hex(0x0b1a33));
    painter.circle_filled(center, 5.5, GOLD);
    paint::kbd(painter, pos2(center.x, rect.bottom() - 13.0), "V", 12.0);
}

/// Incoming damage: a glowing wedge on a ring round the player's tank, turned toward
/// the shot, with a pointer at its peak; in first person (or with the tank off
/// screen) a glow on the screen's edge toward the shot.
fn damage_ring(painter: &Painter, screen: Rect, state: &UiState) {
    let Some((angle, age)) = state.damage_ring else {
        return;
    };
    let remaining = DAMAGE_RING_SECONDS - age;
    let alpha = (remaining * 2.0).clamp(0.0, 1.0) as f32;
    let pulse = if age < 0.5 {
        let t = (1.0 - age / 0.5) as f32;
        t * t
    } else {
        0.0
    };
    let angle = angle as f32;
    let viewer = state.viewer.filter(|_| !state.cockpit);
    let Some((x, y)) = viewer else {
        // The whole screen, glowing red at the edge the shot came from.
        let (dx, dy) = (angle.sin(), -angle.cos());
        let reach = 0.5 / dx.abs().max(dy.abs());
        let point =
            screen.center() + vec2(dx * reach * screen.width(), dy * reach * screen.height());
        let radii = vec2(screen.width() * 0.34, screen.height() * 0.44);
        let blob = paint::radial(
            point,
            radii,
            &[
                (0.0, fade(hexa(0xff2a3a, 0.72), alpha)),
                (0.42, fade(hexa(0xff2a3a, 0.3), alpha)),
                (0.72, Color32::TRANSPARENT),
            ],
        );
        painter.with_clip_rect(screen).add(blob);
        return;
    };
    let center = pos2(
        screen.center().x + x * screen.width() / 2.0,
        screen.center().y - y * screen.height() / 2.0,
    );
    let size = (screen.width().min(screen.height()) * 0.3).clamp(190.0, 290.0);
    let radius = size / 2.0 * (1.0 + 0.14 * pulse);
    let bright = 1.0 + 0.8 * pulse;
    let stops = [
        (0.70, Color32::TRANSPARENT),
        (0.82, hexa(0xff2a3a, 0.2)),
        (0.91, hexa(0xff4458, 0.85)),
        (0.94, hexa(0xffd2d6, 1.0)),
        (0.965, hexa(0xff3a4c, 0.8)),
        (1.0, Color32::TRANSPARENT),
    ];
    let spread = 48f32.to_radians();
    let steps = 24;
    let mut mesh = Mesh::default();
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let theta = angle - spread + 2.0 * spread * t;
        let weight = (1.0 - (2.0 * t - 1.0).abs()) * alpha * bright.min(1.6);
        let direction = vec2(theta.sin(), -theta.cos());
        for &(share, color) in &stops {
            mesh.colored_vertex(center + direction * radius * share, fade(color, weight));
        }
    }
    let rings = stops.len() as u32;
    for step in 0..steps as u32 {
        for ring in 0..rings - 1 {
            let a = step * rings + ring;
            let b = a + 1;
            let c = a + rings;
            let d = c + 1;
            mesh.add_triangle(a, b, c);
            mesh.add_triangle(b, d, c);
        }
    }
    painter.add(Shape::mesh(mesh));
    // The pointer at the wedge's peak.
    let direction = vec2(angle.sin(), -angle.cos());
    let side = direction.rot90() * 10.0;
    let base = center + direction * (radius + 1.0);
    let tip = center + direction * (radius + 13.0);
    let mut pointer = Mesh::default();
    pointer.colored_vertex(tip, fade(hex(0xffb3a8), alpha));
    pointer.colored_vertex(base + side, fade(hex(0xe0162e), alpha));
    pointer.colored_vertex(base - side, fade(hex(0xe0162e), alpha));
    pointer.add_triangle(0, 1, 2);
    painter.circle_filled(
        base + direction * 6.0,
        9.0,
        fade(hexa(0xff2a3a, 0.25), alpha),
    );
    painter.add(Shape::mesh(pointer));
}

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_web::hud::ScorePill;

    fn collect_text(shape: &Shape, text: &mut Vec<String>) {
        match shape {
            Shape::Text(shape) => text.push(shape.galley.job.text.clone()),
            Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect_text(shape, text)),
            _ => {}
        }
    }

    #[test]
    fn room_score_omits_the_single_player_objective_and_team_tab() {
        let context = egui::Context::default();
        theme::install(&context);
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0));
        let mut hud = Hud::default();
        hud.match_state.scores = [42, 37];
        hud.match_state.time = 188.0;
        for room in [false, true] {
            hud.score_pill = if room {
                ScorePill::room(&hud.match_state)
            } else {
                ScorePill::local(&hud.match_state, GameMode::Team, false, 0.0, 100)
            };
            let mut output = context.run_pass(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |context| score_bar(&backdrop(context), screen, &hud, room),
            );
            // This shape-only test has no GPU painter to consume texture updates.
            output.textures_delta.clear();
            let mut labels = Vec::new();
            for shape in output.shapes {
                collect_text(&shape.shape, &mut labels);
            }
            for expected in ["BLUE", "RED", "42", "37", "3:08"] {
                assert!(
                    labels.iter().any(|text| text == expected),
                    "{expected} stays in both HUDs"
                );
            }
            assert_eq!(labels.iter().any(|text| text == "FIRST TO 100"), !room);
            assert_eq!(labels.iter().any(|text| text == "YOUR TEAM"), !room);
        }
    }
}

//! The battle's dialogs: their shared armour-plate frame, head and score plates, the
//! pause menu (`menuMarkup` in `ui-markup.ts`) and the Settings dialog
//! (`settings-dialog.ts`).

use egui::{
    Align2, Color32, Id, LayerId, Order, Painter, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui,
    Vec2, pos2, vec2,
};

use super::icons::Art;
use super::paint::{self, ButtonStyle, hex, hexa};
use super::theme::{GOLD, GOLD_HOT, RED, TEXT_LABEL, bold_font, display_font};
use super::{Action, PauseTab, UiState, View};
use crate::preferences::Preferences;
use sloppy_core::sim::speed_tuning::{MAX_SPEED_SCALE, MIN_SPEED_SCALE};
use sloppy_core::sim::{GameMode, MatchPhase, Team};
use sloppy_web::battle_report::{PauseFact, PauseReport, StandingTone, pause_report};
use sloppy_web::catalog::{
    ControlLine, DEFAULT_VOLUME, GRAPHICS_OPTIONS, KEYBOARD_CONTROLS, SLIDER_STEP, TOUCH_CONTROLS,
};
use sloppy_web::hud::Hud;

/// Room inside a dialog's plate: sides, top and bottom.
pub const DIALOG_PADDING: [f32; 3] = [30.0, 26.0, 28.0];
/// The narrowest a dialog lays out; a narrower or shorter screen (a phone) shows it
/// smaller instead, with the margin around it.
const MIN_DIALOG_WIDTH: f32 = 560.0;
const DIALOG_MARGIN: f32 = 16.0;

/// The arena stays readable behind a dialog, softened and darkest at the edges.
pub fn dim_backdrop(context: &egui::Context) {
    let painter = paint::full_screen(
        context,
        LayerId::new(Order::Background, Id::unique("dialog dim")),
    );
    paint::vignette(
        &painter,
        context.viewport_rect(),
        vec2(0.7, 0.7),
        &[(0.0, hexa(0x071224, 0.24)), (1.0, hexa(0x030812, 0.77))],
        1.0,
    );
}

/// A dialog on an armour plate in the middle of the screen. `build` lays out its
/// content from the given top-left in the given width and returns the height used.
pub fn dialog(
    context: &egui::Context,
    id: &str,
    width: f32,
    build: impl FnOnce(&mut Ui, &Painter, Pos2, f32) -> f32,
) {
    dialog_with_padding(context, id, width, DIALOG_PADDING, build);
}

/// Dialogs with their own CSS padding keep that padding independent of their width.
pub(super) fn dialog_with_padding(
    context: &egui::Context,
    id: &str,
    width: f32,
    padding: [f32; 3],
    build: impl FnOnce(&mut Ui, &Painter, Pos2, f32) -> f32,
) {
    let screen = context.content_rect();
    let room = screen.shrink(DIALOG_MARGIN).size();
    let width = width.min(room.x.max(MIN_DIALOG_WIDTH));
    let shown = egui::Area::new(Id::unique(id))
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .order(Order::Foreground)
        .constrain(false)
        .show(context, |ui| {
            ui.set_clip_rect(paint::shrunk_clip(screen));
            let origin = ui.cursor().min;
            let painter = ui.painter().clone();
            let slot = painter.add(Shape::Noop);
            let [side, top, bottom] = padding;
            let content = build(ui, &painter, origin + vec2(side, top), width - 2.0 * side);
            let rect = Rect::from_min_size(origin, vec2(width, top + content + bottom));
            // A soft shadow: a few widening, faint layers under the plate.
            let mut shapes: Vec<Shape> = [6.0, 14.0, 24.0]
                .into_iter()
                .map(|spread| {
                    Shape::rect_filled(
                        rect.translate(vec2(0.0, 16.0)).expand(spread),
                        spread as u8,
                        hexa(0x000000, 0.1),
                    )
                })
                .collect();
            shapes.extend(paint::plate_shapes(rect, 14.0, None));
            painter.set(slot, Shape::Vec(shapes));
            ui.allocate_rect(rect, Sense::hover());
        });
    let size = shown.response.rect.size();
    let scale = (room.x / size.x).min(room.y / size.y).min(1.0);
    paint::shrink_about(context, shown.response.layer_id, screen.center(), scale);
}

/// The score on two team plates, the viewer's side rimmed in gold and saying YOU;
/// anchored by its right edge's middle. Returns its rect.
pub fn score_plates(
    painter: &Painter,
    right_center: Pos2,
    scores: [u32; 2],
    names: &[&str; 2],
    team: Team,
) -> Rect {
    let score_font = display_font(34.0);
    let name_font = bold_font(11.0);
    let label = |index: usize| {
        let name = names[index].to_uppercase();
        if index == team.index() {
            format!("{name} · YOU")
        } else {
            name
        }
    };
    let widths = [0, 1].map(|index| {
        let score = paint::text_width(
            painter,
            &format!("{}", scores[index]),
            score_font.clone(),
            0.0,
        );
        let name = paint::text_width(painter, &label(index), name_font.clone(), 1.6);
        score.max(name).max(30.0) + 28.0 + 10.0
    });
    let height = 54.0;
    let colon = 18.0;
    let total = widths[0] + colon + widths[1];
    let left = right_center.x - total;
    let top = right_center.y - height / 2.0;
    for index in 0..2 {
        let x = if index == 0 {
            left
        } else {
            left + widths[0] + colon
        };
        let width = widths[index];
        let outline = if index == 0 {
            vec![
                pos2(x + 10.0, top),
                pos2(x + width, top),
                pos2(x + width, top + height),
                pos2(x, top + height),
            ]
        } else {
            vec![
                pos2(x, top),
                pos2(x + width, top),
                pos2(x + width - 10.0, top + height),
                pos2(x, top + height),
            ]
        };
        let (light, deep) = if index == 0 {
            (hex(0x2f8ff2), hex(0x1252b0))
        } else {
            (hex(0xff5367), hex(0xb01b33))
        };
        painter.add(paint::gradient_polygon(&outline, light, deep));
        let center = x + width / 2.0 + if index == 0 { 5.0 } else { -5.0 };
        if index == team as usize {
            painter.add(paint::gradient_polygon(
                &[
                    pos2(x + if index == 0 { 10.0 } else { 0.0 }, top + 3.0),
                    pos2(x + width, top + 3.0),
                    pos2(x + width, top + 16.0),
                    pos2(x, top + 16.0),
                ],
                hexa(0xffcf38, 0.45),
                hexa(0xffcf38, 0.0),
            ));
            painter.rect_filled(
                Rect::from_min_max(
                    pos2(x + if index == 0 { 10.0 } else { 0.0 }, top),
                    pos2(x + width, top + 3.0),
                ),
                0,
                GOLD,
            );
        }
        paint::shadowed(
            painter,
            pos2(center, top + 22.0),
            Align2::CENTER_CENTER,
            &format!("{}", scores[index]),
            score_font.clone(),
            Color32::WHITE,
            0.0,
        );
        let name = names[index].to_uppercase();
        let name_width = paint::text_width(painter, &label(index), name_font.clone(), 1.6);
        let name_left = center - name_width / 2.0;
        let rect = paint::text(
            painter,
            pos2(name_left, top + 45.0),
            Align2::LEFT_CENTER,
            &name,
            name_font.clone(),
            hexa(0xffffff, 0.8),
            1.6,
        );
        if index == team as usize {
            paint::text(
                painter,
                pos2(rect.right(), top + 45.0),
                Align2::LEFT_CENTER,
                " · YOU",
                name_font.clone(),
                GOLD_HOT,
                1.6,
            );
        }
    }
    paint::text(
        painter,
        pos2(left + widths[0] + colon / 2.0, top + height / 2.0),
        Align2::CENTER_CENTER,
        ":",
        display_font(30.0),
        hex(0x6f88a3),
        0.0,
    );
    Rect::from_min_size(pos2(left, top), vec2(total, height))
}

/// The dialog's eyebrow, title and the score beside it; returns the height used.
pub fn dialog_head(
    painter: &Painter,
    origin: Pos2,
    width: f32,
    eyebrow: &str,
    title: &str,
    title_color: Color32,
    score: Option<(&Hud, [u32; 2])>,
) -> f32 {
    paint::eyebrow(painter, origin, eyebrow);
    let title_top = origin.y + 22.0;
    paint::shadowed(
        painter,
        pos2(origin.x, title_top),
        Align2::LEFT_TOP,
        title,
        display_font(50.0),
        title_color,
        1.0,
    );
    if let Some((hud, scores)) = score {
        score_plates(
            painter,
            pos2(origin.x + width, title_top + 26.0),
            scores,
            &hud.team_names,
            hud.human_team,
        );
    }
    22.0 + 58.0
}

/// The controls card's lines and its tip; returns the height used.
pub(super) fn controls_card(painter: &Painter, origin: Pos2, width: f32, columns: usize) -> f32 {
    let line = 40.0;
    let padding = vec2(18.0, 10.0);
    let gap = 26.0;
    let column_width = (width - 2.0 * padding.x - gap * (columns - 1) as f32) / columns as f32;
    let controls: &[ControlLine] = if crate::touch::screen() {
        &TOUCH_CONTROLS
    } else {
        &KEYBOARD_CONTROLS
    };
    let rows = controls.len().div_ceil(columns);
    let height = padding.y + rows as f32 * line + 12.0 + 23.0 + 12.0;
    let well = Rect::from_min_size(origin, vec2(width, height));
    painter.rect_filled(well, 12, hexa(0x050d1c, 0.8));
    for (index, ControlLine { action, keys, note }) in controls.iter().enumerate() {
        let left = origin.x + padding.x + (index % columns) as f32 * (column_width + gap);
        let y = origin.y + padding.y + (index / columns) as f32 * line + line / 2.0;
        painter.line_segment(
            [
                pos2(left, y + line / 2.0),
                pos2(left + column_width, y + line / 2.0),
            ],
            Stroke::new(1.0, hexa(0x9fd0ff, 0.07)),
        );
        paint::text(
            painter,
            pos2(left, y),
            Align2::LEFT_CENTER,
            action,
            bold_font(13.0),
            hex(0xd6e6f5),
            0.0,
        );
        let mut x = left + 106.0;
        for key in *keys {
            let key_width = paint::kbd_width(painter, key, 12.0);
            paint::kbd(painter, pos2(x + key_width / 2.0, y), key, 12.0);
            x += key_width + 4.0;
        }
        if !note.is_empty() {
            let note_painter = painter.with_clip_rect(Rect::from_min_max(
                pos2(x + 6.0, y - line / 2.0),
                pos2(left + column_width, y + line / 2.0),
            ));
            paint::text(
                &note_painter,
                pos2(x + 6.0, y),
                Align2::LEFT_CENTER,
                note,
                egui::FontId::proportional(11.5),
                hex(0x8eaac4),
                0.0,
            );
        }
    }
    let tip_y = origin.y + padding.y + rows as f32 * line + 12.0 + 11.5;
    let tip = Rect::from_min_size(pos2(origin.x + padding.x, tip_y - 11.5), vec2(35.0, 23.0));
    painter.add(paint::rounded_vgradient(
        tip,
        [4.0; 4],
        &[(0.0, GOLD_HOT), (1.0, GOLD)],
    ));
    paint::text(
        painter,
        tip.center(),
        Align2::CENTER_CENTER,
        "TIP",
        bold_font(12.0),
        hex(0x1a2233),
        1.5,
    );
    paint::text(
        painter,
        pos2(tip.right() + 10.0, tip_y),
        Align2::LEFT_CENTER,
        "Crates refill special ammo. Standard shells never run out.",
        egui::FontId::proportional(12.5),
        hex(0xc4dbee),
        0.0,
    );
    height
}

/// A grid of facts: caption, value and an optional qualifier per cell.
fn facts(painter: &Painter, origin: Pos2, width: f32, cells: &[PauseFact]) -> f32 {
    let columns = 3;
    let rows = cells.len().div_ceil(columns);
    let cell_height = 107.5;
    let cell_width = (width - (columns - 1) as f32) / columns as f32;
    let total = Rect::from_min_size(origin, vec2(width, rows as f32 * (cell_height + 1.0) - 1.0));
    painter.rect_filled(total, 12, hexa(0x9fd0ff, 0.08));
    for (index, fact) in cells.iter().enumerate() {
        let column = index % columns;
        let row = index / columns;
        let cell = Rect::from_min_size(
            origin
                + vec2(
                    column as f32 * (cell_width + 1.0),
                    row as f32 * (cell_height + 1.0),
                ),
            vec2(cell_width, cell_height),
        );
        let radius = egui::CornerRadius {
            nw: if index == 0 { 12 } else { 0 },
            ne: if index == columns - 1 { 12 } else { 0 },
            sw: if row == rows - 1 && column == 0 {
                12
            } else {
                0
            },
            se: if row == rows - 1 && column == columns - 1 {
                12
            } else {
                0
            },
        };
        painter.rect_filled(cell, radius, hexa(0x060f1f, 0.9));
        paint::text(
            painter,
            pos2(cell.center().x, cell.top() + 16.0),
            Align2::CENTER_TOP,
            &fact.label.to_uppercase(),
            bold_font(12.0),
            hex(0xa9c6dc),
            1.4,
        );
        paint::text_fit(
            painter,
            pos2(cell.center().x, cell.top() + 35.0),
            Align2::CENTER_TOP,
            &fact.value,
            display_font(if fact.value.len() > 14 { 20.0 } else { 28.0 }),
            Color32::WHITE,
            0.4,
            cell.width() - 16.0,
        );
        if !fact.note.is_empty() {
            paint::text_fit(
                painter,
                pos2(cell.center().x, cell.top() + 68.0),
                Align2::CENTER_TOP,
                &fact.note.to_uppercase(),
                bold_font(12.0),
                hex(0xa9c6dc),
                1.0,
                cell.width() - 16.0,
            );
        }
    }
    total.height()
}

/// Pause's battle tab: a solo run's tally, then the clock, the tank and the round.
fn battle_pane(painter: &Painter, origin: Pos2, width: f32, pause: &PauseReport) -> f32 {
    let mut y = origin.y;
    if let Some(tally) = &pause.tally {
        let half = (width - 10.0) / 2.0;
        for (index, (value, caption)) in [
            (tally.kills.to_string(), "KILLS"),
            (tally.survived.clone(), "SURVIVED"),
        ]
        .into_iter()
        .enumerate()
        {
            let tile = Rect::from_min_size(
                pos2(origin.x + index as f32 * (half + 10.0), y),
                vec2(half, 62.0),
            );
            paint::glass(
                painter,
                tile,
                12.0,
                hex(0x172b4a),
                hex(0x0a162c),
                hexa(0xffcf38, 0.33),
            );
            paint::shadowed(
                painter,
                pos2(tile.left() + 18.0, tile.center().y),
                Align2::LEFT_CENTER,
                &value,
                display_font(40.0),
                GOLD_HOT,
                0.4,
            );
            paint::text(
                painter,
                pos2(tile.right() - 18.0, tile.center().y),
                Align2::RIGHT_CENTER,
                caption,
                bold_font(13.0),
                Color32::WHITE,
                2.4,
            );
        }
        y += 72.0;
    }
    y += facts(painter, pos2(origin.x, y), width, &pause.facts);
    y - origin.y
}

/// Dialog tabs with a gold underline on the chosen one; returns the one clicked.
fn tabs<T: Copy + PartialEq>(
    ui: &mut Ui,
    painter: &Painter,
    origin: Pos2,
    width: f32,
    choices: &[(T, &str)],
    chosen: T,
) -> Option<T> {
    let height = 38.0;
    painter.line_segment(
        [
            pos2(origin.x, origin.y + height),
            pos2(origin.x + width, origin.y + height),
        ],
        Stroke::new(1.0, hexa(0x9fd0ff, 0.14)),
    );
    let mut x = origin.x;
    let mut picked = None;
    for (index, (value, label)) in choices.iter().enumerate() {
        let label_width = paint::text_width(painter, label, bold_font(13.5), 1.6);
        let rect = Rect::from_min_size(pos2(x, origin.y), vec2(label_width + 32.0, height));
        x = rect.right() + 4.0;
        let response = ui.interact(rect, Id::unique(("dialog tab", index)), Sense::click());
        let selected = *value == chosen;
        if selected {
            painter.add(paint::vgradient(
                rect,
                &[(0.2, Color32::TRANSPARENT), (1.0, hexa(0xffcf38, 0.15))],
            ));
            painter.rect_filled(
                Rect::from_min_max(pos2(rect.left(), rect.bottom() - 3.0), rect.max),
                0,
                GOLD,
            );
        } else if response.hovered() {
            painter.rect_filled(rect, 6, hexa(0xffffff, 0.05));
        }
        paint::text(
            painter,
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            bold_font(13.5),
            if selected || response.hovered() {
                Color32::WHITE
            } else {
                hex(0xd3e2f0)
            },
            1.6,
        );
        if response.clicked() {
            picked = Some(*value);
        }
    }
    picked
}

pub fn pause(
    context: &egui::Context,
    state: &mut UiState,
    art: &Art,
    _view: &View,
    actions: &mut Vec<Action>,
) {
    let Some(hud) = state.hud.clone() else { return };
    dim_backdrop(context);
    let solo = hud.game_mode == GameMode::Solo;
    let mode = if solo { "SOLO ASSAULT" } else { "TEAM BATTLE" };
    let pause = pause_report(&hud);
    let mut tab = state.pause_tab;
    let hidden = state.settings_draft.is_some();
    if hidden {
        return;
    }
    dialog_with_padding(
        context,
        "pause menu",
        780.0,
        [42.0, 34.0, 36.0],
        |ui, painter, origin, width| {
            let close = Rect::from_min_size(
                pos2(origin.x + width + 42.0 - 46.0, origin.y - 34.0 + 12.0),
                Vec2::splat(34.0),
            );
            if paint::close_button(ui, art, close, Id::unique("pause close"))
                .on_hover_text("Resume (Esc)")
                .clicked()
            {
                actions.push(Action::Resume);
            }
            let eyebrow = format!("{mode} · {}", hud.map_name.to_uppercase());
            let mut y = origin.y;
            y += dialog_head(
                painter,
                origin,
                width,
                &eyebrow,
                "PAUSED",
                Color32::WHITE,
                (!solo).then_some((&hud, hud.match_state.scores)),
            );
            if solo {
                paint::text(
                    painter,
                    pos2(origin.x, y + 10.0),
                    Align2::LEFT_CENTER,
                    "The battle waits until you resume.",
                    egui::FontId::proportional(14.0),
                    hex(0xc4dbee),
                    0.0,
                );
            } else if let Some(standing) = &pause.standing {
                let tone = match standing.tone {
                    StandingTone::Level => hex(0xa9c0d8),
                    StandingTone::Ahead => hex(0x5fe39a),
                    StandingTone::Behind => hex(0xff4d5e),
                };
                let dot = pos2(origin.x + 4.0, y + 10.0);
                painter.circle_filled(dot, 7.0, paint::fade(tone, 0.25));
                painter.circle_filled(dot, 4.0, tone);
                paint::text(
                    painter,
                    pos2(origin.x + 17.0, y + 10.0),
                    Align2::LEFT_CENTER,
                    &standing.text,
                    egui::FontId::proportional(15.0),
                    hex(0xd5e3f0),
                    0.0,
                );
            }
            y += 30.0 + 18.0;
            if let Some(picked) = tabs(
                ui,
                painter,
                pos2(origin.x, y),
                width,
                &[
                    (PauseTab::Battle, "BATTLE"),
                    (PauseTab::Controls, "CONTROLS"),
                ],
                tab,
            ) {
                tab = picked;
            }
            y += 38.0 + 14.0;
            // Both panes share one height, so switching tabs never moves the buttons.
            let pane_height: f32 = if solo { 72.0 + 216.0 } else { 216.0 };
            match tab {
                PauseTab::Battle => {
                    battle_pane(painter, pos2(origin.x, y), width, &pause);
                }
                PauseTab::Controls => {
                    controls_card(painter, pos2(origin.x, y), width, 2);
                }
            }
            y += pane_height + 20.0;
            let button_height = 48.0;
            let end_width = 160.0;
            let action_gap = 10.0;
            let secondary_width = (width - end_width - 32.0 - action_gap) / 2.4;
            let resume_width = secondary_width * 1.4;
            if paint::button_at(
                ui,
                art,
                Rect::from_min_size(pos2(origin.x, y), vec2(end_width, button_height)),
                "END BATTLE",
                Some("end"),
                ButtonStyle::Danger,
                true,
            )
            .on_hover_text("End the battle and see the report")
            .clicked()
            {
                actions.push(Action::EndBattle);
            }
            painter.line_segment(
                [
                    pos2(origin.x + end_width + 12.0, y + 8.0),
                    pos2(origin.x + end_width + 12.0, y + button_height - 8.0),
                ],
                Stroke::new(1.0, hexa(0x9fd0ff, 0.18)),
            );
            let resume = Rect::from_min_size(
                pos2(origin.x + end_width + 32.0, y),
                vec2(resume_width, button_height),
            );
            if paint::button_at(
                ui,
                art,
                Rect::from_min_size(
                    pos2(resume.right() + action_gap, y),
                    vec2(secondary_width, button_height),
                ),
                "SETTINGS",
                Some("sliders"),
                ButtonStyle::Secondary,
                true,
            )
            .clicked()
            {
                actions.push(Action::OpenSettings);
            }
            if paint::button_at(
                ui,
                art,
                resume,
                "RESUME",
                Some("resume"),
                ButtonStyle::Primary,
                true,
            )
            .clicked()
            {
                actions.push(Action::Resume);
            }
            y + button_height - origin.y
        },
    );
    state.pause_tab = tab;
}

/// A setting's name with its note under it, at the row's left.
fn setting_label(painter: &Painter, pos: Pos2, name: &str, note: &str) {
    paint::text(
        painter,
        pos,
        Align2::LEFT_TOP,
        name,
        bold_font(14.0),
        Color32::WHITE,
        0.2,
    );
    paint::text(
        painter,
        pos + vec2(0.0, 19.0),
        Align2::LEFT_TOP,
        note,
        egui::FontId::proportional(13.0),
        hex(0xb7cde1),
        0.0,
    );
}

/// A settings group's heading with a hairline after it.
fn group_label(painter: &Painter, pos: Pos2, width: f32, label: &str) -> Rect {
    let rect = paint::text(
        painter,
        pos,
        Align2::LEFT_CENTER,
        label,
        bold_font(13.0),
        TEXT_LABEL,
        2.0,
    );
    painter.line_segment(
        [pos2(pos.x, pos.y + 18.0), pos2(pos.x + width, pos.y + 18.0)],
        Stroke::new(1.0, hexa(0x9fd0ff, 0.14)),
    );
    rect
}

/// A slider over `range` with a notch at its default, its value as a percentage in
/// a box beside it and its ends named under it.
#[allow(clippy::too_many_arguments)]
fn slider(
    ui: &mut Ui,
    painter: &Painter,
    rect: Rect,
    id: &str,
    value: &mut f64,
    range: (f64, f64),
    default: f64,
    ends: (&str, &str),
) {
    let output = Rect::from_min_size(pos2(rect.right() - 58.0, rect.top()), vec2(58.0, 26.0));
    let track = Rect::from_min_max(
        pos2(rect.left(), rect.top() + 10.0),
        pos2(output.left() - 18.0, rect.top() + 16.0),
    );
    let hit = Rect::from_min_max(
        pos2(track.left() - 8.0, rect.top()),
        pos2(track.right() + 8.0, rect.top() + 26.0),
    );
    let response = ui.interact(hit, Id::unique(("slider", id)), Sense::click_and_drag());
    if (response.dragged() || response.clicked())
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let t = ((pointer.x - track.left()) / track.width()).clamp(0.0, 1.0) as f64;
        let raw = range.0 + t * (range.1 - range.0);
        *value = (raw / SLIDER_STEP).round() * SLIDER_STEP;
    }
    let share = ((*value - range.0) / (range.1 - range.0)).clamp(0.0, 1.0) as f32;
    painter.rect_filled(track, 3, hex(0x5d7898));
    painter.add(paint::hgradient(
        Rect::from_min_size(track.min, vec2(track.width() * share, track.height())),
        &[(0.0, hex(0xff9d1c)), (1.0, GOLD)],
    ));
    let notch_share = ((default - range.0) / (range.1 - range.0)) as f32;
    let notch_x = track.left() + track.width() * notch_share;
    painter.line_segment(
        [
            pos2(notch_x, track.bottom() + 3.0),
            pos2(notch_x, track.bottom() + 10.0),
        ],
        Stroke::new(2.0, hexa(0xdbe9f6, 0.5)),
    );
    let thumb = pos2(track.left() + track.width() * share, track.center().y);
    let hot = response.hovered() || response.dragged();
    painter.circle_filled(thumb, if hot { 14.0 } else { 13.0 }, hexa(0xffcf38, 0.15));
    painter.circle_filled(thumb, if hot { 10.0 } else { 9.0 }, GOLD);
    painter.circle_stroke(thumb, 7.5, Stroke::new(3.0, hex(0xfff7d6)));
    paint::text(
        painter,
        pos2(output.right(), output.center().y),
        Align2::RIGHT_CENTER,
        &format!("{}%", (*value * 100.0).round()),
        bold_font(16.0),
        Color32::WHITE,
        0.0,
    );
    for (text, x, align) in [
        (ends.0, track.left(), Align2::LEFT_TOP),
        (ends.1, track.right(), Align2::RIGHT_TOP),
    ] {
        paint::text(
            painter,
            pos2(x, track.bottom() + 8.0),
            align,
            text,
            bold_font(13.0),
            hex(0xb7cde1),
            0.0,
        );
    }
}

/// A named choice lifts out of its dark well in gold, like `.setting-choice`.
fn setting_choice<T: Copy + PartialEq>(
    ui: &mut Ui,
    painter: &Painter,
    well: Rect,
    id: &str,
    choices: &[(T, &str)],
    chosen: &mut T,
) {
    painter.rect_filled(well, 10, hexa(0x050d1c, 0.8));
    let segment = (well.width() - 6.0 - 3.0 * (choices.len() - 1) as f32) / choices.len() as f32;
    for (index, (value, label)) in choices.iter().enumerate() {
        let rect = Rect::from_min_size(
            pos2(
                well.left() + 3.0 + index as f32 * (segment + 3.0),
                well.top() + 3.0,
            ),
            vec2(segment, 32.0),
        );
        let response = ui.interact(rect, Id::unique((id, index)), Sense::click());
        let selected = *chosen == *value;
        if selected {
            painter.add(paint::rounded_vgradient(
                rect,
                [7.0; 4],
                &[(0.0, GOLD_HOT), (1.0, GOLD)],
            ));
            painter.line_segment(
                [
                    pos2(rect.left() + 7.0, rect.bottom() - 1.0),
                    pos2(rect.right() - 7.0, rect.bottom() - 1.0),
                ],
                Stroke::new(2.0, hex(0xc97d0e)),
            );
        } else if response.hovered() {
            painter.rect_filled(rect, 7, hexa(0xffffff, 0.07));
        }
        paint::text(
            painter,
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            bold_font(13.0),
            if selected {
                hex(0x1a2233)
            } else {
                hex(0xb9cfe2)
            },
            0.0,
        );
        if response.clicked() {
            *chosen = *value;
        }
    }
}

/// An on/off switch; returns whether it was flipped.
fn switch(ui: &mut Ui, painter: &Painter, rect: Rect, id: &str, on: &mut bool) {
    let response = ui.interact(rect, Id::unique(("switch", id)), Sense::click());
    if response.clicked() {
        *on = !*on;
    }
    let knob_travel = rect.width() - rect.height();
    let t = ui
        .ctx()
        .animate_bool_with_time(Id::unique(("switch knob", id)), *on, 0.12);
    let track = if *on {
        paint::mix(hex(0x34588c), GOLD, t)
    } else {
        paint::mix(hexa(0xffffff, 0.14), GOLD, t)
    };
    painter.rect_filled(rect, rect.height() / 2.0, track);
    if response.hovered() {
        painter.rect_stroke(
            rect,
            rect.height() / 2.0,
            Stroke::new(1.0, hexa(0xffffff, 0.4)),
            StrokeKind::Inside,
        );
    }
    let knob = pos2(
        rect.left() + rect.height() / 2.0 + knob_travel * t,
        rect.center().y,
    );
    painter.circle_filled(knob, rect.height() / 2.0 - 3.0, Color32::WHITE);
}

/// Settings: sound, graphics, fullscreen, Stats for nerds and (single player) the
/// battle speeds. Nothing changes until SAVE; CANCEL, Esc or the X leave every
/// setting as it was.
pub fn settings(
    context: &egui::Context,
    state: &mut UiState,
    art: &Art,
    view: &View,
    actions: &mut Vec<Action>,
) {
    let Some(mut draft) = state.settings_draft.take() else {
        return;
    };
    // The pause menu dims the arena already while it waits under Settings.
    if view.room || !(view.in_round && view.phase == MatchPhase::Paused) {
        dim_backdrop(context);
    }
    let mut close = None;
    if context.input(|input| input.key_pressed(egui::Key::Escape)) {
        close = Some(false);
    }
    dialog_with_padding(
        context,
        "settings",
        700.0,
        [38.0, 30.0, 32.0],
        |ui, painter, origin, width| {
            let close_rect = Rect::from_min_size(
                pos2(origin.x + width + 38.0 - 46.0, origin.y - 30.0 + 12.0),
                Vec2::splat(34.0),
            );
            if paint::close_button(ui, art, close_rect, Id::unique("settings close"))
                .on_hover_text("Close without saving (Esc)")
                .clicked()
            {
                close = Some(false);
            }
            let eyebrow = if view.room {
                "MULTIPLAYER"
            } else if view.in_round {
                "SINGLE PLAYER"
            } else {
                "BATTLE SETUP"
            };
            let mut y = origin.y
                + dialog_head(
                    painter,
                    origin,
                    width,
                    eyebrow,
                    "SETTINGS",
                    Color32::WHITE,
                    None,
                )
                + 12.0;
            let control_width = 376.0_f32.min(width * 0.61);
            let control_x = origin.x + width - control_width;
            // Sound.
            group_label(painter, pos2(origin.x, y + 8.0), width, "SOUND");
            y += 34.0;
            setting_label(
                painter,
                pos2(origin.x, y),
                "Sound",
                "Effects and engine noise",
            );
            slider(
                ui,
                painter,
                Rect::from_min_size(pos2(control_x, y), vec2(control_width, 44.0)),
                "volume",
                &mut draft.volume,
                (0.0, 1.0),
                DEFAULT_VOLUME,
                ("Mute", "Loud"),
            );
            y += 62.0;
            // Display.
            group_label(painter, pos2(origin.x, y + 8.0), width, "DISPLAY");
            y += 34.0;
            setting_label(
                painter,
                pos2(origin.x, y),
                "Graphics",
                "Auto keeps the frame rate smooth",
            );
            setting_choice(
                ui,
                painter,
                Rect::from_min_size(pos2(control_x, y), vec2(control_width, 38.0)),
                "graphics",
                &GRAPHICS_OPTIONS,
                &mut draft.graphics,
            );
            y += 50.0;
            // An iPad app always fills the screen.
            if !crate::touch::screen() {
                setting_label(
                    painter,
                    pos2(origin.x, y),
                    "Fullscreen",
                    if cfg!(target_os = "macos") {
                        "Fill the screen (Cmd+F)"
                    } else {
                        "Fill the screen (F11)"
                    },
                );
                switch(
                    ui,
                    painter,
                    Rect::from_min_size(pos2(origin.x + width - 46.0, y + 4.0), vec2(46.0, 26.0)),
                    "fullscreen",
                    &mut draft.fullscreen,
                );
                y += 46.0;
            }
            group_label(painter, pos2(origin.x, y + 8.0), width, "DEVELOPER");
            y += 34.0;
            setting_label(
                painter,
                pos2(origin.x, y),
                "Debug tools",
                "Extra levels and Stats for nerds",
            );
            setting_choice(
                ui,
                painter,
                Rect::from_min_size(pos2(control_x, y), vec2(control_width, 38.0)),
                "debug tools",
                &[(false, "Off"), (true, "On")],
                &mut draft.debug,
            );
            y += 58.0;
            if !view.room {
                // Battle speed: single player tunes its own battle.
                group_label(painter, pos2(origin.x, y + 8.0), width, "BATTLE SPEED");
                let reset =
                    Rect::from_min_size(pos2(origin.x + width - 143.0, y - 7.0), vec2(143.0, 30.0));
                let changed = draft.tank_speed != 1.0 || draft.bullet_speed != 1.0;
                let response = ui
                    .interact(
                        reset,
                        Id::unique("speeds reset"),
                        if changed {
                            Sense::click()
                        } else {
                            Sense::hover()
                        },
                    )
                    .on_hover_text("Both speeds back to 100%");
                painter.rect_filled(
                    reset,
                    8,
                    if changed {
                        hexa(0xffcf38, 0.1)
                    } else {
                        Color32::TRANSPARENT
                    },
                );
                painter.rect_stroke(
                    reset,
                    8,
                    Stroke::new(
                        1.0,
                        if changed {
                            hexa(0xffcf38, 0.4)
                        } else {
                            hexa(0x9fd0ff, 0.2)
                        },
                    ),
                    StrokeKind::Inside,
                );
                let ink = if !changed {
                    hex(0x8ea6bd)
                } else if response.hovered() {
                    Color32::WHITE
                } else {
                    GOLD_HOT
                };
                art.icon(
                    painter,
                    "reset",
                    Rect::from_center_size(
                        pos2(reset.left() + 18.0, reset.center().y),
                        Vec2::splat(14.0),
                    ),
                    ink,
                );
                paint::text(
                    painter,
                    pos2(reset.left() + 32.0, reset.center().y),
                    Align2::LEFT_CENTER,
                    "Reset to 100%",
                    bold_font(13.0),
                    ink,
                    0.0,
                );
                if changed && response.clicked() {
                    draft.tank_speed = 1.0;
                    draft.bullet_speed = 1.0;
                }
                y += 26.0;
                for (name, note, value, id) in [
                    (
                        "Tank base speed",
                        "How fast every tank drives",
                        &mut draft.tank_speed,
                        "tank speed",
                    ),
                    (
                        "Projectile base speed",
                        "How fast shells and rockets fly",
                        &mut draft.bullet_speed,
                        "bullet speed",
                    ),
                ] {
                    setting_label(painter, pos2(origin.x, y), name, note);
                    slider(
                        ui,
                        painter,
                        Rect::from_min_size(pos2(control_x, y), vec2(control_width, 44.0)),
                        id,
                        value,
                        (MIN_SPEED_SCALE, MAX_SPEED_SCALE),
                        1.0,
                        ("Slower", "Faster"),
                    );
                    y += 54.0;
                }
            }
            y += 10.0;
            let height = 48.0;
            let save_width = (width - 10.0) * 0.645;
            let save = Rect::from_min_size(pos2(origin.x, y), vec2(save_width, height));
            if paint::button_at(ui, art, save, "SAVE", None, ButtonStyle::Primary, true).clicked() {
                close = Some(true);
            }
            if paint::button_at(
                ui,
                art,
                Rect::from_min_size(
                    pos2(save.right() + 10.0, y),
                    vec2(width - save_width - 10.0, height),
                ),
                "CANCEL",
                None,
                ButtonStyle::Secondary,
                true,
            )
            .clicked()
            {
                close = Some(false);
            }
            y + height - origin.y
        },
    );
    match close {
        Some(true) => actions.push(Action::SettingsSaved(Box::new(draft))),
        Some(false) => {}
        None => state.settings_draft = Some(draft),
    }
}

/// A fresh draft of the saved settings.
pub fn open_settings(state: &mut UiState, prefs: &Preferences) {
    state.settings_draft = Some(prefs.clone());
}

/// The engine stopped: say so instead of drawing on.
pub fn failure(context: &egui::Context, message: &str) {
    dim_backdrop(context);
    dialog(context, "failure", 480.0, |_ui, painter, origin, width| {
        paint::eyebrow(painter, origin, "SLOPPY TANKS");
        paint::shadowed(
            painter,
            origin + vec2(0.0, 22.0),
            Align2::LEFT_TOP,
            "THE RENDERER STOPPED",
            display_font(34.0),
            RED,
            0.5,
        );
        let galley = paint::wrapped(
            painter,
            message,
            egui::FontId::proportional(13.0),
            hex(0xc4dbee),
            width,
        );
        let height = galley.size().y;
        painter.galley(origin + vec2(0.0, 70.0), galley, Color32::WHITE);
        paint::text(
            painter,
            origin + vec2(0.0, 82.0 + height),
            Align2::LEFT_TOP,
            "Restart Sloppy Tanks to play again.",
            bold_font(14.0),
            Color32::WHITE,
            0.2,
        );
        100.0 + height
    });
}

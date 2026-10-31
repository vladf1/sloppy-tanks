//! The battle's dialogs: their shared armour-plate frame, head and score plates, the
//! pause menu (`menuMarkup` in `ui-markup.ts`) and the Settings dialog
//! (`settings-dialog.ts`).

use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align2, Color32, Galley, Id, LayerId, Order, Painter, Pos2, Rect, Sense, Shape, Stroke,
    StrokeKind, Ui, Vec2, pos2, vec2,
};
use std::sync::Arc;

use super::icons::Art;
use super::paint::{self, ButtonStyle, hex, hexa};
use super::theme::{GOLD, GOLD_HOT, RED, TEXT_LABEL, bold_font, display_font};
use super::{Action, PauseTab, UiState, View};
use crate::preferences::{Preferences, TouchSetting};
use sloppy_core::sim::speed_tuning::{MAX_SPEED_SCALE, MIN_SPEED_SCALE};
use sloppy_core::sim::{GameMode, Team};
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
/// The page's padding round a dialog (`#overlay`), and a phone's (`.phone #overlay`).
const DIALOG_MARGIN: f32 = 24.0;
const PHONE_DIALOG_MARGIN: f32 = 8.0;
/// A phone's pause dialog: its width and padding, and its buttons' height.
const PHONE_PAUSE_WIDTH: f32 = 420.0;
const PHONE_PAUSE_PADDING: [f32; 3] = [22.0, 22.0, 22.0];
pub const PHONE_BUTTON_HEIGHT: f32 = 52.0;
/// Pause's END BATTLE (`.pause-actions #end-battle`): a phone's at its label's width with
/// this padding each side (the page's 16, less the 4 its icon tucks into it, shared), and
/// on every screen the margin that sets it apart from RESUME, a hairline this far out.
const END_BATTLE_PADDING: f32 = 14.0;
const END_BATTLE_MARGIN: f32 = 22.0;
const END_BATTLE_HAIRLINE: f32 = 12.0;
/// The corner X (`.dialog-close`): its side and its inset from the plate's top-right.
const CLOSE_SIZE: f32 = 32.0;
const CLOSE_INSET: f32 = 12.0;
/// The eyebrow's line over a dialog's title (`dialog_head`).
const EYEBROW_LINE: f32 = 22.0;
/// The space between a phone's title and its score when they share a row.
const TITLE_GAP: f32 = 16.0;
/// Settings beside its rows (`.settings`), and its padding. A screen narrower than this
/// stands each control under its name (`stacked`), at most a phone's dialog wide.
const SETTINGS_WIDTH: f32 = 700.0;
const SETTINGS_PADDING: [f32; 3] = [38.0, 30.0, 32.0];
const PHONE_SETTINGS_WIDTH: f32 = 420.0;
/// A setting's row, its hairline included (`.setting-row`); a stacked row, whose control
/// stands `STACKED_LABEL` lower, under its name.
const SETTING_ROW: f32 = 58.0;
const STACKED_ROW: f32 = 96.0;
const STACKED_LABEL: f32 = 38.0;
/// Where a choice's well and a slider stand in a row beside their name.
const CHOICE_OFFSET: f32 = 10.0;
const SLIDER_OFFSET: f32 = 6.0;
/// The space from the head to the first group's heading, over each later heading, and
/// over the dialog's buttons; and the battle speed group's reset action's height.
const FIRST_GROUP_GAP: f32 = 5.5;
const GROUP_GAP: f32 = 12.0;
const SETTINGS_ACTIONS_GAP: f32 = 22.0;
const RESET_HEIGHT: f32 = 30.0;
/// A pause fact's cell: its least height (half the pane's 216), and its padding.
const FACT_ROW: f32 = 107.5;
const FACT_PADDING: [f32; 3] = [14.0, 16.0, 12.0];
/// Settings' Touch controls choices (`TOUCH_MODES`).
const TOUCH_SETTINGS: [(TouchSetting, &str); 3] = [
    (TouchSetting::Auto, "Auto"),
    (TouchSetting::On, "On"),
    (TouchSetting::Off, "Off"),
];

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
    let room = screen.shrink(dialog_margin()).size();
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
            // No drop shadow: the page's armour plate clips its own box shadow away
            // (`:is(.menu:not(.start), .settings)` keeps only inset ones under its
            // `clip-path`), so the plate sits straight on the blurred arena.
            painter.set(slot, Shape::Vec(paint::plate_shapes(rect, 14.0, None)));
            ui.allocate_rect(rect, Sense::hover());
        });
    let size = shown.response.rect.size();
    let scale = (room.x / size.x).min(room.y / size.y).min(1.0);
    paint::shrink_about(context, shown.response.layer_id, screen.center(), scale);
}

/// The room a dialog has on screen, inside the margin round it.
fn dialog_room(context: &egui::Context) -> Vec2 {
    context.content_rect().shrink(dialog_margin()).size()
}

fn dialog_margin() -> f32 {
    if crate::touch::phone() {
        PHONE_DIALOG_MARGIN
    } else {
        DIALOG_MARGIN
    }
}

/// A phone's dialog `width`, narrowed to fit the screen, as `max-width` narrows the page's:
/// laid out to the screen at full size, it never shrinks to fit across.
pub(super) fn phone_width(context: &egui::Context, width: f32) -> f32 {
    width.min(dialog_room(context).x)
}

/// The corner X's square in a dialog laid out from `origin` in `width` with `padding`.
fn close_rect(origin: Pos2, width: f32, padding: [f32; 3]) -> Rect {
    Rect::from_min_size(
        pos2(
            origin.x + width + padding[0] - CLOSE_INSET - CLOSE_SIZE,
            origin.y - padding[1] + CLOSE_INSET,
        ),
        Vec2::splat(CLOSE_SIZE),
    )
}

/// Text wrapped between words at `width`, `line` points a line, each line centred; laid
/// out to measure before it is drawn with [`draw_centred`]. A word wider than `width`
/// stands whole past it, as the page's text does, rather than break.
fn centred_lines(
    painter: &Painter,
    text: &str,
    font: egui::FontId,
    color: Color32,
    spacing: f32,
    width: f32,
    line: f32,
) -> Arc<Galley> {
    let widest_word = text
        .split_whitespace()
        .map(|word| paint::text_width(painter, word, font.clone(), spacing))
        .fold(0.0, f32::max);
    let mut job = LayoutJob::default();
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: font,
            color,
            extra_letter_spacing: spacing,
            line_height: Some(line),
            ..Default::default()
        },
    );
    job.wrap.max_width = width.max(widest_word);
    job.halign = egui::Align::Center;
    painter.layout_job(job)
}

/// Draw lines from [`centred_lines`] from `top_center` down; returns their height.
fn draw_centred(painter: &Painter, top_center: Pos2, galley: Arc<Galley>) -> f32 {
    let height = galley.size().y;
    painter.galley(top_center, galley, Color32::WHITE);
    height
}

/// The room between the score plates, and how far each plate's outer side slants.
const SCORE_COLON: f32 = 18.0;
const PLATE_SLANT: f32 = 10.0;
/// The glow under the viewer's gold rim (`inset 0 14px 16px -12px #ffcf3880`): the gold's
/// opacity at each depth under the plate's top as that blurred inset shadow falls off.
const YOURS_GLOW: [(f32, f32); 7] = [
    (3.0, 0.225),
    (5.0, 0.177),
    (8.0, 0.113),
    (11.0, 0.065),
    (14.0, 0.033),
    (18.0, 0.011),
    (22.0, 0.0),
];

/// A score plate's name, with YOU on the viewer's side.
fn plate_label(names: &[&str; 2], team: Team, index: usize) -> String {
    let name = names[index].to_uppercase();
    if index == team.index() {
        format!("{name} · YOU")
    } else {
        name
    }
}

/// Each score plate's width: its score or its name, whichever is wider.
fn plate_widths(painter: &Painter, scores: [u32; 2], names: &[&str; 2], team: Team) -> [f32; 2] {
    [0, 1].map(|index| {
        let score = paint::text_width(
            painter,
            &format!("{}", scores[index]),
            display_font(34.0),
            0.0,
        );
        let name = paint::text_width(
            painter,
            &plate_label(names, team, index),
            bold_font(11.0),
            1.6,
        );
        score.max(name).max(30.0) + 28.0 + 10.0
    })
}

/// The width [`score_plates`] takes, to centre them.
pub fn score_plates_width(
    painter: &Painter,
    scores: [u32; 2],
    names: &[&str; 2],
    team: Team,
) -> f32 {
    let [left, right] = plate_widths(painter, scores, names, team);
    left + SCORE_COLON + right
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
    let label = |index: usize| plate_label(names, team, index);
    let widths = plate_widths(painter, scores, names, team);
    let height = 54.0;
    let total = widths[0] + SCORE_COLON + widths[1];
    let colon = SCORE_COLON;
    let left = right_center.x - total;
    let top = right_center.y - height / 2.0;
    for index in 0..2 {
        let x = if index == 0 {
            left
        } else {
            left + widths[0] + colon
        };
        let width = widths[index];
        // The plate's sides at `depth` under its top: blue leans in at its left, red at
        // its right, so the gold rim and wash of the viewer's side follow the slant.
        let band = |from: f32, to: f32| {
            let sides = |depth: f32| {
                let lean = PLATE_SLANT * depth / height;
                if index == 0 {
                    (x + PLATE_SLANT - lean, x + width)
                } else {
                    (x, x + width - lean)
                }
            };
            let ((top_left, top_right), (bottom_left, bottom_right)) = (sides(from), sides(to));
            [
                pos2(top_left, top + from),
                pos2(top_right, top + from),
                pos2(bottom_right, top + to),
                pos2(bottom_left, top + to),
            ]
        };
        let (light, deep) = if index == 0 {
            (hex(0x2f8ff2), hex(0x1252b0))
        } else {
            (hex(0xff5367), hex(0xb01b33))
        };
        painter.add(paint::gradient_polygon(&band(0.0, height), light, deep));
        // The glossy top half (`#ffffff33` to clear at 50%).
        painter.add(paint::gradient_polygon(
            &band(0.0, height / 2.0),
            hexa(0xffffff, 0.2),
            Color32::TRANSPARENT,
        ));
        let center = x + width / 2.0 + if index == 0 { 5.0 } else { -5.0 };
        if index == team as usize {
            // The soft gold glow under the rim, band by band down its fall-off.
            for pair in YOURS_GLOW.windows(2) {
                let [(from, from_alpha), (to, to_alpha)] = [pair[0], pair[1]];
                painter.add(paint::gradient_polygon(
                    &band(from, to),
                    hexa(0xffcf38, from_alpha),
                    hexa(0xffcf38, to_alpha),
                ));
            }
            painter.add(Shape::convex_polygon(
                band(0.0, 3.0).to_vec(),
                GOLD,
                Stroke::NONE,
            ));
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
    let title_top = origin.y + EYEBROW_LINE;
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
    EYEBROW_LINE + 58.0
}

/// A phone's dialog head (`.phone .menu.compact .dialog-title`): the title and its score
/// centred on one row where both fit left of the corner X (`clear` of the width), else
/// the title centred over the score. Returns the height used.
fn phone_head(
    painter: &Painter,
    origin: Pos2,
    width: f32,
    clear: f32,
    title: &str,
    score: Option<(&Hud, [u32; 2])>,
) -> f32 {
    let font = display_font(50.0);
    let title_width = paint::text_width(painter, title, font.clone(), 1.0);
    let Some((hud, scores)) = score else {
        let center = pos2(origin.x + width / 2.0, origin.y);
        paint::shadowed(
            painter,
            center,
            Align2::CENTER_TOP,
            title,
            font,
            Color32::WHITE,
            1.0,
        );
        return 58.0;
    };
    let names = &hud.team_names;
    let plates = score_plates_width(painter, scores, names, hud.human_team);
    let row = title_width + TITLE_GAP + plates;
    // On one row the title stands beside the plates as on a wider dialog's head. Over
    // them, its letters (which start well under the top of their line) sit at the head's
    // top and the plates a few points under their feet.
    let (title_top, title_left, plates_right, height) = if row <= clear {
        // Centred, unless that would run the score under the X.
        let left = origin.x + ((width - row) / 2.0).min(clear - row);
        (
            origin.y + 3.0,
            left,
            pos2(left + row, origin.y + 29.0),
            58.0,
        )
    } else {
        (
            origin.y - 12.0,
            origin.x + (width - title_width) / 2.0,
            pos2(origin.x + (width + plates) / 2.0, origin.y + 69.0),
            96.0,
        )
    };
    paint::shadowed(
        painter,
        pos2(title_left, title_top),
        Align2::LEFT_TOP,
        title,
        font,
        Color32::WHITE,
        1.0,
    );
    score_plates(painter, plates_right, scores, names, hud.human_team);
    height
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
    paint::rounded_vgradient(painter, tip, [4.0; 4], &[(0.0, GOLD_HOT), (1.0, GOLD)]);
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

/// A grid of facts (`.pause-facts`): caption, value and an optional qualifier per cell,
/// each wrapping to the cell's width where it does not fit. Every row takes the
/// tallest cell's height.
fn facts(painter: &Painter, origin: Pos2, width: f32, cells: &[PauseFact]) -> f32 {
    let columns = 3;
    let rows = cells.len().div_ceil(columns);
    let cell_width = (width - (columns - 1) as f32) / columns as f32;
    let [side, top, bottom] = FACT_PADDING;
    let text_width = cell_width - 2.0 * side;
    let lines: Vec<_> = cells
        .iter()
        .map(|fact| {
            let caption = centred_lines(
                painter,
                &fact.label.to_uppercase(),
                bold_font(12.0),
                hex(0xa9c6dc),
                1.4,
                text_width,
                14.4,
            );
            let size = if fact.value.len() > 14 { 20.0 } else { 28.0 };
            let value = centred_lines(
                painter,
                &fact.value,
                display_font(size),
                Color32::WHITE,
                0.4,
                text_width,
                size,
            );
            let note = (!fact.note.is_empty()).then(|| {
                centred_lines(
                    painter,
                    &fact.note.to_uppercase(),
                    bold_font(12.0),
                    hex(0xa9c6dc),
                    1.0,
                    text_width,
                    14.4,
                )
            });
            (caption, value, note)
        })
        .collect();
    let cell_height = lines
        .iter()
        .map(|(caption, value, note)| {
            top + caption.size().y
                + 7.0
                + value.size().y
                + note.as_ref().map_or(0.0, |note| 4.0 + note.size().y)
                + bottom
        })
        .fold(FACT_ROW, f32::max);
    let total = Rect::from_min_size(origin, vec2(width, rows as f32 * (cell_height + 1.0) - 1.0));
    painter.rect_filled(total, 12, hexa(0x9fd0ff, 0.08));
    for (index, (caption, value, note)) in lines.into_iter().enumerate() {
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
        // Hung from the cell's top, so the values of a row share one line.
        let x = cell.center().x;
        let mut y = cell.top() + top;
        y += draw_centred(painter, pos2(x, y), caption) + 7.0;
        y += draw_centred(painter, pos2(x, y), value) + 4.0;
        if let Some(note) = note {
            draw_centred(painter, pos2(x, y), note);
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
        paint::labelled_choice(&response, true, selected, label);
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
        // Centred over the underline's 3 points, as in a tab with a bottom border.
        paint::text(
            painter,
            rect.center() - vec2(0.0, 1.5),
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
    // A phone's pause is the title and score, the battle's facts and the actions: no
    // eyebrow and no Controls tab (its only controls are on screen).
    let phone = crate::touch::phone();
    let (dialog_width, padding) = if phone {
        (phone_width(context, PHONE_PAUSE_WIDTH), PHONE_PAUSE_PADDING)
    } else {
        (780.0, [42.0, 34.0, 36.0])
    };
    dialog_with_padding(
        context,
        "pause menu",
        dialog_width,
        padding,
        |ui, painter, origin, width| {
            let close = close_rect(origin, width, padding);
            let score = (!solo).then_some((&hud, hud.match_state.scores));
            let mut y = origin.y;
            if phone {
                let clear = close.left() - 6.0 - origin.x;
                y += phone_head(painter, origin, width, clear, "PAUSED", score);
            } else {
                let eyebrow = format!("{mode} · {}", hud.map_name.to_uppercase());
                y += dialog_head(
                    painter,
                    origin,
                    width,
                    &eyebrow,
                    "PAUSED",
                    Color32::WHITE,
                    score,
                );
            }
            // Over the head, so no score plate ever covers it.
            if paint::close_button(ui, art, close, Id::unique("pause close"))
                .on_hover_text("Resume (Esc)")
                .clicked()
            {
                actions.push(Action::Resume);
            }
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
                // `.dialog-standing`: under the head's gap and its own 4 points, a 15 point
                // line led by an 8 point dot, which glows only ahead or behind.
                let line_y = y + 6.0 + 4.0 + 19.5 / 2.0;
                let dot = pos2(origin.x + 4.0, line_y);
                if standing.tone != StandingTone::Level {
                    painter.add(paint::radial(
                        dot,
                        Vec2::splat(12.0),
                        &[
                            (0.0, paint::fade(tone, 0.5)),
                            (0.33, paint::fade(tone, 0.4)),
                            (1.0, Color32::TRANSPARENT),
                        ],
                    ));
                }
                painter.circle_filled(dot, 4.0, tone);
                paint::text(
                    painter,
                    pos2(origin.x + 17.0, line_y),
                    Align2::LEFT_CENTER,
                    &standing.text,
                    egui::FontId::proportional(15.0),
                    hex(0xd5e3f0),
                    0.0,
                );
            }
            y += 30.0;
            if phone {
                y += battle_pane(painter, pos2(origin.x, y), width, &pause) + 16.0;
                return y + phone_pause_actions(ui, art, pos2(origin.x, y), width, actions)
                    - origin.y;
            }
            y += 18.0;
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
            // Both panes share one height, so switching tabs never moves the buttons, unless
            // a fact wraps onto more lines than the pane holds.
            let pane_height: f32 = if solo { 72.0 + 216.0 } else { 216.0 };
            let facts_height = match tab {
                PauseTab::Battle => battle_pane(painter, pos2(origin.x, y), width, &pause),
                PauseTab::Controls => {
                    controls_card(painter, pos2(origin.x, y), width, 2);
                    0.0
                }
            };
            y += pane_height.max(facts_height) + 20.0;
            let button_height = 48.0;
            let end_width = 160.0;
            let action_gap = 10.0;
            let resume_left = origin.x + end_width + END_BATTLE_MARGIN + action_gap;
            let secondary_width = (origin.x + width - resume_left - action_gap) / 2.4;
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
            let hairline = origin.x + end_width + END_BATTLE_HAIRLINE;
            painter.line_segment(
                [
                    pos2(hairline, y + 8.0),
                    pos2(hairline, y + button_height - 8.0),
                ],
                Stroke::new(1.0, hexa(0x9fd0ff, 0.18)),
            );
            let resume =
                Rect::from_min_size(pos2(resume_left, y), vec2(resume_width, button_height));
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

/// A phone's pause actions at `origin`: END BATTLE at its own width, set apart from RESUME
/// behind a hairline as on a wider dialog, and SETTINGS under them, each a full thumb's
/// height. Returns the height used.
fn phone_pause_actions(
    ui: &mut Ui,
    art: &Art,
    origin: Pos2,
    width: f32,
    actions: &mut Vec<Action>,
) -> f32 {
    let height = PHONE_BUTTON_HEIGHT;
    let gap = 10.0;
    let end_width =
        paint::button_content_width(ui.painter(), "END BATTLE", true) + 2.0 * END_BATTLE_PADDING;
    let resume_left = origin.x + end_width + END_BATTLE_MARGIN + gap;
    let hairline = origin.x + end_width + END_BATTLE_HAIRLINE;
    ui.painter().line_segment(
        [
            pos2(hairline, origin.y + 8.0),
            pos2(hairline, origin.y + height - 8.0),
        ],
        Stroke::new(1.0, hexa(0x9fd0ff, 0.18)),
    );
    let row =
        |left: f32, width: f32, top: f32| Rect::from_min_size(pos2(left, top), vec2(width, height));
    let buttons = [
        (
            row(origin.x, end_width, origin.y),
            "END BATTLE",
            "end",
            ButtonStyle::Danger,
            Action::EndBattle,
        ),
        (
            row(resume_left, origin.x + width - resume_left, origin.y),
            "RESUME",
            "resume",
            ButtonStyle::Primary,
            Action::Resume,
        ),
        (
            row(origin.x, width, origin.y + height + gap),
            "SETTINGS",
            "sliders",
            ButtonStyle::Secondary,
            Action::OpenSettings,
        ),
    ];
    for (rect, label, icon, style, action) in buttons {
        if paint::button_at(ui, art, rect, label, Some(icon), style, true).clicked() {
            actions.push(action);
        }
    }
    2.0 * height + gap
}

/// Where Settings' rows go: each control beside its name, or under it when the dialog
/// is too narrow (`stacked`).
#[derive(Clone, Copy)]
struct SettingRows {
    left: f32,
    width: f32,
    control_left: f32,
    control_width: f32,
    stacked: bool,
}

impl SettingRows {
    fn row_height(&self) -> f32 {
        if self.stacked {
            STACKED_ROW
        } else {
            SETTING_ROW
        }
    }

    /// A control `height` tall in the row from `top`, `offset` below the row's top when
    /// beside its name (a stacked row puts it under the name).
    fn control(&self, top: f32, offset: f32, height: f32) -> Rect {
        let under = if self.stacked { STACKED_LABEL } else { 0.0 };
        Rect::from_min_size(
            pos2(self.control_left, top + offset + under),
            vec2(self.control_width, height),
        )
    }

    /// A row from `top` (`.setting-row`): its name with the note under it, centred beside
    /// the control or over it, and the hairline closing it. Returns the row's bottom.
    fn row(&self, painter: &Painter, top: f32, name: &str, note: &str) -> f32 {
        let name_y = top + if self.stacked { 17.0 } else { 19.5 };
        paint::text(
            painter,
            pos2(self.left, name_y),
            Align2::LEFT_CENTER,
            name,
            bold_font(14.0),
            Color32::WHITE,
            0.2,
        );
        paint::text(
            painter,
            pos2(self.left, name_y + 19.5),
            Align2::LEFT_CENTER,
            note,
            egui::FontId::proportional(13.0),
            hex(0xb7cde1),
            0.0,
        );
        let bottom = top + self.row_height();
        self.rule(painter, bottom - 0.5, hexa(0x9fd0ff, 0.08));
        bottom
    }

    /// A group's heading over its hairline (`.setting-group-label`), from `top`, with
    /// `action_height` of room for its own action at the row's end. Returns where its
    /// rows start.
    fn group(&self, painter: &Painter, top: f32, label: &str, action_height: f32) -> f32 {
        let content = action_height.max(28.0);
        paint::text(
            painter,
            pos2(self.left, top + content / 2.0),
            Align2::LEFT_CENTER,
            label,
            bold_font(13.0),
            TEXT_LABEL,
            2.0,
        );
        let rule = top + content + 5.5;
        self.rule(painter, rule, hexa(0x9fd0ff, 0.1));
        rule + 0.5
    }

    fn rule(&self, painter: &Painter, y: f32, color: Color32) {
        painter.line_segment(
            [pos2(self.left, y), pos2(self.left + self.width, y)],
            Stroke::new(1.0, color),
        );
    }
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
    response.widget_info(|| egui::WidgetInfo::slider(true, *value, id));
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
    // The default's notch stands under the track, where the thumb never covers it.
    let notch_share = ((default - range.0) / (range.1 - range.0)) as f32;
    let notch_x = track.left() + track.width() * notch_share;
    painter.line_segment(
        [
            pos2(notch_x, track.bottom() + 7.0),
            pos2(notch_x, track.bottom() + 14.0),
        ],
        Stroke::new(2.0, hexa(0xffffff, 0.75)),
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
        (ends.0, track.left(), Align2::LEFT_CENTER),
        (ends.1, track.right(), Align2::RIGHT_CENTER),
    ] {
        paint::text(
            painter,
            pos2(x, track.center().y + 22.0),
            align,
            text,
            bold_font(13.0),
            hex(0xb7cde1),
            0.3,
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
        paint::labelled_choice(&response, true, selected, &format!("{id}: {label}"));
        if selected {
            paint::rounded_vgradient(painter, rect, [7.0; 4], &[(0.0, GOLD_HOT), (1.0, GOLD)]);
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
    paint::labelled_choice(&response, true, *on, id);
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
/// setting as it was. A screen too short for it scrolls the dialog, as the page's
/// does, rather than shrinking its type.
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
    // Its own backdrop, over the pause menu's when it opens from there.
    dim_backdrop(context);
    let mut close = None;
    if context.input(|input| input.key_pressed(egui::Key::Escape)) {
        close = Some(false);
    }
    let room = dialog_room(context);
    let stacked = room.x < SETTINGS_WIDTH;
    let (dialog_width, padding) = if stacked {
        (
            phone_width(context, PHONE_SETTINGS_WIDTH),
            PHONE_PAUSE_PADDING,
        )
    } else {
        (SETTINGS_WIDTH, SETTINGS_PADDING)
    };
    dialog_with_padding(
        context,
        "settings",
        dialog_width,
        padding,
        |ui, _, origin, width| {
            let [side, top, bottom] = padding;
            // The plate's whole inside scrolls, its padding with it.
            let plate =
                Rect::from_min_size(origin - vec2(side, top), vec2(width + 2.0 * side, room.y));
            let mut body = ui.new_child(egui::UiBuilder::new().max_rect(plate));
            let shown = egui::ScrollArea::vertical()
                .id_salt("settings")
                .max_height(room.y)
                .auto_shrink([false, true])
                .show(&mut body, |ui| {
                    let origin = ui.max_rect().min + vec2(side, top);
                    let height = settings_body(
                        ui, art, view, &mut draft, &mut close, origin, width, padding, stacked,
                    );
                    ui.set_min_size(vec2(plate.width(), top + height + bottom));
                });
            shown.content_size.y.min(room.y) - top - bottom
        },
    );
    match close {
        Some(true) => actions.push(Action::SettingsSaved(Box::new(draft))),
        Some(false) => {}
        None => state.settings_draft = Some(draft),
    }
}

/// Settings' head, rows and buttons from `origin` in `width`; returns the height used.
/// `close` becomes whether to keep the draft once the dialog is done.
#[allow(clippy::too_many_arguments)]
fn settings_body(
    ui: &mut Ui,
    art: &Art,
    view: &View,
    draft: &mut Preferences,
    close: &mut Option<bool>,
    origin: Pos2,
    width: f32,
    padding: [f32; 3],
    stacked: bool,
) -> f32 {
    let painter = ui.painter().clone();
    let painter = &painter;
    let eyebrow = if view.room {
        "MULTIPLAYER"
    } else if view.in_round {
        "SINGLE PLAYER"
    } else {
        "BATTLE SETUP"
    };
    let head = dialog_head(
        painter,
        origin,
        width,
        eyebrow,
        "SETTINGS",
        Color32::WHITE,
        None,
    );
    if paint::close_button(
        ui,
        art,
        close_rect(origin, width, padding),
        Id::unique("settings close"),
    )
    .on_hover_text("Close without saving (Esc)")
    .clicked()
    {
        *close = Some(false);
    }
    let control_width = if stacked {
        width
    } else {
        376.0_f32.min(width * 0.61)
    };
    let rows = SettingRows {
        left: origin.x,
        width,
        control_left: origin.x + width - control_width,
        control_width,
        stacked,
    };
    // Sound, and a page's touch controls. The phone edition has only touch, so it keeps
    // no choice of it.
    let touch_setting = crate::touch::offers_setting();
    let group = if touch_setting {
        "SOUND & INPUT"
    } else {
        "SOUND"
    };
    let mut y = rows.group(painter, origin.y + head + FIRST_GROUP_GAP, group, 0.0);
    slider(
        ui,
        painter,
        rows.control(y, SLIDER_OFFSET, 26.0),
        "volume",
        &mut draft.volume,
        (0.0, 1.0),
        DEFAULT_VOLUME,
        ("Mute", "Loud"),
    );
    y = rows.row(painter, y, "Sound", "Effects and engine noise");
    if touch_setting {
        setting_choice(
            ui,
            painter,
            rows.control(y, CHOICE_OFFSET, 38.0),
            "touch controls",
            &TOUCH_SETTINGS,
            &mut draft.touch,
        );
        y = rows.row(
            painter,
            y,
            "Touch controls",
            "Auto shows them on touchscreens",
        );
    }
    y = rows.group(painter, y + GROUP_GAP, "DISPLAY", 0.0);
    setting_choice(
        ui,
        painter,
        rows.control(y, CHOICE_OFFSET, 38.0),
        "graphics",
        &GRAPHICS_OPTIONS,
        &mut draft.graphics,
    );
    y = rows.row(painter, y, "Graphics", "Auto keeps the frame rate smooth");
    // A page goes fullscreen only from a press (the HUD's button), so it keeps no
    // fullscreen choice, as the old page kept none.
    if crate::app::offers_fullscreen() && cfg!(not(target_arch = "wasm32")) {
        // A switch is small enough to stay beside its name in any width.
        let rows = SettingRows {
            stacked: false,
            ..rows
        };
        switch(
            ui,
            painter,
            Rect::from_min_size(
                pos2(origin.x + width - 46.0, y + (SETTING_ROW - 26.0) / 2.0),
                vec2(46.0, 26.0),
            ),
            "fullscreen",
            &mut draft.fullscreen,
        );
        y = rows.row(
            painter,
            y,
            "Fullscreen",
            if cfg!(target_os = "macos") {
                "Fill the screen (Cmd+F)"
            } else {
                "Fill the screen (F11)"
            },
        );
    }
    y = rows.group(painter, y + GROUP_GAP, "DEVELOPER", 0.0);
    setting_choice(
        ui,
        painter,
        rows.control(y, CHOICE_OFFSET, 38.0),
        "debug tools",
        &[(false, "Off"), (true, "On")],
        &mut draft.debug,
    );
    y = rows.row(
        painter,
        y,
        "Debug tools",
        "Extra levels and Stats for nerds",
    );
    if !view.room {
        // Battle speed: single player tunes its own battle.
        let group_top = y + GROUP_GAP;
        let reset = Rect::from_min_size(
            pos2(origin.x + width - 143.0, group_top),
            vec2(143.0, RESET_HEIGHT),
        );
        y = rows.group(painter, group_top, "BATTLE SPEED", RESET_HEIGHT);
        reset_speeds(ui, painter, art, reset, draft);
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
            slider(
                ui,
                painter,
                rows.control(y, SLIDER_OFFSET, 26.0),
                id,
                value,
                (MIN_SPEED_SCALE, MAX_SPEED_SCALE),
                1.0,
                ("Slower", "Faster"),
            );
            y = rows.row(painter, y, name, note);
        }
    }
    y += SETTINGS_ACTIONS_GAP;
    let height = if stacked { PHONE_BUTTON_HEIGHT } else { 48.0 };
    let save_width = (width - 10.0) * 0.645;
    let save = Rect::from_min_size(pos2(origin.x, y), vec2(save_width, height));
    if paint::button_at(ui, art, save, "SAVE", None, ButtonStyle::Primary, true).clicked() {
        *close = Some(true);
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
        *close = Some(false);
    }
    y + height - origin.y
}

/// The battle speed group's own action (`.group-action`): both speeds back to 100%,
/// dimmed in its place while they are there already.
fn reset_speeds(ui: &mut Ui, painter: &Painter, art: &Art, rect: Rect, draft: &mut Preferences) {
    let changed = draft.tank_speed != 1.0 || draft.bullet_speed != 1.0;
    let response = ui
        .interact(
            rect,
            Id::unique("speeds reset"),
            if changed {
                Sense::click()
            } else {
                Sense::hover()
            },
        )
        .on_hover_text("Both speeds back to 100%");
    paint::labelled(&response, changed, "Reset speeds");
    painter.rect_filled(
        rect,
        8,
        if changed {
            hexa(0xffcf38, 0.1)
        } else {
            Color32::TRANSPARENT
        },
    );
    painter.rect_stroke(
        rect,
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
        Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), Vec2::splat(14.0)),
        ink,
    );
    paint::text(
        painter,
        pos2(rect.left() + 32.0, rect.center().y),
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
            if cfg!(target_arch = "wasm32") {
                "Reload the page to play again."
            } else {
                "Restart Sloppy Tanks to play again."
            },
            bold_font(14.0),
            Color32::WHITE,
            0.2,
        );
        100.0 + height
    });
}

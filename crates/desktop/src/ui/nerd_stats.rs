//! Stats for nerds (`src/game/nerd-stats.ts`): the engine's sections and rows
//! (`sloppy_web::nerd_rows`) in folding sections, shown while the debug tools are on and
//! N has opened the panel, each row explained on hover. Single player and a room differ
//! only in the rows the engine writes.

use egui::{Align2, Color32, Id, Order, Rect, Sense, Stroke, StrokeKind, pos2, vec2};

use super::UiState;
use super::paint::{self, hex, hexa};
use super::theme::{TEXT_LABEL, bold_font, display_font};

/// The web panel is 32 monospace characters plus 24 pixels, at 11px type.
const PANEL_COLUMNS: f32 = 32.0;
const ROW_HEIGHT: f32 = 16.5;
const SECTION_HEAD: f32 = 20.8;
const SECTION_GAP: f32 = 6.0;
const HEADER_HEIGHT: f32 = 37.0;
const PANEL_PADDING: f32 = 7.0;

/// Whether `section` shows its rows: every one but Configuration until folded, as the
/// page's panel opens.
fn section_open(state: &UiState, section: &str) -> bool {
    state
        .nerd_sections
        .get(section)
        .copied()
        .unwrap_or(section != "Configuration")
}

/// The corner link that opens the panel (`LINK_LABEL` in `nerd-stats.ts`): N does the
/// same, but a touch screen may have no keyboard.
pub fn link(context: &egui::Context, state: &mut UiState) {
    let screen = context.content_rect();
    let font = egui::FontId::proportional(12.0);
    let painter = context.layer_painter(egui::LayerId::background());
    let width = paint::text_width(&painter, "nerd stats", font.clone(), 0.0);
    let label = Rect::from_min_size(
        pos2(screen.right() - 13.0 - width, screen.bottom() - 10.0 - 14.0),
        vec2(width, 14.0),
    );
    // The page's 5px padding, and a little more for a finger.
    let target = label.expand(if crate::touch::screen() { 10.0 } else { 5.0 });
    egui::Area::new(Id::unique("nerd stats link"))
        .order(Order::Foreground)
        .fixed_pos(target.min)
        .show(context, |ui| {
            let response = ui
                .allocate_exact_size(target.size(), Sense::click())
                .1
                .on_hover_text("Stats for Nerds (N)");
            paint::labelled(&response, true, "Stats for Nerds");
            paint::shadowed(
                ui.painter(),
                label.left_center(),
                Align2::LEFT_CENTER,
                "nerd stats",
                font,
                hexa(0xffffff, if response.hovered() { 0.9 } else { 0.65 }),
                0.0,
            );
            if response.clicked() {
                state.show_nerd_stats = true;
            }
        });
}

pub fn panel(context: &egui::Context, state: &mut UiState) {
    let Some(list) = state.stats.as_ref() else {
        return;
    };
    let screen = context.content_rect();
    let row_font = egui::FontId::monospace(11.0);
    let font_painter = context.layer_painter(egui::LayerId::background());
    let character_width = paint::text_width(&font_painter, "0", row_font.clone(), 0.0);
    let panel_width = (PANEL_COLUMNS * character_width + 24.0).min(screen.width() - 16.0);
    // Values start one space past the widest label, measured, since a page has no
    // monospace face to pad labels in.
    let value_column = list
        .iter()
        .flat_map(|section| &section.rows)
        .map(|row| paint::text_width(&font_painter, row.label, row_font.clone(), 0.0))
        .fold(0.0, f32::max)
        + paint::text_width(&font_painter, "0", row_font.clone(), 0.0);
    let rows: f32 = list
        .iter()
        .map(|section| {
            SECTION_HEAD
                + if section_open(state, section.title) {
                    section.rows.len() as f32 * ROW_HEIGHT
                } else {
                    0.0
                }
        })
        .sum();
    let height = (HEADER_HEIGHT + rows + SECTION_GAP * (list.len() - 1) as f32 + PANEL_PADDING)
        .min(screen.height() - 24.0);
    let mut toggled = None;
    let mut closed = false;
    egui::Area::new(Id::unique("nerd stats"))
        .order(Order::Foreground)
        .fixed_pos(pos2(
            screen.right() - 8.0 - panel_width,
            screen.bottom() - 5.0 - height,
        ))
        .show(context, |ui| {
            let rect = Rect::from_min_size(ui.cursor().min, vec2(panel_width, height));
            let painter = ui.painter().clone();
            painter.add(paint::rounded_vgradient(
                rect,
                [12.0; 4],
                &[(0.0, hexa(0x11264a, 0.94)), (1.0, hexa(0x08142a, 0.95))],
            ));
            painter.rect_stroke(
                rect,
                12,
                Stroke::new(1.0, hexa(0x9fd0ff, 0.15)),
                StrokeKind::Inside,
            );
            let left = rect.left() + 13.0;
            let header = Rect::from_min_max(
                rect.min,
                pos2(rect.right() - 44.0, rect.top() + HEADER_HEIGHT),
            );
            paint::text(
                &painter,
                pos2(left, rect.top() + 12.0),
                Align2::LEFT_TOP,
                "Stats for Nerds",
                display_font(20.0),
                Color32::WHITE,
                0.4,
            );
            closed |= ui
                .interact(header, Id::unique("nerd stats title"), Sense::click())
                .clicked();
            // The header stays while the rows scroll, including the full 44px close target.
            let close =
                Rect::from_min_size(pos2(rect.right() - 44.0, rect.top()), vec2(44.0, 44.0));
            let response = ui
                .interact(close, Id::unique("nerd stats close"), Sense::click())
                .on_hover_text("Hide (N)");
            paint::labelled(&response, true, "Hide Stats for Nerds");
            if response.hovered() {
                painter.rect_filled(close, 6, hexa(0xffffff, 0.12));
            }
            paint::text(
                &painter,
                close.center(),
                Align2::CENTER_CENTER,
                "×",
                egui::FontId::proportional(24.0),
                hex(0xeeeeee),
                0.0,
            );
            closed |= response.clicked();
            let body = Rect::from_min_max(
                pos2(rect.left() + PANEL_PADDING, rect.top() + HEADER_HEIGHT),
                pos2(rect.right() - PANEL_PADDING, rect.bottom() - PANEL_PADDING),
            );
            let mut body_ui = ui.new_child(egui::UiBuilder::new().max_rect(body));
            egui::ScrollArea::vertical()
                .id_salt("nerd stats rows")
                .max_height(body.height())
                .auto_shrink([false, true])
                .show(&mut body_ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    let width = body.width();
                    for (index, section) in list.iter().enumerate() {
                        if index > 0 {
                            ui.add_space(SECTION_GAP);
                        }
                        let open = section_open(state, section.title);
                        let (head, response) =
                            ui.allocate_exact_size(vec2(width, SECTION_HEAD), Sense::click());
                        paint::labelled_choice(&response, true, open, section.title);
                        let painter = ui.painter();
                        let x = head.left() + 5.0;
                        let tip = pos2(x + 4.0, head.center().y);
                        let arrow = if open {
                            vec![
                                tip + vec2(-3.5, -2.5),
                                tip + vec2(3.5, -2.5),
                                tip + vec2(0.0, 3.0),
                            ]
                        } else {
                            vec![
                                tip + vec2(-2.0, -3.5),
                                tip + vec2(3.0, 0.0),
                                tip + vec2(-2.0, 3.5),
                            ]
                        };
                        let color = if response.hovered() {
                            Color32::WHITE
                        } else {
                            TEXT_LABEL
                        };
                        painter.add(egui::Shape::convex_polygon(arrow, color, Stroke::NONE));
                        paint::text(
                            painter,
                            pos2(x + 13.0, head.center().y),
                            Align2::LEFT_CENTER,
                            &section.title.to_uppercase(),
                            bold_font(10.5),
                            color,
                            1.5,
                        );
                        if response.clicked() {
                            toggled = Some((section.title, !open));
                        }
                        if !open {
                            continue;
                        }
                        for row in &section.rows {
                            let (line, response) =
                                ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::hover());
                            for (x, text) in [(0.0, row.label), (value_column, row.value.as_str())]
                            {
                                paint::text(
                                    ui.painter(),
                                    pos2(line.left() + 5.0 + x, line.center().y),
                                    Align2::LEFT_CENTER,
                                    text,
                                    row_font.clone(),
                                    hex(0xe6eef8),
                                    0.0,
                                );
                            }
                            response.on_hover_text(row.hint.as_ref());
                        }
                    }
                });
            ui.allocate_rect(rect, Sense::hover());
        });
    if let Some((section, open)) = toggled {
        state.nerd_sections.insert(section, open);
    }
    if closed {
        state.show_nerd_stats = false;
    }
}

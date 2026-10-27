//! The respawn card (`respawnMarkup` and `wreckedByMarkup` in `ui-markup.ts`): who
//! wrecked the tank with tags for the weapon, their tank and their tally, the
//! countdown ring, and the choice of the next life's tank.

use egui::{Align2, Color32, Id, LayerId, Order, Rect, Sense, Stroke, Vec2, pos2, vec2};

use super::icons::{Art, cause_icon};
use super::paint::{self, fade, hex, hexa};
use super::setup::{TANKS, vehicle_card};
use super::theme::{GOLD, GOLD_HOT, bold_font, display_font};
use super::{Action, UiState, View};
use crate::engine_api::HudState;

/// 1ST, 2ND, 3RD, 4TH … 11TH, 12TH, 13TH, 21ST.
pub fn ordinal(value: u64) -> String {
    let tens = value % 100;
    let suffix = if (11..=13).contains(&tens) {
        "TH"
    } else {
        match value % 10 {
            1 => "ST",
            2 => "ND",
            3 => "RD",
            _ => "TH",
        }
    };
    format!("{value}{suffix}")
}

/// The card's widest, and the room it keeps from the screen's bottom edge.
const CARD_WIDTH: f32 = 720.0;
const BOTTOM_ROOM: f32 = 48.0;

/// The card, risen by `rise` (0 to 1) into place.
#[allow(clippy::too_many_arguments)]
pub fn card(
    context: &egui::Context,
    art: &Art,
    screen: Rect,
    state: &UiState,
    hud: &HudState,
    view: &View,
    rise: f32,
    actions: &mut Vec<Action>,
) {
    // A dark red glow rises from the screen's bottom edge behind the card.
    let backdrop =
        context.layer_painter(LayerId::new(Order::Background, Id::unique("respawn glow")));
    backdrop.with_clip_rect(screen).add(paint::radial(
        pos2(screen.center().x, screen.bottom()),
        vec2(screen.width() * 0.9, screen.height() * 0.6),
        &[(0.0, hexa(0x3a050c, 0.33)), (0.7, Color32::TRANSPARENT)],
    ));
    let width = (screen.width() - 48.0).min(CARD_WIDTH);
    let head_height = 96.0;
    let cards_height = 132.0;
    let height = 14.0 + head_height + 12.0 + 40.0 + cards_height + 16.0;
    let origin = pos2(
        screen.center().x - width / 2.0,
        screen.bottom() - BOTTOM_ROOM - height + (1.0 - rise) * 24.0,
    );
    egui::Area::new(Id::unique("respawn card"))
        .fixed_pos(origin)
        .order(Order::Middle)
        .show(context, |ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
            let painter = ui.painter().clone();
            paint::plate(&painter, rect, 14.0, None);
            // The head: who did it, and the countdown.
            let head = Rect::from_min_size(rect.min, vec2(width, head_height + 14.0));
            painter.with_clip_rect(rect).add(paint::hgradient(
                head,
                &[(0.0, hexa(0xff4d5e, 0.12)), (0.55, Color32::TRANSPARENT)],
            ));
            painter.rect_filled(
                Rect::from_min_size(rect.min + vec2(0.0, 14.0), vec2(4.0, head.height() - 14.0)),
                0,
                hex(0xff4d5e),
            );
            painter.line_segment(
                [head.left_bottom(), head.right_bottom()],
                Stroke::new(1.0, hexa(0x9fd0ff, 0.1)),
            );
            // A red glint runs along the top edge as the card lands.
            if rise < 1.0 {
                let x = rect.left() + rect.width() * rise * 1.2 - 80.0;
                painter.with_clip_rect(rect).add(paint::hgradient(
                    Rect::from_min_size(pos2(x, rect.top()), vec2(160.0, 2.0)),
                    &[
                        (0.0, Color32::TRANSPARENT),
                        (0.5, hex(0xff8592)),
                        (1.0, Color32::TRANSPARENT),
                    ],
                ));
            }
            let timer_width = 230.0;
            wrecked_by(
                &painter,
                art,
                Rect::from_min_max(
                    pos2(rect.left() + 22.0, rect.top() + 14.0),
                    pos2(rect.right() - timer_width, head.bottom() - 10.0),
                ),
                state,
            );
            countdown(
                &painter,
                Rect::from_min_max(
                    pos2(rect.right() - timer_width, rect.top() + 14.0),
                    pos2(rect.right() - 18.0, head.bottom() - 10.0),
                ),
                state,
                hud,
            );
            // The choice of the next tank.
            let mut y = head.bottom() + 12.0;
            paint::text(
                &painter,
                pos2(rect.left() + 18.0, y),
                Align2::LEFT_TOP,
                "CHOOSE YOUR NEXT TANK",
                bold_font(14.0),
                hex(0xeef5fc),
                1.6,
            );
            paint::text(
                &painter,
                pos2(rect.left() + 18.0, y + 19.0),
                Align2::LEFT_TOP,
                "It rolls out when the timer ends",
                egui::FontId::proportional(12.0),
                hex(0xa9c0d8),
                0.0,
            );
            y += 40.0;
            let gap = 10.0;
            let card_width = (width - 36.0 - 2.0 * gap) / 3.0;
            let team = if hud.game_mode == "solo" {
                0
            } else {
                hud.human_team
            };
            for (index, tank) in TANKS.iter().enumerate() {
                let card_rect = Rect::from_min_size(
                    pos2(rect.left() + 18.0 + index as f32 * (card_width + gap), y),
                    vec2(card_width, cards_height),
                );
                let response = ui
                    .interact(
                        card_rect,
                        Id::unique(("respawn tank", tank.id)),
                        Sense::click(),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                vehicle_card(
                    &painter,
                    art,
                    card_rect,
                    tank,
                    team,
                    view.prefs.tank == tank.id,
                    response.hovered(),
                    true,
                );
                if response.clicked() && view.prefs.tank != tank.id {
                    actions.push(Action::NextTank(tank.id.into()));
                }
            }
        });
}

/// The killer: a red eyebrow, their name in their team's colour, and tags for the
/// weapon, their tank and their tally.
fn wrecked_by(painter: &egui::Painter, art: &Art, rect: Rect, state: &UiState) {
    let death = state.death.as_ref();
    let (label, name) = match death {
        None => ("YOUR TANK".to_owned(), "DESTROYED".to_owned()),
        Some(report) if report.own_goal => ("OWN GOAL".to_owned(), "YOU".to_owned()),
        Some(report) => (
            "DESTROYED BY".to_owned(),
            report
                .killer
                .clone()
                .unwrap_or_else(|| "THE YARD".to_owned())
                .to_uppercase(),
        ),
    };
    let name_color = match death.and_then(|report| report.killer_team) {
        Some(0) => hex(0x9ad6ff),
        Some(_) => hex(0xffa0a9),
        None => Color32::WHITE,
    };
    paint::text(
        painter,
        rect.min,
        Align2::LEFT_TOP,
        &label,
        bold_font(12.0),
        hex(0xff8592),
        2.4,
    );
    let name_rect = paint::text_fit(
        painter,
        rect.min + vec2(0.0, 17.0),
        Align2::LEFT_TOP,
        &name,
        display_font(38.0),
        name_color,
        0.5,
        rect.width(),
    );
    let Some(report) = death else { return };
    let mut x = rect.left();
    let y = name_rect.bottom() + 6.0;
    if let Some(weapon) = &report.weapon {
        let label = weapon.to_uppercase();
        let text_width = paint::text_width(painter, &label, bold_font(12.0), 1.0);
        let chip = Rect::from_min_size(pos2(x, y), vec2(text_width + 36.0, 25.0));
        painter.rect_filled(chip, 7, hex(0x2c1219));
        painter.add(paint::vgradient(
            chip.shrink2(vec2(0.0, 6.0)),
            &[(0.0, hex(0x3a1420)), (1.0, hex(0x22101a))],
        ));
        painter.rect_stroke(
            chip,
            7,
            Stroke::new(1.0, hexa(0xff7c88, 0.4)),
            egui::StrokeKind::Inside,
        );
        art.icon(
            painter,
            cause_icon(report.cause.as_deref()),
            Rect::from_center_size(pos2(chip.left() + 15.0, chip.center().y), Vec2::splat(18.0)),
            hex(0xff7c88),
        );
        paint::text(
            painter,
            pos2(chip.left() + 28.0, chip.center().y),
            Align2::LEFT_CENTER,
            &label,
            bold_font(12.0),
            Color32::WHITE,
            1.0,
        );
        x = chip.right() + 10.0;
    }
    if report.own_goal {
        return;
    }
    let mut tags: Vec<(Option<&str>, String)> = Vec::new();
    if let Some(vehicle) = &report.killer_vehicle {
        tags.push((None, vehicle.clone()));
    }
    if let Some(kills) = report.killer_kills.filter(|kills| *kills > 0.0) {
        tags.push((
            Some("skull"),
            format!("THEIR {} KILL", ordinal(kills as u64)),
        ));
    }
    let center_y = y + 12.5;
    for (index, (icon, tag)) in tags.iter().enumerate() {
        if index > 0 {
            painter.circle_filled(pos2(x, center_y), 2.0, hexa(0x9fd0ff, 0.33));
            x += 10.0;
        }
        if let Some(icon) = icon {
            art.icon(
                painter,
                icon,
                Rect::from_center_size(pos2(x + 7.0, center_y), Vec2::splat(15.0)),
                fade(hex(0xb9cadb), 0.8),
            );
            x += 18.0;
        }
        let rect = paint::text(
            painter,
            pos2(x, center_y),
            Align2::LEFT_CENTER,
            tag,
            bold_font(12.0),
            hex(0xb9cadb),
            1.0,
        );
        x = rect.right() + 10.0;
    }
}

/// The countdown: its caption over a gold rule draining toward the ring, and the ring
/// with the seconds in gold.
fn countdown(painter: &egui::Painter, rect: Rect, state: &UiState, hud: &HudState) {
    let left = hud.human.respawn.max(0.0);
    let total = state.respawn_total.max(left);
    let share = if total > 0.0 {
        (left / total) as f32
    } else {
        0.0
    };
    let ring_radius = 28.0;
    let ring = pos2(rect.right() - ring_radius - 2.0, rect.center().y);
    painter.circle_filled(ring, ring_radius + 3.0, hex(0x0a1830));
    paint::timer_ring(painter, ring, ring_radius - 1.0, 4.0, share, GOLD);
    painter.circle_stroke(
        ring,
        ring_radius + 3.0,
        Stroke::new(1.0, hexa(0x9fd0ff, 0.18)),
    );
    paint::shadowed(
        painter,
        ring,
        Align2::CENTER_CENTER,
        &format!("{}", left.ceil()),
        display_font(30.0),
        GOLD_HOT,
        0.0,
    );
    let caption_right = ring.x - ring_radius - 16.0;
    let caption = paint::text(
        painter,
        pos2(caption_right, ring.y - 6.0),
        Align2::RIGHT_BOTTOM,
        "RESPAWN IN",
        bold_font(13.0),
        hex(0xc4d5e7),
        2.4,
    );
    let rule = Rect::from_min_max(
        pos2(caption.left(), ring.y + 1.0),
        pos2(caption_right, ring.y + 4.0),
    );
    painter.rect_filled(rule, 2, hexa(0xffffff, 0.12));
    let fill = rule.width() * share;
    painter.add(paint::hgradient(
        Rect::from_min_max(pos2(rule.right() - fill, rule.top()), rule.max),
        &[(0.0, hex(0xff9d1c)), (1.0, GOLD_HOT)],
    ));
}

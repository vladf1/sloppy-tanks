//! The respawn card (`respawnMarkup` and `wreckedByMarkup` in `ui-markup.ts`): who
//! wrecked the tank with tags for the weapon, their tank and their tally, the
//! countdown ring, and the choice of the next life's tank.

use egui::emath::TSTransform;
use egui::{Align2, Color32, Id, LayerId, Order, Rect, Sense, Stroke, Vec2, pos2, vec2};

use sloppy_core::sim::{GameMode, Team};
use sloppy_web::catalog::TANK_CARDS;
use sloppy_web::hud::Hud;

use super::hud::cubic_bezier;
use super::icons::Art;
use super::paint::{self, fade, hex, hexa};
use super::setup::vehicle_card;
use super::theme::{GOLD, GOLD_DEEP, GOLD_HOT, bold_font, display_font};
use super::touch::{inner_edge, lit_disc};
use super::{Action, UiState, View};

/// The plates' cut corners.
const PLATE_CUT: f32 = 14.0;
/// The countdown ring's gold glow's width: a CSS blur of 34 px, which egui's linear
/// fade needs half again as wide to match.
const RING_GLOW_BLUR: u8 = 51;
/// The heading over the respawn card's tank choice and the line that follows it.
const CHOICE_TITLE: &str = "CHOOSE YOUR NEXT TANK";
const CHOICE_LINE: &str = "It rolls out when the timer ends";
const CHOICE_LINE_FONT: egui::FontId = egui::FontId::proportional(12.0);
const CHOICE_LINE_GAP: f32 = 12.0;
/// The card's widest, and the room it keeps from the screen's bottom edge.
const CARD_WIDTH: f32 = 720.0;
const BOTTOM_ROOM: f32 = 48.0;
/// The score bar's room above the card, which a short screen (a phone on its side)
/// keeps by showing the card smaller.
const TOP_ROOM: f32 = 96.0;

/// The card, risen by `rise` (0 to 1) into place.
#[allow(clippy::too_many_arguments)]
pub fn card(
    context: &egui::Context,
    art: &Art,
    screen: Rect,
    state: &UiState,
    hud: &Hud,
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
    let cards_height = 170.0;
    // The line under the choice's heading goes beside it where both fit, as on the page.
    let font_painter = context.layer_painter(LayerId::background());
    let choice_width = paint::text_width(&font_painter, CHOICE_TITLE, bold_font(14.0), 1.6);
    let line_width = paint::text_width(&font_painter, CHOICE_LINE, CHOICE_LINE_FONT, 0.0);
    let line_beside = 18.0 + choice_width + CHOICE_LINE_GAP + line_width + 18.0 <= width;
    let choice_head = if line_beside { 28.0 } else { 40.0 };
    let height = 14.0 + head_height + 12.0 + choice_head + cards_height + 16.0;
    let origin = pos2(
        screen.center().x - width / 2.0,
        screen.bottom() - BOTTOM_ROOM - height + (1.0 - rise) * 24.0,
    );
    let fit = ((screen.height() - BOTTOM_ROOM - TOP_ROOM) / height).min(1.0);
    paint::shrink_about(
        context,
        LayerId::new(Order::Middle, Id::unique("respawn card")),
        origin + vec2(width / 2.0, height),
        fit,
    );
    egui::Area::new(Id::unique("respawn card"))
        .fixed_pos(origin)
        .order(Order::Middle)
        .show(context, |ui| {
            ui.set_clip_rect(paint::shrunk_clip(screen));
            let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
            let painter = ui.painter().clone();
            paint::plate(&painter, rect, PLATE_CUT, None);
            // The head: who did it, and the countdown.
            let head = Rect::from_min_size(rect.min, vec2(width, head_height + 14.0));
            red_head(&painter, head);
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
            // The killer's tags may run on under the caption, not under the ring.
            let ring_left = rect.right() - 18.0 - CARD_COUNTDOWN.ring;
            wrecked_by(
                &painter,
                art,
                Rect::from_min_max(
                    pos2(rect.left() + 22.0, rect.top() + 14.0),
                    pos2(rect.right() - timer_width, head.bottom() - 10.0),
                ),
                ring_left - 10.0,
                state,
            );
            countdown(
                &painter,
                Rect::from_min_max(
                    pos2(rect.right() - timer_width, rect.top() + 14.0),
                    pos2(rect.right() - 18.0, head.bottom() - 14.0),
                ),
                &CARD_COUNTDOWN,
                state,
                hud,
            );
            // The choice of the next tank.
            let mut y = head.bottom() + 12.0;
            let title = paint::text(
                &painter,
                pos2(rect.left() + 18.0, y),
                Align2::LEFT_TOP,
                CHOICE_TITLE,
                bold_font(14.0),
                hex(0xeef5fc),
                1.6,
            );
            let (line_at, line_align) = if line_beside {
                (
                    pos2(title.right() + CHOICE_LINE_GAP, title.center().y),
                    Align2::LEFT_CENTER,
                )
            } else {
                (pos2(rect.left() + 18.0, y + 19.0), Align2::LEFT_TOP)
            };
            paint::text(
                &painter,
                line_at,
                line_align,
                CHOICE_LINE,
                CHOICE_LINE_FONT,
                hex(0xa9c0d8),
                0.0,
            );
            y += choice_head;
            let gap = 10.0;
            let card_width = (width - 36.0 - 2.0 * gap) / 3.0;
            let team = if hud.game_mode == GameMode::Solo {
                Team::Blue
            } else {
                hud.human_team
            };
            for (index, tank) in TANK_CARDS.iter().enumerate() {
                let card_rect = Rect::from_min_size(
                    pos2(rect.left() + 18.0 + index as f32 * (card_width + gap), y),
                    vec2(card_width, cards_height),
                );
                let response = ui
                    .interact(
                        card_rect,
                        Id::unique(("respawn tank", tank.kind)),
                        Sense::click(),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                paint::labelled(&response, true, tank.name());
                vehicle_card(
                    &painter,
                    art,
                    card_rect,
                    tank,
                    team,
                    view.prefs.tank == tank.kind,
                    response.hovered(),
                    true,
                );
                if response.clicked() && view.prefs.tank != tank.kind {
                    actions.push(Action::NextTank(tank.kind));
                }
            }
        });
}

/// The phone's respawn strip's widest and its distance from the screen's sides and
/// bottom edge; its sides' and foot's padding, its head's padding round the countdown
/// and the gap under it, and the height of its tank cards.
const STRIP_WIDTH: f32 = 520.0;
const STRIP_MARGIN: f32 = 8.0;
const STRIP_BOTTOM: f32 = 10.0;
const STRIP_SIDE: f32 = 14.0;
const STRIP_FOOT: f32 = 12.0;
const STRIP_HEAD_PADDING: f32 = 8.0;
const STRIP_HEAD_GAP: f32 = 8.0;
const STRIP_CARDS: f32 = 88.0;

/// The phone's respawn card (`.phone .menu.respawn`): a low strip of the countdown and
/// small tank cards, so the arena stays in view; the notice mid-screen says what
/// destroyed the tank. Risen by `rise` (0 to 1) into place.
#[allow(clippy::too_many_arguments)]
pub fn strip(
    context: &egui::Context,
    art: &Art,
    screen: Rect,
    state: &UiState,
    hud: &Hud,
    view: &View,
    rise: f32,
    actions: &mut Vec<Action>,
) {
    let width = (screen.width() - 2.0 * STRIP_MARGIN).min(STRIP_WIDTH);
    // The head runs edge to edge over the strip's top padding, its rule under it.
    let head_height = 2.0 * STRIP_HEAD_PADDING + STRIP_COUNTDOWN.ring;
    let height = head_height + 1.0 + STRIP_HEAD_GAP + STRIP_CARDS + STRIP_FOOT;
    let origin = pos2(
        screen.center().x - width / 2.0,
        screen.bottom() - STRIP_BOTTOM - height + (1.0 - rise) * 24.0,
    );
    egui::Area::new(Id::unique("respawn strip"))
        .fixed_pos(origin)
        .order(Order::Middle)
        .show(context, |ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
            let painter = ui.painter().clone();
            paint::plate(&painter, rect, PLATE_CUT, None);
            let head = Rect::from_min_size(rect.min, vec2(width, head_height));
            red_head(&painter, head);
            countdown(
                &painter,
                head.shrink2(vec2(STRIP_SIDE, STRIP_HEAD_PADDING)),
                &STRIP_COUNTDOWN,
                state,
                hud,
            );
            let gap = 8.0;
            let card_width = (width - 2.0 * STRIP_SIDE - 2.0 * gap) / 3.0;
            let top = head.bottom() + 1.0 + STRIP_HEAD_GAP;
            let team = if hud.game_mode == GameMode::Solo {
                Team::Blue
            } else {
                hud.human_team
            };
            for (index, tank) in TANK_CARDS.iter().enumerate() {
                let card_rect = Rect::from_min_size(
                    pos2(
                        rect.left() + STRIP_SIDE + index as f32 * (card_width + gap),
                        top,
                    ),
                    vec2(card_width, STRIP_CARDS),
                );
                let response = ui.interact(
                    card_rect,
                    Id::unique(("respawn strip tank", tank.kind)),
                    Sense::click(),
                );
                paint::labelled(&response, true, tank.name());
                vehicle_card(
                    &painter,
                    art,
                    card_rect,
                    tank,
                    team,
                    view.prefs.tank == tank.kind,
                    false,
                    true,
                );
                if response.clicked() && view.prefs.tank != tank.kind {
                    actions.push(Action::NextTank(tank.kind));
                }
            }
        });
}

/// The killer: a red eyebrow, their name in their team's colour, and tags for the
/// weapon, their tank and their tally, as many as fit before `line_right` (a phone's
/// card has room for fewer).
fn wrecked_by(painter: &egui::Painter, art: &Art, rect: Rect, line_right: f32, state: &UiState) {
    let death = state.death.as_ref();
    let (label, name) = match death {
        None => ("YOUR TANK", "DESTROYED".to_owned()),
        Some(report) if report.own_goal => ("OWN GOAL", "YOU".to_owned()),
        Some(report) => {
            let killer = report.killer.as_deref().unwrap_or("THE YARD");
            ("DESTROYED BY", killer.to_uppercase())
        }
    };
    let name_color = match death.and_then(|report| report.killer_team) {
        Some(Team::Blue) => hex(0x9ad6ff),
        Some(Team::Red) => hex(0xffa0a9),
        None => Color32::WHITE,
    };
    paint::text(
        painter,
        rect.min,
        Align2::LEFT_TOP,
        label,
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
    if let Some(weapon) = report.weapon {
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
            report.icon,
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
    if let Some(vehicle) = report.killer_vehicle {
        tags.push((None, vehicle.to_owned()));
    }
    if let Some(tally) = &report.killer_tally {
        tags.push((Some("skull"), tally.to_ascii_uppercase()));
    }
    let center_y = y + 12.5;
    for (index, (icon, tag)) in tags.iter().enumerate() {
        let width = paint::text_width(painter, tag, bold_font(12.0), 1.0)
            + if index > 0 { 10.0 } else { 0.0 }
            + if icon.is_some() { 18.0 } else { 0.0 };
        if x + width > line_right {
            break;
        }
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

/// The head's red treatment (`.respawn-head`): a red wash from the left, a red bar down
/// the left edge under the plate's cut corner, and a rule under it all.
fn red_head(painter: &egui::Painter, head: Rect) {
    // The wash keeps inside the cut corner: a colour linear in x interpolates exactly
    // across the outline's triangles.
    let wash_right = head.left() + head.width() * 0.55;
    let wash = |x: f32| {
        hexa(
            0xff4d5e,
            0.12 * (1.0 - (x - head.left()) / (wash_right - head.left())),
        )
    };
    let mut mesh = egui::Mesh::default();
    for point in [
        pos2(head.left(), head.top() + PLATE_CUT),
        pos2(head.left() + PLATE_CUT, head.top()),
        pos2(wash_right, head.top()),
        pos2(wash_right, head.bottom()),
        pos2(head.left(), head.bottom()),
    ] {
        mesh.colored_vertex(point, wash(point.x));
    }
    for corner in 1..4 {
        mesh.add_triangle(0, corner, corner + 1);
    }
    painter.add(mesh);
    painter.rect_filled(
        Rect::from_min_max(
            pos2(head.left(), head.top() + PLATE_CUT),
            pos2(head.left() + 4.0, head.bottom()),
        ),
        0,
        hex(0xff4d5e),
    );
    painter.line_segment(
        [head.left_bottom(), head.right_bottom()],
        Stroke::new(1.0, hexa(0x9fd0ff, 0.1)),
    );
}

/// The countdown's ring and type (`.respawn-ring`, `#respawn-count`, `.respawn-timer
/// h2`): the card's, and the phone strip's smaller ring beside larger type.
struct CountdownSize {
    ring: f32,
    digit: f32,
    caption: f32,
}

const CARD_COUNTDOWN: CountdownSize = CountdownSize {
    ring: 80.0,
    digit: 52.0,
    caption: 13.0,
};
const STRIP_COUNTDOWN: CountdownSize = CountdownSize {
    ring: 46.0,
    digit: 30.0,
    caption: 22.0,
};
/// The ring's gold band, the gap between the caption and the ring, and the caption's
/// rule and the gap above it.
const RING_BAND: f32 = 5.0;
const CAPTION_GAP: f32 = 14.0;
const RULE_HEIGHT: f32 = 3.0;
const RULE_GAP: f32 = 7.0;

/// The countdown at the right of `rect`: the ring with the seconds in gold, its band
/// draining as the respawn nears, and beside it the caption over a gold rule that
/// drains toward the ring, the two centred on the ring as one block.
fn countdown(
    painter: &egui::Painter,
    rect: Rect,
    size: &CountdownSize,
    state: &UiState,
    hud: &Hud,
) {
    let left = hud.human.respawn.max(0.0);
    let total = state.respawn_total.max(left);
    let share = if total > 0.0 {
        (left / total) as f32
    } else {
        0.0
    };
    let seconds = left.ceil();
    let radius = size.ring / 2.0;
    let ring = pos2(rect.right() - radius, rect.center().y);
    let disc = Rect::from_center_size(ring, Vec2::splat(size.ring));
    // The ring's shapes from here on, which its beat scales.
    let ring_start = painter.add(egui::Shape::Noop);
    // A gold glow round the ring and a dark hairline at its edge.
    painter.add(
        egui::epaint::Shadow {
            offset: [0, 0],
            blur: RING_GLOW_BLUR,
            spread: 0,
            color: hexa(0xffcf38, 0.25),
        }
        .as_shape(disc, egui::CornerRadius::same(radius as u8)),
    );
    painter.circle_stroke(ring, radius + 0.5, Stroke::new(1.0, hexa(0x000000, 0.4)));
    // The band: gold for the time left, clockwise from the top, faint for the rest.
    painter.circle_filled(ring, radius, hexa(0xffffff, 0.1));
    paint::arc(
        painter,
        ring,
        radius - RING_BAND / 2.0,
        0.0,
        std::f32::consts::TAU * share.clamp(0.0, 1.0),
        Stroke::new(RING_BAND, GOLD),
    );
    // The face inside the band, lit from above its middle, shaded under its top edge.
    let face = radius - RING_BAND;
    painter.add(lit_disc(
        ring,
        face,
        ring - vec2(0.0, face * 0.4),
        face.hypot(face * 1.4),
        &[(0.0, hex(0x1d3a66)), (1.0, hex(0x07122a))],
    ));
    painter.add(inner_edge(ring, face, 5.0, hexa(0x000000, 0.35)));
    // The seconds over a dark gold drop and a soft gold glow.
    painter.add(paint::radial(
        ring,
        Vec2::splat(size.digit * 0.6),
        &[(0.0, hexa(0xffcf38, 0.3)), (1.0, Color32::TRANSPARENT)],
    ));
    let digits = format!("{seconds}");
    paint::text(
        painter,
        ring + vec2(0.0, 2.0),
        Align2::CENTER_CENTER,
        &digits,
        display_font(size.digit),
        hex(0x7a5a00),
        0.0,
    );
    paint::text(
        painter,
        ring,
        Align2::CENTER_CENTER,
        &digits,
        display_font(size.digit),
        GOLD_HOT,
        0.0,
    );
    // The ring beats as each second begins, drawn only: its caption keeps its place.
    let pulse = beat(painter.ctx(), seconds);
    if pulse != 1.0 {
        let ring_end = painter.add(egui::Shape::Noop);
        let transform = TSTransform::new(ring.to_vec2() * (1.0 - pulse), pulse);
        painter.ctx().graphics_mut(|layers| {
            layers
                .entry(painter.layer_id())
                .transform_after_rounding_range(ring_start, ring_end, transform);
        });
    }
    // The caption's line and the rule under it, centred on the ring together.
    let line = size.caption * 1.2;
    let block = line + RULE_GAP + RULE_HEIGHT;
    let caption_right = ring.x - radius - CAPTION_GAP;
    let caption = paint::shadowed(
        painter,
        pos2(caption_right, ring.y - block / 2.0 + line / 2.0),
        Align2::RIGHT_CENTER,
        "RESPAWN IN",
        bold_font(size.caption),
        hex(0xc4d5e7),
        2.4,
    );
    let rule = Rect::from_min_max(
        pos2(caption.left(), ring.y + block / 2.0 - RULE_HEIGHT),
        pos2(caption_right, ring.y + block / 2.0),
    );
    painter.rect_filled(rule, 2, hexa(0xffffff, 0.12));
    let fill = rule.width() * share;
    painter.add(paint::hgradient(
        Rect::from_min_max(pos2(rule.right() - fill, rule.top()), rule.max),
        &[(0.0, GOLD_DEEP), (1.0, GOLD_HOT)],
    ));
}

/// The ring's beat (`@keyframes respawn-beat`): it starts each second this much larger
/// and settles to its size over this share of the second, on CSS's
/// `cubic-bezier(0.2, 0.8, 0.3, 1)`.
const BEAT_SWELL: f32 = 0.08;
const BEAT_SETTLE: f64 = 0.3;
const BEAT_CURVE: [f32; 4] = [0.2, 0.8, 0.3, 1.0];
/// A frame gap this long means the ring was gone: a new countdown beats from its start.
const BEAT_RESTART: f64 = 0.5;

/// The ring's scale this frame. The shell's clock beats once a second; its phase is
/// the moment the count last changed, so each beat lands as a new second shows.
fn beat(context: &egui::Context, seconds: f64) -> f32 {
    let now = context.input(|input| input.time);
    let start = context.data_mut(|data| {
        let (shown, start, seen) =
            data.get_temp_mut_or(Id::unique("respawn beat"), (seconds, now, now));
        if *shown != seconds || now - *seen > BEAT_RESTART {
            *shown = seconds;
            *start = now;
        }
        *seen = now;
        *start
    });
    let settled = ((now - start).rem_euclid(1.0) / BEAT_SETTLE).min(1.0) as f32;
    1.0 + BEAT_SWELL * (1.0 - cubic_bezier(BEAT_CURVE, settled))
}

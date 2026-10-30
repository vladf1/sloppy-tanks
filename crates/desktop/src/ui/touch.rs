//! The touch controls over the arena (`touch-controls.ts`, `touch-controls.css`): the
//! drive stick, a light glass ring whose rim lights toward the push; the mine button,
//! a navy key whose slim gold rim refills as the mine re-arms; the zoom pair over it;
//! and the coach mark that says to touch the arena until the player has. They are
//! painted, not widgets: `crate::touch` hit-tests the same layout, so egui never sees
//! the fingers that use them.

use egui::{Align2, Color32, Id, LayerId, Order, Painter, Pos2, Rect, Stroke, Vec2, pos2, vec2};

use super::UiState;
use super::icons::Art;
use super::paint::{self, hex, hexa};
use super::theme::{GOLD, GOLD_HOT, bold_font};
use crate::input::TouchInput;
use crate::touch::{MINE_RADIUS, TouchLayout, ZOOM_RADIUS};
use sloppy_render::presentation::input::STICK_RADIUS;

/// How far the stick's base ring sits inside its square (a phone's smaller one keeps
/// more of its ring), and the knob's radius.
const BASE_INSET: f32 = 18.0;
const PHONE_BASE_INSET: f32 = 14.0;
const KNOB_RADIUS: f32 = 27.0;
/// The mine key's navy face inside its gold rim.
const MINE_FACE: f32 = 34.4;
/// Seconds the fire hint shows each time the controls appear, and its fade.
const HINT_SECONDS: f64 = 9.0;
const HINT_FADE: f64 = 0.5;
/// Seconds one pulse of the hint's target ring takes.
const HINT_PULSE: f64 = 1.6;
/// The fire hint's top on a phone held upright: under the top row of keys and the kill
/// feed's first row.
const PHONE_UPRIGHT_HINT_TOP: f32 = 102.0;
/// The fire hint's top on a tablet whose score takes the top-left corner: under the
/// score and its YOUR TEAM tab.
const CORNER_SCORE_HINT_TOP: f32 = 106.0;

/// What the touch controls show this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchView {
    pub input: TouchInput,
    pub first_person: bool,
    /// The player has touched the arena: the fire hint has done its job.
    pub learned: bool,
}

/// Paint the touch controls over the arena. A phone's zoom pair is the HUD's, in its
/// top row.
pub fn controls(context: &egui::Context, art: &Art, state: &UiState, touch: &TouchView) {
    let painter = context.layer_painter(LayerId::new(Order::Middle, Id::unique("touch controls")));
    let layout = TouchLayout::of(context);
    stick(&painter, &layout, &touch.input);
    if let (Some(hud), Some(mine)) = (&state.hud, layout.mine) {
        let cooldown = hud.human.mine_cooldown;
        let total = state.mine_total.max(cooldown);
        let ready = if cooldown > 0.0 && total > 0.0 {
            (1.0 - cooldown / total) as f32
        } else {
            1.0
        };
        mine_key(&painter, art, mine, cooldown, ready);
    }
    // The first-person view sits in the turret and has no zoom.
    if !crate::touch::phone() && !touch.first_person {
        zoom_key(&painter, layout.zoom_in.center(), "+");
        zoom_key(&painter, layout.zoom_out.center(), "−");
    }
    let shown = state.touch_shown;
    if !touch.learned && !touch.first_person && shown < HINT_SECONDS + HINT_FADE {
        let opacity = (1.0 - (shown - HINT_SECONDS).max(0.0) / HINT_FADE) as f32;
        fire_hint(context, &painter, opacity);
    }
}

/// A soft round shadow under a thumb control.
fn drop_shadow(painter: &Painter, center: Pos2, radius: f32) {
    painter.add(paint::radial(
        center + vec2(0.0, 5.0),
        Vec2::splat(radius + 12.0),
        &[
            (0.0, hexa(0x000000, 0.4)),
            (radius / (radius + 12.0), hexa(0x000000, 0.3)),
            (1.0, Color32::TRANSPARENT),
        ],
    ));
}

fn stick(painter: &Painter, layout: &TouchLayout, input: &TouchInput) {
    let phone = crate::touch::phone();
    let held = input.drive.is_some();
    let center = layout.stick.center();
    let base_radius =
        layout.stick.width() / 2.0 - if phone { PHONE_BASE_INSET } else { BASE_INSET };
    // Held, the base moves under the thumb (up to `STICK_ANCHOR_SHIFT`) and the knob follows
    // the finger to the rim.
    let base = match input.drive {
        Some((_, origin)) => pos2(origin.0, origin.1),
        None => center,
    };
    painter.add(paint::radial(
        base,
        Vec2::splat(base_radius),
        &[
            (0.0, hexa(0x0a1730, 0.2)),
            (0.38, hexa(0x0a1730, 0.32)),
            (0.39, hexa(0xdff0ff, 0.12)),
            (0.41, hexa(0x0a1730, 0.36)),
            (1.0, hexa(0x0a1730, 0.55)),
        ],
    ));
    painter.circle_stroke(
        base,
        base_radius - 1.0,
        Stroke::new(1.0, hexa(0x000000, 0.4)),
    );
    painter.circle_stroke(
        base,
        base_radius,
        Stroke::new(
            2.0,
            if held {
                hexa(0xffffff, 0.7)
            } else {
                hexa(0xdff0ff, 0.4)
            },
        ),
    );
    if held {
        painter.circle_stroke(
            base,
            base_radius + 3.0,
            Stroke::new(4.0, hexa(0xdff0ff, 0.15)),
        );
    }
    // Four direction ticks inside the rim.
    let tick = hexa(0xbfe2ff, 0.4);
    for (dx, dy) in [(0.0, -1.0), (0.0, 1.0), (-1.0, 0.0), (1.0, 0.0)] {
        let direction = vec2(dx, dy);
        let outer = base + direction * (base_radius - 6.0);
        painter.line_segment([outer, outer - direction * 8.0], Stroke::new(2.0, tick));
    }
    let (x, y) = input.knob;
    let push = x.hypot(y);
    if push > 0.0 {
        // The rim lights toward the drive, brighter the further the knob is pushed.
        let angle = x.atan2(-y);
        let glow = push.min(1.0) * 0.95;
        for (width, color) in [
            (8.0, hexa(0x6fd2ff, 0.3 * glow)),
            (3.0, hexa(0xe8f8ff, glow)),
        ] {
            paint::arc(
                painter,
                base,
                base_radius + 1.0,
                angle - std::f32::consts::FRAC_PI_4,
                std::f32::consts::FRAC_PI_2,
                Stroke::new(width, color),
            );
        }
    }
    let knob = base + vec2(x, y) * STICK_RADIUS / push.max(1.0);
    drop_shadow(painter, knob, KNOB_RADIUS);
    painter.circle_filled(knob, KNOB_RADIUS + 3.0, hexa(0x06102a, 0.4));
    if held {
        painter.circle_filled(knob, KNOB_RADIUS + 8.0, hexa(0x6fd2ff, 0.18));
    }
    // A domed glass cap: lit from above its middle, dark at the rim.
    painter.circle_filled(knob, KNOB_RADIUS, hex(0x12274a));
    painter.add(paint::radial(
        knob - vec2(0.0, KNOB_RADIUS * 0.2),
        Vec2::splat(KNOB_RADIUS * 0.78),
        &[
            (0.0, hex(0x6f97c4)),
            (0.55, hex(0x2b4f7f)),
            (1.0, hex(0x12274a)),
        ],
    ));
    painter.circle_stroke(
        knob,
        KNOB_RADIUS * 0.49,
        Stroke::new(1.5, hexa(0xffffff, 0.25)),
    );
    painter.circle_stroke(
        knob,
        KNOB_RADIUS,
        Stroke::new(2.0, if held { Color32::WHITE } else { hex(0xe8f4ff) }),
    );
    // The stick's name on a small dark tag under its rim; a phone's corner has no room.
    if phone {
        return;
    }
    let label = pos2(center.x, layout.stick.bottom() + 6.0);
    let text = paint::galley(
        painter,
        "DRIVE",
        bold_font(12.0),
        hex(0xe6f0fa),
        2.4,
        f32::INFINITY,
    );
    let tag = Align2::CENTER_BOTTOM.anchor_size(label, text.size() + vec2(22.0, 6.0));
    painter.rect_filled(tag, 255, hexa(0x06102a, 0.77));
    painter.rect_stroke(
        tag,
        255,
        Stroke::new(1.0, hexa(0x9fd0ff, 0.2)),
        egui::StrokeKind::Inside,
    );
    painter.galley(tag.min + vec2(12.0, 3.0), text, Color32::WHITE);
}

/// The mine key: armed, or its rim refilling while the seconds count down.
fn mine_key(painter: &Painter, art: &Art, center: Pos2, cooldown: f64, ready: f32) {
    let cooling = cooldown > 0.0;
    drop_shadow(painter, center, MINE_RADIUS);
    painter.circle_filled(center, MINE_RADIUS, hexa(0xffffff, 0.12));
    paint::arc(
        painter,
        center,
        MINE_RADIUS - 1.5,
        0.0,
        std::f32::consts::TAU * ready,
        Stroke::new(3.0, GOLD),
    );
    if cooling {
        painter.circle_filled(center, MINE_FACE, hex(0x13294a));
        painter.circle_stroke(
            center,
            MINE_RADIUS - 0.5,
            Stroke::new(1.0, hexa(0xbfe2ff, 0.27)),
        );
    } else {
        painter.add(paint::radial(
            center,
            Vec2::splat(MINE_FACE),
            &[(0.0, hex(0x183460)), (1.0, hex(0x0b1a33))],
        ));
        painter.add(paint::radial(
            center - vec2(0.0, MINE_RADIUS * 0.56),
            vec2(MINE_RADIUS * 0.6, MINE_RADIUS * 0.4),
            &[(0.0, hexa(0xffffff, 0.12)), (1.0, Color32::TRANSPARENT)],
        ));
    }
    let icon = Rect::from_center_size(center - vec2(0.0, 7.0), Vec2::splat(26.0));
    if !cooling {
        art.icon(
            painter,
            "mine",
            icon.translate(vec2(0.0, 1.0)),
            hexa(0x000000, 0.55),
        );
    }
    art.icon(
        painter,
        "mine",
        icon,
        if cooling { hex(0x6d84a0) } else { GOLD_HOT },
    );
    let label = if cooling {
        format!("{cooldown:.1}s")
    } else {
        "MINE".to_owned()
    };
    paint::text(
        painter,
        center + vec2(0.0, 15.0),
        Align2::CENTER_CENTER,
        &label,
        bold_font(12.0),
        if cooling {
            hex(0xa1b4c6)
        } else {
            hex(0xe9f2fb)
        },
        1.4,
    );
}

fn zoom_key(painter: &Painter, center: Pos2, label: &str) {
    drop_shadow(painter, center, ZOOM_RADIUS);
    painter.add(paint::radial(
        center,
        Vec2::splat(ZOOM_RADIUS),
        &[(0.0, hex(0x183460)), (1.0, hex(0x0b1a33))],
    ));
    painter.add(paint::radial(
        center - vec2(0.0, ZOOM_RADIUS * 0.56),
        vec2(ZOOM_RADIUS * 0.6, ZOOM_RADIUS * 0.4),
        &[(0.0, hexa(0xffffff, 0.12)), (1.0, Color32::TRANSPARENT)],
    ));
    painter.circle_stroke(
        center,
        ZOOM_RADIUS - 1.0,
        Stroke::new(2.0, hexa(0xbfe2ff, 0.3)),
    );
    paint::text(
        painter,
        center,
        Align2::CENTER_CENTER,
        label,
        bold_font(24.0),
        hex(0xe9f2fb),
        0.0,
    );
}

/// How to aim and fire: a tag in the top-left corner, the one corner no HUD and no
/// thumb uses, with a target ring that breathes; under the score where a narrow tablet's
/// score takes that corner. A phone's tighter corner has the tag level with its score;
/// held upright, where the zoom pair takes that corner, under it.
fn fire_hint(context: &egui::Context, painter: &Painter, opacity: f32) {
    let viewport = context.viewport_rect();
    let content = context.content_rect();
    let (left, top) = if super::hud::score_in_corner(context) {
        (24.0, CORNER_SCORE_HINT_TOP)
    } else if !crate::touch::phone() {
        (24.0, 14.0)
    } else if viewport.height() > viewport.width() {
        (10.0, PHONE_UPRIGHT_HINT_TOP)
    } else {
        (10.0, 6.0)
    };
    let origin = pos2(
        (viewport.left() + left).max(content.left()),
        (viewport.top() + top).max(content.top()),
    );
    let title = paint::galley(
        painter,
        "TOUCH THE ARENA",
        bold_font(12.0),
        Color32::WHITE,
        1.4,
        f32::INFINITY,
    );
    let note = paint::galley(
        painter,
        "to aim and fire",
        egui::FontId::proportional(12.0),
        hex(0xd9e8f6),
        0.0,
        f32::INFINITY,
    );
    let ring = 28.0;
    let text_width = title.size().x.max(note.size().x);
    let size = vec2(6.0 + ring + 9.0 + text_width + 14.0, 38.0);
    let tag = Rect::from_min_size(origin, size);
    let alpha = |color: Color32| color.gamma_multiply(opacity);
    painter.rect_filled(
        tag.translate(vec2(0.0, 6.0)),
        255,
        alpha(hexa(0x000000, 0.25)),
    );
    painter.rect_filled(tag, 255, alpha(hexa(0x06102a, 0.72)));
    painter.rect_stroke(
        tag,
        255,
        Stroke::new(1.0, alpha(hexa(0x9fd0ff, 0.2))),
        egui::StrokeKind::Inside,
    );
    let center = pos2(tag.left() + 6.0 + ring / 2.0, tag.center().y);
    let time = context.input(|input| input.time);
    let pulse = ((time % HINT_PULSE) / HINT_PULSE) as f32;
    painter.circle_stroke(
        center,
        (ring / 2.0 + 1.5) * (1.0 + 0.7 * pulse),
        Stroke::new(2.0, alpha(hexa(0xffffff, 0.7 * (1.0 - pulse)))),
    );
    painter.circle_stroke(
        center,
        ring / 2.0 - 1.5,
        Stroke::new(3.0, alpha(Color32::WHITE)),
    );
    painter.circle_filled(center, ring / 2.0 - 7.0, alpha(GOLD));
    let text_left = center.x + ring / 2.0 + 9.0;
    let title_height = title.size().y;
    painter.galley_with_override_text_color(
        pos2(text_left, tag.center().y - title_height),
        title,
        alpha(Color32::WHITE),
    );
    painter.galley_with_override_text_color(
        pos2(text_left, tag.center().y),
        note,
        alpha(hex(0xd9e8f6)),
    );
    // The ring breathes, so the frame after this one differs.
    context.request_repaint();
}

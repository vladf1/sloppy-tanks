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
use sloppy_core::sim::{GameMode, MatchPhase, Team, Weapon};
use sloppy_web::catalog::{AMMO_OPTIONS, AmmoOption};
use sloppy_web::feedback::CRITICAL_HULL;
use sloppy_web::feedback::cause_icon;
use sloppy_web::hud::{EffectKind, Hud, HudHuman, HullTone, Objective};

/// The screen edge the HUD keeps clear.
const EDGE: f32 = 16.0;
/// The gap between the score bar and the toolbar moved under it on a narrow screen.
const STACK_GAP: f32 = 8.0;
/// The kill feed's place under the toolbar where they sit side by side.
const FEED_TOP: f32 = 74.0;
/// A phone's feed rows, its notices' place down the screen and their widest.
const PHONE_FEED_WIDTH: f32 = 300.0;
const PHONE_TOAST_TOP: f32 = 0.22;
const PHONE_TOAST_MARGIN: f32 = 48.0;
/// The narrowest gap between the drive stick and the mine button, as a share of the
/// tank panel's width, that the panel sits in on a touch screen.
const MIN_PANEL_GAP_SHARE: f32 = 0.75;
/// The toolbar's distance from the top edge.
const TOOLBAR_TOP: f32 = 14.0;
/// A tablet's score sits this far from the top edge, level with the corner buttons, and
/// as far from the left edge where it takes the corner; its toolbar keeps as far from
/// the right edge.
const TABLET_MARGIN: f32 = 14.0;
/// At this width and narrower a tablet's score plates slim their padding and the clock
/// its minimum, so the score and the taller corner buttons never meet; at the second,
/// the score moves to the top-left corner and the buttons keep the top row beside it.
const TABLET_SLIM_SCORE_WIDTH: f32 = 1100.0;
const TABLET_CORNER_SCORE_WIDTH: f32 = 850.0;
/// The rounding of a tablet's score plates at their outer ends.
const TABLET_SCORE_CORNER: f32 = 10.0;
/// Big screens draw the score, the toolbar, the kill feed and the tank panel larger, so
/// their type holds its size against the arena: the screen's smallest size for each
/// zoom, the largest first (desktop only: touch screens lay their HUD out round the
/// thumbs).
const BIG_SCREEN_ZOOMS: [(Vec2, f32); 2] = [
    (Vec2::new(2400.0, 1300.0), 1.5),
    (Vec2::new(1800.0, 1000.0), 1.2),
];

/// The big-screen zoom on a screen of `size`.
fn big_screen_zoom(size: Vec2) -> f32 {
    if crate::touch::screen() {
        return 1.0;
    }
    BIG_SCREEN_ZOOMS
        .iter()
        .find(|(least, _)| size.x >= least.x && size.y >= least.y)
        .map_or(1.0, |&(_, zoom)| zoom)
}

/// Draw `layer` at `fit` about `anchor` (a part shrunk to fit a small screen), then at
/// the big-screen `zoom` about `screen_anchor`, the point of the screen's edge the part
/// keeps its distance from.
fn place_layer(
    context: &egui::Context,
    layer: LayerId,
    anchor: Pos2,
    fit: f32,
    screen_anchor: Pos2,
    zoom: f32,
) {
    let about = |point: Pos2, scale: f32| {
        egui::emath::TSTransform::new(point.to_vec2() * (1.0 - scale), scale)
    };
    context.set_transform_layer(layer, about(screen_anchor, zoom) * about(anchor, fit));
}

/// Whether a tablet's score sits in the top-left corner: centred, it would meet the
/// corner buttons.
pub(super) fn score_in_corner(context: &egui::Context) -> bool {
    crate::touch::screen()
        && !crate::touch::phone()
        && context.viewport_rect().width() <= TABLET_CORNER_SCORE_WIDTH
}

/// The painter under every HUD area: shades, feed, toast and hit feedback.
fn backdrop(context: &egui::Context) -> Painter {
    paint::full_screen(
        context,
        LayerId::new(Order::Background, Id::unique("hud backdrop")),
    )
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
    // Edge effects cover the whole screen; the HUD keeps to the safe area within it.
    let viewport = context.viewport_rect();
    let painter = backdrop(context);
    let time = context.input(|input| input.time);
    let human = &hud.human;
    let critical = playing && human.alive && human.health_ratio < CRITICAL_HULL;
    if critical {
        // A slow red pulse at the screen's edges while the hull is critical.
        let beat = ((time / 1.6 * std::f64::consts::TAU).cos() * 0.5 + 0.5) as f32;
        paint::vignette(
            &painter,
            viewport,
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
            viewport,
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
            viewport,
            pos2(
                viewport.center().x,
                viewport.top() + viewport.height() * 0.46,
            ),
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
        Rect::from_min_size(viewport.min, vec2(viewport.width(), 120.0)),
        &[
            (0.0, hexa(0x030a18, 0.45)),
            (0.55, hexa(0x030a18, 0.15)),
            (1.0, Color32::TRANSPARENT),
        ],
    ));
    if crate::touch::phone() {
        phone_hud(context, art, state, &hud, view, actions);
        return;
    }
    // The score bar shrinks to the screen's width; where the toolbar would cover it, the
    // toolbar and the feed move under it. Positions are the parts' own: the big-screen
    // zoom scales them about the top edge alike, so only the widths need it here.
    let zoom = big_screen_zoom(context.viewport_rect().size());
    let score_layer = LayerId::new(Order::Middle, Id::unique("score bar"));
    let mut score_painter = context.layer_painter(score_layer);
    score_painter.set_clip_rect(paint::shrunk_clip(screen));
    let score = score_bar(&score_painter, screen, &hud, view.room);
    let fit = ((screen.width() - 2.0 * EDGE) / score.width()).min(1.0);
    let (score_anchor, score_right) = if score_in_corner(context) {
        (score.left_top(), score.left() + score.width() * fit)
    } else {
        (
            score.center_top(),
            score.center().x + score.width() * fit * zoom / 2.0,
        )
    };
    place_layer(
        context,
        score_layer,
        score_anchor,
        fit,
        screen.center_top(),
        zoom,
    );
    let tools = toolbar_size();
    let stacked =
        score_right + STACK_GAP * zoom > screen.right() - (toolbar_edge() + tools.x) * zoom;
    let toolbar_top = if stacked {
        score.top() + score.height() * fit + STACK_GAP
    } else {
        screen.top() + TOOLBAR_TOP
    };
    let feed_top = if stacked {
        toolbar_top + tools.y + STACK_GAP
    } else {
        screen.top() + FEED_TOP
    };
    // The feed shares the backdrop, under the damage ring's edge glow; zoomed, it takes a
    // layer of its own over the backdrop.
    let feed_painter = if zoom > 1.0 {
        let layer = LayerId::new(Order::Middle, Id::unique("kill feed"));
        place_layer(
            context,
            layer,
            screen.right_top(),
            1.0,
            screen.right_top(),
            zoom,
        );
        paint::full_screen(context, layer)
    } else {
        painter.clone()
    };
    kill_feed(&feed_painter, art, screen, feed_top, state);
    if playing {
        toast(&painter, screen, state);
        toolbar(
            context,
            art,
            screen,
            toolbar_top,
            zoom,
            state,
            view,
            actions,
        );
        if human.alive {
            damage_ring(&painter, screen, state);
            if state.cockpit && state.aim_hint {
                aim_hint(&painter, screen);
            }
        }
    }
    if let Some(touch) = state.touch.filter(|_| playing && human.alive) {
        super::touch::controls(context, art, state, &touch);
    }
    // The respawn card rises into place over a third of a second.
    let rise = context.animate_bool_with_time(Id::unique("respawn rise"), !human.alive, 0.32);
    if human.alive {
        tank_panel(
            context, art, screen, state, &hud, playing, time, zoom, actions,
        );
    } else if playing && !view.room {
        // A room draws its own respawn card: tanks are chosen between battles there.
        super::respawn::card(context, art, screen, state, &hud, view, rise, actions);
    }
}

/// Under the sight while Esc has freed the cursor (`#cockpit .aim-hint`): how to take
/// it back, and how to reach the menu.
fn aim_hint(painter: &Painter, screen: Rect) {
    let font = bold_font(13.0);
    let label = "Click to aim · Esc for menu";
    let spacing = 13.0 * 0.04;
    let width = paint::text_width(painter, label, font.clone(), spacing) + 28.0;
    let pill = Rect::from_min_size(
        pos2(screen.center().x - width / 2.0, screen.center().y + 48.0),
        vec2(width, 29.0),
    );
    painter.rect(
        pill,
        CornerRadius::same(15),
        hexa(0x0b1830, 0.85),
        Stroke::new(1.0, hexa(0xbfe2ff, 0.33)),
        StrokeKind::Inside,
    );
    paint::text(
        painter,
        pill.center(),
        Align2::CENTER_CENTER,
        label,
        font,
        hex(0xf0e9ce),
        spacing,
    );
}

/// The phone's HUD (`.phone #hud`): only what a small screen carries over the arena.
/// The score pill and the row of keys share the top edge, the drive stick holds the
/// bottom-left corner and a touch on the arena aims and fires; the feed keeps the
/// viewer's own kills, notices flash mid-screen, and first person shows the hull compass
/// over a small hull plate. Overhead, the bar over the player's tank is the hull readout.
fn phone_hud(
    context: &egui::Context,
    art: &Art,
    state: &mut UiState,
    hud: &Hud,
    view: &View,
    actions: &mut Vec<Action>,
) {
    let playing = view.phase == MatchPhase::Playing;
    let screen = context.content_rect();
    let painter = backdrop(context);
    let keys = crate::touch::PhoneBar::of(context);
    let score_painter =
        context.layer_painter(LayerId::new(Order::Middle, Id::unique("phone score")));
    phone_score(&score_painter, screen, keys.view.top(), hud, view.room);
    kill_feed(&painter, art, screen, keys.view.bottom() + 10.0, state);
    let human = &hud.human;
    if playing {
        toast(&painter, screen, state);
        phone_keys(context, art, &keys, state, actions);
        if human.alive {
            damage_ring(&painter, screen, state);
        }
    }
    if let Some(touch) = state.touch.filter(|_| playing && human.alive) {
        super::touch::controls(context, art, state, &touch);
    }
    let rise = context.animate_bool_with_time(Id::unique("respawn rise"), !human.alive, 0.32);
    if human.alive {
        if state.cockpit {
            phone_cockpit(&painter, art, screen, state, human);
        }
    } else if playing && !view.room {
        super::respawn::strip(context, art, screen, state, hud, view, rise, actions);
    }
}

/// The corners of a phone key or plate that round: none, its left pair, its right pair,
/// or all four (`[nw, ne, sw, se]`).
const SQUARE: [f32; 4] = [0.0; 4];
const LEFT_ROUND: [f32; 4] = [PHONE_CORNER, 0.0, PHONE_CORNER, 0.0];
const RIGHT_ROUND: [f32; 4] = [0.0, PHONE_CORNER, 0.0, PHONE_CORNER];
const ALL_ROUND: [f32; 4] = [PHONE_CORNER; 4];
/// The phone's keys and score pill share this corner (`--hud-corner`).
const PHONE_CORNER: f32 = 10.0;

fn corner_radius([nw, ne, sw, se]: [f32; 4]) -> CornerRadius {
    CornerRadius {
        nw: nw as u8,
        ne: ne as u8,
        sw: sw as u8,
        se: se as u8,
    }
}

/// The phone's score pill (`.phone #hud .scoreboard`), a key's height at the top's
/// middle: blue score, the clock with the goal under it, red score. The team colours
/// stand in for the names; your side's plate wears a gold rim and a YOU tag. A solo
/// run's pill is its kills and the clock.
fn phone_score(painter: &Painter, screen: Rect, top: f32, hud: &Hud, room: bool) {
    let solo = hud.game_mode == GameMode::Solo;
    let height = crate::touch::PHONE_KEY;
    let score_font = display_font(19.0);
    let tag_font = bold_font(11.0);
    let small_font = bold_font(10.0);
    let pill = &hud.score_pill;
    // A phone's pill has room for the goal alone; its pause menu adds the rule.
    let objective = pill.objective.map(|objective| match objective {
        Objective::Survive => "SURVIVE".to_owned(),
        objective => objective.to_string(),
    });
    let time = pill.clock().to_string();
    let (time_font, time_color) = if pill.overtime {
        (bold_font(12.0), hex(0xff8a96))
    } else {
        (display_font(16.0), Color32::WHITE)
    };
    // Each plate's score, the tag under it and whether it is your side.
    let plates: Vec<(String, Option<&str>, bool)> = if solo {
        vec![(hud.human.kills.to_string(), Some("KILLS"), false)]
    } else {
        [Team::Blue, Team::Red]
            .into_iter()
            .map(|team| {
                let yours = !room && team == hud.human_team;
                let score = hud.match_state.scores[team.index()].to_string();
                (score, yours.then_some("YOU"), yours)
            })
            .collect()
    };
    let plate_width = |(score, tag, _): &(String, Option<&str>, bool)| {
        let tag = tag.map_or(0.0, |tag| {
            paint::text_width(painter, tag, tag_font.clone(), 1.2) + 10.0
        });
        (paint::text_width(painter, score, score_font.clone(), 0.0).max(tag) + 20.0).max(40.0)
    };
    let objective_width = objective.as_deref().map_or(0.0, |objective| {
        paint::text_width(painter, objective, small_font.clone(), 0.9)
    });
    let clock_width =
        (paint::text_width(painter, &time, time_font.clone(), 0.0).max(objective_width) + 12.0)
            .max(48.0);
    let widths: Vec<f32> = plates.iter().map(plate_width).collect();
    let total = widths.iter().sum::<f32>() + clock_width;
    let mut x = screen.center().x - total / 2.0;
    let upper = top + 16.0;
    let lower = top + 33.0;
    for (index, (score, tag, yours)) in plates.iter().enumerate() {
        let rect = Rect::from_min_size(pos2(x, top), vec2(widths[index], height));
        let corners = if index == 0 { LEFT_ROUND } else { RIGHT_ROUND };
        let (light, deep) = if index == 0 {
            (hex(0x2f8ff2), hex(0x1252b0))
        } else {
            (hex(0xff5367), hex(0xb01b33))
        };
        painter.add(paint::rounded_vgradient(
            rect,
            corners,
            &[(0.0, light), (1.0, deep)],
        ));
        if *yours {
            let rim = [corners[0], corners[1], 0.0, 0.0];
            painter.add(paint::rounded_vgradient(
                Rect::from_min_size(rect.min, vec2(rect.width(), 3.0)),
                rim,
                &[(0.0, GOLD), (1.0, GOLD)],
            ));
        }
        let score_y = if tag.is_some() {
            upper
        } else {
            rect.center().y
        };
        paint::shadowed(
            painter,
            pos2(rect.center().x, score_y),
            Align2::CENTER_CENTER,
            score,
            score_font.clone(),
            Color32::WHITE,
            0.0,
        );
        if let Some(tag) = tag {
            let width = paint::text_width(painter, tag, tag_font.clone(), 1.2) + 10.0;
            let badge = Rect::from_center_size(pos2(rect.center().x, lower), vec2(width, 15.0));
            let ink = if *yours {
                painter.add(paint::rounded_vgradient(
                    badge,
                    [3.0; 4],
                    &[(0.0, GOLD_HOT), (1.0, GOLD)],
                ));
                hex(0x10213a)
            } else {
                painter.rect_filled(badge, 3, hexa(0xffffff, 0.18));
                Color32::WHITE
            };
            paint::text(
                painter,
                badge.center(),
                Align2::CENTER_CENTER,
                tag,
                tag_font.clone(),
                ink,
                1.2,
            );
        }
        x = rect.right();
        if index == 0 {
            // The clock between the plates, or after a solo run's only plate.
            let clock = Rect::from_min_size(pos2(x, top), vec2(clock_width, height));
            painter.add(paint::rounded_vgradient(
                clock,
                if solo { RIGHT_ROUND } else { SQUARE },
                &[(0.0, hex(0x0f2140)), (1.0, hex(0x07111f))],
            ));
            let time_y = if objective.is_some() {
                upper
            } else {
                clock.center().y
            };
            paint::shadowed(
                painter,
                pos2(clock.center().x, time_y),
                Align2::CENTER_CENTER,
                &time,
                time_font.clone(),
                time_color,
                0.0,
            );
            if let Some(objective) = &objective {
                paint::text(
                    painter,
                    pos2(clock.center().x, lower),
                    Align2::CENTER_CENTER,
                    objective,
                    small_font.clone(),
                    GOLD_HOT,
                    0.9,
                );
            }
            x = clock.right();
        }
    }
}

/// One glass key of the phone's top row, its rounded corners as `corners` say; `lit`
/// (first person on) in gold.
fn phone_key(painter: &Painter, rect: Rect, corners: [f32; 4], lit: bool) {
    let radius = corner_radius(corners);
    painter.rect_filled(rect.translate(vec2(0.0, 4.0)), radius, hexa(0x000000, 0.25));
    let stops = if lit {
        [(0.0, GOLD_HOT), (1.0, GOLD)]
    } else {
        [(0.0, hexa(0x13284a, 0.9)), (1.0, hexa(0x08142a, 0.95))]
    };
    painter.add(paint::rounded_vgradient(rect, corners, &stops));
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(1.0, theme::PLATE_EDGE),
        StrokeKind::Inside,
    );
}

/// The phone's top row: the zoom pair (which the touch controls hit-test, so a thumb on
/// them never reaches egui), first person and pause. First person has no zoom.
fn phone_keys(
    context: &egui::Context,
    art: &Art,
    keys: &crate::touch::PhoneBar,
    state: &mut UiState,
    actions: &mut Vec<Action>,
) {
    if state.settings_draft.is_some() {
        return;
    }
    let painter = context.layer_painter(LayerId::new(Order::Middle, Id::unique("phone keys")));
    let divider = |x: f32, rect: Rect| {
        painter.line_segment(
            [pos2(x, rect.top() + 9.0), pos2(x, rect.bottom() - 9.0)],
            Stroke::new(1.0, hexa(0x9fd0ff, 0.14)),
        );
    };
    if state.touch.is_some_and(|touch| !touch.first_person) {
        phone_key(&painter, keys.zoom_out, LEFT_ROUND, false);
        phone_key(&painter, keys.zoom_in, RIGHT_ROUND, false);
        divider(keys.zoom_in.left(), keys.zoom_in);
        for (rect, label) in [(keys.zoom_out, "−"), (keys.zoom_in, "+")] {
            paint::text(
                &painter,
                rect.center(),
                Align2::CENTER_CENTER,
                label,
                bold_font(18.0),
                hex(0xc9d9ea),
                0.0,
            );
        }
    }
    // Held upright first person and pause are one pair; held sideways two keys apart.
    let (view_corners, pause_corners) = if keys.upright {
        (LEFT_ROUND, RIGHT_ROUND)
    } else {
        (ALL_ROUND, ALL_ROUND)
    };
    let both = keys.view.union(keys.pause);
    egui::Area::new(Id::unique("phone keys"))
        .fixed_pos(both.min)
        .order(Order::Middle)
        .show(context, |ui| {
            ui.allocate_exact_size(both.size(), Sense::hover());
            for (index, (rect, corners, icon, action)) in [
                (keys.view, view_corners, "eye", Action::ToggleView),
                (keys.pause, pause_corners, "pause", Action::Pause),
            ]
            .into_iter()
            .enumerate()
            {
                let response = ui.interact(rect, Id::unique(("phone key", index)), Sense::click());
                paint::labelled(
                    &response,
                    true,
                    if index == 0 { "Switch view" } else { "Pause" },
                );
                let lit = index == 0 && state.cockpit;
                phone_key(&painter, rect, corners, lit);
                art.icon(
                    &painter,
                    icon,
                    Rect::from_center_size(rect.center(), Vec2::splat(18.0)),
                    if lit { hex(0x10233c) } else { hex(0xe9f2fb) },
                );
                if response.clicked() {
                    actions.push(action);
                }
            }
            if keys.upright {
                divider(keys.pause.left(), keys.pause);
            }
        });
}

/// First person on a phone (`.phone #hud.first-person .bottom`): the hull compass low in
/// the middle, between the drive stick and the thumb that turns the view, over a small
/// hull plate, since from inside the turret the bar over the tank is out of sight.
fn phone_cockpit(painter: &Painter, art: &Art, screen: Rect, state: &UiState, human: &HudHuman) {
    let ramp = hex(human.hull_ramp);
    let hp = format!("{}", human.hp.max(0.0).ceil());
    let number_font = display_font(20.0);
    let number_width = paint::text_width(painter, &hp, number_font.clone(), 0.0);
    let bar_width = 60.0;
    let width = 12.0 + 16.0 + 4.0 + number_width + 8.0 + bar_width + 12.0;
    let plate = Rect::from_center_size(
        pos2(screen.center().x, screen.bottom() - 10.0 - 15.5),
        vec2(width, 31.0),
    );
    paint::plate(painter, plate, 6.0, None);
    let middle = plate.center().y;
    art.icon(
        painter,
        "hull",
        Rect::from_center_size(pos2(plate.left() + 20.0, middle), Vec2::splat(16.0)),
        ramp,
    );
    paint::shadowed(
        painter,
        pos2(plate.left() + 32.0, middle),
        Align2::LEFT_CENTER,
        &hp,
        number_font,
        Color32::WHITE,
        0.0,
    );
    let bar = Rect::from_min_size(
        pos2(plate.right() - 12.0 - bar_width, middle - 3.5),
        vec2(bar_width, 7.0),
    );
    painter.rect_filled(bar, 3, hex(0x030913));
    let share = human.health_ratio.clamp(0.0, 1.0) as f32;
    painter.rect_filled(
        Rect::from_min_size(bar.min, vec2(bar.width() * share, bar.height())),
        3,
        ramp,
    );
    for segment in 1..10 {
        let x = bar.left() + bar.width() * segment as f32 / 10.0;
        painter.rect_filled(
            Rect::from_min_max(pos2(x - 1.0, bar.top()), pos2(x, bar.bottom())),
            0,
            hex(0x030913),
        );
    }
    // The compass in its ring over the plate.
    let center = pos2(plate.center().x, plate.top() - 6.0 - 24.0);
    painter.circle_filled(center, 24.0, hexa(0x050d1c, 0.85));
    painter.circle_stroke(center, 24.0, Stroke::new(1.0, theme::PLATE_EDGE));
    compass_dial(
        painter,
        Rect::from_min_size(pos2(center.x - 32.0, center.y - 28.0), vec2(68.0, 56.0)),
        state.hull_angle,
    );
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
/// them, each with a bar filling toward the score limit; your side carries gold. A
/// tablet's (`.touch-enabled .scoreboard`) sits level with the corner buttons, its plates
/// rounded at their outer ends; it slims on a narrower screen and takes the top-left
/// corner on a narrower one still.
/// The score bar; returns where it is.
fn score_bar(painter: &Painter, screen: Rect, hud: &Hud, room: bool) -> Rect {
    let solo = hud.game_mode == GameMode::Solo;
    let match_state = &hud.match_state;
    let no_limit = room || solo || hud.endless_match;
    let height = if room { 55.0 } else { 62.0 };
    // The phone edition draws its own score.
    let tablet = crate::touch::screen();
    let viewport = painter.ctx().viewport_rect();
    let slim = tablet && viewport.width() <= TABLET_SLIM_SCORE_WIDTH;
    let top = if tablet {
        screen.top() + TABLET_MARGIN
    } else {
        screen.top()
    };
    // Each plate's name keeps from its outer end, its score from the clock.
    let (label_inset, score_inset) = if slim { (18.0, 14.0) } else { (24.0, 18.0) };
    let label_font = bold_font(14.0);
    let score_font = display_font(38.0);
    let plate_width = |label: &str, score: &str| {
        let label = paint::text_width(painter, label, label_font.clone(), 1.6);
        let score = paint::text_width(painter, score, score_font.clone(), 0.0);
        (label + score + 10.0 + (label_inset + score_inset)).max(156.0)
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
    let clock_least = if slim { 96.0 } else { 136.0 };
    let clock_width = (objective_width + 24.0).max(clock_least) + if solo { 12.0 } else { 0.0 };
    let widths = [
        plate_width(&labels[0], &scores[0]),
        if solo {
            0.0
        } else {
            plate_width(&labels[1], &scores[1])
        },
    ];
    let total = widths[0] + clock_width + widths[1];
    let left = if score_in_corner(painter.ctx()) {
        (viewport.left() + TABLET_MARGIN).max(screen.left())
    } else {
        screen.center().x - total / 2.0
    };
    let shift = vec2(0.0, top);
    for team in 0..if solo { 1 } else { 2 } {
        let width = widths[team];
        let x = if team == 0 {
            left
        } else {
            left + widths[0] + clock_width
        };
        let (light, deep) = if team == 0 {
            (hex(0x2f8ff2), hex(0x1252b0))
        } else {
            (hex(0xff5367), hex(0xb01b33))
        };
        let yours = !room && !solo && team == hud.human_team.index();
        // A tablet's plate rounds its outer end where the desktop's is cut toward the
        // clock; the gloss over its top half and your side's gold rim follow the corner.
        let outer = |corners: [f32; 4]| {
            if team == 0 {
                corners
            } else {
                [corners[1], corners[0], corners[3], corners[2]]
            }
        };
        if tablet {
            let corner = TABLET_SCORE_CORNER;
            let plate = Rect::from_min_size(pos2(x, top), vec2(width, height));
            painter.add(paint::rounded_vgradient(
                plate,
                outer([corner, 0.0, corner, 0.0]),
                &[(0.0, light), (1.0, deep)],
            ));
            painter.add(paint::rounded_vgradient(
                Rect::from_min_size(plate.min, vec2(width, height * 0.46)),
                outer([corner, 0.0, 0.0, 0.0]),
                &[(0.0, hexa(0xffffff, 0.2)), (1.0, hexa(0xffffff, 0.0))],
            ));
            if yours {
                // Deep enough that the band keeps the corner's full rounding.
                let band = 24.0;
                painter.add(paint::rounded_vgradient(
                    Rect::from_min_size(plate.min, vec2(width, band)),
                    outer([corner, 0.0, 0.0, 0.0]),
                    &[
                        (0.0, GOLD),
                        (3.0 / band, GOLD),
                        (3.5 / band, hexa(0xffcf38, 0.4)),
                        (16.0 / band, hexa(0xffcf38, 0.0)),
                        (1.0, hexa(0xffcf38, 0.0)),
                    ],
                ));
            }
        } else {
            let mut outline = team_plate(x, width, height, team == 0);
            outline.iter_mut().for_each(|point| *point += shift);
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
        }
        let middle = top + height / 2.0 - 2.0;
        let (label_pos, label_align, score_pos, score_align) = if team == 0 {
            (
                pos2(x + label_inset, middle),
                Align2::LEFT_CENTER,
                pos2(x + width - score_inset, middle),
                Align2::RIGHT_CENTER,
            )
        } else {
            (
                pos2(x + width - label_inset, middle),
                Align2::RIGHT_CENTER,
                pos2(x + score_inset, middle),
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
    // With the YOUR TEAM tab under a plate.
    Rect::from_min_size(pos2(left, top), vec2(total, height + 26.0))
}

/// The corner toolbar: first person, fullscreen, pause and settings on one plate.
/// Thumb-sized on a touch screen (`.touch-enabled .hud-actions button`).
fn toolbar_button() -> Vec2 {
    if crate::touch::screen() {
        vec2(48.0, 46.0)
    } else {
        Vec2::splat(38.0)
    }
}

/// The toolbar's distance from the right edge.
fn toolbar_edge() -> f32 {
    if crate::touch::screen() {
        TABLET_MARGIN
    } else {
        EDGE
    }
}

/// The toolbar's plate: first person, fullscreen (not in the iPad app), pause and
/// settings.
fn toolbar_size() -> Vec2 {
    let count = if crate::app::offers_fullscreen() {
        4.0
    } else {
        3.0
    };
    let button = toolbar_button();
    vec2(button.x * count + 6.0, button.y + 6.0)
}

#[allow(clippy::too_many_arguments)]
fn toolbar(
    context: &egui::Context,
    art: &Art,
    screen: Rect,
    top: f32,
    zoom: f32,
    state: &mut UiState,
    view: &View,
    actions: &mut Vec<Action>,
) {
    let id = Id::unique("hud toolbar");
    place_layer(
        context,
        LayerId::new(Order::Middle, id),
        screen.right_top(),
        1.0,
        screen.right_top(),
        zoom,
    );
    if state.settings_draft.is_some() {
        return;
    }
    let button = toolbar_button();
    let fullscreen = (
        if view.prefs.fullscreen {
            "collapse"
        } else {
            "expand"
        },
        match (view.prefs.fullscreen, cfg!(target_os = "macos")) {
            (true, true) => "Exit fullscreen (Cmd+F)",
            (true, false) => "Exit fullscreen (F11)",
            (false, true) => "Enter fullscreen (Cmd+F)",
            (false, false) => "Enter fullscreen (F11)",
        },
        Action::ToggleFullscreen,
    );
    let buttons: Vec<(&str, &str, Action)> = [
        Some(("eye", "First-person view (V)", Action::ToggleView)),
        crate::app::offers_fullscreen().then_some(fullscreen),
        Some(("pause", "Pause (Esc)", Action::Pause)),
        Some(("gear", "Settings", Action::OpenSettings)),
    ]
    .into_iter()
    .flatten()
    .collect();
    let size = toolbar_size();
    let origin = pos2(screen.right() - toolbar_edge() - size.x, top);
    egui::Area::new(id)
        .fixed_pos(origin)
        .order(Order::Middle)
        .show(context, |ui| {
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            let painter = ui.painter().clone();
            paint::plate(&painter, rect, 9.0, None);
            for (index, (icon, tip, action)) in buttons.into_iter().enumerate() {
                let cell = Rect::from_min_size(
                    rect.min + vec2(3.0 + index as f32 * button.x, 3.0),
                    button,
                );
                let response = ui
                    .interact(cell, Id::unique(("hud tool", index)), Sense::click())
                    .on_hover_text(tip);
                paint::labelled(&response, true, tip);
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
                    if action == Action::OpenSettings {
                        state.resume_after_settings = true;
                    }
                    actions.push(action);
                }
            }
        });
}

/// The kill feed: killer, weapon, victim, newest on top, one fixed-width steel plate
/// per row with each side's team as a bright edge at its end. A phone's keeps only the
/// viewer's own kills, under its right-hand keys.
fn kill_feed(painter: &Painter, art: &Art, screen: Rect, top: f32, state: &UiState) {
    let phone = crate::touch::phone();
    let (width, right) = if phone {
        (
            PHONE_FEED_WIDTH.min(screen.width() - 16.0),
            screen.right() - 8.0,
        )
    } else {
        (336.0_f32.min(screen.width() * 0.42), screen.right() - EDGE)
    };
    let row_height = 30.0;
    let lines = if state.cockpit { 2 } else { super::FEED_LINES };
    let first = top;
    let entries = state
        .kill_feed
        .iter()
        .filter(|entry| !phone || entry.own_kill);
    for (index, entry) in entries.take(lines).enumerate() {
        let top = first + index as f32 * (row_height + 4.0);
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
    let phone = crate::touch::phone();
    // The respawn card names the killer; a phone's small strip leaves that to the notice.
    if toast.text.is_empty() || (toast.tone == ToastTone::Death && !phone) {
        return;
    }
    let (rule, color, size) = match toast.tone {
        ToastTone::Pickup => (hexa(0x6fe6ff, 0.47), hex(0xbff6ff), 14.0),
        ToastTone::Promotion => (hexa(0xffcf38, 0.4), GOLD_HOT, 17.0),
        ToastTone::Death => (hexa(0xff4d5e, 0.53), hex(0xffd6da), 14.0),
    };
    // A phone's notices flash mid-screen, smaller.
    let (size, spacing, padding, max) = if phone {
        (
            12.0,
            1.0,
            vec2(28.0, 14.0),
            screen.width() - PHONE_TOAST_MARGIN,
        )
    } else {
        (
            size,
            1.4,
            vec2(88.0, 16.0),
            (screen.width() - 40.0).min(720.0),
        )
    };
    let font = bold_font(size);
    let max = max - padding.x;
    let galley = paint::galley(painter, &toast.text, font.clone(), color, spacing, max);
    // The entrance settles from a slightly smaller band.
    let grow = 0.92 + 0.08 * ((toast.age / 0.25).min(1.0) as f32);
    let size = (galley.size() + padding) * vec2(grow, 1.0);
    let top = if phone {
        screen.top() + screen.height() * PHONE_TOAST_TOP
    } else {
        screen.top() + 150.0
    };
    let rect = Rect::from_center_size(pos2(screen.center().x, top + size.y / 2.0), size);
    let band = hexa(0x08142a, 0.91);
    // The band fades out at both ends, past a phone's tighter text.
    let edge = if phone { 0.06 } else { 0.16 };
    painter.add(paint::hgradient(
        rect,
        &[
            (0.0, Color32::TRANSPARENT),
            (edge, band),
            (1.0 - edge, band),
            (1.0, Color32::TRANSPARENT),
        ],
    ));
    for y in [rect.top(), rect.bottom()] {
        painter.add(paint::hgradient(
            Rect::from_min_max(pos2(rect.left(), y - 0.5), pos2(rect.right(), y + 0.5)),
            &[
                (0.0, Color32::TRANSPARENT),
                (edge, rule),
                (1.0 - edge, rule),
                (1.0, Color32::TRANSPARENT),
            ],
        ));
    }
    let shadow = paint::galley(painter, &toast.text, font, hexa(0, 0.6), spacing, max);
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
/// A touch screen's panel (`.touch-enabled .bottom`): its foot's distance from the
/// screen's bottom, and the widest the column between the thumbs that it centres in.
const TOUCH_PANEL_BOTTOM: f32 = 22.0;
const TOUCH_PANEL_COLUMN: f32 = 460.0;
/// A touch screen's status line: its height (the hull number's), the gap between its
/// parts, the hull bar's narrowest and half the name's line, which it centres.
const TOUCH_LINE_HEIGHT: f32 = 26.0;
const TOUCH_LINE_GAP: f32 = 12.0;
const TOUCH_HULL_BAR: f32 = 130.0;
const TOUCH_NAME_HALF: f32 = 8.0;
/// Inside a touch key: the padding and border above and below its content, its icons,
/// and its count's type (the loaded round's, in its top-right corner, a step larger:
/// that count's right end's inset and its middle's depth).
const TOUCH_KEY_TOP: f32 = 7.0;
const TOUCH_KEY_BOTTOM: f32 = 6.0;
const TOUCH_KEY_ICON: f32 = 22.0;
const TOUCH_COUNT_SIZE: f32 = 13.0;
const TOUCH_LOADED_COUNT_SIZE: f32 = 15.0;
const TOUCH_COUNT_INSET: f32 = 8.0;
const TOUCH_COUNT_MIDDLE: f32 = 18.5;
/// The gap between the tank's name and its rank.
const RANK_GAP: f32 = 7.0;

/// The hull number's type, and how far under the readout's middle its foot and the
/// maximum's sit, so the two share a baseline.
struct ReadoutType {
    size: f32,
    number_drop: f32,
    max_drop: f32,
}

const DESKTOP_READOUT: ReadoutType = ReadoutType {
    size: 32.0,
    number_drop: 13.0,
    max_drop: 11.0,
};
/// A touch screen's readout is a step smaller, on one line with the name and the bar,
/// its number centred on the hull icon.
const TOUCH_READOUT: ReadoutType = ReadoutType {
    size: 26.0,
    number_drop: 16.0,
    max_drop: 15.0,
};
/// The hull number's distance from the readout's left end, past the hull icon; the
/// maximum's type, its letter spacing and its gap after the number.
const READOUT_NUMBER_LEFT: f32 = 24.0;
const READOUT_MAX_SIZE: f32 = 21.0;
const READOUT_MAX_SPACING: f32 = 0.5;
const READOUT_MAX_GAP: f32 = 5.0;

/// The width of a round's key in the desktop rack: the loaded one names itself.
fn slot_width(selected: bool) -> f32 {
    if selected {
        SELECTED_SLOT_WIDTH
    } else {
        SLOT_WIDTH
    }
}

/// The desktop rack's width: every round's key and the gaps between them.
fn desktop_rack_width(human: &HudHuman) -> f32 {
    AMMO_OPTIONS
        .iter()
        .map(|option| {
            slot_width(
                human
                    .ammo
                    .iter()
                    .any(|slot| slot.weapon == option.weapon && slot.selected),
            )
        })
        .sum::<f32>()
        + SLOT_GAP * (AMMO_OPTIONS.len() - 1) as f32
}

/// A touch screen's tank panel (`.touch-enabled .combat-status` and the media queries
/// that tighten it): the tank's name and rank, the hull number and the bar on one line,
/// and under it the rack, its keys spread across the panel's width.
#[derive(Clone, Copy)]
struct TouchPanel {
    /// Inside the plate: left, top, right, bottom.
    padding: [f32; 4],
    /// Between the status line and the rack.
    row_gap: f32,
    /// Whether the tank's name and rank show: a narrow or short screen leaves them out.
    label: bool,
    /// The rack's narrowest: a wide screen's keys keep a thumb's width.
    min_rack: f32,
    rack: TouchRack,
}

/// A touch screen's rack (`.touch-enabled .ammo-strip`).
#[derive(Clone, Copy)]
struct TouchRack {
    key_height: f32,
    gap: f32,
    /// A key's side padding and border, which it keeps however narrow: the keys share
    /// out the rest.
    key_frame: f32,
    /// The loaded round's name.
    name_size: f32,
    name_spacing: f32,
}

/// How a touch screen's status line shares the panel's width: the name and rank with
/// the gap after them (nothing where they do not show), the hull readout, and the bar,
/// which takes what is left.
struct StatusLine {
    width: f32,
    label: f32,
    readout: f32,
    bar: f32,
}

impl TouchPanel {
    /// The panel on a screen of `size`: `touch-controls.css`'s rules in their order.
    fn on(size: Vec2) -> Self {
        let mut panel = Self {
            padding: [13.0, 9.0, 10.0, 9.0],
            row_gap: 8.0,
            label: true,
            min_rack: 380.0,
            rack: TouchRack {
                key_height: 54.0,
                gap: 4.0,
                key_frame: 10.0,
                name_size: 12.0,
                name_spacing: 0.3,
            },
        };
        // An upright tablet's rack shares the gap between the thumbs: its keys narrow
        // with it and the loaded round's name takes a tighter setting.
        if size.x <= 900.0 {
            panel.min_rack = 0.0;
            panel.rack.key_frame = 6.0;
            panel.rack.name_size = 11.0;
            panel.rack.name_spacing = 0.0;
        }
        // A narrow or short screen tightens the panel to fit beside the stick.
        if size.x <= 760.0 || size.y <= 600.0 {
            panel.padding = [5.0; 4];
            panel.label = false;
            panel.min_rack = 0.0;
            panel.rack.gap = 2.0;
            panel.rack.key_height = 48.0;
            panel.rack.name_size = 11.0;
            panel.rack.name_spacing = 0.0;
        }
        if size.y >= size.x && size.x >= 700.0 {
            panel.padding = [6.0; 4];
            panel.row_gap = 4.0;
            panel.rack.gap = 3.0;
        }
        panel
    }

    /// How the status line shares the panel's width. The readout keeps a full hull's
    /// width, so the panel holds its size as the hull drains.
    fn line(&self, painter: &Painter, human: &HudHuman) -> StatusLine {
        let label = if self.label {
            tank_label_width(painter, human) + TOUCH_LINE_GAP
        } else {
            0.0
        };
        let full = format!("{}", human.max_hp.round());
        let readout = READOUT_NUMBER_LEFT
            + paint::text_width(painter, &full, display_font(TOUCH_READOUT.size), 0.0)
            + READOUT_MAX_GAP
            + paint::text_width(
                painter,
                &format!("/ {full}"),
                display_font(READOUT_MAX_SIZE),
                READOUT_MAX_SPACING,
            );
        let width = (label + readout + TOUCH_LINE_GAP + TOUCH_HULL_BAR).max(self.min_rack);
        StatusLine {
            width,
            label,
            readout,
            bar: width - label - readout - TOUCH_LINE_GAP,
        }
    }

    fn plate_size(&self, line: &StatusLine) -> Vec2 {
        let [left, top, right, bottom] = self.padding;
        vec2(
            left + line.width + right,
            top + TOUCH_LINE_HEIGHT + self.row_gap + self.rack.key_height + bottom,
        )
    }
}

impl TouchRack {
    /// The keys' widths across `width`: equal shares, the standard shell's two, so the
    /// rack reads as the round in hand plus the four that crates bring.
    fn key_widths(&self, width: f32) -> [f32; AMMO_OPTIONS.len()] {
        let shares = |option: &AmmoOption| {
            if option.weapon == Weapon::Standard {
                2.0
            } else {
                1.0
            }
        };
        let keys = AMMO_OPTIONS.len() as f32;
        let share = (width - self.gap * (keys - 1.0) - self.key_frame * keys)
            / AMMO_OPTIONS.iter().map(shares).sum::<f32>();
        AMMO_OPTIONS.map(|option| self.key_frame + share.max(0.0) * shares(&option))
    }
}

/// The tank panel on an armour plate tinted by your team: hull on the left, the
/// ammunition rack and the mine socket on the right; a touch screen's ([`TouchPanel`])
/// has its own mine button. Power-up tabs stand on its top edge, the ammunition
/// notice above its left end, and in first person the hull compass joins its right
/// edge.
#[allow(clippy::too_many_arguments)]
fn tank_panel(
    context: &egui::Context,
    art: &Art,
    screen: Rect,
    state: &UiState,
    hud: &Hud,
    playing: bool,
    time: f64,
    zoom: f32,
    actions: &mut Vec<Action>,
) {
    let human = &hud.human;
    let id = Id::unique("tank panel");
    let layer = LayerId::new(Order::Middle, id);
    let touch = crate::touch::screen().then(|| {
        let panel = TouchPanel::on(context.viewport_rect().size());
        let line = panel.line(&context.layer_painter(layer), human);
        (panel, line)
    });
    let strip_width = desktop_rack_width(human);
    let socket = 10.0 + 10.0 + MINE_WIDTH;
    let inner = vec2(
        STATUS_WIDTH + 10.0 + 10.0 + strip_width + socket,
        PANEL_HEIGHT,
    );
    let padding = [14.0, 10.0, 11.0, 10.0];
    let plate_size = match &touch {
        Some((panel, line)) => panel.plate_size(line),
        None => vec2(
            inner.x + padding[0] + padding[2],
            inner.y + padding[1] + padding[3],
        ),
    };
    let compass = state.cockpit;
    let size = plate_size + vec2(if compass { COMPASS_WIDTH } else { 0.0 }, 0.0);
    // Where its bottom-left corner sits, the width it has and, on a touch screen, the
    // column the ammunition notice keeps to (its left end and width). There it sits
    // between the drive stick and the mine button, centred in that gap, so it never
    // covers the arena's middle or sits under a thumb; where that gap is too narrow for
    // it it stands over the controls at the screen's width.
    let (corner, room, notice_column) = if touch.is_some() {
        let layout = crate::touch::TouchLayout::of(context);
        let (from, to) = layout.panel_span(screen.right() - EDGE);
        if to - from >= size.x * MIN_PANEL_GAP_SHARE {
            let middle = (from + to) / 2.0;
            let column = (to - from).min(TOUCH_PANEL_COLUMN);
            let bottom =
                (context.viewport_rect().bottom() - TOUCH_PANEL_BOTTOM).min(screen.bottom());
            let left = (middle - size.x / 2.0).max(from);
            (
                pos2(left, bottom),
                to - from,
                Some((middle - column / 2.0, column)),
            )
        } else {
            let top = layout.stick.top().min(layout.zoom_in.top());
            (
                pos2(screen.left() + EDGE, top - STACK_GAP),
                screen.width() - 2.0 * EDGE,
                Some((screen.left() + EDGE, screen.width() - 2.0 * EDGE)),
            )
        }
    } else {
        (
            pos2(screen.left() + EDGE, screen.bottom() - EDGE),
            screen.width() - 2.0 * EDGE,
            None,
        )
    };
    let origin = corner - vec2(0.0, size.y);
    // A narrow screen (a phone) shows the whole panel smaller, its corner in place.
    let fit = (room / size.x).min(1.0);
    place_layer(context, layer, corner, fit, screen.left_bottom(), zoom);
    egui::Area::new(id)
        .fixed_pos(origin)
        .order(Order::Middle)
        .show(context, |ui| {
            ui.set_clip_rect(paint::shrunk_clip(screen));
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
            match &touch {
                Some((panel, line)) => {
                    let [left, top, ..] = panel.padding;
                    let status = Rect::from_min_size(
                        plate_rect.min + vec2(left, top),
                        vec2(line.width, TOUCH_LINE_HEIGHT),
                    );
                    hull_line(ui, &painter, art, status, line, state, human, playing, time);
                    let strip = Rect::from_min_size(
                        pos2(status.left(), status.bottom() + panel.row_gap),
                        vec2(line.width, panel.rack.key_height),
                    );
                    let rack = Some(&panel.rack);
                    ammo_rack(ui, &painter, art, strip, human, playing, rack, actions);
                }
                None => {
                    let content =
                        Rect::from_min_size(plate_rect.min + vec2(padding[0], padding[1]), inner);
                    hull_status(ui, &painter, art, content, state, human, playing, time);
                    let mut x = content.left() + STATUS_WIDTH + 10.0;
                    painter.line_segment(
                        [pos2(x, content.top()), pos2(x, content.bottom())],
                        Stroke::new(1.0, hexa(0xffffff, 0.09)),
                    );
                    x += 10.0;
                    let strip =
                        Rect::from_min_size(pos2(x, content.top()), vec2(strip_width, inner.y));
                    ammo_rack(ui, &painter, art, strip, human, playing, None, actions);
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
                }
            }
            effect_tabs(&painter, art, plate_rect, human);
            if let Some((notice, _)) = &state.ammo_notice {
                let (left, width) =
                    notice_column.unwrap_or((plate_rect.left(), screen.width() - 2.0 * EDGE));
                ammo_notice(&painter, left, plate_rect.top(), width, notice);
            }
        });
}

/// A slow red pulse behind the hull's status while it is critical.
fn critical_glow(painter: &Painter, rect: Rect, human: &HudHuman, playing: bool, time: f64) {
    let critical = playing && human.alive && human.health_ratio < CRITICAL_HULL;
    if critical {
        let beat = (0.5 - 0.5 * (time / 1.6 * std::f64::consts::TAU).cos()) as f32;
        painter.rect_filled(
            rect.expand2(vec2(5.0, 5.0)).intersect(rect.expand(5.0)),
            8,
            hexa(0xff3b4a, 0.06 + 0.14 * beat),
        );
    }
}

/// The desktop's status column: tank and rank, the hull number and the segmented bar.
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
    critical_glow(painter, rect, human, playing, time);
    tank_label(
        ui,
        painter,
        rect.left_top(),
        STATUS_WIDTH - 40.0,
        rect.right() + 30.0,
        human,
    );
    hull_readout(
        painter,
        art,
        rect.left(),
        rect.top() + 36.0,
        &DESKTOP_READOUT,
        human,
        playing,
        time,
    );
    let bar = Rect::from_min_size(
        pos2(rect.left(), rect.bottom() - 9.0),
        vec2(STATUS_WIDTH, 9.0),
    );
    hull_bar(painter, bar, state, human);
}

/// A touch screen's status line: the tank's name and rank, the hull readout and the
/// bar, each centred on the line.
#[allow(clippy::too_many_arguments)]
fn hull_line(
    ui: &mut egui::Ui,
    painter: &Painter,
    art: &Art,
    rect: Rect,
    line: &StatusLine,
    state: &UiState,
    human: &HudHuman,
    playing: bool,
    time: f64,
) {
    critical_glow(painter, rect, human, playing, time);
    let middle = rect.center().y;
    if line.label > 0.0 {
        let name = pos2(rect.left(), middle - TOUCH_NAME_HALF);
        tank_label(ui, painter, name, f32::INFINITY, f32::INFINITY, human);
    }
    let readout = rect.left() + line.label;
    hull_readout(
        painter,
        art,
        readout,
        middle,
        &TOUCH_READOUT,
        human,
        playing,
        time,
    );
    let bar = Rect::from_min_size(
        pos2(readout + line.readout + TOUCH_LINE_GAP, middle - 4.5),
        vec2(line.bar, 9.0),
    );
    hull_bar(painter, bar, state, human);
}

/// The tank's name from `top_left`, cut to `name_width`, and its rank's pill after it,
/// ending by `pill_end` at the latest.
fn tank_label(
    ui: &mut egui::Ui,
    painter: &Painter,
    top_left: Pos2,
    name_width: f32,
    pill_end: f32,
    human: &HudHuman,
) {
    let name = human.vehicle_name.to_uppercase();
    let name_rect = paint::text_fit(
        painter,
        top_left,
        Align2::LEFT_TOP,
        &name,
        bold_font(13.0),
        Color32::WHITE,
        1.4,
        name_width,
    );
    let rank = human.rank_name.to_uppercase();
    let rank_width = paint::text_width(painter, &rank, bold_font(12.0), 0.8) + 12.0;
    let rank_rect = Rect::from_min_size(
        pos2(name_rect.right() + RANK_GAP, top_left.y - 1.0),
        vec2(
            rank_width.min(pill_end - name_rect.right() - RANK_GAP),
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
}

/// The width the tank's name and its rank's pill take.
fn tank_label_width(painter: &Painter, human: &HudHuman) -> f32 {
    let name = human.vehicle_name.to_uppercase();
    let rank = human.rank_name.to_uppercase();
    paint::text_width(painter, &name, bold_font(13.0), 1.4)
        + RANK_GAP
        + paint::text_width(painter, &rank, bold_font(12.0), 0.8)
        + 12.0
}

/// The hull readout from `left`, centred on `middle`: its icon, the number in the
/// ramp's colour and the maximum.
#[allow(clippy::too_many_arguments)]
fn hull_readout(
    painter: &Painter,
    art: &Art,
    left: f32,
    middle: f32,
    kind: &ReadoutType,
    human: &HudHuman,
    playing: bool,
    time: f64,
) {
    let ramp = hex(human.hull_ramp);
    let tone = human.hull_tone;
    art.icon(
        painter,
        "hull",
        Rect::from_center_size(pos2(left + 10.0, middle), Vec2::splat(20.0)),
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
        pos2(left + READOUT_NUMBER_LEFT, middle + kind.number_drop),
        Align2::LEFT_BOTTOM,
        &hp,
        display_font(kind.size),
        number_color,
        0.0,
    );
    paint::shadowed(
        painter,
        pos2(number.right() + READOUT_MAX_GAP, middle + kind.max_drop),
        Align2::LEFT_BOTTOM,
        &format!("/ {}", human.max_hp.round()),
        display_font(READOUT_MAX_SIZE),
        hex(0xb9cadd),
        READOUT_MAX_SPACING,
    );
}

/// The segmented hull bar: the fill drops at once; a ghost trails it to show the chunk.
fn hull_bar(painter: &Painter, bar: Rect, state: &UiState, human: &HudHuman) {
    let ramp = hex(human.hull_ramp);
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

/// An ∞'s width, height and stroke as shares of its type's size, its path's inside
/// the stroke; and how far under the digits' middle it sits, on the type's maths axis.
const INFINITY_WIDTH: f32 = 0.62;
const INFINITY_HEIGHT: f32 = 0.37;
const INFINITY_STROKE: f32 = 0.05;
const INFINITY_DROP: f32 = 1.0;

/// A round's count in type of `size` at `pos`: its number, or for an unlimited round a
/// thin ∞, as the display face (which has none) falls back to a light text face's.
fn count_mark(
    painter: &Painter,
    pos: Pos2,
    align: Align2,
    count: Option<f64>,
    size: f32,
    color: Color32,
) {
    let Some(count) = count else {
        const STEPS: usize = 48;
        let half = vec2(INFINITY_WIDTH, INFINITY_HEIGHT) * size / 2.0;
        let outer = (INFINITY_WIDTH + INFINITY_STROKE) * size;
        let center = align.anchor_size(pos, vec2(outer, 0.0)).center() + vec2(0.0, INFINITY_DROP);
        let points = (0..STEPS)
            .map(|step| {
                let turn = step as f32 / STEPS as f32 * std::f32::consts::TAU;
                center + vec2(half.x * turn.cos(), half.y * (2.0 * turn).sin())
            })
            .collect();
        painter.add(Shape::closed_line(
            points,
            Stroke::new(INFINITY_STROKE * size, Color32::WHITE),
        ));
        return;
    };
    paint::shadowed(
        painter,
        pos,
        align,
        &format!("{count}"),
        display_font(size),
        color,
        0.0,
    );
}

/// Ghost sockets side by side share the rack's one channel, split by a hairline that
/// curves round the key's rounded edge.
fn ghost_seam(painter: &Painter, key: Rect) {
    let seam = painter.with_clip_rect(Rect::from_min_max(
        pos2(key.left() - 1.5, key.top()),
        pos2(key.left() + 4.0, key.bottom()),
    ));
    seam.rect_stroke(
        Rect::from_min_max(
            pos2(key.left() - 1.0, key.top() + 1.5),
            pos2(key.right() - 4.0, key.bottom() - 1.5),
        ),
        7,
        Stroke::new(1.0, hexa(0xffffff, 0.07)),
        StrokeKind::Inside,
    );
}

/// The rack: every round keeps its slot and key; the loaded round's slot is wide and
/// names it, a round not carried is a dim ghost in the channel. A touch screen's keys
/// (`touch`) share out the rack's width and name no keyboard keys.
#[allow(clippy::too_many_arguments)]
fn ammo_rack(
    ui: &mut egui::Ui,
    painter: &Painter,
    art: &Art,
    strip: Rect,
    human: &HudHuman,
    playing: bool,
    touch: Option<&TouchRack>,
    actions: &mut Vec<Action>,
) {
    painter.rect_filled(strip, 8, hexa(0x020812, 0.35));
    let touch_widths = touch.map(|rack| rack.key_widths(strip.width()));
    let gap = touch.map_or(SLOT_GAP, |rack| rack.gap);
    let mut x = strip.left();
    let mut after_ghost = false;
    for (index, option) in AMMO_OPTIONS.iter().enumerate() {
        let (weapon, label, name) = (option.weapon.as_str(), option.label(), option.name);
        let slot = human.ammo.iter().find(|slot| slot.weapon == option.weapon);
        let selected = slot.is_some_and(|slot| slot.selected);
        let available = slot.is_some_and(|slot| slot.available);
        let count = slot.and_then(|slot| slot.count);
        let width = touch_widths.map_or_else(|| slot_width(selected), |widths| widths[index]);
        let rect = Rect::from_min_size(pos2(x, strip.top()), vec2(width, strip.height()));
        x += width + gap;
        let color = hex(option.color());
        let count_text = match count {
            None => "unlimited".to_owned(),
            Some(count) => format!("{count} remaining"),
        };
        let response = ui
            .interact(rect, Id::unique(("ammo slot", index)), Sense::click())
            .on_hover_text(format!("{}: {name} · {count_text}", index + 1));
        let usable = playing && human.alive && available;
        paint::labelled_choice(&response, usable, selected, name);
        if !available {
            if touch.is_some() && after_ghost {
                ghost_seam(painter, rect);
            }
            after_ghost = true;
            // A ghost of what a crate would load.
            let ghost = if touch.is_some() {
                TOUCH_KEY_ICON
            } else {
                20.0
            };
            art.icon(
                painter,
                weapon,
                Rect::from_center_size(rect.center(), Vec2::splat(ghost)),
                fade(hex(0x8195ad), 0.38),
            );
            continue;
        }
        after_ghost = false;
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
        if let Some(rack) = touch {
            touch_key_face(painter, art, rect, rack, option, selected, count, color);
        } else if selected {
            art.icon(
                painter,
                weapon,
                Rect::from_min_size(rect.min + vec2(9.0, 14.0), Vec2::splat(24.0)),
                color,
            );
            let count_pos = pos2(rect.right() - 9.0, rect.top() + 26.0);
            count_mark(
                painter,
                count_pos,
                Align2::RIGHT_CENTER,
                count,
                20.0,
                Color32::WHITE,
            );
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
            count_mark(
                painter,
                count_pos,
                Align2::CENTER_CENTER,
                count,
                14.0,
                theme::TEXT,
            );
        }
        if usable && response.clicked() {
            actions.push(Action::SelectAmmo(index as u8 + 1));
        }
    }
}

/// A touch key's face (`.touch-enabled .ammo-slot`): the round's icon in the middle of
/// its upper part; the loaded round names itself under it, its count in the key's
/// corner, and every other round shows its count under its icon.
#[allow(clippy::too_many_arguments)]
fn touch_key_face(
    painter: &Painter,
    art: &Art,
    rect: Rect,
    rack: &TouchRack,
    option: &AmmoOption,
    selected: bool,
    count: Option<f64>,
    color: Color32,
) {
    let top = rect.top() + TOUCH_KEY_TOP;
    let bottom = rect.bottom() - TOUCH_KEY_BOTTOM;
    // The line under the icon: the loaded round's name, or another's count.
    let foot = if selected {
        rack.name_size * 1.15
    } else {
        TOUCH_COUNT_SIZE
    };
    art.icon(
        painter,
        option.weapon.as_str(),
        Rect::from_center_size(
            pos2(rect.center().x, (top + bottom - foot) / 2.0),
            Vec2::splat(TOUCH_KEY_ICON),
        ),
        color,
    );
    if selected {
        count_mark(
            painter,
            pos2(
                rect.right() - TOUCH_COUNT_INSET,
                rect.top() + TOUCH_COUNT_MIDDLE,
            ),
            Align2::RIGHT_CENTER,
            count,
            TOUCH_LOADED_COUNT_SIZE,
            Color32::WHITE,
        );
        paint::text_fit(
            painter,
            pos2(rect.center().x, bottom),
            Align2::CENTER_BOTTOM,
            option.label(),
            bold_font(rack.name_size),
            color,
            rack.name_spacing,
            rect.width() - rack.key_frame,
        );
    } else {
        count_mark(
            painter,
            pos2(rect.center().x, bottom - TOUCH_COUNT_SIZE / 2.0),
            Align2::CENTER_CENTER,
            count,
            TOUCH_COUNT_SIZE,
            theme::TEXT,
        );
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

/// The ammunition notice over the panel whose top is `plate_top`, from `left` and at
/// most the `room` of its column wide.
fn ammo_notice(painter: &Painter, left: f32, plate_top: f32, room: f32, notice: &str) {
    let font = bold_font(12.0);
    let max = room.min(600.0) - 24.0;
    let galley = paint::galley(painter, notice, font, GOLD_HOT, 0.0, max);
    let size = galley.size() + vec2(23.0, 14.0);
    let rect = Rect::from_min_size(pos2(left, plate_top - 42.0 - size.y), size);
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
/// with the key back to the overhead view at its foot. A touch screen has the eye button
/// instead of the key, and its dial sits in the segment's middle.
fn compass_dial(painter: &Painter, rect: Rect, hull_angle: f64) {
    let middle = if crate::touch::screen() {
        rect.center().y
    } else {
        rect.top() + 8.0 + 20.0
    };
    let center = pos2(rect.center().x - 2.0, middle);
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
    // The key that leaves first person; a touch screen has the eye button instead.
    if !crate::touch::screen() {
        paint::kbd(painter, pos2(center.x, rect.bottom() - 13.0), "V", 12.0);
    }
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
                |context| {
                    score_bar(&backdrop(context), screen, &hud, room);
                },
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

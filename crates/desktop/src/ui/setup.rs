//! Battle Setup (`src/game/battle-setup.html`, `start-menu.ts`, `game-options.ts`,
//! `map-picker.ts`): a pane of glass over a dusk battlefield with the logo on top, the
//! tank cards, the map, the battle format and difficulty, the status well and GO.

use egui::{
    Align2, Color32, CornerRadius, Id, LayerId, Mesh, Order, Painter, Rect, Sense, Shape, Stroke,
    StrokeKind, Vec2, pos2, vec2,
};

use sloppy_core::sim::{GameMode, Team, VehicleKind};
use sloppy_web::catalog::{
    BATTLE_FORMATS, BattleFormat, DIFFICULTY_OPTIONS, DifficultyOption, KEY_STRIP, MAP_CARDS,
    MapCard, TANK_CARDS, TOUCH_STRIP, TankCard,
};

use super::icons::Art;
use super::paint::{self, ButtonStyle, fade, hex, hexa, mix};
use super::theme::{GOLD, TEXT_DIM, bold_font, display_font};
use super::{Action, UiState, View};
use crate::preferences::Preferences;

/// How far an unavailable battle format fades (`.choice-card:has(input:disabled)`).
const UNAVAILABLE_OPACITY: f32 = 0.45;

/// The dusk battlefield behind Battle Setup: sky, a warm horizon, ridges, two tanks
/// on watch and a vignette.
fn backdrop(context: &egui::Context, art: &Art) {
    // The whole screen, under an iPad's home indicator too.
    let painter = paint::full_screen(
        context,
        LayerId::new(Order::Background, Id::unique("setup backdrop")),
    );
    let screen = context.viewport_rect();
    painter.add(paint::vgradient(
        screen,
        &[
            (0.0, hex(0x060c1a)),
            (0.42, hex(0x0f2347)),
            (0.6, hex(0x2a3d6b)),
            (0.7, hex(0x5a4a6a)),
            (0.78, hex(0x1a2338)),
            (1.0, hex(0x070f1d)),
        ],
    ));
    let at = |x: f32, y: f32| {
        pos2(
            screen.left() + screen.width() * x,
            screen.top() + screen.height() * y,
        )
    };
    painter.add(paint::radial(
        at(0.5, 0.0),
        vec2(screen.width() * 0.7, screen.height() * 0.6),
        &[(0.0, hexa(0x1d3f7a, 0.4)), (0.7, Color32::TRANSPARENT)],
    ));
    painter.add(paint::radial(
        at(0.5, 0.7),
        vec2(screen.width() * 0.9, screen.height() * 0.45),
        &[(0.0, hexa(0xff7a3d, 0.18)), (0.75, Color32::TRANSPARENT)],
    ));
    painter.add(paint::radial(
        at(0.5, 0.66),
        vec2(screen.width() * 0.55, screen.height() * 0.3),
        &[(0.0, hexa(0xffb35c, 0.33)), (0.7, Color32::TRANSPARENT)],
    ));
    // Three ridges, far to near, as height fields.
    let ridge = |base: f32, waves: [(f32, f32, f32); 3], top: Color32, bottom: Color32| {
        let steps = 96;
        let mut mesh = Mesh::default();
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let y = base
                + waves
                    .iter()
                    .map(|(amplitude, frequency, phase)| amplitude * (t * frequency + phase).sin())
                    .sum::<f32>();
            let x = screen.left() + screen.width() * t;
            mesh.colored_vertex(pos2(x, screen.top() + screen.height() * y), top);
            mesh.colored_vertex(pos2(x, screen.bottom()), bottom);
            if step > 0 {
                let index = (step as u32 - 1) * 2;
                mesh.add_triangle(index, index + 1, index + 2);
                mesh.add_triangle(index + 1, index + 3, index + 2);
            }
        }
        Shape::mesh(mesh)
    };
    painter.add(ridge(
        0.7,
        [(0.03, 7.0, 0.4), (0.015, 17.0, 1.3), (0.006, 41.0, 0.2)],
        hex(0x3a4f86),
        hex(0x22325a),
    ));
    painter.add(ridge(
        0.77,
        [(0.025, 5.0, 2.1), (0.012, 13.0, 0.7), (0.005, 37.0, 1.1)],
        hex(0x1b2a4c),
        hex(0x121d36),
    ));
    painter.add(ridge(
        0.85,
        [(0.02, 4.0, 4.0), (0.01, 11.0, 2.5), (0.004, 29.0, 0.4)],
        hex(0x0d1730),
        hex(0x070d19),
    ));
    let tank_height = (screen.height() * 0.09).clamp(36.0, 90.0);
    let tank_size = vec2(tank_height * art.aspect("backdrop-tank"), tank_height);
    let left = Rect::from_min_size(at(0.04, 0.83) - vec2(0.0, tank_height * 0.6), tank_size);
    art.icon(&painter, "backdrop-tank", left, hex(0x070d19));
    let right = Rect::from_min_size(
        at(0.96, 0.82) - vec2(tank_size.x * 1.1, tank_height * 0.7),
        tank_size * 1.1,
    );
    art.icon_flipped(&painter, "backdrop-tank", right, hex(0x070d19));
    paint::vignette(
        &painter,
        screen,
        vec2(0.8, 0.8),
        &[(0.55, Color32::TRANSPARENT), (1.0, hexa(0x02050b, 0.8))],
        1.0,
    );
}

/// The narrowest tank card with room for its hull and speed line.
const SPEC_LINE_WIDTH: f32 = 200.0;

/// A tank card: a little showroom with a spotlight on the tank, its name and class,
/// and its hull and speed as bars. The chosen tank glows gold.
#[allow(clippy::too_many_arguments)]
pub fn vehicle_card(
    painter: &Painter,
    art: &Art,
    rect: Rect,
    tank: &TankCard,
    team: Team,
    selected: bool,
    hovered: bool,
    compact: bool,
) {
    let rect = if hovered && !selected {
        rect.translate(vec2(0.0, -2.0))
    } else {
        rect
    };
    let radius = CornerRadius::same(10);
    if selected {
        painter.rect_filled(rect.expand(4.0), 14, hexa(0xffcf38, 0.12));
    }
    let (top, bottom, spot) = if selected {
        (hex(0x1f4a80), hex(0x0e2546), hexa(0xffd36a, 0.23))
    } else if hovered {
        (hex(0x183a6a), hex(0x0c1d3a), hexa(0x4f93db, 0.33))
    } else {
        (hex(0x14305a), hex(0x0a1830), hexa(0x3d7cc2, 0.27))
    };
    painter.rect_filled(rect, radius, bottom);
    painter.add(paint::vgradient(
        rect.shrink2(vec2(0.0, 10.0)),
        &[(0.0, top), (1.0, bottom)],
    ));
    painter.rect_filled(
        Rect::from_min_size(rect.min, vec2(rect.width(), 10.0)),
        CornerRadius {
            nw: 10,
            ne: 10,
            sw: 0,
            se: 0,
        },
        top,
    );
    painter.add(paint::radial(
        pos2(rect.center().x, rect.top() + rect.height() * 0.42),
        vec2(rect.width() * 0.6, rect.height() * 0.55),
        &[(0.0, spot), (0.7, Color32::TRANSPARENT)],
    ));
    // A card too narrow for its labels (a phone's row of three) shows the name and the
    // tank alone.
    let narrow = rect.width() < SPEC_LINE_WIDTH;
    let padding = if compact || narrow { 10.0 } else { 14.0 };
    let name_size = if compact || narrow { 20.0 } else { 24.0 };
    let name = paint::shadowed(
        painter,
        rect.min + vec2(padding, 10.0),
        Align2::LEFT_TOP,
        tank.name(),
        display_font(name_size),
        Color32::WHITE,
        0.5,
    );
    if compact {
        let next = Rect::from_min_size(
            pos2(rect.right() - 61.0, rect.top() + 10.0),
            vec2(51.0, 19.0),
        );
        // Where the name leaves no room, the gold ring alone says which comes next.
        if selected && name.right() + 6.0 <= next.left() {
            painter.rect_filled(next, 4, GOLD);
            paint::text(
                painter,
                next.center(),
                Align2::CENTER_CENTER,
                "NEXT",
                bold_font(12.0),
                hex(0x10213a),
                1.4,
            );
        }
    } else if !narrow {
        // The class tag, led by a dot in the class's colour.
        let class = tank.class.to_uppercase();
        let class_font = bold_font(11.0);
        let class_width = paint::text_width(painter, &class, class_font.clone(), 0.4) + 26.0;
        let tag = Rect::from_min_size(
            pos2(rect.right() - padding - class_width, rect.top() + 15.0),
            vec2(class_width, 19.0),
        );
        // A wide display face leaves the tag no room beside the name.
        let fits = name.right() + 8.0 <= tag.left();
        let (tag_fill, tag_ink, dot) = if selected {
            (GOLD, hex(0x1a2233), hex(0x1a2233))
        } else {
            (hexa(0xffffff, 0.07), hex(0xcfe3f5), hex(tank.color))
        };
        if fits {
            painter.rect_filled(tag, 5, tag_fill);
            painter.circle_filled(pos2(tag.left() + 10.0, tag.center().y), 3.0, dot);
            paint::text(
                painter,
                pos2(tag.left() + 18.0, tag.center().y),
                Align2::LEFT_CENTER,
                &class,
                class_font,
                tag_ink,
                0.4,
            );
        }
    }
    // The tank, tucked under the name line, sized by its class.
    let spec_height = 26.0;
    let stage = if compact || narrow {
        Rect::from_min_max(
            pos2(rect.left() + 6.0, rect.top() + 31.0),
            pos2(
                rect.right() - 6.0,
                rect.bottom() - if narrow { 6.0 } else { 40.0 },
            ),
        )
    } else {
        // The desktop garage keeps its picture above the specification line.
        Rect::from_center_size(
            pos2(rect.center().x, rect.top() + 73.0),
            vec2(
                rect.width() * 0.85,
                if rect.width() >= 270.0 { 112.0 } else { 96.0 },
            ),
        )
    };
    let scale = if !compact && tank.kind == VehicleKind::Heavy {
        1.0
    } else {
        tank.scale
    } * if hovered { 1.05 } else { 1.0 };
    painter.add(paint::radial(
        pos2(stage.center().x, stage.bottom() - stage.height() * 0.12),
        vec2(stage.height() * 1.2 * scale, stage.height() * 0.16),
        &[(0.0, hexa(0x020a14, 0.55)), (1.0, Color32::TRANSPARENT)],
    ));
    art.tank(
        &painter.with_clip_rect(rect.shrink(1.0)),
        tank.kind,
        team,
        stage,
        scale * 1.05,
        false,
    );
    // Hull and speed as bars that fill by each tank's share of the best.
    if !narrow {
        let spec_top = rect.bottom() - spec_height - 11.0;
        let half = (rect.width() - 2.0 * padding - 16.0) / 2.0;
        for (index, (label, share)) in [
            (
                format!("{} HIT POINTS", tank.hit_points()),
                tank.hit_points_share(),
            ),
            (format!("{} KM/H", tank.km_per_hour()), tank.speed_share()),
        ]
        .into_iter()
        .enumerate()
        {
            let x = rect.left() + padding + index as f32 * (half + 16.0);
            paint::text_fit(
                painter,
                pos2(x + if index == 1 { half } else { 0.0 }, spec_top + 6.0),
                if index == 1 {
                    Align2::RIGHT_TOP
                } else {
                    Align2::LEFT_TOP
                },
                &label,
                bold_font(11.0),
                hex(0xdbe9f6),
                0.6,
                half + if compact && index == 0 { 16.0 } else { 0.0 },
            );
            let thickness = if compact { 6.0 } else { 4.0 };
            let track = Rect::from_min_size(pos2(x, spec_top + 24.0), vec2(half, thickness));
            painter.rect_filled(track, 2, hexa(0xffffff, 0.1));
            let fill = Rect::from_min_size(
                track.min
                    + vec2(
                        if index == 1 {
                            half * (1.0 - share)
                        } else {
                            0.0
                        },
                        0.0,
                    ),
                vec2(half * share, thickness),
            );
            let (left, right) = if index == 0 {
                (hex(0x6fc6ff), hex(0xb8e4ff))
            } else {
                (super::theme::GOLD_HOT, GOLD)
            };
            painter.add(paint::gradient_polygon(
                &[
                    fill.left_top(),
                    fill.right_top(),
                    fill.right_bottom(),
                    fill.left_bottom(),
                ],
                left,
                right,
            ));
        }
    }
    if selected {
        painter.rect_stroke(rect, radius, Stroke::new(2.0, GOLD), StrokeKind::Inside);
    } else {
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(1.0, hexa(0x9fd0ff, 0.13)),
            StrokeKind::Inside,
        );
    }
}

/// A selectable card's frame (`.choice-card`): lit glass with a gold glow when chosen,
/// a darker well otherwise.
pub(super) fn choice_frame(painter: &Painter, rect: Rect, selected: bool, hovered: bool) {
    if selected {
        painter.rect_filled(rect.expand(4.0), 14, hexa(0xffcf38, 0.08));
        let base = hex(0x22477a);
        painter.add(paint::rounded_vgradient(
            rect,
            [10.0; 4],
            &[
                (0.0, mix(base, Color32::WHITE, 0.07)),
                (0.6, base),
                (1.0, base),
            ],
        ));
    } else {
        painter.rect_filled(
            rect,
            10,
            if hovered {
                hex(0x1b3966)
            } else {
                hex(0x10264a)
            },
        );
    }
}

pub(super) fn choice_ring(painter: &Painter, rect: Rect, selected: bool) {
    let radius = CornerRadius::same(10);
    if selected {
        painter.rect_stroke(rect, radius, Stroke::new(2.0, GOLD), StrokeKind::Inside);
    } else {
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(1.0, hexa(0x9fd0ff, 0.08)),
            StrokeKind::Inside,
        );
    }
}

/// A map card: its name and line over its landscape, washed in the map's tint.
fn map_card(
    painter: &Painter,
    art: &Art,
    rect: Rect,
    map: &MapCard,
    selected: bool,
    hovered: bool,
) {
    let extra = map.extra();
    choice_frame(painter, rect, selected, hovered);
    let tint = hex(map.tint);
    let clip = painter.with_clip_rect(rect.shrink(1.0));
    clip.add(paint::vgradient(
        rect,
        &[(0.3, Color32::TRANSPARENT), (1.0, fade(tint, 0.6))],
    ));
    if extra {
        let tile = Rect::from_center_size(
            pos2(rect.right() - rect.height() * 0.6, rect.center().y),
            Vec2::splat(rect.height() * 0.7),
        );
        clip.rect_filled(tile, 8, fade(tint, 0.6));
        art.icon(
            &clip,
            map.icon,
            tile.shrink(tile.width() * 0.2),
            Color32::WHITE,
        );
    } else {
        let art_name = format!("art-{}", map.id.as_str());
        let height = rect.height() * 0.8;
        let size = vec2(height * art.aspect(&art_name), height);
        let place = Rect::from_min_size(
            pos2(rect.right() - size.x + 6.0, rect.bottom() - size.y),
            size,
        );
        art.icon(&clip, &art_name, place, Color32::WHITE);
        // The art fades in from behind the text.
        let base = if selected {
            hex(0x163a68)
        } else if hovered {
            hex(0x122d55)
        } else {
            hex(0x0e2242)
        };
        clip.add(paint::hgradient(
            Rect::from_min_max(pos2(rect.right() - 150.0, rect.top()), rect.max),
            &[(0.0, base), (0.6, fade(base, 0.0))],
        ));
    }
    let text_width = rect.width() - 24.0;
    let name = paint::shadowed(
        painter,
        rect.min + vec2(12.0, 17.0),
        Align2::LEFT_TOP,
        map.name(),
        bold_font(15.0),
        Color32::WHITE,
        0.0,
    );
    if extra {
        let badge = Rect::from_min_size(name.right_top() + vec2(8.0, 2.0), vec2(42.0, 16.0));
        painter.rect_filled(badge, 4, hex(0x6a5ea8));
        paint::text(
            painter,
            badge.center(),
            Align2::CENTER_CENTER,
            "EXTRA",
            bold_font(10.0),
            Color32::WHITE,
            0.8,
        );
    }
    // A phone's row of three has room for the name alone.
    if rect.width() >= SPEC_LINE_WIDTH {
        paint::text_fit(
            painter,
            pos2(rect.left() + 12.0, rect.top() + 38.0),
            Align2::LEFT_TOP,
            map.short_description.unwrap_or(map.description),
            egui::FontId::proportional(12.0),
            hex(0xc9def0),
            0.0,
            text_width,
        );
    }
    choice_ring(painter, rect, selected);
}

/// A battle format card (`.battle-choice`): a tall poster of tanks over its name and
/// `detail`, the player's side in their team's colour.
fn battle_card(
    painter: &Painter,
    art: &Art,
    rect: Rect,
    battle: &BattleFormat,
    detail: &str,
    selected: bool,
    hovered: bool,
) {
    let (team, name) = (battle.mode == GameMode::Team, battle.name);
    choice_frame(painter, rect, selected, hovered);
    let clip = painter.with_clip_rect(rect.shrink(1.0));
    let at =
        |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(rect.min + vec2(x, y), vec2(w, h));
    let width = rect.width();
    let height = rect.height();
    // The authored scene washes distinguish a two-sided fight from a lone tank.
    let glows: &[(f32, f32, f32, f32, u32, f32)] = if team {
        &[
            (1.0, 0.1, 0.6, 0.7, 0xff4d5e, 0.30),
            (0.0, 0.1, 0.6, 0.7, 0x3ea6ff, 0.28),
        ]
    } else {
        &[
            (0.5, 0.62, 0.46, 0.4, 0xffd686, 0.32),
            (0.6, 0.35, 0.7, 0.7, 0xd0283f, 0.36),
            (0.2, 0.0, 0.8, 0.6, 0x3ea6ff, 0.18),
        ]
    };
    for &(x, y, radius_x, radius_y, color, opacity) in glows {
        clip.add(paint::radial(
            rect.min + vec2(width * x, height * y),
            vec2(width * radius_x, height * radius_y),
            &[(0.0, hexa(color, opacity)), (0.7, Color32::TRANSPARENT)],
        ));
    }
    clip.add(paint::vgradient(
        rect,
        &[(0.55, Color32::TRANSPARENT), (1.0, hexa(0x050c1a, 0.6))],
    ));
    if team {
        // Authored poster colours stay fixed: blue below, red above.
        let art_height = height - 68.0;
        let tank_width = width * 0.66;
        let tank_height = tank_width * 300.0 / 560.0;
        art.tank(
            &clip,
            VehicleKind::Heavy,
            Team::Red,
            at(
                width * 0.98 - tank_width,
                6.0 + art_height * 0.06,
                tank_width,
                tank_height,
            ),
            1.0,
            false,
        );
        art.tank(
            &clip,
            VehicleKind::Balanced,
            Team::Blue,
            at(
                width * 0.02,
                6.0 + art_height * 0.96 - tank_height,
                tank_width,
                tank_height,
            ),
            1.0,
            true,
        );
        let versus = rect.min + vec2(width / 2.0, 6.0 + art_height / 2.0);
        painter.circle_filled(versus, 22.0, hex(0x10264a));
        painter.circle_stroke(versus, 21.0, Stroke::new(2.0, GOLD));
        paint::text(
            painter,
            versus,
            Align2::CENTER_CENTER,
            "VS",
            display_font(20.0),
            hex(0xffe58a),
            0.5,
        );
    } else {
        let art_height = 152.0_f32.min(height - 68.0);
        let art_top = height - 62.0 - ((height - 220.0) / 2.0).max(0.0) - art_height;
        for (kind, x_share, y_share, size_share) in [
            (VehicleKind::Scout, 0.07, 0.08, 0.36),
            (VehicleKind::Balanced, 0.37, 0.0, 0.30),
            (VehicleKind::Heavy, 0.0, 0.10, 0.36),
        ] {
            let tank_height = art_height * size_share;
            let tank_width = tank_height * 560.0 / 300.0;
            let left = if kind == VehicleKind::Heavy {
                width * 0.93 - tank_width
            } else {
                width * x_share
            };
            art.tank(
                &clip,
                kind,
                Team::Blue,
                at(
                    left,
                    art_top + art_height * y_share,
                    tank_width,
                    tank_height,
                ),
                1.0,
                false,
            );
        }
        let tank_height = art_height * 0.8;
        let tank_width = tank_height * 560.0 / 300.0;
        art.tank(
            &clip,
            VehicleKind::Balanced,
            Team::Red,
            at(
                (width - tank_width) / 2.0,
                art_top + art_height - tank_height,
                tank_width,
                tank_height,
            ),
            1.0,
            false,
        );
    }
    // A phone's half-width poster takes a smaller title.
    let title_size = if width < 240.0 { 18.0 } else { 24.0 };
    paint::shadowed(
        painter,
        rect.min + vec2(16.0, rect.height() - 33.0 - title_size),
        Align2::LEFT_TOP,
        &name.to_uppercase(),
        display_font(title_size),
        Color32::WHITE,
        0.5,
    );
    paint::text_fit(
        painter,
        rect.min + vec2(16.0, rect.height() - 28.0),
        Align2::LEFT_TOP,
        detail,
        egui::FontId::proportional(12.0),
        hex(0xc9def0),
        0.0,
        rect.width() - 32.0,
    );
    choice_ring(painter, rect, selected);
}

/// The narrowest difficulty level that says what it changes.
const DIFFICULTY_DETAIL_WIDTH: f32 = 200.0;

/// A difficulty level (`.difficulty-setting .segment`): its name, what it changes and
/// its strength bars.
fn difficulty_segment(
    painter: &Painter,
    rect: Rect,
    level: usize,
    entry: &DifficultyOption,
    selected: bool,
    hovered: bool,
) {
    choice_frame(painter, rect, selected, hovered);
    if rect.width() < DIFFICULTY_DETAIL_WIDTH {
        // A phone's row of three: the name over its bars.
        paint::shadowed(
            painter,
            rect.center_top() + vec2(0.0, 9.0),
            Align2::CENTER_TOP,
            &entry.name.to_uppercase(),
            display_font(19.0),
            Color32::WHITE,
            0.5,
        );
        let bars = Rect::from_center_size(
            pos2(rect.center().x, rect.bottom() - 17.0),
            vec2(26.0, 18.0),
        );
        difficulty_bars(painter, bars, level, selected);
        choice_ring(painter, rect, selected);
        return;
    }
    let galley = paint::wrapped(
        painter,
        entry.detail,
        egui::FontId::proportional(12.5),
        hex(0xd0e4f3),
        rect.width() - 94.0,
    );
    let content_top = ((rect.height() - 30.0 - galley.size().y) / 2.0).max(5.0);
    paint::shadowed(
        painter,
        rect.min + vec2(18.0, content_top),
        Align2::LEFT_TOP,
        &entry.name.to_uppercase(),
        display_font(24.0),
        Color32::WHITE,
        0.5,
    );
    painter.galley(
        rect.min + vec2(18.0, content_top + 30.0),
        galley,
        Color32::WHITE,
    );
    let bars = Rect::from_min_size(
        pos2(rect.right() - 58.4, rect.center().y - 15.6),
        vec2(36.4, 31.2),
    );
    difficulty_bars(painter, bars, level, selected);
    choice_ring(painter, rect, selected);
}

/// Desktop sheets at the web layout's normal and wide breakpoints. Shorter windows
/// scale the sheet down as `fitPanels` does, without enlarging its typography.
const DESIGN: Vec2 = vec2(1280.0, 800.0);
const MIN_SCALE: f32 = 0.6;
const WIDE_DESIGN: Vec2 = vec2(1400.0, 823.0);
const WIDE_GARAGE_GROWTH: f32 = 28.0;

/// Where the parts sit on the design canvas (`.menu.start` at 1280 × 800).
const FRAME: Rect = Rect::from_min_max(pos2(32.5, 25.25), pos2(1247.5, 774.75));
const TANK_TAB: Rect = Rect::from_min_max(pos2(60.0, 135.0), pos2(304.0, 195.0));
const TANK_WELL: Rect = Rect::from_min_max(pos2(60.0, 194.0), pos2(331.0, 708.0));
const PANELS: Rect = Rect::from_min_max(pos2(350.0, 194.0), pos2(1220.0, 708.0));
const CARD_GAP: f32 = 7.0;

/// The tray's glass: the tank well and the panels share it, joined to their tabs.
fn tray(painter: &Painter, rect: Rect, open_left: bool, open_right: bool) {
    let radius = CornerRadius {
        nw: if open_left { 0 } else { 16 },
        ne: if open_right { 0 } else { 16 },
        sw: 16,
        se: 16,
    };
    painter.rect_filled(rect.translate(vec2(0.0, 10.0)), radius, hexa(0, 0.25));
    painter.add(paint::rounded_vgradient(
        rect,
        [radius.nw, radius.ne, radius.sw, radius.se].map(f32::from),
        &[
            (0.0, hex(0x15305a)),
            (0.45, hex(0x11294d)),
            (1.0, hex(0x0d2242)),
        ],
    ));
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(0.5, hexa(0x8fc4f0, 0.29)),
        StrokeKind::Inside,
    );
}

/// A folder tab over the tray: lit with a gold edge when open (`.play-tab`), or the tank
/// column's with a light one.
fn folder_tab(painter: &Painter, rect: Rect, open: bool, edge: Color32) {
    let radius = CornerRadius {
        nw: 14,
        ne: 14,
        sw: 0,
        se: 0,
    };
    if open {
        painter.rect_filled(rect, radius, hex(0x15305a));
        painter.rect_filled(
            Rect::from_min_size(rect.min + vec2(6.0, 0.0), vec2(rect.width() - 12.0, 3.0)),
            1,
            edge,
        );
    } else {
        painter.rect_filled(rect, radius, hex(0x0d1f3c));
        painter.add(paint::vgradient(
            rect.shrink2(vec2(0.0, 14.0)),
            &[(0.0, hex(0x0f2342)), (1.0, hex(0x0a1830))],
        ));
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(0.5, hexa(0x9fd0ff, 0.12)),
            StrokeKind::Inside,
        );
    }
}

/// A play tab: its badge, its name and what it is (`.play-tab`); `compact` (a phone's
/// half width) leaves out what it is.
fn play_tab(
    ui: &mut egui::Ui,
    painter: &Painter,
    rect: Rect,
    (multiplayer, name, line): (bool, &str, &str),
    open: bool,
    compact: bool,
) -> bool {
    let response = ui
        .interact(rect, Id::unique(("play tab", multiplayer)), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    folder_tab(painter, rect, open, GOLD);
    let badge_size = if compact { 28.0 } else { 34.0 };
    let badge = Rect::from_min_size(
        rect.min
            + vec2(
                if compact { 10.0 } else { 16.0 },
                (rect.height() - badge_size) / 2.0,
            ),
        Vec2::splat(badge_size),
    );
    let ink = if open { hex(0x1a2233) } else { hex(0xc9dbec) };
    if open {
        painter.rect_filled(badge, 9, GOLD);
        painter.add(paint::vgradient(
            badge.shrink2(vec2(0.0, 9.0)),
            &[(0.0, hex(0xffe58a)), (1.0, GOLD)],
        ));
    } else {
        painter.rect_filled(badge, 9, hexa(0xffffff, 0.06));
    }
    // One driver, or two side by side.
    let heads: &[f32] = if multiplayer { &[-5.0, 5.0] } else { &[0.0] };
    for dx in heads {
        let head = badge.center() + vec2(*dx, -4.0);
        painter.circle_stroke(head, 3.5, Stroke::new(1.8, ink));
        painter.line_segment(
            [head + vec2(-6.0, 11.0), head + vec2(6.0, 11.0)],
            Stroke::new(1.8, ink),
        );
        painter.line_segment(
            [head + vec2(-6.0, 11.0), head + vec2(-4.0, 6.5)],
            Stroke::new(1.8, ink),
        );
        painter.line_segment(
            [head + vec2(6.0, 11.0), head + vec2(4.0, 6.5)],
            Stroke::new(1.8, ink),
        );
    }
    let title = paint::shadowed(
        painter,
        pos2(
            badge.right() + if compact { 8.0 } else { 12.0 },
            rect.center().y + 1.0,
        ),
        Align2::LEFT_CENTER,
        name,
        display_font(if compact { 17.0 } else { 23.0 }),
        if open { Color32::WHITE } else { hex(0xc9dbec) },
        if compact { 0.3 } else { 0.6 },
    );
    if compact {
        return response.clicked();
    }
    paint::text(
        painter,
        pos2(title.right() + 12.0, rect.center().y + 3.0),
        Align2::LEFT_CENTER,
        line,
        egui::FontId::proportional(13.0),
        hex(0xd3e7f5),
        0.0,
    );
    response.clicked()
}

/// Strength bars for a difficulty: one, two or three lit in its colour.
fn difficulty_bars(painter: &Painter, rect: Rect, level: usize, selected: bool) {
    let colors = [hex(0x5fe39a), GOLD, hex(0xff5a4f)];
    let width = rect.width() / 4.0;
    for bar in 0..3 {
        let height = rect.height() * [10.0 / 24.0, 17.0 / 24.0, 1.0][bar];
        let left = rect.left() + bar as f32 * (rect.width() - width) / 2.0;
        let column = Rect::from_min_max(
            pos2(left, rect.bottom() - height),
            pos2(left + width, rect.bottom()),
        );
        let lit = bar <= level;
        let color = if lit {
            colors[level]
        } else {
            hexa(0x9fd0ff, if selected { 0.22 } else { 0.16 })
        };
        painter.rect_filled(column, 2, color);
    }
}

/// While the arena prepares, its progress on the BATTLE heading's line; gone once ready.
fn status_line(painter: &Painter, right: egui::Pos2, state: &UiState, time: f64) {
    if state.ready && !state.starting {
        return;
    }
    let track = Rect::from_min_size(right - vec2(72.0, -6.0), vec2(72.0, 3.0));
    painter.rect_filled(track, 2, hexa(0xffffff, 0.1));
    // The share loaded, or a sweep while there is none to show.
    let fill = match state.progress {
        Some(share) => Rect::from_min_size(track.min, vec2(track.width() * share, track.height())),
        None => {
            let sweep = (time * 0.6).fract() as f32;
            let width = track.width() * 0.35;
            let left = track.left() - width + (track.width() + width) * sweep;
            Rect::from_min_max(
                pos2(left.max(track.left()), track.top()),
                pos2((left + width).min(track.right()), track.bottom()),
            )
        }
    };
    painter.rect_filled(fill, 2, GOLD);
    let status = if state.starting {
        "Rolling out…"
    } else if state.status.is_empty() {
        "Loading game…"
    } else {
        &state.status
    };
    paint::text(
        painter,
        pos2(track.left() - 10.0, track.center().y),
        Align2::RIGHT_CENTER,
        status,
        bold_font(12.0),
        hex(0xdbe9f6),
        0.2,
    );
}

pub fn battle_setup(
    context: &egui::Context,
    state: &mut UiState,
    art: &Art,
    view: &View,
    actions: &mut Vec<Action>,
) {
    backdrop(context, art);
    let mut prefs = view.prefs.clone();
    let screen = context.content_rect();
    if screen.width() < DESIGN.x * MIN_SCALE || screen.height() < DESIGN.y * MIN_SCALE {
        compact_setup(context, state, art, view, &mut prefs, actions);
    } else {
        sheet(context, state, art, view, &mut prefs, actions);
    }
    if prefs != *view.prefs {
        actions.push(Action::SettingsSaved(Box::new(prefs)));
    }
}

/// Battle Setup's sheet, scaled to the window.
fn sheet(
    context: &egui::Context,
    state: &mut UiState,
    art: &Art,
    view: &View,
    prefs: &mut Preferences,
    actions: &mut Vec<Action>,
) {
    let screen = context.content_rect();
    let time = context.input(|input| input.time);
    let human_team = state.hud.as_ref().map_or(Team::Blue, |hud| hud.human_team);
    // What single player would play: a saved extra level only with the debug tools,
    // and as its own team battle.
    let options = prefs.game_options(human_team, view.debug);
    let wide = screen.width() >= 1500.0 && screen.height() >= 860.0;
    let design = if wide { WIDE_DESIGN } else { DESIGN };
    let extra_width = design.x - DESIGN.x;
    let extra_height = design.y - DESIGN.y;
    let garage_growth = if wide { WIDE_GARAGE_GROWTH } else { 0.0 };
    let frame = Rect::from_min_max(FRAME.min, FRAME.max + vec2(extra_width, extra_height));
    let tank_well = Rect::from_min_max(
        TANK_WELL.min,
        TANK_WELL.max + vec2(garage_growth, extra_height),
    );
    let panels = Rect::from_min_max(
        PANELS.min + vec2(garage_growth, 0.0),
        PANELS.max + vec2(extra_width, extra_height),
    );
    let tab_width = (panels.width() - 6.0) / 2.0;
    let scale = (screen.width() / design.x)
        .min(screen.height() / design.y)
        .clamp(MIN_SCALE, 1.0);
    let offset = screen.center() - design * scale / 2.0;
    let id = Id::unique("battle setup");
    context.set_transform_layer(
        LayerId::new(Order::Middle, id),
        egui::emath::TSTransform::new(offset.to_vec2(), scale),
    );
    egui::Area::new(id)
        .fixed_pos(pos2(0.0, 0.0))
        .order(Order::Middle)
        .constrain(false)
        .show(context, |ui| {
            let canvas = Rect::from_min_size(pos2(0.0, 0.0), design);
            ui.set_clip_rect(canvas);
            let _ = ui.allocate_rect(canvas, Sense::hover());
            let painter = ui.painter().clone();
            // The menu's own glass.
            painter.rect_filled(
                frame.translate(vec2(0.0, 30.0)).expand(10.0),
                26,
                hexa(0, 0.35),
            );
            painter.add(paint::rounded_vgradient(
                frame,
                [16.0; 4],
                &[
                    (0.0, hexa(0x12294c, 0.9)),
                    (0.6, hexa(0x091629, 0.94)),
                    (1.0, hexa(0x070f1f, 0.96)),
                ],
            ));
            painter.rect_stroke(
                frame,
                16,
                Stroke::new(0.5, hexa(0xa9d4ff, 0.18)),
                StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    frame.left_top() + vec2(16.0, 0.5),
                    frame.right_top() + vec2(-16.0, 0.5),
                ],
                Stroke::new(1.0, hexa(0xffffff, 0.11)),
            );
            // Where the page's sheet puts `.setup-brand` at 1280 × 800.
            art.logo(&painter, pos2(71.0, 53.0));
            corner_buttons(
                ui,
                art,
                Rect::from_min_size(pos2(1137.0 + extra_width, 64.0), vec2(82.0, 44.0)),
                actions,
            );

            // The tank column, for both ways to play.
            folder_tab(&painter, TANK_TAB, true, hexa(0x9fd0ff, 0.4));
            painter.rect_filled(
                Rect::from_min_size(TANK_TAB.min + vec2(16.0, 18.0), vec2(6.0, 24.0)),
                3,
                GOLD,
            );
            let title = paint::text(
                &painter,
                TANK_TAB.min + vec2(34.0, 31.0),
                Align2::LEFT_CENTER,
                "YOUR TANK",
                display_font(23.0),
                Color32::WHITE,
                1.0,
            );
            // Beside the title where the display face leaves room (Impact does).
            let note = egui::FontId::proportional(13.0);
            let note_left = title.right().max(TANK_TAB.left() + 144.0) + 10.0;
            if note_left + paint::text_width(&painter, "Both modes", note.clone(), 0.0)
                <= TANK_TAB.right() - 8.0
            {
                paint::text(
                    &painter,
                    pos2(note_left, TANK_TAB.top() + 31.0),
                    Align2::LEFT_CENTER,
                    "Both modes",
                    note,
                    hex(0xd3e7f5),
                    0.0,
                );
            }
            tray(&painter, tank_well, true, false);
            for (index, tank) in TANK_CARDS.iter().enumerate() {
                let rect = Rect::from_min_size(
                    pos2(72.0, 206.0 + index as f32 * (165.5 + extra_height / 3.0)),
                    vec2(246.0 + garage_growth, 158.0 + extra_height / 3.0),
                );
                let response = ui
                    .interact(rect, Id::unique(("setup tank", tank.kind)), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                vehicle_card(
                    &painter,
                    art,
                    rect,
                    tank,
                    human_team,
                    prefs.tank == tank.kind,
                    response.hovered(),
                    false,
                );
                if response.clicked() {
                    prefs.tank = tank.kind;
                }
            }

            // The play tabs and their panel.
            let multiplayer = state.rooms.multiplayer;
            for (index, tab) in [
                (false, "SINGLE PLAYER", "You versus the bots"),
                (true, "MULTIPLAYER", "Online rooms with friends"),
            ]
            .into_iter()
            .enumerate()
            {
                let open = multiplayer == tab.0;
                let left = panels.left() + index as f32 * (panels.width() - tab_width);
                let rect = if open {
                    Rect::from_min_size(pos2(left, 135.0), vec2(tab_width, 60.0))
                } else {
                    Rect::from_min_size(pos2(left, 141.0), vec2(tab_width, 52.0))
                };
                if play_tab(ui, &painter, rect, tab, open, false) {
                    state.rooms.multiplayer = tab.0;
                }
            }
            tray(&painter, panels, !multiplayer, multiplayer);
            let inner = panels.shrink(14.0);
            let right_width = 302.0 + (extra_width - garage_growth) * 1.22 / 3.22;
            let right_left = inner.right() - right_width;
            let left_width = right_left - 22.0 - inner.left();
            {
                // MAP over the panel's width; extra levels share the row on `--debug`. In
                // multiplayer it is the new room's, and an open room chosen shows its own.
                let heading = paint::section_label(&painter, pos2(inner.left(), 208.0), "MAP");
                let room_map = state.rooms.selected_map().filter(|_| multiplayer);
                if multiplayer {
                    paint::text(
                        &painter,
                        pos2(heading.right() + 10.0, heading.center().y + 1.0),
                        Align2::LEFT_CENTER,
                        if room_map.is_some() {
                            "The room's map"
                        } else {
                            "For your new room"
                        },
                        egui::FontId::proportional(13.0),
                        hex(0xd3e7f5),
                        0.0,
                    );
                }
                let shown_map = room_map.unwrap_or(options.map_mode);
                let maps: Vec<&MapCard> = MAP_CARDS
                    .iter()
                    .filter(|map| view.debug || !map.extra())
                    .collect();
                let map_well =
                    Rect::from_min_max(pos2(inner.left(), 235.0), pos2(inner.right(), 317.0));
                painter.rect_filled(map_well, 14, hexa(0x071328, 0.85));
                let row = map_well.shrink(5.0);
                let map_width =
                    (row.width() - CARD_GAP * (maps.len() - 1) as f32) / maps.len() as f32;
                for (index, map) in maps.into_iter().enumerate() {
                    let rect = Rect::from_min_size(
                        pos2(
                            row.left() + index as f32 * (map_width + CARD_GAP),
                            row.top(),
                        ),
                        vec2(map_width, row.height()),
                    );
                    let locked = room_map.is_some();
                    let id = Id::unique(("setup map", map.id));
                    let response = if locked {
                        ui.interact(rect, id, Sense::hover())
                    } else {
                        ui.interact(rect, id, Sense::click())
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                    };
                    map_card(
                        &painter,
                        art,
                        rect,
                        map,
                        shown_map == map.id,
                        !locked && response.hovered(),
                    );
                    if !locked && response.clicked() {
                        prefs.map = map.id;
                    }
                }
            }
            if multiplayer {
                super::rooms::multiplayer(
                    ui,
                    art,
                    &mut state.rooms,
                    view,
                    prefs,
                    Rect::from_min_max(
                        pos2(inner.left(), 330.0),
                        pos2(inner.right(), 693.0 + extra_height),
                    ),
                    actions,
                );
            } else {
                // BATTLE: two posters down the panel's height.
                paint::section_label(&painter, pos2(inner.left(), 330.0), "BATTLE");
                status_line(&painter, pos2(right_left - 28.0, 334.0), state, time);
                painter.rect_filled(
                    Rect::from_min_max(
                        pos2(inner.left(), 357.0),
                        pos2(right_left - 22.0, 693.0 + extra_height),
                    ),
                    14,
                    hexa(0x071328, 0.85),
                );
                for (index, battle) in BATTLE_FORMATS.iter().enumerate() {
                    let rect = Rect::from_min_size(
                        pos2(
                            inner.left() + 5.0 + index as f32 * ((left_width - 16.0) / 2.0 + 6.0),
                            362.0,
                        ),
                        vec2((left_width - 16.0) / 2.0, 326.0 + extra_height),
                    );
                    // An extra level plays its own team battle; Solo Assault waits for a
                    // standard map (`showBattleFormat`).
                    let unavailable = battle.unavailable_on(options.map_mode);
                    let id = Id::unique(("setup battle", battle.mode));
                    let response = match &unavailable {
                        Some(tip) => ui
                            .interact(rect, id, Sense::hover())
                            .on_hover_cursor(egui::CursorIcon::NotAllowed)
                            .on_hover_text(tip),
                        None => ui
                            .interact(rect, id, Sense::click())
                            .on_hover_cursor(egui::CursorIcon::PointingHand),
                    };
                    let mut card_painter = painter.clone();
                    if unavailable.is_some() {
                        card_painter.multiply_opacity(UNAVAILABLE_OPACITY);
                    }
                    battle_card(
                        &card_painter,
                        art,
                        rect,
                        battle,
                        &battle.detail_on(options.map_mode),
                        options.game_mode == battle.mode,
                        unavailable.is_none() && response.hovered(),
                    );
                    // The format kept for the standard maps changes only by choosing one
                    // there, never by the extra level's own team battle.
                    if response.clicked() && options.game_mode != battle.mode {
                        prefs.game_mode = battle.mode;
                    }
                }
                // DIFFICULTY, and GO under it.
                paint::section_label(&painter, pos2(right_left, 330.0), "DIFFICULTY");
                painter.rect_filled(
                    Rect::from_min_max(
                        pos2(right_left, 357.0),
                        pos2(inner.right(), 618.0 + extra_height),
                    ),
                    14,
                    hexa(0x071328, 0.85),
                );
                for (index, entry) in DIFFICULTY_OPTIONS.iter().enumerate() {
                    let rect = Rect::from_min_size(
                        pos2(
                            right_left + 5.0,
                            362.0 + index as f32 * (85.5 + extra_height / 3.0),
                        ),
                        vec2(right_width - 10.0, 79.0 + extra_height / 3.0),
                    );
                    let response = ui
                        .interact(
                            rect,
                            Id::unique(("setup difficulty", entry.difficulty)),
                            Sense::click(),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    difficulty_segment(
                        &painter,
                        rect,
                        index,
                        entry,
                        prefs.difficulty == entry.difficulty,
                        response.hovered(),
                    );
                    if response.clicked() {
                        prefs.difficulty = entry.difficulty;
                    }
                }
                let go = Rect::from_min_size(
                    pos2(right_left, 631.0 + extra_height),
                    vec2(right_width, 62.0),
                );
                let mut go_ui = ui.new_child(egui::UiBuilder::new().max_rect(go));
                let label = if state.starting { "LOADING…" } else { "GO!" };
                if paint::button(
                    &mut go_ui,
                    art,
                    label,
                    None,
                    ButtonStyle::Primary,
                    go.size(),
                    !state.starting,
                )
                .clicked()
                {
                    actions.push(Action::Start);
                }
            }
            painter.line_segment(
                [
                    pos2(tank_well.left(), 722.0 + extra_height),
                    pos2(panels.right(), 722.0 + extra_height),
                ],
                Stroke::new(1.0, hexa(0x9fd0ff, 0.12)),
            );
            key_strip(&painter, pos2(design.x / 2.0, 745.0 + extra_height));
        });
    // Build metadata belongs to the screen's corner, outside the setup sheet.
    let frame_bottom = offset.y + frame.bottom() * scale;
    if frame_bottom + 8.0 <= screen.bottom() - 20.0 {
        let painter = context.layer_painter(LayerId::new(
            Order::Foreground,
            Id::unique("setup build label"),
        ));
        paint::text(
            &painter,
            screen.right_bottom() - vec2(12.0, 9.0),
            Align2::RIGHT_BOTTOM,
            view.build,
            egui::FontId::proportional(10.0),
            fade(TEXT_DIM, 0.7),
            0.4,
        );
    }
}

/// The compact column's widest, its margin and the gap between its rows.
const COMPACT_WIDTH: f32 = 560.0;
const COMPACT_MARGIN: f32 = 12.0;
const ROW_GAP: f32 = 12.0;
/// Row heights in the compact column.
const COMPACT_TANK_HEIGHT: f32 = 112.0;
const COMPACT_MAP_HEIGHT: f32 = 62.0;
const COMPACT_BATTLE_HEIGHT: f32 = 150.0;
const COMPACT_DIFFICULTY_HEIGHT: f32 = 62.0;
const COMPACT_GO_HEIGHT: f32 = 56.0;
/// The Multiplayer tab's panel in the compact column.
const COMPACT_ROOMS_HEIGHT: f32 = 560.0;

/// Battle Setup as one column where the sheet does not fit at its smallest scale (a
/// phone either way up, a small window): the same choices in rows of cards, which
/// scroll when taller than the screen, without the key strip.
fn compact_setup(
    context: &egui::Context,
    state: &mut UiState,
    art: &Art,
    view: &View,
    prefs: &mut Preferences,
    actions: &mut Vec<Action>,
) {
    let screen = context.content_rect();
    let time = context.input(|input| input.time);
    let human_team = state.hud.as_ref().map_or(Team::Blue, |hud| hud.human_team);
    let options = prefs.game_options(human_team, view.debug);
    let multiplayer = state.rooms.multiplayer;
    let maps: Vec<&MapCard> = MAP_CARDS
        .iter()
        .filter(|map| view.debug || !map.extra())
        .collect();
    let map_rows = maps.len().div_ceil(3);
    let width = (screen.width() - 2.0 * COMPACT_MARGIN).min(COMPACT_WIDTH);
    let label = 30.0;
    let below_maps = if multiplayer {
        COMPACT_ROOMS_HEIGHT
    } else {
        2.0 * (label + ROW_GAP)
            + COMPACT_BATTLE_HEIGHT
            + COMPACT_DIFFICULTY_HEIGHT
            + COMPACT_GO_HEIGHT
    };
    let tray_height = 10.0
        + 2.0 * (label + ROW_GAP)
        + COMPACT_TANK_HEIGHT
        + map_rows as f32 * (COMPACT_MAP_HEIGHT + CARD_GAP)
        + below_maps
        + 10.0;
    let header = 82.0;
    let tabs = 50.0;
    let height = COMPACT_MARGIN + header + tabs + tray_height + 34.0;
    egui::Area::new(Id::unique("compact battle setup"))
        .fixed_pos(screen.min)
        .order(Order::Middle)
        .show(context, |ui| {
            egui::ScrollArea::vertical()
                .max_height(screen.height())
                .min_scrolled_height(screen.height())
                .show(ui, |ui| {
                    let (canvas, _) =
                        ui.allocate_exact_size(vec2(screen.width(), height), Sense::hover());
                    let painter = ui.painter().clone();
                    let left = canvas.center().x - width / 2.0;
                    let mut y = canvas.top() + COMPACT_MARGIN;
                    let row = |y: f32, height: f32| {
                        Rect::from_min_size(pos2(left, y), vec2(width, height))
                    };
                    art.logo(&painter, pos2(left + 6.0, y + 10.0));
                    corner_buttons(
                        ui,
                        art,
                        Rect::from_min_size(pos2(left + width - 82.0, y + 8.0), vec2(82.0, 44.0)),
                        actions,
                    );
                    y += header;

                    // The play tabs over the tray.
                    let tab_width = (width - 6.0) / 2.0;
                    for (index, tab) in [(false, "SINGLE PLAYER", ""), (true, "MULTIPLAYER", "")]
                        .into_iter()
                        .enumerate()
                    {
                        let open = multiplayer == tab.0;
                        let tab_left = left + index as f32 * (width - tab_width);
                        let rect = if open {
                            Rect::from_min_size(pos2(tab_left, y), vec2(tab_width, tabs))
                        } else {
                            Rect::from_min_size(
                                pos2(tab_left, y + 6.0),
                                vec2(tab_width, tabs - 7.0),
                            )
                        };
                        if play_tab(ui, &painter, rect, tab, open, true) {
                            state.rooms.multiplayer = tab.0;
                        }
                    }
                    y += tabs - 1.0;
                    let tray_rect = row(y, tray_height);
                    tray(&painter, tray_rect, !multiplayer, multiplayer);
                    let inner = tray_rect.shrink(10.0);
                    let cells = |y: f32, count: usize, height: f32| {
                        let cell = (inner.width() - CARD_GAP * (count - 1) as f32) / count as f32;
                        (0..count).map(move |index| {
                            Rect::from_min_size(
                                pos2(inner.left() + index as f32 * (cell + CARD_GAP), y),
                                vec2(cell, height),
                            )
                        })
                    };
                    y = inner.top() + 2.0;

                    paint::section_label(&painter, pos2(inner.left(), y), "YOUR TANK");
                    y += label;
                    for (tank, rect) in TANK_CARDS.iter().zip(cells(y, 3, COMPACT_TANK_HEIGHT)) {
                        let response = ui
                            .interact(rect, Id::unique(("setup tank", tank.kind)), Sense::click())
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        vehicle_card(
                            &painter,
                            art,
                            rect,
                            tank,
                            human_team,
                            prefs.tank == tank.kind,
                            response.hovered(),
                            false,
                        );
                        if response.clicked() {
                            prefs.tank = tank.kind;
                        }
                    }
                    y += COMPACT_TANK_HEIGHT + ROW_GAP;

                    // MAP: an open room chosen on the Multiplayer tab shows its own.
                    paint::section_label(&painter, pos2(inner.left(), y), "MAP");
                    y += label;
                    let room_map = state.rooms.selected_map().filter(|_| multiplayer);
                    let shown_map = room_map.unwrap_or(options.map_mode);
                    for chunk in maps.chunks(3) {
                        for (map, rect) in chunk.iter().zip(cells(y, 3, COMPACT_MAP_HEIGHT)) {
                            let locked = room_map.is_some();
                            let id = Id::unique(("setup map", map.id));
                            let response = if locked {
                                ui.interact(rect, id, Sense::hover())
                            } else {
                                ui.interact(rect, id, Sense::click())
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                            };
                            map_card(
                                &painter,
                                art,
                                rect,
                                map,
                                shown_map == map.id,
                                !locked && response.hovered(),
                            );
                            if !locked && response.clicked() {
                                prefs.map = map.id;
                            }
                        }
                        y += COMPACT_MAP_HEIGHT + CARD_GAP;
                    }
                    y += ROW_GAP - CARD_GAP;

                    if multiplayer {
                        super::rooms::multiplayer(
                            ui,
                            art,
                            &mut state.rooms,
                            view,
                            prefs,
                            Rect::from_min_size(
                                pos2(inner.left(), y),
                                vec2(inner.width(), COMPACT_ROOMS_HEIGHT),
                            ),
                            actions,
                        );
                    } else {
                        paint::section_label(&painter, pos2(inner.left(), y), "BATTLE");
                        status_line(&painter, pos2(inner.right(), y + 4.0), state, time);
                        y += label;
                        for (battle, rect) in
                            BATTLE_FORMATS
                                .iter()
                                .zip(cells(y, 2, COMPACT_BATTLE_HEIGHT))
                        {
                            let unavailable = battle.unavailable_on(options.map_mode);
                            let id = Id::unique(("setup battle", battle.mode));
                            let response = match &unavailable {
                                Some(tip) => {
                                    ui.interact(rect, id, Sense::hover()).on_hover_text(tip)
                                }
                                None => ui
                                    .interact(rect, id, Sense::click())
                                    .on_hover_cursor(egui::CursorIcon::PointingHand),
                            };
                            let mut card_painter = painter.clone();
                            if unavailable.is_some() {
                                card_painter.multiply_opacity(UNAVAILABLE_OPACITY);
                            }
                            battle_card(
                                &card_painter,
                                art,
                                rect,
                                battle,
                                &battle.detail_on(options.map_mode),
                                options.game_mode == battle.mode,
                                unavailable.is_none() && response.hovered(),
                            );
                            if response.clicked() && options.game_mode != battle.mode {
                                prefs.game_mode = battle.mode;
                            }
                        }
                        y += COMPACT_BATTLE_HEIGHT + ROW_GAP;

                        paint::section_label(&painter, pos2(inner.left(), y), "DIFFICULTY");
                        y += label;
                        for ((index, entry), rect) in DIFFICULTY_OPTIONS
                            .iter()
                            .enumerate()
                            .zip(cells(y, 3, COMPACT_DIFFICULTY_HEIGHT))
                        {
                            let response = ui
                                .interact(
                                    rect,
                                    Id::unique(("setup difficulty", entry.difficulty)),
                                    Sense::click(),
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            difficulty_segment(
                                &painter,
                                rect,
                                index,
                                entry,
                                prefs.difficulty == entry.difficulty,
                                response.hovered(),
                            );
                            if response.clicked() {
                                prefs.difficulty = entry.difficulty;
                            }
                        }
                        y += COMPACT_DIFFICULTY_HEIGHT + ROW_GAP;

                        let go = Rect::from_min_size(
                            pos2(inner.left(), y),
                            vec2(inner.width(), COMPACT_GO_HEIGHT),
                        );
                        let mut go_ui = ui.new_child(egui::UiBuilder::new().max_rect(go));
                        let label = if state.starting { "LOADING…" } else { "GO!" };
                        if paint::button(
                            &mut go_ui,
                            art,
                            label,
                            None,
                            ButtonStyle::Primary,
                            go.size(),
                            !state.starting,
                        )
                        .clicked()
                        {
                            actions.push(Action::Start);
                        }
                    }
                    paint::text(
                        &painter,
                        pos2(canvas.center().x, tray_rect.bottom() + 16.0),
                        Align2::CENTER_CENTER,
                        view.build,
                        egui::FontId::proportional(10.0),
                        fade(TEXT_DIM, 0.7),
                        0.4,
                    );
                });
        });
}

/// The controls' essentials as key caps on one line.
fn key_strip(painter: &Painter, center: egui::Pos2) {
    let size = 11.0;
    let gap = 18.0;
    let hint_width = |(keys, label): &(&[&str], &str)| {
        keys.iter()
            .map(|key| paint::kbd_width(painter, key, size) + 3.0)
            .sum::<f32>()
            + 5.0
            + paint::text_width(painter, label, bold_font(12.0), 0.4)
    };
    let hints: &[(&[&str], &str)] = if crate::touch::screen() {
        &TOUCH_STRIP
    } else {
        &KEY_STRIP
    };
    let total = hints.iter().map(hint_width).sum::<f32>() + gap * (hints.len() - 1) as f32;
    let mut x = center.x - total / 2.0;
    for hint in hints {
        for key in hint.0 {
            let width = paint::kbd_width(painter, key, size);
            paint::kbd(painter, pos2(x + width / 2.0, center.y), key, size);
            x += width + 3.0;
        }
        x += 5.0;
        let label = paint::text(
            painter,
            pos2(x, center.y),
            Align2::LEFT_CENTER,
            hint.1,
            bold_font(12.0),
            hex(0xc9dbeb),
            0.4,
        );
        x = label.right() + gap;
    }
}

/// Settings and Quit on a small plate at the header's right. An iPad app or a page
/// never quits itself, so there it is Settings alone, at the plate's right end.
fn corner_buttons(ui: &mut egui::Ui, art: &Art, rect: Rect, actions: &mut Vec<Action>) {
    let button = 38.0;
    let painter = ui.painter().clone();
    let buttons: &[(&str, &str)] = if crate::QUITS {
        &[("gear", "Settings"), ("power", "Quit Sloppy Tanks")]
    } else {
        &[("gear", "Settings")]
    };
    let rect = Rect::from_min_max(
        pos2(
            rect.right() - button * buttons.len() as f32 - 6.0,
            rect.top(),
        ),
        rect.max,
    );
    paint::plate(&painter, rect, 9.0, None);
    for (index, &(icon, tip)) in buttons.iter().enumerate() {
        let cell = Rect::from_min_size(
            rect.min + vec2(3.0 + index as f32 * button, 3.0),
            Vec2::splat(button),
        );
        let response = ui
            .interact(cell, Id::unique(("setup corner", index)), Sense::click())
            .on_hover_text(tip);
        if response.hovered() {
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
        art.icon(
            &painter,
            icon,
            Rect::from_center_size(cell.center(), Vec2::splat(20.0)),
            if response.hovered() {
                Color32::WHITE
            } else {
                hex(0xe9f2fb)
            },
        );
        if response.clicked() {
            actions.push(if index == 0 {
                Action::OpenSettings
            } else {
                Action::Quit
            });
        }
    }
}

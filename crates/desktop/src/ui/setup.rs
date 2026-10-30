//! Battle Setup (`src/game/battle-setup.html`, `start-menu.ts`, `game-options.ts`,
//! `map-picker.ts`): a pane of glass over a dusk battlefield with the logo on top, the
//! tank cards, the map, the battle format and difficulty, the status well and GO.

use egui::{
    Align2, Color32, CornerRadius, Id, LayerId, Order, Painter, Pos2, Rect, Sense, Shape, Stroke,
    StrokeKind, Vec2, pos2, vec2,
};

use sloppy_core::sim::{GameMode, Team, VehicleKind};
use sloppy_web::catalog::{
    BATTLE_FORMATS, BattleFormat, DIFFICULTY_OPTIONS, DifficultyOption, KEY_STRIP, MAP_CARDS,
    MapCard, TANK_CARDS, TOUCH_STRIP, TankCard,
};

use super::icons::{Art, LOGO_BOX};
use super::paint::{self, fade, hex, hexa, mix};
use super::theme::{GOLD, TEXT_DIM, TEXT_LABEL, bold_font, display_font};
use super::{Action, UiState, View};
use crate::preferences::Preferences;

/// How far an unavailable battle format fades (`.choice-card:has(input:disabled)`).
const UNAVAILABLE_OPACITY: f32 = 0.45;

/// The narrowest tank card with room for its hull and speed line.
const SPEC_LINE_WIDTH: f32 = 200.0;
/// The shortest map card with room for its line under its name.
const MAP_DETAIL_HEIGHT: f32 = 56.0;

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
    let rect = showroom(painter, rect, selected, hovered);
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
    showroom_ring(painter, rect, selected);
}

/// A tank card's showroom: its glass, lit by a spotlight, and a gold glow when chosen.
/// A hovered card lifts; returns where it is drawn.
fn showroom(painter: &Painter, rect: Rect, selected: bool, hovered: bool) -> Rect {
    let rect = if hovered && !selected {
        rect.translate(vec2(0.0, -2.0))
    } else {
        rect
    };
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
    painter.rect_filled(rect, CornerRadius::same(10), bottom);
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
    rect
}

/// The showroom's edge: the chosen tank's gold ring, or a faint line.
fn showroom_ring(painter: &Painter, rect: Rect, selected: bool) {
    let radius = CornerRadius::same(10);
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
        painter.rect_filled(rect, 10, card_fill(hovered));
    }
}

/// An unchosen card's fill (`--card`, `--card-hover`).
fn card_fill(hovered: bool) -> Color32 {
    if hovered {
        hex(0x1b3966)
    } else {
        hex(0x10264a)
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

/// Where a map card puts its art and its words.
#[derive(Clone, Copy, PartialEq)]
enum MapLayout {
    /// The name and line at the left, the art in a column at the right faded in from a
    /// scrim that runs solid under the words (`.map-row .choice-card`).
    Row,
    /// An upright phone's full-width card: the same, its art taller and its scrim wider.
    Phone,
    /// A narrow window's short button: its name at the top over small art centred at
    /// the bottom, at `name_size`.
    Narrow { name_size: f32 },
}

/// An upright phone's map card keeps this much of its right for its art.
const PHONE_MAP_ART_COLUMN: f32 = 150.0;
/// A map's whole scene, width over height, of which its art is the right three quarters.
const MAP_SCENE_ASPECT: f32 = 3.0;

/// A map card: its name and line over its landscape, washed in the map's tint.
#[allow(clippy::too_many_arguments)]
fn map_card(
    painter: &Painter,
    art: &Art,
    rect: Rect,
    map: &MapCard,
    selected: bool,
    hovered: bool,
    layout: MapLayout,
) {
    let extra = map.extra();
    choice_frame(painter, rect, selected, hovered);
    let tint = hex(map.tint);
    let clip = painter.with_clip_rect(rect.shrink(1.0));
    clip.add(paint::vgradient(
        rect,
        &[(0.3, Color32::TRANSPARENT), (1.0, fade(tint, 0.5))],
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
        // The art's height and how far it runs past the right edge, and where the scrim
        // is solid to and clear from, from the right (`var(--art) right -6px bottom /
        // auto 74%` under `#0b1b36 calc(100% - 150px), transparent calc(100% - 60px)`).
        let art_name = format!("art-{}", map.id.as_str());
        let (share, past_edge, scrim_solid, scrim_clear) = match layout {
            MapLayout::Row => (0.74, 6.0, 150.0, 60.0),
            MapLayout::Phone => (0.96, 10.0, 180.0, 100.0),
            MapLayout::Narrow { .. } => (0.46, 0.0, 0.0, 0.0),
        };
        let height = rect.height() * share;
        let size = vec2(height * art.aspect(&art_name), height);
        let place = if let MapLayout::Narrow { .. } = layout {
            // Centred as the whole scene is, of which the art is the right three quarters.
            let scene_left = rect.center().x - height * MAP_SCENE_ASPECT / 2.0;
            Rect::from_min_size(
                pos2(
                    scene_left + height * MAP_SCENE_ASPECT - size.x,
                    rect.bottom() - size.y,
                ),
                size,
            )
        } else {
            Rect::from_min_size(
                pos2(rect.right() - size.x + past_edge, rect.bottom() - size.y),
                size,
            )
        };
        art.icon(&clip, &art_name, place, Color32::WHITE);
        if scrim_solid > 0.0 {
            let scrim = hex(0x0b1b36);
            let from_right = |inset: f32| ((rect.width() - inset) / rect.width()).max(0.0);
            clip.add(paint::hgradient(
                rect,
                &[
                    (0.0, scrim),
                    (from_right(scrim_solid), scrim),
                    (from_right(scrim_clear), Color32::TRANSPARENT),
                    (1.0, Color32::TRANSPARENT),
                ],
            ));
        }
    }
    // Until it is chosen the card shows its scene layer at part strength: the card's own
    // fill over it at the rest makes the same blend.
    if !selected {
        let strength = if hovered { 0.8 } else { 0.55 };
        clip.rect_filled(rect, 10, fade(card_fill(hovered), 1.0 - strength));
    }
    let text_width = rect.width() - 24.0;
    // A phone's short cards held sideways have room for the name alone, in the card's
    // middle; a narrow window's buttons show it at their top.
    let detail = !matches!(layout, MapLayout::Narrow { .. })
        && rect.width() >= SPEC_LINE_WIDTH
        && rect.height() >= MAP_DETAIL_HEIGHT;
    let name_galley = map_name_galley(painter, map, rect.width(), layout);
    let name_size = name_galley.size();
    let (name_at, line_top) = match layout {
        MapLayout::Narrow { .. } => (
            rect.min + vec2(NARROW_MAP_PADDING.1, NARROW_MAP_PADDING.0),
            0.0,
        ),
        _ if detail => {
            let block = name_size.y + MAP_DETAIL_GAP + MAP_DETAIL_LINE;
            let top = (rect.height() - block) / 2.0;
            (
                rect.min + vec2(MAP_PADDING, top),
                top + name_size.y + MAP_DETAIL_GAP,
            )
        }
        _ => (
            rect.min + vec2(MAP_PADDING, (rect.height() - name_size.y) / 2.0),
            0.0,
        ),
    };
    painter.galley_with_override_text_color(
        name_at + vec2(0.0, 1.0),
        name_galley.clone(),
        hexa(0, 0.55),
    );
    painter.galley(name_at, name_galley, Color32::WHITE);
    let name = Rect::from_min_size(name_at, name_size);
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
    if detail {
        paint::text_fit(
            painter,
            pos2(
                rect.left() + MAP_PADDING,
                rect.top() + line_top + MAP_DETAIL_LINE / 2.0,
            ),
            Align2::LEFT_CENTER,
            map.short_description.unwrap_or(map.description),
            egui::FontId::proportional(12.0),
            hex(0xc9def0),
            0.0,
            text_width,
        );
    }
    choice_ring(painter, rect, selected);
}

/// Team Battle's poster: each tank's width as a share of the poster's, how far down the
/// art the red tank's top stands and how far up the blue tank's foot (`.team-art`).
#[derive(Clone, Copy)]
struct TeamArt {
    tank_width: f32,
    red_top: f32,
    blue_bottom: f32,
}

const GARAGE_TEAM_ART: TeamArt = TeamArt {
    tank_width: 0.66,
    red_top: 0.06,
    blue_bottom: 0.04,
};
/// The stacked layout's posters are narrower and keep their tanks a little smaller.
const STACKED_TEAM_ART: TeamArt = TeamArt {
    tank_width: 0.62,
    red_top: 0.08,
    blue_bottom: 0.06,
};

/// A battle format's scene washes, which tell a two-sided fight from a lone tank, in
/// `rect` under `clip`.
fn battle_scene(clip: &Painter, rect: Rect, team: bool) {
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
            rect.min + vec2(rect.width() * x, rect.height() * y),
            vec2(rect.width() * radius_x, rect.height() * radius_y),
            &[(0.0, hexa(color, opacity)), (0.7, Color32::TRANSPARENT)],
        ));
    }
    clip.add(paint::vgradient(
        rect,
        &[(0.55, Color32::TRANSPARENT), (1.0, hexa(0x050c1a, 0.6))],
    ));
}

/// A battle format card (`.battle-choice`): a tall poster of tanks over its name and
/// `detail`, the player's side in their team's colour, Team Battle's tanks placed by
/// `team_art`.
#[allow(clippy::too_many_arguments)]
fn battle_card(
    painter: &Painter,
    art: &Art,
    rect: Rect,
    battle: &BattleFormat,
    detail: &str,
    selected: bool,
    hovered: bool,
    team_art: TeamArt,
) {
    let (team, name) = (battle.mode == GameMode::Team, battle.name);
    choice_frame(painter, rect, selected, hovered);
    let clip = painter.with_clip_rect(rect.shrink(1.0));
    let at =
        |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(rect.min + vec2(x, y), vec2(w, h));
    let width = rect.width();
    let height = rect.height();
    battle_scene(&clip, rect, team);
    if team {
        // Authored poster colours stay fixed: blue below, red above.
        let art_height = height - 68.0;
        let tank_width = width * team_art.tank_width;
        let tank_height = tank_width * 300.0 / 560.0;
        art.tank(
            &clip,
            VehicleKind::Heavy,
            Team::Red,
            at(
                width * 0.98 - tank_width,
                6.0 + art_height * team_art.red_top,
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
                6.0 + art_height * (1.0 - team_art.blue_bottom) - tank_height,
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
    paint::shadowed(
        painter,
        rect.min + vec2(16.0, rect.height() - 33.0 - BATTLE_TITLE_SIZE),
        Align2::LEFT_TOP,
        &name.to_uppercase(),
        display_font(BATTLE_TITLE_SIZE),
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

/// A battle poster's title (`.battle-choice .choice-card b`).
const BATTLE_TITLE_SIZE: f32 = 24.0;
/// From a one-column battle button's left edge to its name.
const BATTLE_KEY_PADDING: f32 = 12.0;

/// A narrow window's battle format: a short button with its name over its scene's
/// washes, without the poster's tanks or line.
fn battle_key(
    painter: &Painter,
    rect: Rect,
    battle: &BattleFormat,
    selected: bool,
    hovered: bool,
    title_size: f32,
) {
    choice_frame(painter, rect, selected, hovered);
    battle_scene(
        &painter.with_clip_rect(rect.shrink(1.0)),
        rect,
        battle.mode == GameMode::Team,
    );
    paint::shadowed(
        painter,
        rect.left_center() + vec2(BATTLE_KEY_PADDING, 0.0),
        Align2::LEFT_CENTER,
        &battle.name.to_uppercase(),
        display_font(title_size),
        Color32::WHITE,
        0.8,
    );
    choice_ring(painter, rect, selected);
}

/// A difficulty level (`.difficulty-setting .segment`): its name, what it changes and
/// its strength bars; in one column (`name_only`, the name's size) the name over its bars.
fn difficulty_segment(
    painter: &Painter,
    rect: Rect,
    level: usize,
    entry: &DifficultyOption,
    selected: bool,
    hovered: bool,
    name_only: Option<f32>,
) {
    choice_frame(painter, rect, selected, hovered);
    if let Some(name_size) = name_only {
        paint::shadowed(
            painter,
            rect.center_top() + vec2(0.0, 12.0),
            Align2::CENTER_TOP,
            &entry.name.to_uppercase(),
            display_font(name_size),
            Color32::WHITE,
            0.6,
        );
        let bars = Rect::from_min_size(
            pos2(rect.center().x - 14.0, rect.bottom() - 10.0 - 24.0),
            vec2(28.0, 24.0),
        );
        difficulty_bars(painter, bars, level, selected);
        choice_ring(painter, rect, selected);
        return;
    }
    let galley = difficulty_detail(painter, entry, rect.width());
    let content_top =
        ((rect.height() - DIFFICULTY_NAME_ROW - galley.size().y) / 2.0).max(DIFFICULTY_PADDING);
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
        rect.min + vec2(18.0, content_top + DIFFICULTY_NAME_ROW),
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
const WIDE_DESIGN: Vec2 = vec2(1400.0, 823.0);
const WIDE_GARAGE_GROWTH: f32 = 28.0;

/// Where the parts sit on the design canvas (`.menu.start` at 1280 × 800).
const FRAME: Rect = Rect::from_min_max(pos2(32.5, 25.25), pos2(1247.5, 774.75));
const TANK_TAB: Rect = Rect::from_min_max(pos2(60.0, 135.0), pos2(304.0, 195.0));
const TANK_WELL: Rect = Rect::from_min_max(pos2(60.0, 194.0), pos2(331.0, 708.0));
const PANELS: Rect = Rect::from_min_max(pos2(350.0, 194.0), pos2(1220.0, 708.0));
const CARD_GAP: f32 = 7.0;

/// The menu's own glass (`.menu.start`) under the whole sheet.
fn sheet_glass(painter: &Painter, frame: Rect) {
    // Its soft drop shadow (`0 40px 140px #000c`) and the faint blue glow round it.
    painter.add(soft_shadow(frame, 16, [0, 40], 200, hexa(0, 0.7)));
    painter.add(soft_shadow(frame, 16, [0, 0], 170, hexa(0x2a6fd0, 0.13)));
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
}

/// A blurred shadow of `rect` moved by `offset` (a CSS `box-shadow`), `blur` wide at its
/// edge: egui fades it linearly, so a CSS blur radius takes about half again as much.
fn soft_shadow(
    rect: Rect,
    radius: impl Into<CornerRadius>,
    offset: [i8; 2],
    blur: u8,
    color: Color32,
) -> Shape {
    egui::epaint::Shadow {
        offset,
        blur,
        spread: 0,
        color,
    }
    .as_shape(rect, radius)
    .into()
}

/// The tray's glass: the tank well and the panels share it, joined to their tabs.
fn tray(painter: &Painter, rect: Rect, open_left: bool, open_right: bool) {
    let radius = CornerRadius {
        nw: if open_left { 0 } else { 16 },
        ne: if open_right { 0 } else { 16 },
        sw: 16,
        se: 16,
    };
    painter.add(soft_shadow(rect, radius, [0, 10], 45, hexa(0, 0.4)));
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

/// A folder tab over the tray: open, the tray's own surface with its outline, lit along
/// its rounded top by `edge` (`inset 0 3px`) and reaching a pixel down over the tray's
/// edge so the two are one; closed, recessed and darker, shaded at its foot.
fn folder_tab(painter: &Painter, rect: Rect, open: bool, edge: Color32) {
    let radius = CornerRadius {
        nw: 14,
        ne: 14,
        sw: 0,
        se: 0,
    };
    if open {
        painter.rect_filled(rect, radius, hex(0x15305a));
        painter.rect_filled(rect, radius, edge);
        painter.with_clip_rect(rect).rect_filled(
            rect.translate(vec2(0.0, 3.0)),
            radius,
            hex(0x15305a),
        );
        // The open folder's glow under its lit edge.
        if edge == GOLD {
            painter.with_clip_rect(rect).add(paint::radial(
                rect.center_top(),
                vec2(rect.width() * 0.32, 22.0),
                &[(0.0, hexa(0xffcf38, 0.25)), (1.0, Color32::TRANSPARENT)],
            ));
        }
        // The tray's outline up the sides and over the top, open at the foot.
        let edge_line = Stroke::new(1.0, hexa(0x8fc4f0, 0.29));
        painter
            .with_clip_rect(rect.with_max_y(rect.bottom() - 1.0))
            .rect_stroke(
                rect.with_max_y(rect.bottom() + 16.0),
                CornerRadius::same(14),
                edge_line,
                StrokeKind::Inside,
            );
    } else {
        painter.rect_filled(rect, radius, hex(0x0d1f3c));
        painter.add(paint::vgradient(
            rect.shrink2(vec2(0.0, 14.0)),
            &[(0.0, hex(0x0f2342)), (1.0, hex(0x0a1830))],
        ));
        painter.add(paint::vgradient(
            Rect::from_min_max(pos2(rect.left(), rect.bottom() - 10.0), rect.max),
            &[(0.0, Color32::TRANSPARENT), (1.0, hexa(0, 0.35))],
        ));
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(1.0, hexa(0x9fd0ff, 0.12)),
            StrokeKind::Inside,
        );
    }
}

/// A map's name, as its card shows it.
fn map_name(id: sloppy_core::sim::map_options::MapId) -> &'static str {
    sloppy_core::sim::map_options::map_option_for(id).name
}

/// Where a play tab's line goes: beside its title, under it where the tab is too narrow
/// for both on one row, or nowhere (a phone's or a narrow window's tab names itself alone).
#[derive(Clone, Copy, PartialEq)]
enum TabLine {
    Beside,
    Below,
    Hidden,
}

/// How a play tab dresses (`.play-tab`): its badge's size, if it has one, its title's
/// size, its line, its left padding, and the room above and below its content when open
/// and when closed (a closed tab stands lower, its title still on the open one's line).
#[derive(Clone, Copy)]
struct TabDress {
    badge: Option<f32>,
    title_size: f32,
    line: TabLine,
    padding_left: f32,
    open_padding: (f32, f32),
    closed_padding: (f32, f32),
}

/// The garage's tabs and the stacked layout's, with their line beside the title.
const WIDE_TAB: TabDress = TabDress {
    badge: Some(34.0),
    title_size: 23.0,
    line: TabLine::Beside,
    padding_left: 17.0,
    open_padding: (10.0, 10.0),
    closed_padding: (6.0, 13.0),
};
/// The page's play tabs: the title over its line where they share no row.
const TAB_TITLE_LINE: f32 = 23.0;
const TAB_LINE_GAP: f32 = 2.0;
const TAB_SMALL_LINE: f32 = 16.0;
/// Between a tab's badge and its title, and its title and its line.
const TAB_GAP: f32 = 12.0;

/// A play tab: its badge, its name and what it is (`.play-tab`), dressed by `dress`.
fn play_tab(
    ui: &mut egui::Ui,
    painter: &Painter,
    rect: Rect,
    (multiplayer, name, line): (bool, &str, &str),
    open: bool,
    dress: TabDress,
) -> bool {
    let response = ui
        .interact(rect, Id::unique(("play tab", multiplayer)), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    paint::labelled_choice(&response, true, open, name);
    folder_tab(painter, rect, open, GOLD);
    let (above, below) = if open {
        dress.open_padding
    } else {
        dress.closed_padding
    };
    let content = Rect::from_min_max(
        pos2(rect.left() + dress.padding_left, rect.top() + above),
        pos2(rect.right() - dress.padding_left, rect.bottom() - below),
    );
    let ink = if open { hex(0x1a2233) } else { hex(0xc9dbec) };
    let mut title_left = content.left();
    if let Some(size) = dress.badge {
        let badge = Rect::from_min_size(
            pos2(content.left(), content.center().y - size / 2.0),
            Vec2::splat(size),
        );
        if open {
            painter.add(paint::rounded_vgradient(
                badge,
                [9.0; 4],
                &[(0.0, super::theme::GOLD_HOT), (1.0, GOLD)],
            ));
        } else {
            painter.rect_filled(badge, 9, hexa(0xffffff, 0.05));
        }
        player_icon(painter, badge.shrink(6.0), multiplayer, ink);
        title_left = badge.right() + if size < 30.0 { 10.0 } else { TAB_GAP };
    }
    let title_color = if open { Color32::WHITE } else { hex(0xc9dbec) };
    let line_color = if open { hex(0xd3e7f5) } else { hex(0xc9dbec) };
    let line_font = egui::FontId::proportional(13.0);
    if dress.line == TabLine::Below {
        let top = content.center().y - (TAB_TITLE_LINE + TAB_LINE_GAP + TAB_SMALL_LINE) / 2.0;
        paint::text(
            painter,
            pos2(title_left, top + TAB_TITLE_LINE / 2.0),
            Align2::LEFT_CENTER,
            name,
            display_font(dress.title_size),
            title_color,
            1.0,
        );
        paint::text(
            painter,
            pos2(
                title_left,
                top + TAB_TITLE_LINE + TAB_LINE_GAP + TAB_SMALL_LINE / 2.0,
            ),
            Align2::LEFT_CENTER,
            line,
            line_font,
            line_color,
            0.0,
        );
        return response.clicked();
    }
    let title = paint::text(
        painter,
        pos2(title_left, content.center().y + 1.0),
        Align2::LEFT_CENTER,
        name,
        display_font(dress.title_size),
        title_color,
        1.0,
    );
    if dress.line == TabLine::Beside {
        paint::text(
            painter,
            pos2(title.right() + TAB_GAP, content.center().y + 3.0),
            Align2::LEFT_CENTER,
            line,
            line_font,
            line_color,
            0.0,
        );
    }
    response.clicked()
}

/// Whether a wide tab `width` wide has room for its title and line on one row.
fn tab_line_fits(painter: &Painter, width: f32, (_, name, line): (bool, &str, &str)) -> bool {
    let badge = WIDE_TAB.badge.unwrap_or(0.0) + TAB_GAP;
    paint::text_width(painter, name, display_font(WIDE_TAB.title_size), 1.0)
        + TAB_GAP
        + paint::text_width(painter, line, egui::FontId::proportional(13.0), 0.0)
        <= width - 2.0 * WIDE_TAB.padding_left - badge
}

/// The play tabs' person, or two people (the page's 24-unit tab icons), drawn in `rect`.
fn player_icon(painter: &Painter, rect: Rect, multiplayer: bool, ink: Color32) {
    let unit = rect.width() / 24.0;
    let at = |x: f32, y: f32| rect.min + vec2(x, y) * unit;
    let stroke = Stroke::new(1.8 * unit, ink);
    let curve = |points: [Pos2; 4]| {
        Shape::CubicBezier(egui::epaint::CubicBezierShape::from_points_stroke(
            points,
            false,
            Color32::TRANSPARENT,
            stroke,
        ))
    };
    if multiplayer {
        painter.circle_stroke(at(9.0, 8.0), 3.5 * unit, stroke);
        painter.add(curve([
            at(2.0, 20.0),
            at(2.0, 16.1),
            at(5.1, 13.8),
            at(9.0, 13.8),
        ]));
        painter.add(curve([
            at(9.0, 13.8),
            at(12.9, 13.8),
            at(16.0, 16.1),
            at(16.0, 20.0),
        ]));
        painter.circle_stroke(at(17.0, 7.0), 3.0 * unit, stroke);
        painter.add(curve([
            at(16.5, 12.6),
            at(19.7, 12.8),
            at(22.0, 14.9),
            at(22.0, 18.5),
        ]));
    } else {
        painter.circle_stroke(at(12.0, 8.0), 4.0 * unit, stroke);
        painter.add(curve([
            at(4.0, 21.0),
            at(4.0, 16.6),
            at(7.6, 14.0),
            at(12.0, 14.0),
        ]));
        painter.add(curve([
            at(12.0, 14.0),
            at(16.4, 14.0),
            at(20.0, 16.6),
            at(20.0, 21.0),
        ]));
    }
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

/// The status line's progress track, the gap before it, and how far under the line's
/// anchor its text's middle sits.
const STATUS_TRACK: f32 = 72.0;
const STATUS_TRACK_GAP: f32 = 10.0;
const STATUS_TEXT_DROP: f32 = 7.5;

/// What the arena is doing while it prepares, or nothing once it is ready.
fn status_text(state: &UiState) -> Option<&str> {
    if state.ready && !state.starting {
        None
    } else if state.starting {
        Some("Rolling out…")
    } else if state.status.is_empty() {
        Some("Loading game…")
    } else {
        Some(&state.status)
    }
}

/// The status line's width: its text, the gap and the track.
fn status_line_width(painter: &Painter, state: &UiState) -> f32 {
    status_text(state).map_or(0.0, |status| {
        paint::text_width(painter, status, bold_font(12.0), 0.2) + STATUS_TRACK_GAP + STATUS_TRACK
    })
}

/// While the arena prepares, its progress on one slim line ending at `right` (the BATTLE
/// heading's line, or one column's own row); gone once ready.
fn status_line(painter: &Painter, right: egui::Pos2, state: &UiState, time: f64) {
    let Some(status) = status_text(state) else {
        return;
    };
    let track = Rect::from_min_size(
        right - vec2(STATUS_TRACK, 1.5 - STATUS_TEXT_DROP),
        vec2(STATUS_TRACK, 3.0),
    );
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
    paint::text(
        painter,
        pos2(track.left() - STATUS_TRACK_GAP, track.center().y),
        Align2::RIGHT_CENTER,
        status,
        bold_font(12.0),
        hex(0xdbe9f6),
        0.2,
    );
}

/// The page's widths for Battle Setup's layouts (`style.css`): the garage from this wide,
/// the garage stacked over the tray below it, and one column at the narrow width and under.
const GARAGE_MIN_WIDTH: f32 = 1240.0;
const NARROW_MAX_WIDTH: f32 = 600.0;
/// Whether Battle Setup changes layout at the page's widths: on the page and in the iPad
/// app. The desktop app keeps the garage at every size its window takes.
const FOLLOWS_PAGE_WIDTHS: bool = cfg!(any(target_arch = "wasm32", target_os = "ios"));

pub fn battle_setup(
    context: &egui::Context,
    state: &mut UiState,
    art: &Art,
    view: &View,
    actions: &mut Vec<Action>,
) {
    super::backdrop::backdrop(context, art);
    let mut prefs = view.prefs.clone();
    let screen = context.content_rect();
    if crate::touch::phone() {
        phone_setup(context, state, art, view, &mut prefs, actions);
    } else if !FOLLOWS_PAGE_WIDTHS || screen.width() >= GARAGE_MIN_WIDTH {
        sheet(context, state, art, view, &mut prefs, actions);
    } else {
        stacked_setup(context, state, art, view, &mut prefs, actions);
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
        .min(1.0);
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
            // Room round the sheet for its soft shadow.
            ui.set_clip_rect(canvas.expand(SHADOW_ROOM));
            let _ = ui.allocate_rect(canvas, Sense::hover());
            let painter = ui.painter().clone();
            sheet_glass(&painter, frame);
            // Where the page's sheet puts `.setup-brand` at 1280 × 800.
            art.logo(&painter, pos2(71.0, 53.0));
            corner_buttons(
                ui,
                art,
                Rect::from_min_size(pos2(1137.0 + extra_width, 64.0), vec2(82.0, 44.0)),
                actions,
            );

            // The tank column, for both ways to play; its tab covers the tray's edge.
            tray(&painter, tank_well, true, false);
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
            for (index, tank) in TANK_CARDS.iter().enumerate() {
                let rect = Rect::from_min_size(
                    pos2(72.0, 206.0 + index as f32 * (165.5 + extra_height / 3.0)),
                    vec2(246.0 + garage_growth, 158.0 + extra_height / 3.0),
                );
                let response = ui
                    .interact(rect, Id::unique(("setup tank", tank.kind)), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                paint::labelled_choice(&response, true, prefs.tank == tank.kind, tank.name());
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

            // The play tabs over their panel, the open one covering its edge.
            let multiplayer = state.rooms.multiplayer;
            tray(&painter, panels, !multiplayer, multiplayer);
            for (index, tab) in PLAY_TABS.into_iter().enumerate() {
                let open = multiplayer == tab.0;
                let left = panels.left() + index as f32 * (panels.width() - tab_width);
                let rect = if open {
                    Rect::from_min_size(pos2(left, 135.0), vec2(tab_width, 60.0))
                } else {
                    Rect::from_min_size(pos2(left, 141.0), vec2(tab_width, 52.0))
                };
                if play_tab(ui, &painter, rect, tab, open, WIDE_TAB) {
                    state.rooms.multiplayer = tab.0;
                }
            }
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
                // A linked room's extra level shows even where it is not offered.
                let maps: Vec<&MapCard> = MAP_CARDS
                    .iter()
                    .filter(|map| view.debug || !map.extra() || room_map == Some(map.id))
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
                    paint::labelled_choice(
                        &response,
                        !locked,
                        shown_map == map.id,
                        map_name(map.id),
                    );
                    map_card(
                        &painter,
                        art,
                        rect,
                        map,
                        shown_map == map.id,
                        !locked && response.hovered(),
                        MapLayout::Row,
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
                    &super::rooms::MultiplayerLayout::garage(Rect::from_min_max(
                        pos2(inner.left(), 330.0),
                        pos2(inner.right(), 693.0 + extra_height),
                    )),
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
                    paint::labelled_choice(
                        &response,
                        unavailable.is_none(),
                        options.game_mode == battle.mode,
                        battle.name,
                    );
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
                        GARAGE_TEAM_ART,
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
                    paint::labelled_choice(
                        &response,
                        true,
                        prefs.difficulty == entry.difficulty,
                        entry.name,
                    );
                    difficulty_segment(
                        &painter,
                        rect,
                        index,
                        entry,
                        prefs.difficulty == entry.difficulty,
                        response.hovered(),
                        None,
                    );
                    if response.clicked() {
                        prefs.difficulty = entry.difficulty;
                    }
                }
                let go = Rect::from_min_size(
                    pos2(right_left, 631.0 + extra_height),
                    vec2(right_width, 62.0),
                );
                go_button(ui, art, go, state, GO_LETTERING, actions);
            }
            painter.line_segment(
                [
                    pos2(tank_well.left(), 722.0 + extra_height),
                    pos2(panels.right(), 722.0 + extra_height),
                ],
                Stroke::new(1.0, hexa(0x9fd0ff, 0.12)),
            );
            key_strip(
                &painter,
                pos2(design.x / 2.0, 745.0 + extra_height),
                f32::INFINITY,
            );
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

/// The page's padding round Battle Setup (`#overlay`, 12 px in one column), and the room
/// `fitPanels` keeps under the stacked layout before scaling it to the window's height.
const OVERLAY_PADDING: f32 = 24.0;
const NARROW_OVERLAY_PADDING: f32 = 12.0;
const FIT_MARGIN: f32 = 50.0;
/// The stacked layout's widest (`.menu.start`).
const STACK_MAX_WIDTH: f32 = 1040.0;
/// Windows this short leave out the logo's row and the key strip (`max-height: 770px`).
const SHORT_MAX_HEIGHT: f32 = 770.0;
/// Windows this narrow give a tank card's class a line of its own (`max-width: 900px`),
/// and these leave out the key strip (`max-width: 800px`).
const UPRIGHT_TABLET_MAX_WIDTH: f32 = 900.0;
const NO_KEY_STRIP_MAX_WIDTH: f32 = 800.0;
/// The menu's 1 px border and its padding inside it (top, sides, bottom), in the stacked
/// layout and in one column.
const MENU_BORDER: f32 = 1.0;
const STACK_PADDING: [f32; 3] = [18.0, 26.0, 14.0];
const NARROW_PADDING: [f32; 3] = [14.0, 10.0, 12.0];
/// The tray's border and padding, the gap between the setup's rows (less on a short
/// screen), and between the tab panel's columns and rows.
const TRAY_INSET: f32 = 15.0;
const SETUP_GAP: f32 = 14.0;
const SHORT_SETUP_GAP: f32 = 12.0;
const PANEL_COLUMN_GAP: f32 = 22.0;
const PANEL_ROW_GAP: f32 = 14.0;
/// Room kept round a sheet's canvas for its soft shadow.
const SHADOW_ROOM: f32 = 260.0;
/// Every choice sits in a dark well: its padding and the gap between its choices.
const WELL_PADDING: f32 = 5.0;
const WELL_GAP: f32 = 6.0;
/// A section heading's row (`.setup-label` and its margin).
const HEADING_ROW: f32 = super::rooms::HEADING_ROW;
/// The logo's row: room above and below it for its tilt and shadow (less in one column,
/// where it has the row to itself), and the gap to the tabs beside it.
const LOGO_ABOVE: f32 = 6.0;
const LOGO_BELOW: f32 = 18.0;
const NARROW_LOGO_BELOW: f32 = 14.0;
const LOGO_TAB_GAP: f32 = 22.0;
/// A narrow window's smaller logo (TANKS at 34 px against 41).
const NARROW_LOGO_SCALE: f32 = 34.0 / 41.0;
/// Between the two play tabs, and between them and the Settings plate.
const TABS_GAP: f32 = 6.0;
const CORNER_GAP: f32 = 10.0;
/// The action's row: GO, CREATE ROOM and the room's status well.
const ACTION_HEIGHT: f32 = 62.0;
/// Over the key strip, a hairline and this padding.
const FOOTER_PADDING: f32 = 11.0;

/// A tank card's rows: its name's size, the picture's box (shorter on a short screen),
/// which tucks under the line above it and into the one below, and the hull and speed
/// line.
const TANK_NAME_SIZE: f32 = 26.0;
const TANK_PREVIEW: f32 = 108.0;
const SHORT_TANK_PREVIEW: f32 = 90.0;
const PREVIEW_TUCK: (f32, f32) = (20.0, 6.0);
const ROW_GAP_UNDER_NAME: f32 = 4.0;
const CLASS_TAG_HEIGHT: f32 = 19.0;
const NARROW_CLASS_TAG_HEIGHT: f32 = 17.0;
const SPEC_HEIGHT: f32 = 30.0;
const UPRIGHT_SPEC_HEIGHT: f32 = 29.0;
/// Map cards: their least height (taller in one column), the column the art keeps at
/// their right, and their name's and line's rows.
const MAP_CARD_MIN: f32 = 72.0;
const NARROW_MAP_CARD_MIN: f32 = 78.0;
const MAP_ART_COLUMN: f32 = 128.0;
const MAP_NAME_LINE: f32 = 18.0;
const MAP_DETAIL_GAP: f32 = 3.0;
const MAP_DETAIL_LINE: f32 = 16.2;
const MAP_PADDING: f32 = 12.0;
const NARROW_MAP_PADDING: (f32, f32) = (9.0, 10.0);
/// One column's battle formats and difficulty levels, short rows of buttons, and the
/// status line's row between them and GO.
const NARROW_BATTLE_HEIGHT: f32 = 64.0;
const NARROW_DIFFICULTY_HEIGHT: f32 = 78.0;
const STATUS_ROW: f32 = 16.0;
/// A difficulty level: its padding, its name's row and each line of what it changes.
const DIFFICULTY_PADDING: f32 = 8.0;
const DIFFICULTY_NAME_ROW: f32 = 30.0;
const DIFFICULTY_LINE: f32 = 16.875;
/// From a level's edges to what it changes (`padding: 8px 76px 8px 18px`).
const DIFFICULTY_TEXT_INSET: f32 = 94.0;

/// GO's type (`.start #start`), and in one column.
const GO_LETTERING: paint::Lettering = paint::Lettering::Display {
    size: 40.0,
    spacing: 4.0,
};
const NARROW_GO_LETTERING: paint::Lettering = paint::Lettering::Display {
    size: 30.0,
    spacing: 4.0,
};

/// The play tabs: which is multiplayer, its name and what it is.
const PLAY_TABS: [(bool, &str, &str); 2] = [
    (false, "SINGLE PLAYER", "You versus the bots"),
    (true, "MULTIPLAYER", "Online rooms with friends"),
];

/// The page's `clamp(least, share × viewport width, most)` type.
fn viewport_size(viewport_width: f32, least: f32, share: f32, most: f32) -> f32 {
    (viewport_width * share / 100.0).clamp(least, most)
}

/// What the stacked layout knows of the window: one column at the narrow width, a short
/// screen, and an upright tablet's narrow tank cards.
#[derive(Clone, Copy)]
struct StackWindow {
    narrow: bool,
    short: bool,
    upright_tablet: bool,
    key_strip: bool,
    viewport_width: f32,
}

impl StackWindow {
    fn of(screen: Rect) -> Self {
        let narrow = screen.width() <= NARROW_MAX_WIDTH;
        let short = screen.height() <= SHORT_MAX_HEIGHT;
        Self {
            narrow,
            short,
            upright_tablet: screen.width() <= UPRIGHT_TABLET_MAX_WIDTH,
            key_strip: !short && screen.width() > NO_KEY_STRIP_MAX_WIDTH,
            viewport_width: screen.width(),
        }
    }

    fn padding(&self) -> [f32; 3] {
        if self.narrow {
            NARROW_PADDING
        } else {
            STACK_PADDING
        }
    }

    fn gap(&self) -> f32 {
        if self.short {
            SHORT_SETUP_GAP
        } else {
            SETUP_GAP
        }
    }

    /// The play tabs' dress: a wide tab's badge and line, or one column's names alone.
    fn tab_dress(&self, line: TabLine) -> TabDress {
        let (open_padding, closed_padding) = if self.short {
            ((10.0, 7.0), (7.0, 6.0))
        } else {
            (WIDE_TAB.open_padding, WIDE_TAB.closed_padding)
        };
        if self.narrow {
            TabDress {
                badge: None,
                title_size: viewport_size(self.viewport_width, 14.0, 4.6, 20.0),
                line: TabLine::Hidden,
                open_padding,
                closed_padding,
                ..WIDE_TAB
            }
        } else {
            TabDress {
                line,
                open_padding,
                closed_padding,
                ..WIDE_TAB
            }
        }
    }

    /// The open and closed tabs' heights (`.play-tab`, and its short-screen form).
    fn tab_heights(&self, line: TabLine) -> (f32, f32) {
        let dress = self.tab_dress(line);
        let content = match line {
            TabLine::Below => TAB_TITLE_LINE + TAB_LINE_GAP + TAB_SMALL_LINE,
            _ => dress.badge.unwrap_or(0.0),
        };
        let (open_least, closed_least) = if self.short {
            (54.0, 46.0)
        } else {
            (60.0, 52.0)
        };
        (
            (dress.open_padding.0 + content + dress.open_padding.1).max(open_least),
            (dress.closed_padding.0 + content + dress.closed_padding.1).max(closed_least),
        )
    }

    fn tank_rows(&self) -> TankRows {
        TankRows {
            narrow: self.narrow,
            class_below: self.upright_tablet,
            name_size: if self.narrow {
                viewport_size(self.viewport_width, 13.0, 4.4, 22.0)
            } else {
                TANK_NAME_SIZE
            },
            preview: if self.short {
                SHORT_TANK_PREVIEW
            } else {
                TANK_PREVIEW
            },
        }
    }

    fn map_layout(&self) -> MapLayout {
        if self.narrow {
            MapLayout::Narrow {
                name_size: viewport_size(self.viewport_width, 12.0, 3.6, 15.0),
            }
        } else {
            MapLayout::Row
        }
    }
}

/// How a stacked layout's tank card lays out its rows (`.start .vehicle` under 1240 px).
#[derive(Clone, Copy)]
struct TankRows {
    /// One column's tight card: no hull and speed line.
    narrow: bool,
    /// The class takes its one-word name on a line under the tank's name, and the hull
    /// reads as HP.
    class_below: bool,
    name_size: f32,
    preview: f32,
}

impl TankRows {
    /// Padding: top, left, right and bottom.
    fn padding(&self) -> [f32; 4] {
        if self.narrow {
            [8.0, 9.0, 6.0, 6.0]
        } else {
            [12.0, 16.0, 16.0, 12.0]
        }
    }

    fn name_line(&self) -> f32 {
        self.name_size * 1.2
    }

    fn tag_height(&self) -> f32 {
        if self.narrow {
            NARROW_CLASS_TAG_HEIGHT
        } else {
            CLASS_TAG_HEIGHT
        }
    }

    fn spec_height(&self) -> f32 {
        if self.narrow {
            0.0
        } else if self.class_below {
            UPRIGHT_SPEC_HEIGHT + ROW_GAP_UNDER_NAME
        } else {
            SPEC_HEIGHT
        }
    }

    fn height(&self) -> f32 {
        let [top, _, _, bottom] = self.padding();
        let class_row = if self.class_below {
            ROW_GAP_UNDER_NAME + self.tag_height() + ROW_GAP_UNDER_NAME
        } else {
            0.0
        };
        top + self.name_line() + class_row + self.preview - PREVIEW_TUCK.0 - PREVIEW_TUCK.1
            + self.spec_height()
            + bottom
    }
}

/// A stacked layout's tank card: its name over its class, or the class beside it, the
/// tank under them and its hull and speed as bars (left out in one column).
#[allow(clippy::too_many_arguments)]
fn stack_tank_card(
    painter: &Painter,
    art: &Art,
    rect: Rect,
    tank: &TankCard,
    team: Team,
    selected: bool,
    hovered: bool,
    rows: TankRows,
) {
    let rect = showroom(painter, rect, selected, hovered);
    let [top, left, right, bottom] = rows.padding();
    let name_y = rect.top() + top + rows.name_line() / 2.0;
    paint::shadowed(
        painter,
        pos2(rect.left() + left, name_y),
        Align2::LEFT_CENTER,
        tank.name(),
        display_font(rows.name_size),
        Color32::WHITE,
        0.5,
    );
    let (class, font_size, spacing, padding, dot_gap) = if rows.narrow {
        (tank.short_class, 9.5, 0.2, 6.0, 4.0)
    } else if rows.class_below {
        (tank.short_class, 11.0, 0.4, 7.0, 6.0)
    } else {
        (tank.class, 11.0, 0.4, 7.0, 6.0)
    };
    let class = class.to_uppercase();
    let font = bold_font(font_size);
    let tag_width = padding
        + 6.0
        + dot_gap
        + paint::text_width(painter, &class, font.clone(), spacing)
        + padding;
    let tag = if rows.class_below {
        Rect::from_min_size(
            pos2(
                rect.left() + left,
                rect.top() + top + rows.name_line() + ROW_GAP_UNDER_NAME,
            ),
            vec2(tag_width, rows.tag_height()),
        )
    } else {
        Rect::from_min_size(
            pos2(
                rect.right() - right - tag_width,
                name_y - rows.tag_height() / 2.0,
            ),
            vec2(tag_width, rows.tag_height()),
        )
    };
    let (tag_fill, tag_ink, dot) = if selected {
        (GOLD, hex(0x1a2233), hex(0x1a2233))
    } else {
        (hexa(0xffffff, 0.07), hex(0xcfe3f5), hex(tank.color))
    };
    painter.rect_filled(tag, 5, tag_fill);
    painter.circle_filled(pos2(tag.left() + padding + 3.0, tag.center().y), 3.0, dot);
    paint::text(
        painter,
        pos2(tag.left() + padding + 6.0 + dot_gap, tag.center().y),
        Align2::LEFT_CENTER,
        &class,
        font,
        tag_ink,
        spacing,
    );
    // The picture's box, tucked under the line above it; the tank fits its height at its
    // class's scale.
    let above = if rows.class_below {
        tag.bottom() + ROW_GAP_UNDER_NAME
    } else {
        rect.top() + top + rows.name_line()
    };
    let lift = if hovered { -2.0 } else { 0.0 };
    let stage = Rect::from_min_max(
        pos2(rect.left() + left, above - PREVIEW_TUCK.0 + lift),
        pos2(
            rect.right() - right,
            above - PREVIEW_TUCK.0 + rows.preview + lift,
        ),
    );
    let scale = tank.scale * if hovered { 1.05 } else { 1.0 };
    let tank_height = stage.height() * scale;
    painter.add(paint::radial(
        pos2(stage.center().x, stage.center().y + tank_height * 0.38),
        vec2(tank_height * 1.1, tank_height * 0.16),
        &[(0.0, hexa(0x020a14, 0.55)), (1.0, Color32::TRANSPARENT)],
    ));
    art.tank(
        &painter.with_clip_rect(rect.shrink(1.0)),
        tank.kind,
        team,
        stage,
        scale,
        false,
    );
    if !rows.narrow {
        let (hull, font_size, spacing) = if rows.class_below {
            (format!("{} HP", tank.hit_points()), 10.5, 0.2)
        } else {
            (format!("{} HIT POINTS", tank.hit_points()), 11.0, 1.2)
        };
        let spec = Rect::from_min_max(
            pos2(rect.left() + left, rect.bottom() - bottom - SPEC_HEIGHT),
            pos2(rect.right() - right, rect.bottom() - bottom),
        );
        spec_bars(
            painter,
            spec,
            [
                (hull, tank.hit_points_share()),
                (format!("{} KM/H", tank.km_per_hour()), tank.speed_share()),
            ],
            bold_font(font_size),
            spacing,
        );
    }
    showroom_ring(painter, rect, selected);
}

/// Hull and speed as two labelled bars side by side in `rect` (`.spec`): the hull's
/// fills from the left in blue, the speed's from the right in gold, each by its share of
/// the best tank's.
fn spec_bars(
    painter: &Painter,
    rect: Rect,
    specs: [(String, f32); 2],
    font: egui::FontId,
    spacing: f32,
) {
    const BAR_GAP: f32 = 16.0;
    const BAR: f32 = 4.0;
    let half = (rect.width() - BAR_GAP) / 2.0;
    for (index, (label, share)) in specs.into_iter().enumerate() {
        let left = rect.left() + index as f32 * (half + BAR_GAP);
        paint::text_fit(
            painter,
            pos2(
                if index == 0 { left } else { left + half },
                rect.bottom() - BAR - 5.0 - 6.5,
            ),
            if index == 0 {
                Align2::LEFT_CENTER
            } else {
                Align2::RIGHT_CENTER
            },
            &label,
            font.clone(),
            hex(0xdbe9f6),
            spacing,
            half,
        );
        let track = Rect::from_min_size(pos2(left, rect.bottom() - BAR), vec2(half, BAR));
        painter.rect_filled(track, 2, hexa(0xffffff, 0.09));
        let fill = if index == 0 {
            Rect::from_min_size(track.min, vec2(half * share, BAR))
        } else {
            Rect::from_min_max(pos2(track.right() - half * share, track.top()), track.max)
        };
        let (from, to) = if index == 0 {
            (hex(0x6fc6ff), hex(0xb8e4ff))
        } else {
            (super::theme::GOLD_HOT, GOLD)
        };
        painter.add(paint::hgradient(fill, &[(0.0, from), (1.0, to)]));
    }
}

/// A map card's name, wrapped where its column is too narrow (`.map-row .choice-card b`):
/// the column runs from its left padding to the art's, but never narrower than its line,
/// which never wraps.
fn map_name_galley(
    painter: &Painter,
    map: &MapCard,
    width: f32,
    layout: MapLayout,
) -> std::sync::Arc<egui::Galley> {
    let (size, line, wrap) = match layout {
        MapLayout::Narrow { name_size } => (
            name_size,
            name_size * 1.2,
            width - 2.0 * NARROW_MAP_PADDING.1,
        ),
        MapLayout::Row | MapLayout::Phone => {
            let detail = paint::text_width(
                painter,
                map.short_description.unwrap_or(map.description),
                egui::FontId::proportional(12.0),
                0.0,
            );
            let column = width
                - MAP_PADDING
                - if layout == MapLayout::Phone {
                    PHONE_MAP_ART_COLUMN
                } else {
                    MAP_ART_COLUMN
                };
            (15.0, MAP_NAME_LINE, column.max(detail))
        }
    };
    let mut job = egui::text::LayoutJob::single_section(
        map.name().to_owned(),
        egui::TextFormat {
            font_id: bold_font(size),
            color: Color32::WHITE,
            line_height: Some(line),
            ..Default::default()
        },
    );
    job.wrap.max_width = wrap;
    painter.layout_job(job)
}

/// The height a row of map cards `width` wide needs: the least, or more for a name that
/// wraps.
fn map_row_height(painter: &Painter, maps: &[&MapCard], width: f32, layout: MapLayout) -> f32 {
    maps.iter()
        .map(|map| {
            let name = map_name_galley(painter, map, width, layout).size().y;
            match layout {
                MapLayout::Narrow { .. } => {
                    (2.0 * NARROW_MAP_PADDING.0 + name).max(NARROW_MAP_CARD_MIN)
                }
                _ => (2.0 * 10.0 + name + MAP_DETAIL_GAP + MAP_DETAIL_LINE).max(MAP_CARD_MIN),
            }
        })
        .fold(0.0, f32::max)
}

/// The lines a difficulty level's detail takes in a level `width` wide, with its height.
fn difficulty_detail(
    painter: &Painter,
    entry: &DifficultyOption,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(
        entry.detail.to_owned(),
        egui::TextFormat {
            font_id: egui::FontId::proportional(12.5),
            color: hex(0xd0e4f3),
            line_height: Some(DIFFICULTY_LINE),
            ..Default::default()
        },
    );
    job.wrap.max_width = paint::wrap_width(
        painter,
        entry.detail,
        egui::FontId::proportional(12.5),
        width - DIFFICULTY_TEXT_INSET,
    );
    painter.layout_job(job)
}

/// A difficulty level's least height in a level `width` wide.
fn difficulty_height(painter: &Painter, entry: &DifficultyOption, width: f32) -> f32 {
    2.0 * DIFFICULTY_PADDING
        + DIFFICULTY_NAME_ROW
        + difficulty_detail(painter, entry, width).size().y
}

/// Rows of `count` cells `height` tall across `width` from `min`, `gap` apart.
fn cells(min: Pos2, width: f32, count: usize, height: f32, gap: f32) -> impl Iterator<Item = Rect> {
    let cell = (width - gap * (count - 1) as f32) / count as f32;
    (0..count).map(move |index| {
        Rect::from_min_size(
            pos2(min.x + index as f32 * (cell + gap), min.y),
            vec2(cell, height),
        )
    })
}

/// Heights shared out: each its least and an equal part of what `total` has more.
fn stretched(least: &[f32], gap: f32, total: f32) -> Vec<f32> {
    let used = least.iter().sum::<f32>() + gap * (least.len() - 1) as f32;
    let share = ((total - used) / least.len() as f32).max(0.0);
    least.iter().map(|height| height + share).collect()
}

/// Where a stacked Battle Setup puts its rows, in the page's CSS pixels from the menu's
/// top-left corner.
struct Stack {
    window: StackWindow,
    size: Vec2,
    content_left: f32,
    content_width: f32,
    /// The logo's origin and scale, when it shows.
    logo: Option<(Pos2, f32)>,
    corner: Rect,
    tabs: [Rect; 2],
    tab_line: TabLine,
    tray: Rect,
    inner_left: f32,
    inner_width: f32,
    tank_top: f32,
    tank_card: f32,
    map_top: f32,
    map_card: f32,
    map_rows: usize,
    panel_top: f32,
    /// The mid-size panel's first row (the choices over GO), shared by both tabs.
    panel_row: f32,
    footer: Option<f32>,
}

/// The stacked layout of the open tab at `width`, for `maps` map cards.
fn stack_layout(
    painter: &Painter,
    window: StackWindow,
    width: f32,
    maps: &[&MapCard],
    rooms: &super::rooms::RoomsState,
) -> Stack {
    let [pad_top, pad_side, pad_bottom] = window.padding();
    let content_left = MENU_BORDER + pad_side;
    let content_width = width - 2.0 * content_left;
    let content_right = content_left + content_width;
    let gap = window.gap();
    let corner_width = corner_plate_width();
    let top = MENU_BORDER + pad_top;

    // The logo (unless the screen is short) and the tabs: beside it, or in one column on
    // a row under it. The Settings plate keeps the tabs' row's right end, or the logo's
    // row's in one column.
    let logo_scale = if window.narrow {
        NARROW_LOGO_SCALE
    } else {
        1.0
    };
    let logo_size = vec2(LOGO_BOX[0], LOGO_BOX[1]) * logo_scale;
    let header = if window.short {
        0.0
    } else {
        LOGO_ABOVE
            + logo_size.y
            + if window.narrow {
                NARROW_LOGO_BELOW
            } else {
                LOGO_BELOW
            }
    };
    let logo = (!window.short).then(|| {
        let x = if window.narrow {
            content_left + (content_width - logo_size.x) / 2.0
        } else {
            content_left
        };
        (pos2(x, top + LOGO_ABOVE), logo_scale)
    });
    let corner_in_logo_row = window.narrow && !window.short;
    let tabs_left = if window.narrow || window.short {
        content_left
    } else {
        content_left + logo_size.x + LOGO_TAB_GAP
    };
    let tabs_right = if corner_in_logo_row {
        content_right
    } else {
        content_right - corner_width - CORNER_GAP
    };
    let tab_width = (tabs_right - tabs_left - TABS_GAP) / 2.0;
    let tab_line = if window.narrow {
        TabLine::Hidden
    } else if PLAY_TABS
        .iter()
        .all(|tab| tab_line_fits(painter, tab_width, *tab))
    {
        TabLine::Beside
    } else {
        TabLine::Below
    };
    let (open_height, closed_height) = window.tab_heights(tab_line);
    let tray_top = if window.narrow {
        top + header + open_height - 1.0
    } else {
        top + header.max(open_height - 1.0)
    };
    let tabs = [0, 1].map(|index| {
        let left = tabs_left + index as f32 * (tab_width + TABS_GAP);
        let open = (index == 1) == rooms.multiplayer;
        if open {
            Rect::from_min_max(
                pos2(left, tray_top + 1.0 - open_height),
                pos2(left + tab_width, tray_top + 1.0),
            )
        } else {
            Rect::from_min_max(
                pos2(left, tray_top - closed_height),
                pos2(left + tab_width, tray_top),
            )
        }
    });
    // In the tabs' row the plate keeps the same gap from the tray under it as from the
    // tabs beside it: centred on the open tab, which reaches down into the tray, its
    // edge all but touched the tray's.
    let corner_top = if corner_in_logo_row {
        top + LOGO_ABOVE + (logo_size.y - CORNER_PLATE_HEIGHT) / 2.0
    } else {
        let centred = tabs[usize::from(rooms.multiplayer)].center().y - CORNER_PLATE_HEIGHT / 2.0;
        centred.min(tray_top - CORNER_GAP - CORNER_PLATE_HEIGHT)
    };
    let corner = Rect::from_min_size(
        pos2(content_right - corner_width, corner_top),
        vec2(corner_width, CORNER_PLATE_HEIGHT),
    );

    // The tray: the tanks, the maps, and the open tab's panel.
    let inner_left = content_left + TRAY_INSET;
    let inner_width = content_width - 2.0 * TRAY_INSET;
    let mut y = tray_top + TRAY_INSET;
    let tank_top = y;
    let tank_card = window.tank_rows().height();
    y += HEADING_ROW + 2.0 * WELL_PADDING + tank_card + gap;
    let map_top = y;
    let map_rows = maps.len().div_ceil(3).max(1);
    let map_width = (inner_width - 2.0 * WELL_PADDING - 2.0 * WELL_GAP) / 3.0;
    let map_card = maps
        .chunks(3)
        .map(|row| map_row_height(painter, row, map_width, window.map_layout()))
        .fold(0.0, f32::max);
    y += HEADING_ROW
        + 2.0 * WELL_PADDING
        + map_rows as f32 * map_card
        + (map_rows - 1) as f32 * WELL_GAP
        + gap;
    let panel_top = y;
    let column = (inner_width - 2.0 * PANEL_COLUMN_GAP) / 3.0;
    let well = column - 2.0 * WELL_PADDING;
    let difficulty_column = HEADING_ROW
        + 2.0 * WELL_PADDING
        + 2.0 * WELL_GAP
        + DIFFICULTY_OPTIONS
            .iter()
            .map(|entry| difficulty_height(painter, entry, well))
            .sum::<f32>();
    let driver_column = 2.0 * HEADING_ROW
        + super::rooms::NAME_FIELD_HEIGHT
        + PANEL_ROW_GAP
        + super::rooms::team_well_height(painter, column);
    let panel_row = difficulty_column.max(driver_column);
    let panel = if !window.narrow {
        panel_row + PANEL_ROW_GAP + ACTION_HEIGHT
    } else if rooms.multiplayer {
        HEADING_ROW
            + super::rooms::room_list_height(painter, rooms, inner_width)
            + PANEL_ROW_GAP
            + HEADING_ROW
            + super::rooms::NAME_FIELD_HEIGHT
            + PANEL_ROW_GAP
            + HEADING_ROW
            + super::rooms::team_well_height(painter, inner_width)
            + 2.0 * (PANEL_ROW_GAP + ACTION_HEIGHT)
    } else {
        HEADING_ROW
            + 2.0 * WELL_PADDING
            + NARROW_BATTLE_HEIGHT
            + PANEL_ROW_GAP
            + HEADING_ROW
            + 2.0 * WELL_PADDING
            + NARROW_DIFFICULTY_HEIGHT
            + 2.0 * PANEL_ROW_GAP
            + STATUS_ROW
            + PANEL_ROW_GAP
            + ACTION_HEIGHT
    };
    y += panel + TRAY_INSET;
    let tray = Rect::from_min_max(pos2(content_left, tray_top), pos2(content_right, y));
    let footer = (window.key_strip && !window.narrow).then(|| {
        let line = y + gap;
        y = line + FOOTER_PADDING + key_strip_height(painter, content_width);
        line
    });
    y += pad_bottom + MENU_BORDER;
    Stack {
        window,
        size: vec2(width, y),
        content_left,
        content_width,
        logo,
        corner,
        tabs,
        tab_line,
        tray,
        inner_left,
        inner_width,
        tank_top,
        tank_card,
        map_top,
        map_card,
        map_rows,
        panel_top,
        panel_row,
        footer,
    }
}

/// Battle Setup under the garage's width on the page and in the iPad app (`style.css`
/// below 1240 px): the logo shares the tabs' row over one tray holding the tank cards,
/// the maps and the open tab's choices, scaled down to fit a short window as `fitPanels`
/// scales it. At 600 px and under the logo has a row of its own and every choice is a
/// short row of buttons in one column, which scrolls rather than scales: its tabs have
/// heights of their own.
fn stacked_setup(
    context: &egui::Context,
    state: &mut UiState,
    art: &Art,
    view: &View,
    prefs: &mut Preferences,
    actions: &mut Vec<Action>,
) {
    let screen = context.content_rect();
    let window = StackWindow::of(screen);
    let multiplayer = state.rooms.multiplayer;
    // A linked room's extra level shows even where it is not offered.
    let room_map = state.rooms.selected_map().filter(|_| multiplayer);
    let maps: Vec<&MapCard> = MAP_CARDS
        .iter()
        .filter(|map| view.debug || !map.extra() || room_map == Some(map.id))
        .collect();
    let measure = context.layer_painter(LayerId::new(Order::Middle, Id::unique("stack measure")));
    if window.narrow {
        let width = screen.width() - 2.0 * NARROW_OVERLAY_PADDING;
        let stack = stack_layout(&measure, window, width, &maps, &state.rooms);
        let page = stack.size.y + 2.0 * NARROW_OVERLAY_PADDING;
        egui::Area::new(Id::unique("narrow battle setup"))
            .fixed_pos(screen.min)
            .order(Order::Middle)
            .show(context, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(screen.height())
                    .min_scrolled_height(screen.height())
                    .show(ui, |ui| {
                        let (canvas, _) =
                            ui.allocate_exact_size(vec2(screen.width(), page), Sense::hover());
                        let origin = canvas.min + Vec2::splat(NARROW_OVERLAY_PADDING);
                        draw_stack(
                            ui, &stack, origin, state, art, view, prefs, &maps, room_map, actions,
                        );
                    });
            });
        // The build line keeps the screen's corner only where the column leaves it clear.
        if page + 20.0 <= screen.height() {
            build_label(context, screen, view);
        }
        return;
    }
    // As `fitPanels`: lay out at the window's width; where that is too tall, scale it to
    // fit and lay it out again as wide as the scaled window allows.
    let room = screen.height() - FIT_MARGIN;
    let container = screen.width() - 2.0 * OVERLAY_PADDING;
    let first = stack_layout(
        &measure,
        window,
        container.min(STACK_MAX_WIDTH),
        &maps,
        &state.rooms,
    );
    let mut scale = (room / first.size.y).min(1.0);
    let stack = if scale < 1.0 {
        let stack = stack_layout(
            &measure,
            window,
            (container / scale).min(STACK_MAX_WIDTH),
            &maps,
            &state.rooms,
        );
        scale = scale.min(room / stack.size.y);
        stack
    } else {
        first
    };
    let offset = screen.center() - stack.size * scale / 2.0;
    let id = Id::unique("stacked battle setup");
    context.set_transform_layer(
        LayerId::new(Order::Middle, id),
        egui::emath::TSTransform::new(offset.to_vec2(), scale),
    );
    egui::Area::new(id)
        .fixed_pos(pos2(0.0, 0.0))
        .order(Order::Middle)
        .constrain(false)
        .show(context, |ui| {
            let canvas = Rect::from_min_size(pos2(0.0, 0.0), stack.size);
            // Room round the menu for its soft shadow.
            ui.set_clip_rect(canvas.expand(SHADOW_ROOM));
            let _ = ui.allocate_rect(canvas, Sense::hover());
            draw_stack(
                ui, &stack, canvas.min, state, art, view, prefs, &maps, room_map, actions,
            );
        });
    // The build line keeps the screen's corner where the menu leaves it clear.
    let menu = Rect::from_min_size(offset, stack.size * scale);
    let label = Rect::from_min_max(screen.right_bottom() - vec2(140.0, 30.0), screen.max);
    if menu.right() <= label.left() || menu.bottom() <= label.top() {
        build_label(context, screen, view);
    }
}

/// The build line in the screen's bottom-right corner, outside Battle Setup.
fn build_label(context: &egui::Context, screen: Rect, view: &View) {
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

/// Draw a stacked Battle Setup laid out as `stack`, its menu's top-left at `origin`.
#[allow(clippy::too_many_arguments)]
fn draw_stack(
    ui: &mut egui::Ui,
    stack: &Stack,
    origin: Pos2,
    state: &mut UiState,
    art: &Art,
    view: &View,
    prefs: &mut Preferences,
    maps: &[&MapCard],
    room_map: Option<sloppy_core::sim::map_options::MapId>,
    actions: &mut Vec<Action>,
) {
    let window = stack.window;
    let time = ui.input(|input| input.time);
    let human_team = state.hud.as_ref().map_or(Team::Blue, |hud| hud.human_team);
    let options = prefs.game_options(human_team, view.debug);
    let multiplayer = state.rooms.multiplayer;
    let at = |x: f32, y: f32| origin + vec2(x, y);
    let place = |rect: Rect| rect.translate(origin.to_vec2());
    let painter = ui.painter().clone();
    sheet_glass(&painter, Rect::from_min_size(origin, stack.size));
    if let Some((logo, scale)) = stack.logo {
        art.logo_scaled(&painter, origin + logo.to_vec2(), scale);
    }
    corner_buttons(ui, art, place(stack.corner), actions);

    // The tray under its tabs, the open one covering its edge.
    let tray_rect = place(stack.tray);
    let tabs = stack.tabs.map(place);
    tray(
        &painter,
        tray_rect,
        tabs[0].left() <= tray_rect.left() + 0.5,
        tabs[1].right() >= tray_rect.right() - 0.5,
    );
    let dress = window.tab_dress(stack.tab_line);
    for (tab, rect) in PLAY_TABS.into_iter().zip(tabs) {
        if play_tab(ui, &painter, rect, tab, multiplayer == tab.0, dress) {
            state.rooms.multiplayer = tab.0;
        }
    }
    let inner_left = origin.x + stack.inner_left;
    let inner_width = stack.inner_width;
    let inner_right = inner_left + inner_width;

    // YOUR TANK: three cards in a well.
    let tank_label = at(stack.inner_left, stack.tank_top);
    paint::section_label(&painter, tank_label, "YOUR TANK");
    let tank_well = Rect::from_min_size(
        tank_label + vec2(0.0, HEADING_ROW),
        vec2(inner_width, stack.tank_card + 2.0 * WELL_PADDING),
    );
    painter.rect_filled(tank_well, 14, hexa(0x071328, 0.85));
    for (tank, rect) in TANK_CARDS.iter().zip(cells(
        tank_well.min + Vec2::splat(WELL_PADDING),
        inner_width - 2.0 * WELL_PADDING,
        3,
        stack.tank_card,
        WELL_GAP,
    )) {
        let response = ui
            .interact(rect, Id::unique(("setup tank", tank.kind)), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        paint::labelled_choice(&response, true, prefs.tank == tank.kind, tank.name());
        stack_tank_card(
            &painter,
            art,
            rect,
            tank,
            human_team,
            prefs.tank == tank.kind,
            response.hovered(),
            window.tank_rows(),
        );
        if response.clicked() {
            prefs.tank = tank.kind;
        }
    }

    // MAP: an open room chosen on the Multiplayer tab shows its own.
    let map_label = at(stack.inner_left, stack.map_top);
    let heading = paint::section_label(&painter, map_label, "MAP");
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
    let map_well = Rect::from_min_size(
        map_label + vec2(0.0, HEADING_ROW),
        vec2(
            inner_width,
            2.0 * WELL_PADDING + stack.map_rows as f32 * (stack.map_card + WELL_GAP) - WELL_GAP,
        ),
    );
    painter.rect_filled(map_well, 14, hexa(0x071328, 0.85));
    for (row, chunk) in maps.chunks(3).enumerate() {
        let top = map_well.top() + WELL_PADDING + row as f32 * (stack.map_card + WELL_GAP);
        for (map, rect) in chunk.iter().zip(cells(
            pos2(map_well.left() + WELL_PADDING, top),
            inner_width - 2.0 * WELL_PADDING,
            3,
            stack.map_card,
            WELL_GAP,
        )) {
            let locked = room_map.is_some();
            let id = Id::unique(("setup map", map.id));
            let response = if locked {
                ui.interact(rect, id, Sense::hover())
            } else {
                ui.interact(rect, id, Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
            };
            paint::labelled_choice(&response, !locked, shown_map == map.id, map_name(map.id));
            map_card(
                &painter,
                art,
                rect,
                map,
                shown_map == map.id,
                !locked && response.hovered(),
                window.map_layout(),
            );
            if !locked && response.clicked() {
                prefs.map = map.id;
            }
        }
    }

    // The open tab's panel: three columns, or one.
    let panel_top = origin.y + stack.panel_top;
    let column = (inner_width - 2.0 * PANEL_COLUMN_GAP) / 3.0;
    let wide_column = 2.0 * column + PANEL_COLUMN_GAP;
    let right_column = inner_right - column;
    if multiplayer {
        let layout = if window.narrow {
            let list = super::rooms::room_list_height(&painter, &state.rooms, inner_width);
            let room_list = Rect::from_min_size(
                pos2(inner_left, panel_top + HEADING_ROW),
                vec2(inner_width, list),
            );
            let name_field = Rect::from_min_size(
                pos2(inner_left, room_list.bottom() + PANEL_ROW_GAP + HEADING_ROW),
                vec2(inner_width, super::rooms::NAME_FIELD_HEIGHT),
            );
            let team_well = Rect::from_min_size(
                pos2(
                    inner_left,
                    name_field.bottom() + PANEL_ROW_GAP + HEADING_ROW,
                ),
                vec2(
                    inner_width,
                    super::rooms::team_well_height(&painter, inner_width),
                ),
            );
            let status_well = Rect::from_min_size(
                pos2(inner_left, team_well.bottom() + PANEL_ROW_GAP),
                vec2(inner_width, ACTION_HEIGHT),
            );
            super::rooms::MultiplayerLayout {
                rooms_heading: pos2(inner_left, panel_top),
                room_list,
                rules_below: super::rooms::rules_below(inner_width - 2.0 * WELL_PADDING),
                status_well,
                name_field,
                team_well,
                action: status_well.translate(vec2(0.0, ACTION_HEIGHT + PANEL_ROW_GAP)),
            }
        } else {
            let row = stack.panel_row;
            let action_top = panel_top + row + PANEL_ROW_GAP;
            let name_field = Rect::from_min_size(
                pos2(right_column, panel_top + HEADING_ROW),
                vec2(column, super::rooms::NAME_FIELD_HEIGHT),
            );
            super::rooms::MultiplayerLayout {
                rooms_heading: pos2(inner_left, panel_top),
                room_list: Rect::from_min_max(
                    pos2(inner_left, panel_top + HEADING_ROW),
                    pos2(inner_left + wide_column, panel_top + row),
                ),
                rules_below: super::rooms::rules_below(wide_column - 2.0 * WELL_PADDING),
                status_well: Rect::from_min_size(
                    pos2(inner_left, action_top),
                    vec2(wide_column, ACTION_HEIGHT),
                ),
                name_field,
                team_well: Rect::from_min_max(
                    pos2(
                        right_column,
                        name_field.bottom() + PANEL_ROW_GAP + HEADING_ROW,
                    ),
                    pos2(inner_right, panel_top + row),
                ),
                action: Rect::from_min_size(
                    pos2(right_column, action_top),
                    vec2(column, ACTION_HEIGHT),
                ),
            }
        };
        super::rooms::multiplayer(ui, art, &mut state.rooms, view, prefs, &layout, actions);
    } else {
        // BATTLE: posters down the panel beside DIFFICULTY and GO, or one column's
        // short buttons over the levels, the status line and GO.
        let (battle_well, difficulty_label, difficulty_well, status_right, go, go_lettering) =
            if window.narrow {
                let battle_well = Rect::from_min_size(
                    pos2(inner_left, panel_top + HEADING_ROW),
                    vec2(inner_width, NARROW_BATTLE_HEIGHT + 2.0 * WELL_PADDING),
                );
                let difficulty_label = pos2(inner_left, battle_well.bottom() + PANEL_ROW_GAP);
                let difficulty_well = Rect::from_min_size(
                    difficulty_label + vec2(0.0, HEADING_ROW),
                    vec2(inner_width, NARROW_DIFFICULTY_HEIGHT + 2.0 * WELL_PADDING),
                );
                let status_y = difficulty_well.bottom() + PANEL_ROW_GAP + STATUS_ROW / 2.0;
                let go = Rect::from_min_size(
                    pos2(inner_left, status_y + STATUS_ROW / 2.0 + PANEL_ROW_GAP),
                    vec2(inner_width, ACTION_HEIGHT),
                );
                let status_width = status_line_width(&painter, state);
                (
                    battle_well,
                    difficulty_label,
                    difficulty_well,
                    pos2(
                        inner_left + (inner_width + status_width) / 2.0,
                        status_y - STATUS_TEXT_DROP,
                    ),
                    go,
                    NARROW_GO_LETTERING,
                )
            } else {
                let row = stack.panel_row;
                let battle_well = Rect::from_min_max(
                    pos2(inner_left, panel_top + HEADING_ROW),
                    pos2(
                        inner_left + wide_column,
                        panel_top + row + PANEL_ROW_GAP + ACTION_HEIGHT,
                    ),
                );
                (
                    battle_well,
                    pos2(right_column, panel_top),
                    Rect::from_min_max(
                        pos2(right_column, panel_top + HEADING_ROW),
                        pos2(inner_right, panel_top + row),
                    ),
                    pos2(battle_well.right(), panel_top + 4.0),
                    Rect::from_min_size(
                        pos2(right_column, panel_top + row + PANEL_ROW_GAP),
                        vec2(column, ACTION_HEIGHT),
                    ),
                    GO_LETTERING,
                )
            };
        paint::section_label(&painter, pos2(inner_left, panel_top), "BATTLE");
        status_line(&painter, status_right, state, time);
        painter.rect_filled(battle_well, 14, hexa(0x071328, 0.85));
        let posters = battle_well.shrink(WELL_PADDING);
        for (battle, rect) in BATTLE_FORMATS.iter().zip(cells(
            posters.min,
            posters.width(),
            2,
            posters.height(),
            WELL_GAP,
        )) {
            // An extra level plays its own team battle; Solo Assault waits for a standard
            // map (`showBattleFormat`).
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
            paint::labelled_choice(
                &response,
                unavailable.is_none(),
                options.game_mode == battle.mode,
                battle.name,
            );
            let mut card_painter = painter.clone();
            if unavailable.is_some() {
                card_painter.multiply_opacity(UNAVAILABLE_OPACITY);
            }
            let selected = options.game_mode == battle.mode;
            let hovered = unavailable.is_none() && response.hovered();
            if window.narrow {
                battle_key(
                    &card_painter,
                    rect,
                    battle,
                    selected,
                    hovered,
                    viewport_size(window.viewport_width, 14.0, 4.8, 19.0),
                );
            } else {
                battle_card(
                    &card_painter,
                    art,
                    rect,
                    battle,
                    &battle.detail_on(options.map_mode),
                    selected,
                    hovered,
                    STACKED_TEAM_ART,
                );
            }
            // The format kept for the standard maps changes only by choosing one there,
            // never by the extra level's own team battle.
            if response.clicked() && options.game_mode != battle.mode {
                prefs.game_mode = battle.mode;
            }
        }
        // DIFFICULTY: the levels stacked, each its least height and a share of the
        // column's; one column's in a row of three.
        paint::section_label(&painter, difficulty_label, "DIFFICULTY");
        painter.rect_filled(difficulty_well, 14, hexa(0x071328, 0.85));
        let levels = difficulty_well.shrink(WELL_PADDING);
        let rects: Vec<Rect> = if window.narrow {
            cells(levels.min, levels.width(), 3, levels.height(), WELL_GAP).collect()
        } else {
            let least: Vec<f32> = DIFFICULTY_OPTIONS
                .iter()
                .map(|entry| difficulty_height(&painter, entry, levels.width()))
                .collect();
            let mut top = levels.top();
            stretched(&least, WELL_GAP, levels.height())
                .into_iter()
                .map(|height| {
                    let rect =
                        Rect::from_min_size(pos2(levels.left(), top), vec2(levels.width(), height));
                    top = rect.bottom() + WELL_GAP;
                    rect
                })
                .collect()
        };
        for ((index, entry), rect) in DIFFICULTY_OPTIONS.iter().enumerate().zip(rects) {
            let response = ui
                .interact(
                    rect,
                    Id::unique(("setup difficulty", entry.difficulty)),
                    Sense::click(),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            paint::labelled_choice(
                &response,
                true,
                prefs.difficulty == entry.difficulty,
                entry.name,
            );
            difficulty_segment(
                &painter,
                rect,
                index,
                entry,
                prefs.difficulty == entry.difficulty,
                response.hovered(),
                window
                    .narrow
                    .then(|| viewport_size(window.viewport_width, 13.0, 4.4, 19.0)),
            );
            if response.clicked() {
                prefs.difficulty = entry.difficulty;
            }
        }
        go_button(ui, art, go, state, go_lettering, actions);
    }

    // The key strip under a hairline.
    if let Some(line) = stack.footer {
        let line = origin.y + line;
        let left = origin.x + stack.content_left;
        painter.line_segment(
            [pos2(left, line), pos2(left + stack.content_width, line)],
            Stroke::new(1.0, hexa(0x8bb9dc, 0.12)),
        );
        key_strip(
            &painter,
            pos2(
                left + stack.content_width / 2.0,
                line + FOOTER_PADDING + KEY_LINE / 2.0,
            ),
            stack.content_width,
        );
    }
}

/// GO, or LOADING… while the round starts, in `rect`.
fn go_button(
    ui: &mut egui::Ui,
    art: &Art,
    rect: Rect,
    state: &UiState,
    lettering: paint::Lettering,
    actions: &mut Vec<Action>,
) {
    let label = if state.starting { "LOADING…" } else { "GO!" };
    if paint::action_button(ui, art, rect, label, lettering, !state.starting).clicked() {
        actions.push(Action::Start);
    }
}

/// A phone's Battle Setup: the sheet's distance from the screen's edges, its padding,
/// and the room it keeps between rows and inside the tray.
const PHONE_MARGIN: f32 = 8.0;
const PHONE_PADDING: f32 = 12.0;
const PHONE_GAP: f32 = 8.0;
const PHONE_TRAY_PADDING: f32 = 10.0;
/// Its rows: a section heading, the play tabs, the battle formats' least and most height
/// upright, and GO (taller under an upright phone's thumb). A primary button this tall
/// takes the display face.
const PHONE_LABEL: f32 = 26.0;
const PHONE_TABS: f32 = 50.0;
const PHONE_FORMATS: (f32, f32) = (50.0, 72.0);
const PHONE_GO: f32 = 52.0;
const PHONE_UPRIGHT_GO: f32 = 64.0;
/// The logo's box over an upright phone's sheet.
const PHONE_LOGO: Vec2 = vec2(104.0, 56.0);
/// The Home Screen tip's line in an iPhone's browser.
const PHONE_TIP: f32 = 18.0;
/// Tank and map cards' least and most height, upright (narrow tank cards, maps with
/// their lines where they have the room) and sideways (short cards with their names
/// alone). Upright the least fits an iPhone's Safari with its bars (393×659).
const PHONE_UPRIGHT_TANK: (f32, f32) = (110.0, 200.0);
const PHONE_UPRIGHT_MAP: (f32, f32) = (48.0, 76.0);
const PHONE_SIDEWAYS_TANK: (f32, f32) = (56.0, 90.0);
const PHONE_SIDEWAYS_MAP: (f32, f32) = (40.0, 48.0);
/// A phone's tank card at least this tall stands its labels over the tank.
const PHONE_TALL_CARD: f32 = 110.0;
/// A tall phone card's padding (above and below, at the sides), its name's line, its
/// class tag and the gap between its rows (`.phone .menu.start .vehicle`).
const PHONE_CARD_PADDING: [f32; 2] = [10.0, 12.0];
const PHONE_NAME_LINE: f32 = 20.4;
const PHONE_TAG_HEIGHT: f32 = 17.6;
const PHONE_ROW_GAP: f32 = 4.0;
/// Its hull and speed lines: a label's line and its bar, the two rows' whole height with
/// production's gaps and closed up, and the least box the tank keeps over them.
const PHONE_SPEC_LABEL: f32 = 12.6;
const PHONE_SPEC_BAR: f32 = 4.0;
const PHONE_SPEC: f32 = 48.2;
const PHONE_TIGHT_SPEC: f32 = 37.2;
const PHONE_LEAST_STAGE: f32 = 40.0;
/// The tank's box, and how much larger than their class's scale the tanks fill it; the
/// heavy one takes a smaller scale so it stays inside its narrow card.
const PHONE_PREVIEW: f32 = 82.0;
const PHONE_TANK_GROWTH: f32 = 1.22;
const PHONE_HEAVY_SCALE: f32 = 0.88;

/// A phone's play tabs (`.phone .play-tab`): the name alone at 17 px, and held sideways
/// a small round badge before it.
fn phone_tab_dress(upright: bool) -> TabDress {
    TabDress {
        badge: (!upright).then_some(20.0),
        title_size: 17.0,
        line: TabLine::Hidden,
        padding_left: 15.0,
        open_padding: (10.0, 9.0),
        closed_padding: (6.0, 11.0),
    }
}

/// Battle Setup on a phone (`.phone` rules in `style.css`): the tank, the map and the
/// battle format, single player's bots always Easy, and multiplayer one action that joins
/// the busiest open room or creates one; no difficulty, room list, name or Settings.
/// Held upright the tank comes first, over the tabs, the maps stack, and GO sits at the
/// bottom under the thumb; held sideways the tabs head the sheet and each choice is one
/// row, GO beside the formats.
fn phone_setup(
    context: &egui::Context,
    state: &mut UiState,
    art: &Art,
    view: &View,
    prefs: &mut Preferences,
    actions: &mut Vec<Action>,
) {
    let screen = context.content_rect();
    let time = context.input(|input| input.time);
    let upright = screen.height() > screen.width();
    let human_team = state.hud.as_ref().map_or(Team::Blue, |hud| hud.human_team);
    let options = prefs.game_options(human_team, view.debug);
    let multiplayer = state.rooms.multiplayer;
    let maps: Vec<&MapCard> = MAP_CARDS
        .iter()
        .filter(|map| view.debug || !map.extra())
        .collect();
    let inset = PHONE_MARGIN + PHONE_PADDING;
    let width = screen.width() - 2.0 * inset;
    let go_height = if upright { PHONE_UPRIGHT_GO } else { PHONE_GO };
    let (tank_range, map_rows, map_range) = if upright {
        (PHONE_UPRIGHT_TANK, maps.len(), PHONE_UPRIGHT_MAP)
    } else {
        (
            PHONE_SIDEWAYS_TANK,
            maps.len().div_ceil(3),
            PHONE_SIDEWAYS_MAP,
        )
    };
    let maps_height = |map: f32| map_rows as f32 * (map + CARD_GAP) - CARD_GAP;
    // Everything but the rows that grow: upright the logo, the tanks, the tabs and the
    // tray of maps, formats and GO; sideways the tabs over a tray of tanks, maps and the
    // formats beside GO.
    let tip = if crate::touch::home_screen_tip() {
        PHONE_TIP
    } else {
        0.0
    };
    let fixed = 2.0 * inset + PHONE_TABS - 1.0
        + tip
        + 2.0 * PHONE_TRAY_PADDING
        + 2.0 * (PHONE_LABEL + PHONE_GAP)
        + go_height
        // The tanks' and the maps' wells, and upright the formats'.
        + 2.0 * WELL_PADDING * if upright { 3.0 } else { 2.0 }
        + if upright {
            PHONE_LOGO.y + 3.0 * PHONE_GAP + PHONE_FORMATS.0
        } else {
            0.0
        };
    // What the screen has beyond the least: the tanks grow, then the maps, then upright
    // the formats; what is left goes round the logo, so GO stays at the bottom with no
    // empty band over it. Sideways the sheet centres.
    let least = fixed + tank_range.0 + maps_height(map_range.0);
    let room = (screen.height() - least).max(0.0);
    let mut spare = room;
    let mut grow = |most: f32| {
        let step = spare.min(most);
        spare -= step;
        step
    };
    let tank_height = tank_range.0 + grow(tank_range.1 - tank_range.0);
    let rows = map_rows as f32;
    let map_height = map_range.0 + grow((map_range.1 - map_range.0) * rows) / rows;
    let (formats_height, logo_band) = if upright {
        let formats = PHONE_FORMATS.0 + grow(PHONE_FORMATS.1 - PHONE_FORMATS.0);
        (formats, PHONE_LOGO.y + grow(f32::INFINITY))
    } else {
        (PHONE_FORMATS.0, 0.0)
    };
    let needed = least + room - spare;
    let page = needed.max(screen.height());
    let top = (page - needed) / 2.0;
    egui::Area::new(Id::unique("phone battle setup"))
        .fixed_pos(screen.min)
        .order(Order::Middle)
        .show(context, |ui| {
            egui::ScrollArea::vertical()
                .max_height(screen.height())
                .min_scrolled_height(screen.height())
                .show(ui, |ui| {
                    let (canvas, _) =
                        ui.allocate_exact_size(vec2(screen.width(), page), Sense::hover());
                    let painter = ui.painter().clone();
                    let sheet = Rect::from_min_size(
                        pos2(canvas.left(), canvas.top() + top),
                        vec2(screen.width(), needed),
                    )
                    .shrink(PHONE_MARGIN);
                    sheet_glass(&painter, sheet);
                    let left = sheet.left() + PHONE_PADDING;
                    let mut y = sheet.top() + PHONE_PADDING;
                    let cells = |y: f32, left: f32, width: f32, count: usize, height: f32| {
                        let cell = (width - CARD_GAP * (count - 1) as f32) / count as f32;
                        (0..count).map(move |index| {
                            Rect::from_min_size(
                                pos2(left + index as f32 * (cell + CARD_GAP), y),
                                vec2(cell, height),
                            )
                        })
                    };
                    let well = |rect: Rect| painter.rect_filled(rect, 14, hexa(0x071328, 0.85));
                    let mut tank_row = |ui: &mut egui::Ui, y: f32, left: f32, width: f32| {
                        paint::section_label(&painter, pos2(left, y), "YOUR TANK");
                        let y = y + PHONE_LABEL;
                        well(Rect::from_min_size(
                            pos2(left, y),
                            vec2(width, tank_height + 2.0 * WELL_PADDING),
                        ));
                        for (tank, rect) in TANK_CARDS.iter().zip(cells(
                            y + WELL_PADDING,
                            left + WELL_PADDING,
                            width - 2.0 * WELL_PADDING,
                            3,
                            tank_height,
                        )) {
                            let response = ui.interact(
                                rect,
                                Id::unique(("setup tank", tank.kind)),
                                Sense::click(),
                            );
                            paint::labelled_choice(
                                &response,
                                true,
                                prefs.tank == tank.kind,
                                tank.name(),
                            );
                            phone_tank_card(
                                &painter,
                                art,
                                rect,
                                tank,
                                human_team,
                                prefs.tank == tank.kind,
                            );
                            if response.clicked() {
                                prefs.tank = tank.kind;
                            }
                        }
                        y + tank_height + 2.0 * WELL_PADDING + PHONE_GAP
                    };
                    // An iPhone's browser suggests the Home Screen: a line under the logo,
                    // since it is about the whole game; sideways it alone heads the sheet.
                    let home_screen_tip = |y: f32| {
                        if tip > 0.0 {
                            paint::text(
                                &painter,
                                pos2(canvas.center().x, y + tip / 2.0),
                                Align2::CENTER_CENTER,
                                "Play full screen: Share, then Add to Home Screen",
                                egui::FontId::proportional(13.0),
                                TEXT_LABEL,
                                0.0,
                            );
                        }
                        y + tip
                    };
                    if upright {
                        art.logo(
                            &painter,
                            pos2(
                                canvas.center().x - PHONE_LOGO.x / 2.0,
                                y + (logo_band - PHONE_LOGO.y) / 2.0,
                            ),
                        );
                        y = home_screen_tip(y + logo_band) + PHONE_GAP;
                        y = tank_row(ui, y, left, width) + PHONE_GAP;
                    } else {
                        y = home_screen_tip(y);
                    }
                    // The play tabs span the sheet's top edge over the tray.
                    let tab_width = (width - TABS_GAP) / 2.0;
                    for (index, tab) in PLAY_TABS.into_iter().enumerate() {
                        let open = multiplayer == tab.0;
                        let tab_left = left + index as f32 * (width - tab_width);
                        // The other tab stands lower, still a thumb's height.
                        let rect = if open {
                            Rect::from_min_size(pos2(tab_left, y), vec2(tab_width, PHONE_TABS))
                        } else {
                            Rect::from_min_size(
                                pos2(tab_left, y + 5.0),
                                vec2(tab_width, PHONE_TABS - 6.0),
                            )
                        };
                        if play_tab(ui, &painter, rect, tab, open, phone_tab_dress(upright)) {
                            state.rooms.multiplayer = tab.0;
                        }
                    }
                    y += PHONE_TABS - 1.0;
                    let tray_rect = Rect::from_min_max(
                        pos2(left, y),
                        pos2(left + width, sheet.bottom() - PHONE_PADDING),
                    );
                    tray(&painter, tray_rect, true, true);
                    let inner = tray_rect.shrink(PHONE_TRAY_PADDING);
                    y = inner.top();
                    if !upright {
                        y = tank_row(ui, y, inner.left(), inner.width());
                    }

                    // MAP: an open room the phone picked shows its own.
                    let heading = paint::section_label(&painter, pos2(inner.left(), y), "MAP");
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
                            egui::FontId::proportional(12.0),
                            hex(0xd3e7f5),
                            0.0,
                        );
                    }
                    y += PHONE_LABEL;
                    let shown_map = room_map.unwrap_or(options.map_mode);
                    let per_row = if upright { 1 } else { 3 };
                    let map_rows_shown = maps.len().div_ceil(per_row) as f32;
                    well(Rect::from_min_size(
                        pos2(inner.left(), y),
                        vec2(
                            inner.width(),
                            map_rows_shown * (map_height + CARD_GAP) - CARD_GAP
                                + 2.0 * WELL_PADDING,
                        ),
                    ));
                    y += WELL_PADDING;
                    for chunk in maps.chunks(per_row) {
                        let row = cells(
                            y,
                            inner.left() + WELL_PADDING,
                            inner.width() - 2.0 * WELL_PADDING,
                            per_row,
                            map_height,
                        );
                        for (map, rect) in chunk.iter().zip(row) {
                            let locked = room_map.is_some();
                            let id = Id::unique(("setup map", map.id));
                            let response = if locked {
                                ui.interact(rect, id, Sense::hover())
                            } else {
                                ui.interact(rect, id, Sense::click())
                            };
                            paint::labelled_choice(
                                &response,
                                !locked,
                                shown_map == map.id,
                                map_name(map.id),
                            );
                            map_card(
                                &painter,
                                art,
                                rect,
                                map,
                                shown_map == map.id,
                                false,
                                if upright {
                                    MapLayout::Phone
                                } else {
                                    MapLayout::Row
                                },
                            );
                            if !locked && response.clicked() {
                                prefs.map = map.id;
                            }
                        }
                        y += map_height + CARD_GAP;
                    }
                    y += WELL_PADDING + PHONE_GAP - CARD_GAP;

                    // The formats (or the room's status) and the action under them, or
                    // held sideways beside them.
                    let (choices, action) = if upright {
                        (
                            Rect::from_min_size(
                                pos2(inner.left(), y),
                                vec2(inner.width(), formats_height + 2.0 * WELL_PADDING),
                            ),
                            Rect::from_min_max(
                                pos2(inner.left(), inner.bottom() - go_height),
                                inner.max,
                            ),
                        )
                    } else {
                        let third = (inner.width() - 2.0 * CARD_GAP) / 3.0;
                        (
                            Rect::from_min_size(
                                pos2(inner.left(), y),
                                vec2(inner.width() - third - CARD_GAP, go_height),
                            ),
                            Rect::from_min_size(
                                pos2(inner.right() - third, y),
                                vec2(third, go_height),
                            ),
                        )
                    };
                    if multiplayer {
                        super::rooms::phone_multiplayer(
                            ui,
                            art,
                            &mut state.rooms,
                            view,
                            prefs,
                            choices,
                            action,
                            actions,
                        );
                        return;
                    }
                    well(choices);
                    let keys = choices.shrink(WELL_PADDING);
                    for (battle, rect) in BATTLE_FORMATS.iter().zip(cells(
                        keys.top(),
                        keys.left(),
                        keys.width(),
                        2,
                        keys.height(),
                    )) {
                        let unavailable = battle.unavailable_on(options.map_mode);
                        let id = Id::unique(("setup battle", battle.mode));
                        let sense = if unavailable.is_some() {
                            Sense::hover()
                        } else {
                            Sense::click()
                        };
                        let response = ui.interact(rect, id, sense);
                        paint::labelled_choice(
                            &response,
                            unavailable.is_none(),
                            options.game_mode == battle.mode,
                            battle.name,
                        );
                        let mut key_painter = painter.clone();
                        if unavailable.is_some() {
                            key_painter.multiply_opacity(UNAVAILABLE_OPACITY);
                        }
                        format_key(
                            &key_painter,
                            rect,
                            battle,
                            options.game_mode == battle.mode,
                            upright,
                        );
                        if response.clicked() && options.game_mode != battle.mode {
                            prefs.game_mode = battle.mode;
                        }
                    }
                    // Upright a phone is a narrow window: GO takes one column's size.
                    let lettering = if upright {
                        NARROW_GO_LETTERING
                    } else {
                        GO_LETTERING
                    };
                    go_button(ui, art, action, state, lettering, actions);
                    phone_status_pill(&painter, action, state, time);
                });
        });
}

/// A phone's tank card (`.phone .start .vehicle`). Upright, a narrow card: the name, the
/// class in a small tag led by its colour's dot, the tank, and its hull and speed as
/// bars the card's width, the only way a new player tells the tanks apart. Sideways, a
/// short card: the name and the tank beside it.
fn phone_tank_card(
    painter: &Painter,
    art: &Art,
    rect: Rect,
    tank: &TankCard,
    team: Team,
    selected: bool,
) {
    let rect = showroom(painter, rect, selected, false);
    let tall = rect.height() >= PHONE_TALL_CARD;
    let [pad_y, pad_x] = PHONE_CARD_PADDING;
    paint::shadowed(
        painter,
        pos2(
            rect.left() + pad_x,
            rect.top() + pad_y + PHONE_NAME_LINE / 2.0,
        ),
        Align2::LEFT_CENTER,
        tank.name(),
        display_font(17.0),
        Color32::WHITE,
        0.5,
    );
    if !tall {
        art.tank(
            &painter.with_clip_rect(rect.shrink(1.0)),
            tank.kind,
            team,
            Rect::from_min_max(
                pos2(rect.left() + rect.width() * 0.4, rect.top() + 6.0),
                pos2(rect.right() - 6.0, rect.bottom() - 4.0),
            ),
            tank.scale * 0.85,
            false,
        );
        showroom_ring(painter, rect, selected);
        return;
    }
    // The class in one word in a small tag, led by a dot in its colour.
    let class = tank.short_class.to_uppercase();
    let font = bold_font(10.5);
    let tag = Rect::from_min_size(
        pos2(
            rect.left() + pad_x,
            rect.top() + pad_y + PHONE_NAME_LINE + PHONE_ROW_GAP,
        ),
        vec2(
            5.0 + 5.0 + 4.0 + paint::text_width(painter, &class, font.clone(), 0.2) + 6.0,
            PHONE_TAG_HEIGHT,
        ),
    );
    painter.rect_filled(tag, 4, hexa(0xffffff, 0.06));
    painter.rect_stroke(
        tag,
        4,
        Stroke::new(1.0, hexa(0xffffff, 0.1)),
        StrokeKind::Inside,
    );
    painter.circle_filled(pos2(tag.left() + 7.5, tag.center().y), 2.5, hex(tank.color));
    paint::text(
        painter,
        pos2(tag.left() + 14.0, tag.center().y),
        Align2::LEFT_CENTER,
        &class,
        font,
        hex(0xdbe8f4),
        0.2,
    );
    // Hull and speed, each a label over a bar the card's width, their rows closing up
    // where a short screen leaves the tank little room.
    let band = rect.height() - (tag.bottom() - rect.top()) - PHONE_ROW_GAP - pad_y;
    let roomy = ((band - PHONE_LEAST_STAGE - PHONE_TIGHT_SPEC) / (PHONE_SPEC - PHONE_TIGHT_SPEC))
        .clamp(0.0, 1.0);
    let label_gap = 1.0 + 4.0 * roomy;
    let row_gap = 2.0 + 3.0 * roomy;
    let row = PHONE_SPEC_LABEL + label_gap + PHONE_SPEC_BAR;
    let spec_top = rect.bottom() - pad_y - 2.0 * row - row_gap;
    for (index, (label, share)) in [
        (format!("{} HP", tank.hit_points()), tank.hit_points_share()),
        (format!("{} KM/H", tank.km_per_hour()), tank.speed_share()),
    ]
    .into_iter()
    .enumerate()
    {
        let top = spec_top + index as f32 * (row + row_gap);
        paint::text(
            painter,
            pos2(rect.left() + pad_x, top + PHONE_SPEC_LABEL / 2.0),
            Align2::LEFT_CENTER,
            &label,
            bold_font(10.5),
            hex(0xdbe9f6),
            0.0,
        );
        let track = Rect::from_min_size(
            pos2(rect.left() + pad_x, top + PHONE_SPEC_LABEL + label_gap),
            vec2(rect.width() - 2.0 * pad_x, PHONE_SPEC_BAR),
        );
        painter.rect_filled(track, 2, hexa(0xffffff, 0.09));
        let (from, to) = if index == 0 {
            (hex(0x6fc6ff), hex(0xb8e4ff))
        } else {
            (GOLD, super::theme::GOLD_HOT)
        };
        painter.add(paint::hgradient(
            Rect::from_min_size(track.min, vec2(track.width() * share, track.height())),
            &[(0.0, from), (1.0, to)],
        ));
    }
    // The tank fills its box between the tag and the bars, well clear of the card's
    // edges: the heavy one a little smaller than its class's scale elsewhere.
    let between = Rect::from_min_max(
        pos2(rect.left() + pad_x, tag.bottom() + PHONE_ROW_GAP),
        pos2(rect.right() - pad_x, spec_top - PHONE_ROW_GAP),
    );
    let stage = Rect::from_center_size(
        between.center() - vec2(0.0, 3.0),
        vec2(between.width(), between.height().min(PHONE_PREVIEW)),
    );
    let class_scale = if tank.kind == VehicleKind::Heavy {
        PHONE_HEAVY_SCALE
    } else {
        tank.scale
    };
    art.tank(
        &painter.with_clip_rect(rect.shrink(1.0)),
        tank.kind,
        team,
        stage,
        class_scale * PHONE_TANK_GROWTH,
        false,
    );
    showroom_ring(painter, rect, selected);
}

/// A phone's battle format: a key with its name alone, which says what it is without the
/// posters' art or a BATTLE heading; centred upright, where the two share the room over
/// GO, and at the left held sideways.
fn format_key(painter: &Painter, rect: Rect, battle: &BattleFormat, selected: bool, upright: bool) {
    choice_frame(painter, rect, selected, false);
    let (at, align) = if upright {
        (rect.center(), Align2::CENTER_CENTER)
    } else {
        (rect.left_center() + vec2(12.0, 0.0), Align2::LEFT_CENTER)
    };
    paint::shadowed(
        painter,
        at,
        align,
        &battle.name.to_uppercase(),
        display_font(19.0),
        Color32::WHITE,
        0.5,
    );
    choice_ring(painter, rect, selected);
}

/// While the arena prepares, a slim pill over GO's top edge with the status and its
/// progress, so it takes no row of its own; gone once the arena is ready.
fn phone_status_pill(painter: &Painter, go: Rect, state: &UiState, time: f64) {
    if state.ready && !state.starting {
        return;
    }
    let status = if state.starting {
        "Rolling out…"
    } else if state.status.is_empty() {
        "Loading game…"
    } else {
        &state.status
    };
    let font = bold_font(11.0);
    let text_width = paint::text_width(painter, status, font.clone(), 0.2);
    let pill = Rect::from_center_size(
        go.center_top(),
        vec2((text_width + 24.0).min(go.width() * 2.0), 26.0),
    );
    painter.rect_filled(pill, 13, hexa(0x050d1c, 0.94));
    painter.rect_stroke(
        pill,
        13,
        Stroke::new(1.0, hexa(0x9fd0ff, 0.18)),
        StrokeKind::Inside,
    );
    paint::text_fit(
        painter,
        pill.center() - vec2(0.0, 2.0),
        Align2::CENTER_CENTER,
        status,
        font,
        hex(0xdbe9f6),
        0.2,
        pill.width() - 16.0,
    );
    let track = Rect::from_min_max(
        pos2(pill.left() + 12.0, pill.bottom() - 6.0),
        pos2(pill.right() - 12.0, pill.bottom() - 4.0),
    );
    painter.rect_filled(track, 1, hexa(0xffffff, 0.1));
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
    painter.rect_filled(fill, 1, GOLD);
}

/// The key strip's caps and what they do (`.menu-help kbd` 12 px, `.key-hint` 13), the
/// gap between its hints, and the height of each of its lines where it wraps.
const KEY_SIZE: f32 = 12.0;
const KEY_LABEL_SIZE: f32 = 13.0;
const KEY_HINT_GAP: f32 = 18.0;
const KEY_LINE: f32 = 28.0;
const KEY_LINE_GAP: f32 = 6.0;

/// The hints the strip shows: the touch line on a touch screen.
fn key_hints() -> &'static [(&'static [&'static str], &'static str)] {
    if crate::touch::screen() {
        &TOUCH_STRIP
    } else {
        &KEY_STRIP
    }
}

/// The key strip's hints in lines no wider than `width`, as their indices.
fn key_strip_lines(painter: &Painter, width: f32) -> Vec<std::ops::Range<usize>> {
    let hint_width = |(keys, label): &(&[&str], &str)| {
        keys.iter()
            .map(|key| paint::kbd_width(painter, key, KEY_SIZE) + 3.0)
            .sum::<f32>()
            + 5.0
            + 2.0
            + paint::text_width(painter, label, bold_font(KEY_LABEL_SIZE), 0.0)
    };
    let mut lines = Vec::new();
    let mut start = 0;
    let mut used = 0.0;
    for (index, hint) in key_hints().iter().enumerate() {
        let wide = hint_width(hint);
        if index > start && used + KEY_HINT_GAP + wide > width {
            lines.push(start..index);
            start = index;
            used = wide;
        } else {
            used += if index > start { KEY_HINT_GAP } else { 0.0 } + wide;
        }
    }
    lines.push(start..key_hints().len());
    lines
}

/// The key strip's height in lines no wider than `width`.
fn key_strip_height(painter: &Painter, width: f32) -> f32 {
    let lines = key_strip_lines(painter, width).len() as f32;
    lines * KEY_LINE + (lines - 1.0) * KEY_LINE_GAP
}

/// The controls' essentials as key caps, each line no wider than `width` and centred,
/// the first on `center`.
fn key_strip(painter: &Painter, center: egui::Pos2, width: f32) {
    let hints = key_hints();
    for (row, line) in key_strip_lines(painter, width).into_iter().enumerate() {
        let y = center.y + row as f32 * (KEY_LINE + KEY_LINE_GAP);
        // Lay the line out once to measure it, then again centred.
        let draw = |painter: &Painter, left: f32| {
            let mut x = left;
            for hint in &hints[line.clone()] {
                for key in hint.0 {
                    let width = paint::kbd_width(painter, key, KEY_SIZE);
                    paint::kbd(painter, pos2(x + width / 2.0, y), key, KEY_SIZE);
                    x += width + 3.0;
                }
                x += 7.0;
                let label = paint::text(
                    painter,
                    pos2(x, y),
                    Align2::LEFT_CENTER,
                    hint.1,
                    bold_font(KEY_LABEL_SIZE),
                    hex(0xdbe8f4),
                    0.0,
                );
                x = label.right() + KEY_HINT_GAP;
            }
            x - KEY_HINT_GAP - left
        };
        let mut measuring = painter.clone();
        measuring.set_invisible();
        let total = draw(&measuring, 0.0);
        draw(painter, center.x - total / 2.0);
    }
}

/// The corner plate's keys and its height.
const CORNER_BUTTON: f32 = 38.0;
const CORNER_PLATE_HEIGHT: f32 = 44.0;

/// The corner plate's keys: their icons and what they do.
fn corner_actions() -> &'static [(&'static str, &'static str)] {
    if crate::QUITS {
        &[("gear", "Settings"), ("power", "Quit Sloppy Tanks")]
    } else {
        &[("gear", "Settings")]
    }
}

/// The corner plate's width for its keys.
fn corner_plate_width() -> f32 {
    CORNER_BUTTON * corner_actions().len() as f32 + 6.0
}

/// Settings and Quit on a small plate at the header's right. An iPad app or a page
/// never quits itself, so there it is Settings alone, at the plate's right end.
fn corner_buttons(ui: &mut egui::Ui, art: &Art, rect: Rect, actions: &mut Vec<Action>) {
    let button = CORNER_BUTTON;
    let painter = ui.painter().clone();
    let buttons = corner_actions();
    let rect = Rect::from_min_max(
        pos2(rect.right() - corner_plate_width(), rect.top()),
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
        paint::labelled(&response, true, tip);
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

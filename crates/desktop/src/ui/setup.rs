//! Battle Setup (`src/game/battle-setup.html`, `start-menu.ts`, `game-options.ts`,
//! `map-picker.ts`): a pane of glass over a dusk battlefield with the logo on top, the
//! tank cards, the map, the battle format and difficulty, the status well and GO.

use egui::{
    Align2, Color32, CornerRadius, Id, LayerId, Mesh, Order, Painter, Rect, Sense, Shape, Stroke,
    StrokeKind, Vec2, pos2, vec2,
};

use super::icons::Art;
use super::paint::{self, ButtonStyle, fade, hex, hexa, mix};
use super::theme::{GOLD, GOLD_HOT, TEXT_DIM, bold_font, display_font};
use super::{Action, UiState, View};

/// A tank a player drives: id, name, class line, hit points, km/h, and each as a share
/// of the best (the card's bars), its class colour and its size in the card.
pub struct Tank {
    pub id: &'static str,
    pub name: &'static str,
    pub class: &'static str,
    /// The class in one word, for the respawn card's smaller cards.
    pub short_class: &'static str,
    pub hit_points: u32,
    pub speed: u32,
    pub hit_points_share: f32,
    pub speed_share: f32,
    pub color: u32,
    pub scale: f32,
}

pub const TANKS: [Tank; 3] = [
    Tank {
        id: "scout",
        name: "SKIPPER",
        class: "LIGHT SCOUT",
        short_class: "SCOUT",
        hit_points: 80,
        speed: 40,
        hit_points_share: 0.57,
        speed_share: 1.0,
        color: 0x5fd8ff,
        scale: 0.8,
    },
    Tank {
        id: "balanced",
        name: "BRUISER",
        class: "BALANCED TANK",
        short_class: "BALANCED",
        hit_points: 100,
        speed: 32,
        hit_points_share: 0.71,
        speed_share: 0.8,
        color: 0xffcf38,
        scale: 0.94,
    },
    Tank {
        id: "heavy",
        name: "BIG RIG",
        class: "HEAVY TANK",
        short_class: "HEAVY",
        hit_points: 140,
        speed: 24,
        hit_points_share: 1.0,
        speed_share: 0.6,
        color: 0xff8a4c,
        scale: 1.1,
    },
];

/// A map choice: id, name, line, symbol and tint (`map-options.ts`).
pub struct MapChoice {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub icon: &'static str,
    pub tint: u32,
}

/// The standard maps: id, name (the app checks a saved map against them).
pub const MAPS: [(&str, &str); 3] = [
    ("village", "Pine Village"),
    ("harbor", "Harbor Havoc"),
    ("quarry", "Dusty Dig"),
];

/// The standard maps' cards.
pub const MAP_CHOICES: [MapChoice; 3] = [
    MapChoice {
        id: "village",
        name: "Pine Village",
        description: "Quiet lanes. Loud tanks.",
        icon: "village",
        tint: 0x2f7d4f,
    },
    MapChoice {
        id: "harbor",
        name: "Harbor Havoc",
        description: "Salt air. Hot steel.",
        icon: "harbor",
        tint: 0x2c6fa8,
    },
    MapChoice {
        id: "quarry",
        name: "Dusty Dig",
        description: "Open pits. Dig a shortcut.",
        icon: "quarry",
        tint: 0xa8733a,
    },
];

/// The extra levels a `--debug` run offers.
pub const EXTRA_MAP_CHOICES: [MapChoice; 2] = [
    MapChoice {
        id: "stress-test",
        name: "Stress Grid",
        description: "30 tanks · 75 destructibles",
        icon: "stress",
        tint: 0x6a5ea8,
    },
    MapChoice {
        id: "superstress",
        name: "Scrap Yard",
        description: "Compact yard · cover rebuilds",
        icon: "yard",
        tint: 0x8a5a44,
    },
];

/// The battle formats: id, name and line.
pub const BATTLES: [(&str, &str, &str); 2] = [
    ("team", "Team Battle", "6 vs 6 · Respawns · First to 100"),
    ("solo", "Solo Assault", "Survive endless enemies"),
];

pub const DIFFICULTIES: [(&str, &str, &str); 3] = [
    (
        "easy",
        "Easy",
        "Slower, less accurate bots · you take 10% less damage",
    ),
    (
        "normal",
        "Normal",
        "Bots aim well and react fast · the intended fight",
    ),
    (
        "hard",
        "Hard",
        "Sharp, fast-reloading bots · you take 15% more damage",
    ),
];

/// The essentials on one line under the dialog; the pause menu lists every control.
const KEY_HINTS: [(&[&str], &str); 6] = [
    (&["W", "A", "S", "D"], "Drive"),
    (&["Mouse"], "Aim"),
    (&["Left click"], "Fire"),
    (&["Right click"], "Mine"),
    (&["Q", "E"], "Ammo"),
    (&["Esc"], "All controls"),
];

/// Room the dialog keeps from the window's edges.
const MARGIN: f32 = 16.0;
const SECTION_GAP: f32 = 14.0;
const LABEL_HEIGHT: f32 = 27.0;

/// The dusk battlefield behind Battle Setup: sky, a warm horizon, ridges, two tanks
/// on watch and a vignette.
fn backdrop(context: &egui::Context, art: &Art) {
    let painter = context.layer_painter(LayerId::new(
        Order::Background,
        Id::unique("setup backdrop"),
    ));
    let screen = context.content_rect();
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

/// The logo: SLOPPY over a bigger gold TANKS, tilted like a stencil slapped on.
fn logo(painter: &Painter, origin: egui::Pos2) -> f32 {
    let angle = (-4.0f32).to_radians();
    let sloppy = paint::galley(
        painter,
        "SLOPPY",
        display_font(27.0),
        Color32::WHITE,
        0.3,
        f32::INFINITY,
    );
    let tanks = paint::galley(
        painter,
        "TANKS",
        display_font(41.0),
        GOLD,
        0.3,
        f32::INFINITY,
    );
    let width = sloppy.size().x.max(tanks.size().x);
    let place =
        |galley: std::sync::Arc<egui::Galley>, pos: egui::Pos2, color: Color32, shadow: Color32| {
            painter.add(
                egui::epaint::TextShape::new(pos + vec2(0.0, 3.0), galley.clone(), shadow)
                    .with_override_text_color(shadow)
                    .with_angle(angle),
            );
            painter.add(egui::epaint::TextShape::new(pos, galley, color).with_angle(angle));
        };
    place(
        sloppy,
        origin + vec2(2.0, 4.0),
        Color32::WHITE,
        hex(0x0b1a30),
    );
    place(tanks, origin + vec2(0.0, 25.0), GOLD, hex(0x7a4a0a));
    width + 6.0
}

/// A tank card: a little showroom with a spotlight on the tank, its name and class,
/// and its hull and speed as bars. The chosen tank glows gold.
#[allow(clippy::too_many_arguments)]
pub fn vehicle_card(
    painter: &Painter,
    art: &Art,
    rect: Rect,
    tank: &Tank,
    team: u8,
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
    let padding = if compact { 10.0 } else { 16.0 };
    let name_size = if compact { 21.0 } else { 26.0 };
    paint::shadowed(
        painter,
        rect.min + vec2(padding, 10.0),
        Align2::LEFT_TOP,
        tank.name,
        display_font(name_size),
        Color32::WHITE,
        0.5,
    );
    // The class tag, led by a dot in the class's colour.
    let class = if compact {
        tank.short_class
    } else {
        tank.class
    };
    let class_font = bold_font(11.0);
    let class_width = paint::text_width(painter, class, class_font.clone(), 0.4) + 26.0;
    let tag = Rect::from_min_size(
        pos2(rect.right() - padding - class_width, rect.top() + 12.0),
        vec2(class_width, 20.0),
    );
    let (tag_fill, tag_ink, dot) = if selected {
        (GOLD, hex(0x1a2233), hex(0x1a2233))
    } else {
        (hexa(0xffffff, 0.07), hex(0xcfe3f5), hex(tank.color))
    };
    painter.rect_filled(tag, 5, tag_fill);
    painter.circle_filled(pos2(tag.left() + 10.0, tag.center().y), 3.0, dot);
    paint::text(
        painter,
        pos2(tag.left() + 18.0, tag.center().y),
        Align2::LEFT_CENTER,
        class,
        class_font,
        tag_ink,
        0.4,
    );
    // The tank, tucked under the name line, sized by its class.
    let spec_height = if compact { 0.0 } else { 30.0 };
    let stage = Rect::from_min_max(
        pos2(
            rect.left() + 6.0,
            rect.top() + if compact { 22.0 } else { 28.0 },
        ),
        pos2(rect.right() - 6.0, rect.bottom() - spec_height - 4.0),
    );
    let scale = tank.scale * if hovered { 1.05 } else { 1.0 };
    painter.add(paint::radial(
        pos2(stage.center().x, stage.bottom() - stage.height() * 0.12),
        vec2(stage.height() * 1.2 * scale, stage.height() * 0.16),
        &[(0.0, hexa(0x020a14, 0.55)), (1.0, Color32::TRANSPARENT)],
    ));
    art.tank(
        &painter.with_clip_rect(rect.shrink(1.0)),
        tank.id,
        team,
        stage,
        scale * 1.05,
    );
    if !compact {
        // Hull and speed as bars that fill by each tank's share of the best.
        let spec_top = rect.bottom() - spec_height - 6.0;
        let half = (rect.width() - 2.0 * padding - 16.0) / 2.0;
        for (index, (label, share)) in [
            (
                format!("{} HIT POINTS", tank.hit_points),
                tank.hit_points_share,
            ),
            (format!("{} KM/H", tank.speed), tank.speed_share),
        ]
        .into_iter()
        .enumerate()
        {
            let x = rect.left() + padding + index as f32 * (half + 16.0);
            paint::text_fit(
                painter,
                pos2(x, spec_top + 6.0),
                Align2::LEFT_TOP,
                &label,
                bold_font(11.0),
                hex(0xdbe9f6),
                1.2,
                half,
            );
            let track = Rect::from_min_size(pos2(x, spec_top + 24.0), vec2(half, 4.0));
            painter.rect_filled(track, 2, hexa(0xffffff, 0.1));
            painter.rect_filled(
                Rect::from_min_size(track.min, vec2(half * share, 4.0)),
                2,
                if selected { GOLD } else { hex(tank.color) },
            );
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

/// A selectable card's frame in its well: lit gold when chosen.
fn choice_frame(painter: &Painter, rect: Rect, selected: bool, hovered: bool) {
    let radius = CornerRadius::same(10);
    if selected {
        painter.rect_filled(rect.expand(3.0), 13, hexa(0xffcf38, 0.1));
    }
    let (top, bottom) = if selected {
        (hex(0x1f4a80), hex(0x0e2546))
    } else if hovered {
        (hex(0x183a6a), hex(0x0c1d3a))
    } else {
        (hex(0x12294d), hex(0x0a1830))
    };
    painter.rect_filled(rect, radius, mix(top, bottom, 0.5));
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
    painter.rect_filled(
        Rect::from_min_max(pos2(rect.left(), rect.bottom() - 10.0), rect.max),
        CornerRadius {
            nw: 0,
            ne: 0,
            sw: 10,
            se: 10,
        },
        bottom,
    );
}

fn choice_ring(painter: &Painter, rect: Rect, selected: bool) {
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

/// A map card: its name and line over its landscape, washed in the map's tint.
fn map_card(
    painter: &Painter,
    art: &Art,
    rect: Rect,
    map: &MapChoice,
    extra: bool,
    selected: bool,
    hovered: bool,
) {
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
        let art_name = format!("art-{}", map.id);
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
    let text_width = rect.width() - 28.0;
    let name = paint::shadowed(
        painter,
        rect.min + vec2(14.0, 12.0),
        Align2::LEFT_TOP,
        &map.name.to_uppercase(),
        display_font(20.0),
        if selected { GOLD_HOT } else { Color32::WHITE },
        0.8,
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
    paint::text_fit(
        painter,
        pos2(rect.left() + 14.0, name.bottom() + 4.0),
        Align2::LEFT_TOP,
        map.description,
        egui::FontId::proportional(12.5),
        hex(0xc9dbeb),
        0.0,
        text_width,
    );
    choice_ring(painter, rect, selected);
}

/// A battle format card: its poster of tanks and its name.
fn battle_card(
    painter: &Painter,
    art: &Art,
    rect: Rect,
    battle: (&str, &str, &str),
    human_team: u8,
    selected: bool,
    hovered: bool,
) {
    let (id, name, detail) = battle;
    choice_frame(painter, rect, selected, hovered);
    let tint = if id == "team" {
        hex(0x3b67a0)
    } else {
        hex(0x9a4d3c)
    };
    painter.with_clip_rect(rect.shrink(1.0)).add(paint::radial(
        pos2(rect.right() - rect.width() * 0.25, rect.center().y),
        vec2(rect.width() * 0.4, rect.height() * 0.9),
        &[(0.0, fade(tint, 0.45)), (1.0, Color32::TRANSPARENT)],
    ));
    let poster = Rect::from_min_max(
        pos2(rect.left() + rect.width() * 0.46, rect.top() + 4.0),
        pos2(rect.right() - 8.0, rect.bottom() - 4.0),
    );
    if id == "team" {
        let left = Rect::from_min_max(poster.min, pos2(poster.center().x - 14.0, poster.bottom()));
        let right = Rect::from_min_max(pos2(poster.center().x + 14.0, poster.top()), poster.max);
        art.tank(painter, "balanced", 0, left, 0.95);
        art.tank(painter, "heavy", 1, right, 0.95);
        paint::shadowed(
            painter,
            poster.center(),
            Align2::CENTER_CENTER,
            "VS",
            display_font(22.0),
            GOLD_HOT,
            1.0,
        );
    } else {
        let enemy_team = if human_team == 0 { 1 } else { 0 };
        let width = poster.width() / 4.0;
        for (index, kind) in ["scout", "balanced", "heavy"].iter().enumerate() {
            let cell = Rect::from_min_size(
                pos2(
                    poster.left() + index as f32 * width * 0.8,
                    poster.top() + 4.0,
                ),
                vec2(width, poster.height() * 0.6),
            );
            art.tank(painter, kind, enemy_team, cell, 0.9);
        }
        let hero = Rect::from_min_max(
            pos2(
                poster.right() - poster.width() * 0.42,
                poster.top() + poster.height() * 0.25,
            ),
            poster.max,
        );
        art.tank(painter, "balanced", 1 - enemy_team, hero, 1.0);
    }
    let title = paint::shadowed(
        painter,
        rect.min + vec2(16.0, rect.height() / 2.0 - 2.0),
        Align2::LEFT_BOTTOM,
        &name.to_uppercase(),
        display_font(24.0),
        if selected { GOLD_HOT } else { Color32::WHITE },
        0.8,
    );
    let detail_galley = paint::wrapped(
        painter,
        detail,
        egui::FontId::proportional(12.5),
        hex(0xc9dbeb),
        rect.width() * 0.46 - 20.0,
    );
    painter.galley(
        title.left_bottom() + vec2(0.0, 4.0),
        detail_galley,
        Color32::WHITE,
    );
    choice_ring(painter, rect, selected);
}

/// A difficulty segment: its name and what it changes.
fn difficulty_segment(
    painter: &Painter,
    rect: Rect,
    entry: (&str, &str, &str),
    selected: bool,
    hovered: bool,
) {
    let (_, name, detail) = entry;
    if selected {
        painter.rect_filled(rect, 8, hex(0x1d3f6c));
        painter.add(paint::vgradient(
            rect.shrink2(vec2(0.0, 8.0)),
            &[(0.0, hexa(0xffcf38, 0.16)), (1.0, hexa(0xffcf38, 0.04))],
        ));
        painter.rect_stroke(rect, 8, Stroke::new(2.0, GOLD), StrokeKind::Inside);
    } else if hovered {
        painter.rect_filled(rect, 8, hexa(0xffffff, 0.06));
    }
    paint::text(
        painter,
        rect.min + vec2(14.0, 10.0),
        Align2::LEFT_TOP,
        name,
        bold_font(15.0),
        if selected { GOLD_HOT } else { Color32::WHITE },
        0.4,
    );
    let galley = paint::wrapped(
        painter,
        detail,
        egui::FontId::proportional(12.0),
        hex(0xb9cde0),
        rect.width() - 28.0,
    );
    painter.galley(rect.min + vec2(14.0, 31.0), galley, Color32::WHITE);
}

/// The status well beside GO: what the arena is doing, with its progress track.
fn status_well(painter: &Painter, rect: Rect, state: &UiState, time: f64) {
    painter.rect_filled(rect, 10, hexa(0x020812, 0.5));
    painter.rect_stroke(
        rect,
        10,
        Stroke::new(1.0, hexa(0x9fd0ff, 0.12)),
        StrokeKind::Inside,
    );
    let ready = state.ready && !state.starting;
    let dot = pos2(rect.left() + 20.0, rect.top() + 21.0);
    let beat = (0.5 + 0.5 * (time * 4.0).sin()) as f32;
    let dot_color = if ready {
        hex(0x5fe39a)
    } else {
        mix(GOLD, GOLD_HOT, beat)
    };
    painter.circle_filled(dot, 7.0, fade(dot_color, 0.25));
    painter.circle_filled(dot, 4.0, dot_color);
    let status = if state.status.is_empty() {
        "Loading game…"
    } else {
        &state.status
    };
    paint::text_fit(
        painter,
        pos2(rect.left() + 34.0, rect.top() + 21.0),
        Align2::LEFT_CENTER,
        status,
        bold_font(14.0),
        Color32::WHITE,
        0.2,
        rect.width() - 48.0,
    );
    let hint = if state.starting {
        "Rolling out…"
    } else if ready {
        "Press GO! when you are set."
    } else {
        "Pick your loadout while we get ready."
    };
    paint::text_fit(
        painter,
        pos2(rect.left() + 34.0, rect.top() + 39.0),
        Align2::LEFT_CENTER,
        hint,
        egui::FontId::proportional(12.0),
        TEXT_DIM,
        0.0,
        rect.width() - 48.0,
    );
    let track = Rect::from_min_max(
        pos2(rect.left() + 12.0, rect.bottom() - 7.0),
        pos2(rect.right() - 12.0, rect.bottom() - 4.0),
    );
    painter.rect_filled(track, 2, hexa(0xffffff, 0.08));
    if ready {
        painter.rect_filled(track, 2, hex(0x5fe39a));
    } else if let Some(share) = shader_progress(&state.status) {
        painter.rect_filled(
            Rect::from_min_size(track.min, vec2(track.width() * share, track.height())),
            2,
            GOLD,
        );
    } else {
        // Working, with no count to show: a segment sweeps the track.
        let sweep = (time * 0.6).fract() as f32;
        let width = track.width() * 0.3;
        let left = track.left() - width + (track.width() + width) * sweep;
        let segment = Rect::from_min_max(
            pos2(left.max(track.left()), track.top()),
            pos2((left + width).min(track.right()), track.bottom()),
        );
        painter.rect_filled(segment, 2, GOLD);
    }
}

/// "Shaders loaded: 12 of 40" as a share.
fn shader_progress(status: &str) -> Option<f32> {
    let rest = status.strip_prefix("Shaders loaded: ")?;
    let (done, total) = rest.split_once(" of ")?;
    let done: f32 = done.trim().parse().ok()?;
    let total: f32 = total.trim().parse().ok()?;
    (total > 0.0).then(|| (done / total).clamp(0.0, 1.0))
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
    let time = context.input(|input| input.time);
    let width = (screen.width() - 2.0 * MARGIN).min(1000.0);
    let human_team = state.hud.as_ref().map_or(0, |hud| hud.human_team);
    let extra_rows = if view.debug { 1.0 } else { 0.0 };
    // Everything but the tank cards has a fixed height; the cards take what is left.
    let fixed = 64.0
        + 12.0
        + 14.0
        + (LABEL_HEIGHT + SECTION_GAP) * 4.0
        + 64.0 * (1.0 + extra_rows)
        + 8.0 * extra_rows
        + 84.0
        + 64.0
        + SECTION_GAP
        + 56.0
        + 14.0
        + 58.0;
    let card_height = (screen.height() - 2.0 * MARGIN - fixed).clamp(118.0, 196.0);
    let total = fixed + card_height;
    let top = screen.top() + ((screen.height() - total) / 2.0).max(MARGIN);
    let left = screen.center().x - width / 2.0;
    egui::Area::new(Id::unique("battle setup"))
        .fixed_pos(screen.min)
        .order(Order::Middle)
        .show(context, |ui| {
            ui.set_clip_rect(screen);
            egui::ScrollArea::vertical()
                .max_height(screen.height())
                .min_scrolled_height(screen.height())
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let (area, _) = ui.allocate_exact_size(
                        vec2(screen.width(), (total + 2.0 * MARGIN).max(screen.height())),
                        Sense::hover(),
                    );
                    let offset = area.top() - screen.top();
                    let painter = ui.painter().clone();
                    let mut y = top + offset;
                    // The logo heads the screen, the title quieter beside it.
                    let logo_width = logo(&painter, pos2(left + 4.0, y));
                    let divider = left + logo_width + 22.0;
                    painter.line_segment(
                        [pos2(divider, y + 10.0), pos2(divider, y + 56.0)],
                        Stroke::new(1.0, hexa(0xffffff, 0.12)),
                    );
                    paint::shadowed(
                        &painter,
                        pos2(divider + 20.0, y + 34.0),
                        Align2::LEFT_CENTER,
                        "BATTLE SETUP",
                        display_font(26.0),
                        hex(0xdbe9f6),
                        4.0,
                    );
                    corner_buttons(ui, art, screen, offset, actions);
                    y += 64.0 + 12.0;
                    // The tray: the tank, the map, the format, the difficulty and GO.
                    let tray =
                        Rect::from_min_size(pos2(left, y), vec2(width, total - 64.0 - 12.0 - 58.0));
                    painter.rect_filled(tray.translate(vec2(0.0, 10.0)), 16, hexa(0, 0.25));
                    paint::glass(
                        &painter,
                        tray,
                        16.0,
                        hex(0x183a66),
                        hex(0x0d2242),
                        hexa(0x9fd0ff, 0.2),
                    );
                    let inner = tray.shrink(14.0);
                    y = inner.top();
                    paint::section_label(&painter, pos2(inner.left(), y), "YOUR TANK");
                    y += LABEL_HEIGHT;
                    let gap = 10.0;
                    let card_width = (inner.width() - 2.0 * gap) / 3.0;
                    for (index, tank) in TANKS.iter().enumerate() {
                        let rect = Rect::from_min_size(
                            pos2(inner.left() + index as f32 * (card_width + gap), y),
                            vec2(card_width, card_height),
                        );
                        let response = ui
                            .interact(rect, Id::unique(("setup tank", tank.id)), Sense::click())
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        vehicle_card(
                            &painter,
                            art,
                            rect,
                            tank,
                            0,
                            prefs.tank == tank.id,
                            response.hovered(),
                            false,
                        );
                        if response.clicked() {
                            prefs.tank = tank.id.into();
                        }
                    }
                    y += card_height + SECTION_GAP;
                    paint::section_label(&painter, pos2(inner.left(), y), "MAP");
                    y += LABEL_HEIGHT;
                    let rows: Vec<(&[MapChoice], bool)> = if view.debug {
                        vec![(&MAP_CHOICES[..], false), (&EXTRA_MAP_CHOICES[..], true)]
                    } else {
                        vec![(&MAP_CHOICES[..], false)]
                    };
                    for (maps, extra) in rows {
                        let map_width = (inner.width() - 2.0 * gap) / 3.0;
                        for (index, map) in maps.iter().enumerate() {
                            let rect = Rect::from_min_size(
                                pos2(inner.left() + index as f32 * (map_width + gap), y),
                                vec2(map_width, 64.0),
                            );
                            let response = ui
                                .interact(rect, Id::unique(("setup map", map.id)), Sense::click())
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            map_card(
                                &painter,
                                art,
                                rect,
                                map,
                                extra,
                                prefs.map == map.id,
                                response.hovered(),
                            );
                            if response.clicked() {
                                prefs.map = map.id.into();
                            }
                        }
                        y += 64.0 + 8.0;
                    }
                    y += SECTION_GAP - 8.0;
                    paint::section_label(&painter, pos2(inner.left(), y), "BATTLE");
                    y += LABEL_HEIGHT;
                    let battle_width = (inner.width() - gap) / 2.0;
                    for (index, battle) in BATTLES.iter().enumerate() {
                        let rect = Rect::from_min_size(
                            pos2(inner.left() + index as f32 * (battle_width + gap), y),
                            vec2(battle_width, 84.0),
                        );
                        let response = ui
                            .interact(rect, Id::unique(("setup battle", battle.0)), Sense::click())
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        battle_card(
                            &painter,
                            art,
                            rect,
                            *battle,
                            human_team,
                            prefs.game_mode == battle.0,
                            response.hovered(),
                        );
                        if response.clicked() {
                            prefs.game_mode = battle.0.into();
                        }
                    }
                    y += 84.0 + SECTION_GAP;
                    paint::section_label(&painter, pos2(inner.left(), y), "DIFFICULTY");
                    y += LABEL_HEIGHT;
                    let well =
                        Rect::from_min_size(pos2(inner.left(), y), vec2(inner.width(), 64.0));
                    painter.rect_filled(well, 10, hexa(0x020812, 0.45));
                    let segment_width = (well.width() - 8.0 - 2.0 * 4.0) / 3.0;
                    for (index, entry) in DIFFICULTIES.iter().enumerate() {
                        let rect = Rect::from_min_size(
                            pos2(
                                well.left() + 4.0 + index as f32 * (segment_width + 4.0),
                                y + 4.0,
                            ),
                            vec2(segment_width, 56.0),
                        );
                        let response = ui
                            .interact(
                                rect,
                                Id::unique(("setup difficulty", entry.0)),
                                Sense::click(),
                            )
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        difficulty_segment(
                            &painter,
                            rect,
                            *entry,
                            prefs.difficulty == entry.0,
                            response.hovered(),
                        );
                        if response.clicked() {
                            prefs.difficulty = entry.0.into();
                        }
                    }
                    y += 64.0 + SECTION_GAP;
                    // The status well and the one yellow action.
                    let go_width = 220.0;
                    status_well(
                        &painter,
                        Rect::from_min_size(
                            pos2(inner.left(), y),
                            vec2(inner.width() - go_width - gap, 56.0),
                        ),
                        state,
                        time,
                    );
                    let label = if state.starting { "LOADING…" } else { "GO!" };
                    let mut go_ui =
                        ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(
                            pos2(inner.right() - go_width, y),
                            vec2(go_width, 56.0),
                        )));
                    if paint::button(
                        &mut go_ui,
                        art,
                        label,
                        None,
                        ButtonStyle::Primary,
                        vec2(go_width, 56.0),
                        !state.starting,
                    )
                    .clicked()
                    {
                        actions.push(Action::Start);
                    }
                    y = tray.bottom() + 16.0;
                    key_strip(&painter, pos2(screen.center().x, y + 12.0));
                    paint::text(
                        &painter,
                        pos2(screen.center().x, y + 38.0),
                        Align2::CENTER_CENTER,
                        view.build,
                        egui::FontId::proportional(11.0),
                        fade(TEXT_DIM, 0.8),
                        0.4,
                    );
                });
        });
    if prefs != *view.prefs {
        actions.push(Action::SettingsSaved(Box::new(prefs)));
        actions.push(Action::OptionsChanged);
    }
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
    let total = KEY_HINTS.iter().map(hint_width).sum::<f32>() + gap * (KEY_HINTS.len() - 1) as f32;
    let mut x = center.x - total / 2.0;
    for hint in &KEY_HINTS {
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

/// Settings and Quit on a small plate in the screen's corner.
fn corner_buttons(
    ui: &mut egui::Ui,
    art: &Art,
    screen: Rect,
    offset: f32,
    actions: &mut Vec<Action>,
) {
    let button = 38.0;
    let size = vec2(button * 2.0 + 6.0, button + 6.0);
    let rect = Rect::from_min_size(
        pos2(
            screen.right() - MARGIN - size.x,
            screen.top() + 14.0 + offset,
        ),
        size,
    );
    let painter = ui.painter().clone();
    paint::plate(&painter, rect, 9.0, None);
    for (index, (icon, tip)) in [("gear", "Settings"), ("power", "Quit Sloppy Tanks")]
        .into_iter()
        .enumerate()
    {
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

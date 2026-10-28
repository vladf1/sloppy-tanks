//! Battle Setup (`src/game/battle-setup.html`, `start-menu.ts`, `game-options.ts`,
//! `map-picker.ts`): a pane of glass over a dusk battlefield with the logo on top, the
//! tank cards, the map, the battle format and difficulty, the status well and GO.

use egui::{
    Align2, Color32, CornerRadius, Id, LayerId, Mesh, Order, Painter, Rect, Sense, Shape, Stroke,
    StrokeKind, Vec2, pos2, vec2,
};

use super::icons::Art;
use super::paint::{self, ButtonStyle, fade, hex, hexa, mix};
use super::theme::{GOLD, TEXT_DIM, bold_font, display_font};
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

/// The extra levels: id, name.
pub const EXTRA_MAPS: [(&str, &str); 2] = [
    ("stress-test", "Stress Grid"),
    ("superstress", "Scrap Yard"),
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
    let padding = if compact { 10.0 } else { 14.0 };
    let name_size = if compact { 21.0 } else { 22.0 };
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
        pos2(rect.right() - padding - class_width, rect.top() + 15.0),
        vec2(class_width, 19.0),
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
    let spec_height = if compact { 0.0 } else { 26.0 };
    let stage = if compact {
        Rect::from_min_max(
            pos2(rect.left() + 6.0, rect.top() + 22.0),
            pos2(rect.right() - 6.0, rect.bottom() - 4.0),
        )
    } else {
        // The page's tank picture: 174 × 76 under the name line.
        Rect::from_center_size(
            pos2(rect.center().x, rect.top() + 71.0),
            vec2(rect.width() * 0.71, 76.0),
        )
    };
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
        let spec_top = rect.bottom() - spec_height - 11.0;
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

/// A selectable card's frame (`.choice-card`): lit glass with a gold glow when chosen,
/// a darker well otherwise.
fn choice_frame(painter: &Painter, rect: Rect, selected: bool, hovered: bool) {
    let radius = CornerRadius::same(10);
    if selected {
        painter.rect_filled(rect.expand(4.0), 14, hexa(0xffcf38, 0.08));
        painter.rect_filled(rect, radius, hex(0x15305a));
        painter.add(paint::vgradient(
            Rect::from_min_max(
                rect.min + vec2(0.0, 10.0),
                pos2(rect.right(), rect.top() + rect.height() * 0.6),
            ),
            &[(0.0, hexa(0xffffff, 0.07)), (1.0, hexa(0xffffff, 0.0))],
        ));
        painter.rect_filled(
            Rect::from_min_size(rect.min, vec2(rect.width(), 10.0)),
            CornerRadius {
                nw: 10,
                ne: 10,
                sw: 0,
                se: 0,
            },
            mix(hex(0x15305a), Color32::WHITE, 0.07),
        );
    } else {
        painter.rect_filled(
            rect,
            radius,
            if hovered {
                hex(0x143060)
            } else {
                hex(0x10264a)
            },
        );
    }
}

fn choice_ring(painter: &Painter, rect: Rect, selected: bool) {
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
    let text_width = rect.width() - 24.0;
    let name = paint::shadowed(
        painter,
        rect.min + vec2(12.0, 17.0),
        Align2::LEFT_TOP,
        map.name,
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
    paint::text_fit(
        painter,
        pos2(rect.left() + 12.0, rect.top() + 38.0),
        Align2::LEFT_TOP,
        map.description,
        egui::FontId::proportional(12.0),
        hex(0xc9def0),
        0.0,
        text_width,
    );
    choice_ring(painter, rect, selected);
}

/// A battle format card (`.battle-choice`): a tall poster of tanks over its name, the
/// player's side in their team's colour.
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
    let enemy_team = 1 - human_team.min(1);
    let clip = painter.with_clip_rect(rect.shrink(1.0));
    let at =
        |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(rect.min + vec2(x, y), vec2(w, h));
    let glow = if human_team == 0 { 0x3d7cc2 } else { 0xc2503d };
    clip.add(paint::radial(
        rect.min + vec2(rect.width() * 0.5, 130.0),
        vec2(rect.width() * 0.6, 150.0),
        &[(0.0, hexa(glow, 0.22)), (1.0, Color32::TRANSPARENT)],
    ));
    if id == "team" {
        // The player's tank top right, the enemy's bottom left, VS between them.
        art.tank(&clip, "heavy", human_team, at(81.0, 22.0, 165.0, 89.0), 1.0);
        art.tank(
            &clip,
            "balanced",
            enemy_team,
            at(5.0, 165.0, 165.0, 89.0),
            1.0,
        );
        let versus = at(104.0, 113.0, 44.0, 44.0).center();
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
        // Three enemies at the back, the player's tank big in front.
        for (index, kind) in ["scout", "balanced", "heavy"].iter().enumerate() {
            let x = 20.0 + index as f32 * 62.0;
            let y = 40.0 + (index % 2) as f32 * 14.0;
            art.tank(&clip, kind, enemy_team, at(x, y, 110.0, 60.0), 0.9);
        }
        art.tank(
            &clip,
            "heavy",
            human_team,
            at(30.0, 110.0, 210.0, 120.0),
            1.05,
        );
    }
    paint::shadowed(
        painter,
        rect.min + vec2(16.0, 269.0),
        Align2::LEFT_TOP,
        &name.to_uppercase(),
        display_font(24.0),
        Color32::WHITE,
        0.5,
    );
    paint::text_fit(
        painter,
        rect.min + vec2(16.0, 298.0),
        Align2::LEFT_TOP,
        detail,
        egui::FontId::proportional(12.0),
        hex(0xc9def0),
        0.0,
        rect.width() - 32.0,
    );
    choice_ring(painter, rect, selected);
}

/// A difficulty level (`.difficulty-setting .segment`): its name, what it changes and
/// its strength bars.
fn difficulty_segment(
    painter: &Painter,
    rect: Rect,
    level: usize,
    entry: (&str, &str, &str),
    selected: bool,
    hovered: bool,
) {
    let (_, name, detail) = entry;
    choice_frame(painter, rect, selected, hovered);
    paint::shadowed(
        painter,
        rect.min + vec2(18.0, 5.0),
        Align2::LEFT_TOP,
        &name.to_uppercase(),
        display_font(24.0),
        Color32::WHITE,
        0.5,
    );
    let galley = paint::wrapped(
        painter,
        detail,
        egui::FontId::proportional(12.5),
        hex(0xd0e4f3),
        199.0,
    );
    painter.galley(rect.min + vec2(18.0, 38.0), galley, Color32::WHITE);
    let bars = Rect::from_min_size(
        pos2(rect.right() - 44.0, rect.center().y - 14.0),
        vec2(27.0, 28.0),
    );
    difficulty_bars(painter, bars, level, selected);
    choice_ring(painter, rect, selected);
}

/// "Shaders loaded: 12 of 40" as a share.
fn shader_progress(status: &str) -> Option<f32> {
    let rest = status.strip_prefix("Shaders loaded: ")?;
    let (done, total) = rest.split_once(" of ")?;
    let done: f32 = done.trim().parse().ok()?;
    let total: f32 = total.trim().parse().ok()?;
    (total > 0.0).then(|| (done / total).clamp(0.0, 1.0))
}

/// The size Battle Setup is laid out at, the page's at a 1280 × 800 window; it scales
/// as one picture to the window, between these bounds.
const DESIGN: Vec2 = vec2(1280.0, 800.0);
const MIN_SCALE: f32 = 0.6;
const MAX_SCALE: f32 = 1.3;

/// Where the parts sit on the design canvas (`.menu.start` at 1280 × 800).
const FRAME: Rect = Rect::from_min_max(pos2(33.0, 27.0), pos2(1247.0, 772.0));
const TANK_TAB: Rect = Rect::from_min_max(pos2(60.0, 135.0), pos2(304.0, 195.0));
const TANK_WELL: Rect = Rect::from_min_max(pos2(60.0, 194.0), pos2(331.0, 708.0));
const PANELS: Rect = Rect::from_min_max(pos2(350.0, 194.0), pos2(1220.0, 708.0));
const TAB_WIDTH: f32 = 432.0;
const CARD_GAP: f32 = 7.0;

/// The tray's glass: the tank well and the panels share it, joined to their tabs.
fn tray(painter: &Painter, rect: Rect, open_corner: bool) {
    let radius = CornerRadius {
        nw: if open_corner { 0 } else { 16 },
        ne: 16,
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

/// A play tab: its badge, its name and what it is (`.play-tab`).
fn play_tab(
    ui: &mut egui::Ui,
    painter: &Painter,
    rect: Rect,
    (multiplayer, name, line): (bool, &str, &str),
    open: bool,
) -> bool {
    let response = ui
        .interact(rect, Id::unique(("play tab", multiplayer)), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    folder_tab(painter, rect, open, GOLD);
    let badge = Rect::from_min_size(
        rect.min + vec2(16.0, rect.height() / 2.0 - 17.0),
        vec2(34.0, 34.0),
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
        pos2(badge.right() + 12.0, rect.center().y + 1.0),
        Align2::LEFT_CENTER,
        name,
        display_font(23.0),
        if open { Color32::WHITE } else { hex(0xc9dbec) },
        0.6,
    );
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
    let width = 7.0;
    for bar in 0..3 {
        let height = rect.height() * (0.45 + 0.275 * bar as f32);
        let column = Rect::from_min_max(
            pos2(
                rect.left() + bar as f32 * (width + 3.0),
                rect.bottom() - height,
            ),
            pos2(
                rect.left() + bar as f32 * (width + 3.0) + width,
                rect.bottom(),
            ),
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
    match shader_progress(&state.status) {
        Some(share) => {
            painter.rect_filled(
                Rect::from_min_size(track.min, vec2(track.width() * share, track.height())),
                2,
                GOLD,
            );
        }
        None => {
            let sweep = (time * 0.6).fract() as f32;
            let width = track.width() * 0.35;
            let left = track.left() - width + (track.width() + width) * sweep;
            painter.rect_filled(
                Rect::from_min_max(
                    pos2(left.max(track.left()), track.top()),
                    pos2((left + width).min(track.right()), track.bottom()),
                ),
                2,
                GOLD,
            );
        }
    }
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
    let time = context.input(|input| input.time);
    let human_team = state.hud.as_ref().map_or(0, |hud| hud.human_team);
    // One picture at the page's proportions, scaled to the window and centred.
    let scale = (screen.width() / DESIGN.x)
        .min(screen.height() / DESIGN.y)
        .clamp(MIN_SCALE, MAX_SCALE);
    let offset = screen.center() - DESIGN * scale / 2.0;
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
            let canvas = Rect::from_min_size(pos2(0.0, 0.0), DESIGN);
            ui.set_clip_rect(canvas);
            let _ = ui.allocate_rect(canvas, Sense::hover());
            let painter = ui.painter().clone();
            // The menu's own glass.
            painter.rect_filled(
                FRAME.translate(vec2(0.0, 30.0)).expand(10.0),
                26,
                hexa(0, 0.35),
            );
            painter.add(paint::rounded_vgradient(
                FRAME,
                [16.0; 4],
                &[
                    (0.0, hexa(0x12294c, 0.9)),
                    (0.6, hexa(0x091629, 0.94)),
                    (1.0, hexa(0x070f1f, 0.96)),
                ],
            ));
            painter.rect_stroke(
                FRAME,
                16,
                Stroke::new(0.5, hexa(0xa9d4ff, 0.18)),
                StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    FRAME.left_top() + vec2(16.0, 0.5),
                    FRAME.right_top() + vec2(-16.0, 0.5),
                ],
                Stroke::new(1.0, hexa(0xffffff, 0.11)),
            );
            logo(&painter, pos2(71.0, 48.0));
            corner_buttons(
                ui,
                art,
                Rect::from_min_size(pos2(1137.0, 64.0), vec2(82.0, 44.0)),
                actions,
            );

            // The tank column, for both ways to play.
            folder_tab(&painter, TANK_TAB, true, hexa(0x9fd0ff, 0.4));
            paint::section_label(&painter, TANK_TAB.min + vec2(16.0, 19.0), "YOUR TANK");
            paint::text(
                &painter,
                TANK_TAB.min + vec2(146.0, 31.0),
                Align2::LEFT_CENTER,
                "Both modes",
                egui::FontId::proportional(13.0),
                hex(0xd3e7f5),
                0.0,
            );
            tray(&painter, TANK_WELL, true);
            for (index, tank) in TANKS.iter().enumerate() {
                let rect = Rect::from_min_size(
                    pos2(72.0, 206.0 + index as f32 * 165.5),
                    vec2(246.0, 158.0),
                );
                let response = ui
                    .interact(rect, Id::unique(("setup tank", tank.id)), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                vehicle_card(
                    &painter,
                    art,
                    rect,
                    tank,
                    human_team,
                    prefs.tank == tank.id,
                    response.hovered(),
                    false,
                );
                if response.clicked() {
                    prefs.tank = tank.id.into();
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
                let left = PANELS.left() + index as f32 * (PANELS.width() - TAB_WIDTH);
                let rect = if open {
                    Rect::from_min_size(pos2(left, 135.0), vec2(TAB_WIDTH, 60.0))
                } else {
                    Rect::from_min_size(pos2(left, 141.0), vec2(TAB_WIDTH, 52.0))
                };
                if play_tab(ui, &painter, rect, tab, open) {
                    state.rooms.multiplayer = tab.0;
                }
            }
            tray(&painter, PANELS, !multiplayer);
            let inner = PANELS.shrink(14.0);
            if multiplayer {
                let mut rooms_ui = ui.new_child(egui::UiBuilder::new().max_rect(inner));
                egui::ScrollArea::vertical()
                    .id_salt("setup rooms")
                    .max_height(inner.height())
                    .show(&mut rooms_ui, |ui| {
                        super::rooms::multiplayer(ui, &mut state.rooms, view, &prefs, actions);
                    });
            } else {
                // MAP over the panel's width; extra levels share the row on `--debug`.
                paint::section_label(&painter, pos2(inner.left(), 208.0), "MAP");
                let maps: Vec<(&MapChoice, bool)> = MAP_CHOICES
                    .iter()
                    .map(|map| (map, false))
                    .chain(
                        view.debug
                            .then_some(EXTRA_MAP_CHOICES.iter().map(|map| (map, true)))
                            .into_iter()
                            .flatten(),
                    )
                    .collect();
                let row = Rect::from_min_max(pos2(369.0, 240.0), pos2(1201.0, 312.0));
                let map_width =
                    (row.width() - CARD_GAP * (maps.len() - 1) as f32) / maps.len() as f32;
                for (index, (map, extra)) in maps.into_iter().enumerate() {
                    let rect = Rect::from_min_size(
                        pos2(
                            row.left() + index as f32 * (map_width + CARD_GAP),
                            row.top(),
                        ),
                        vec2(map_width, row.height()),
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
                // BATTLE: two posters down the panel's height.
                paint::section_label(&painter, pos2(inner.left(), 330.0), "BATTLE");
                status_line(&painter, pos2(876.0, 334.0), state, time);
                for (index, battle) in BATTLES.iter().enumerate() {
                    let rect = Rect::from_min_size(
                        pos2(369.0 + index as f32 * 257.0, 362.0),
                        vec2(251.0, 326.0),
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
                // DIFFICULTY, and GO under it.
                paint::section_label(&painter, pos2(904.0, 330.0), "DIFFICULTY");
                for (index, entry) in DIFFICULTIES.iter().enumerate() {
                    let rect = Rect::from_min_size(
                        pos2(909.0, 362.0 + index as f32 * 85.5),
                        vec2(292.0, 79.0),
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
                        index,
                        *entry,
                        prefs.difficulty == entry.0,
                        response.hovered(),
                    );
                    if response.clicked() {
                        prefs.difficulty = entry.0.into();
                    }
                }
                let go = Rect::from_min_size(pos2(904.0, 631.0), vec2(302.0, 62.0));
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
            key_strip(&painter, pos2(DESIGN.x / 2.0, 745.0));
            paint::text(
                &painter,
                pos2(FRAME.right() - 22.0, 745.0),
                Align2::RIGHT_CENTER,
                view.build,
                egui::FontId::proportional(11.0),
                fade(TEXT_DIM, 0.7),
                0.4,
            );
        });
    if prefs != *view.prefs {
        actions.push(Action::SettingsSaved(Box::new(prefs)));
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

/// Settings and Quit on a small plate at the header's right.
fn corner_buttons(ui: &mut egui::Ui, art: &Art, rect: Rect, actions: &mut Vec<Action>) {
    let button = 38.0;
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

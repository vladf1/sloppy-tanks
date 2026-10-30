//! The battle report (the results branch of `menuMarkup`), laid out from the words
//! `sloppy_web::battle_report` chose: the outcome and its sentence, the round's feats,
//! the hero kills tile measured against the personal best, the stat tiles with the
//! standout lit, notable highlights, the personal-bests note, and the team scoreboard
//! with the MVP and the viewer's row.

use egui::{
    Align2, Color32, CornerRadius, Mesh, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2,
    pos2, vec2,
};

use super::icons::Art;
use super::menus::{dialog_with_padding, dim_backdrop, phone_width, score_plates};
use super::paint::{self, ButtonStyle, fade, hex, hexa};
use super::theme::{GOLD, GOLD_HOT, bold_font, display_font};
use super::{Action, UiState, View};
use sloppy_core::sim::{GameMode, Team};
use sloppy_web::battle_report::{BattleReport, BestProgress, Outcome};
use sloppy_web::hud::{Hud, HudScore};
use sloppy_web::recap::RoundRecap;

/// The report's width, and the screen widths under which the team boards stand under the
/// viewer's round and the stat tiles go two to a row (`style.css`'s media queries).
const RESULTS_WIDTH: f32 = 1080.0;
const STACKED_BOARD_SCREEN: f32 = 960.0;
const PAIRED_TILES_SCREEN: f32 = 760.0;
/// The gap between the viewer's round and the boards (`.results-body.with-board`).
const BODY_GAP: f32 = 18.0;
/// The banner's cut corner, as the plate's.
const BANNER_CUT: f32 = 14.0;

/// The side's light in the banner (`--team`): gold for a win or a lead held, else the
/// viewer's team.
fn banner_light(outcome: Outcome, team: Team) -> Color32 {
    if outcome.won() {
        hex(0xffb21c)
    } else if team == Team::Blue {
        hex(0x3ea6ff)
    } else {
        hex(0xff5367)
    }
}

/// The banner's ground over `banner` (`.results-banner`): a win is the panel's navy lit
/// from its top-left corner by the side's light and a gold glow, under gold rays; a loss,
/// a draw or an early end is a dark wash fading into the side's light across it. Both
/// carry the faint diagonal stripes.
fn banner_ground(painter: &Painter, banner: Rect, outcome: Outcome, light: Color32, early: bool) {
    let won = outcome.won();
    if won {
        painter.add(paint::gradient_polygon(
            &banner_outline(banner),
            hex(0x172e52),
            hex(0x10233f),
        ));
    } else {
        painter.add(angled_gradient(
            banner,
            100.0,
            &[
                (0.0, hexa(0x1d2a40, 0.95)),
                (0.6, fade(light, 0.14)),
                (1.0, Color32::TRANSPARENT),
            ],
        ));
    }
    in_banner(painter, banner, |clip| {
        if won {
            clip.add(paint::radial(
                banner.min,
                vec2(banner.width() * 0.7, banner.height() * 1.9),
                &[(0.0, fade(light, 0.34)), (0.7, Color32::TRANSPARENT)],
            ));
            clip.add(paint::radial(
                pos2(banner.left() + banner.width() * 0.06, banner.top()),
                vec2(banner.width() * 0.4, banner.height() * 1.4),
                &[(0.0, hexa(0xffcf38, 0.15)), (0.7, Color32::TRANSPARENT)],
            ));
        }
        // `repeating-linear-gradient(-60deg, …)`: a 2 point stripe every 12 points across.
        let slant = banner.height() / 3.0_f32.sqrt();
        let spacing = 12.0 / std::f32::consts::FRAC_PI_6.cos();
        let mut x = banner.left() - slant;
        while x < banner.right() {
            clip.line_segment(
                [pos2(x, banner.bottom()), pos2(x + slant, banner.top())],
                Stroke::new(2.0, hexa(0xffffff, 0.024)),
            );
            x += spacing;
        }
        if won {
            // Rays from above the title, fading out from their middle (the CSS mask).
            let center = banner.min + vec2(220.0, 80.0);
            let strength = if early { 0.6 } else { 1.0 };
            for ray in 0..24 {
                let angle = ray as f32 * std::f32::consts::TAU / 24.0;
                let next = angle + 5.0_f32.to_radians();
                let mut mesh = Mesh::default();
                mesh.colored_vertex(center, hexa(0xffcf38, 0.14 * strength));
                for edge in [angle, next] {
                    mesh.colored_vertex(
                        center + vec2(edge.cos(), edge.sin()) * RAY_REACH,
                        Color32::TRANSPARENT,
                    );
                }
                mesh.add_triangle(0, 1, 2);
                clip.add(Shape::mesh(mesh));
            }
        }
    });
}

/// Draw through clips that cover `banner` but for its cut corner: a staircase of strips
/// down the cut, two points a step, then the rest.
fn in_banner(painter: &Painter, banner: Rect, draw: impl Fn(&Painter)) {
    const STEPS: usize = 7;
    let step = BANNER_CUT / STEPS as f32;
    for index in 0..STEPS {
        let top = banner.top() + index as f32 * step;
        let left = banner.left() + BANNER_CUT - (index as f32 + 0.5) * step;
        draw(&painter.with_clip_rect(Rect::from_min_max(
            pos2(left, top),
            pos2(banner.right(), top + step),
        )));
    }
    draw(&painter.with_clip_rect(Rect::from_min_max(
        pos2(banner.left(), banner.top() + BANNER_CUT),
        banner.max,
    )));
}

/// How far the victory rays reach before they fade out.
const RAY_REACH: f32 = 312.0;

/// The banner's outline: its top-left corner cut like the plate's.
fn banner_outline(banner: Rect) -> [Pos2; 5] {
    [
        banner.min + vec2(BANNER_CUT, 0.0),
        banner.right_top(),
        banner.right_bottom(),
        banner.left_bottom(),
        banner.min + vec2(0.0, BANNER_CUT),
    ]
}

/// A CSS `linear-gradient(degrees, stops)` over `rect` with its top-left corner cut, in
/// columns narrow enough that its stops stay straight.
fn angled_gradient(rect: Rect, degrees: f32, stops: &[(f32, Color32)]) -> Shape {
    const COLUMNS: usize = 32;
    let angle = degrees.to_radians();
    let direction = vec2(angle.sin(), -angle.cos());
    let length = rect.width() * direction.x.abs() + rect.height() * direction.y.abs();
    let color_at = |point: Pos2| {
        let share = (point - rect.center()).dot(direction) / length + 0.5;
        let mut previous = stops[0];
        for &stop in stops {
            if share <= stop.0 {
                let span = (stop.0 - previous.0).max(f32::EPSILON);
                return paint::mix(previous.1, stop.1, (share - previous.0) / span);
            }
            previous = stop;
        }
        previous.1
    };
    let mut xs: Vec<f32> = (0..=COLUMNS)
        .map(|column| rect.left() + rect.width() * column as f32 / COLUMNS as f32)
        .collect();
    xs.push(rect.left() + BANNER_CUT);
    xs.sort_by(f32::total_cmp);
    let mut mesh = Mesh::default();
    for (index, &x) in xs.iter().enumerate() {
        let top = pos2(x, rect.top() + (BANNER_CUT - (x - rect.left())).max(0.0));
        let bottom = pos2(x, rect.bottom());
        mesh.colored_vertex(top, color_at(top));
        mesh.colored_vertex(bottom, color_at(bottom));
        if index > 0 {
            let base = (index as u32 - 1) * 2;
            mesh.add_triangle(base, base + 1, base + 2);
            mesh.add_triangle(base + 1, base + 3, base + 2);
        }
    }
    Shape::mesh(mesh)
}

/// The rule under the banner (`.results-banner::after`): gold fading by 90% for a win,
/// else the side's light fading by 70%.
fn banner_rule(painter: &Painter, banner: Rect, outcome: Outcome, light: Color32) {
    let won = outcome.won();
    let rule = Rect::from_min_max(
        pos2(banner.left(), banner.bottom() - if won { 3.0 } else { 2.0 }),
        banner.max,
    );
    let stops = if won {
        [(0.0, GOLD_HOT), (0.45, GOLD), (0.9, Color32::TRANSPARENT)]
    } else {
        [
            (0.0, light),
            (0.7, Color32::TRANSPARENT),
            (1.0, Color32::TRANSPARENT),
        ]
    };
    painter.add(paint::hgradient(rule, &stops));
}

/// The outcome's title colour: gold for a win or a lead held, the team's pale tone or red
/// for a loss, white for the rest.
fn title_color(outcome: Outcome, team: Team) -> Color32 {
    if outcome.won() {
        GOLD_HOT
    } else if outcome.lost() && team == Team::Blue {
        hex(0x9cc6ef)
    } else if outcome.lost() {
        hex(0xff8a96)
    } else {
        Color32::WHITE
    }
}

/// The sentence under the title. A solo run's loss leads with the death that ended it.
fn lede(state: &UiState, hud: &Hud, report: &BattleReport) -> String {
    match &state.death {
        Some(death) if hud.game_mode == GameMode::Solo && report.outcome == Outcome::Lost => {
            format!("{} {}", death.sentence, report.lede)
        }
        _ => report.lede.clone(),
    }
}

pub fn results(
    context: &egui::Context,
    state: &mut UiState,
    art: &Art,
    _view: &View,
    actions: &mut Vec<Action>,
) {
    let Some(hud) = state.hud.clone() else { return };
    let Some(recap) = hud.recap.as_deref() else {
        return;
    };
    if state.settings_draft.is_some() {
        return;
    }
    dim_backdrop(context);
    let solo = hud.game_mode == GameMode::Solo;
    let report = &recap.report;
    if crate::touch::phone() {
        phone_results(context, art, &hud, report, actions);
        return;
    }
    let title = report.title;
    let lede = lede(state, &hud, report);
    let mode = if solo { "SOLO ASSAULT" } else { "TEAM BATTLE" };
    let width = RESULTS_WIDTH;
    let screen = context.content_rect();
    let compact = screen.height() <= 800.0;
    let stacked = screen.width() <= STACKED_BOARD_SCREEN;
    let layout = RecapLayout {
        compact,
        tile_columns: if screen.width() <= PAIRED_TILES_SCREEN {
            2
        } else {
            4
        },
    };
    let ended_early = hud.match_state.ended_early.unwrap_or(false);
    dialog_with_padding(
        context,
        "results",
        width,
        [30.0, 0.0, if compact { 18.0 } else { 24.0 }],
        |ui, painter, origin, width| {
            let won = report.outcome.won();
            let light = banner_light(report.outcome, hud.human_team);
            let title_color = title_color(report.outcome, hud.human_team);
            let inset_top = if compact { 14.0 } else { 22.0 };
            let inset_bottom = if compact { 12.0 } else { 18.0 };
            let title_size = if compact { 56.0 } else { 72.0 };
            let eyebrow_height = if ended_early && !solo { 26.0 } else { 15.0 };
            let title_top = origin.y + inset_top + eyebrow_height + 6.0;
            let title_height = title_size * 0.95;
            let lede_top = title_top + title_height + 18.0;
            let lede_galley = paint::wrapped(painter, &lede, bold_font(14.0), hex(0xe7f1fb), width);
            let banner_bottom = lede_top + lede_galley.size().y + inset_bottom;
            let banner = Rect::from_min_max(
                pos2(origin.x - 30.0, origin.y),
                pos2(origin.x + width + 30.0, banner_bottom),
            );
            banner_ground(painter, banner, report.outcome, light, ended_early);
            // The side's stripe down the left edge, and a hairline along the foot.
            painter.rect_filled(
                Rect::from_min_max(
                    banner.min + vec2(0.0, BANNER_CUT),
                    pos2(banner.left() + 4.0, banner.bottom()),
                ),
                0,
                light,
            );
            painter.line_segment(
                [
                    pos2(banner.left(), banner.bottom() - 0.5),
                    pos2(banner.right(), banner.bottom() - 0.5),
                ],
                Stroke::new(1.0, hexa(0xffffff, 0.12)),
            );
            banner_rule(painter, banner, report.outcome, light);
            let eyebrow = format!("{mode} · {}", hud.map_name.to_uppercase());
            let eyebrow_rect = paint::text(
                painter,
                origin + vec2(0.0, inset_top + (eyebrow_height - 15.0) / 2.0),
                Align2::LEFT_TOP,
                &eyebrow,
                bold_font(12.0),
                hexa(0xffffff, 0.8),
                2.4,
            );
            if ended_early && !solo {
                let tag = Rect::from_min_size(
                    pos2(eyebrow_rect.right() + 14.0, origin.y + inset_top),
                    vec2(
                        paint::text_width(painter, "ENDED EARLY", bold_font(13.0), 1.6) + 20.0,
                        26.0,
                    ),
                );
                painter.add(paint::rounded_vgradient(
                    tag,
                    [5.0; 4],
                    &[(0.0, GOLD_HOT), (1.0, GOLD)],
                ));
                paint::text(
                    painter,
                    tag.center(),
                    Align2::CENTER_CENTER,
                    "ENDED EARLY",
                    bold_font(13.0),
                    hex(0x1a2233),
                    1.6,
                );
            }
            let title_position = pos2(origin.x, title_top);
            if won {
                let title_rect = paint::text(
                    painter,
                    title_position + vec2(0.0, 4.0),
                    Align2::LEFT_TOP,
                    title,
                    display_font(title_size),
                    hexa(0, 0.3),
                    2.0,
                )
                .translate(vec2(0.0, -4.0));
                paint::text(
                    &painter.with_clip_rect(Rect::from_min_max(
                        title_rect.min,
                        pos2(title_rect.right(), title_rect.center().y),
                    )),
                    title_position,
                    Align2::LEFT_TOP,
                    title,
                    display_font(title_size),
                    hex(0xfffbe6),
                    2.0,
                );
                paint::text(
                    &painter.with_clip_rect(Rect::from_min_max(
                        pos2(title_rect.left(), title_rect.center().y),
                        title_rect.max,
                    )),
                    title_position,
                    Align2::LEFT_TOP,
                    title,
                    display_font(title_size),
                    GOLD_HOT,
                    2.0,
                );
            } else {
                paint::shadowed(
                    painter,
                    title_position,
                    Align2::LEFT_TOP,
                    title,
                    display_font(title_size),
                    title_color,
                    2.0,
                );
            }
            if !solo {
                score_plates(
                    painter,
                    pos2(origin.x + width, title_top + title_height / 2.0),
                    hud.match_state.scores,
                    &hud.team_names,
                    hud.human_team,
                );
            }
            painter.galley(pos2(origin.x, lede_top), lede_galley, Color32::WHITE);
            let mut y = banner_bottom + if compact { 12.0 } else { 16.0 };
            let body_origin = pos2(origin.x, y);
            let body = if solo {
                recap_body(painter, art, body_origin, width, 0.0, &hud, recap, layout)
            } else if stacked {
                // The boards under the viewer's round, full width.
                let you = recap_body(painter, art, body_origin, width, 0.0, &hud, recap, layout);
                let board_top = pos2(origin.x, y + you + BODY_GAP);
                you + BODY_GAP + board(painter, board_top, width, &hud, report, 0.0)
            } else {
                // Beside the boards both columns end on one line: the hero tile grows to
                // the boards' height, or the boards to the round's.
                let you_width = (width - BODY_GAP) * 1.25 / 2.25;
                let you = recap_body(
                    painter,
                    art,
                    body_origin,
                    you_width,
                    board_height(report),
                    &hud,
                    recap,
                    layout,
                );
                let board_left = origin.x + you_width + BODY_GAP;
                you.max(board(
                    painter,
                    pos2(board_left, y),
                    origin.x + width - board_left,
                    &hud,
                    report,
                    you,
                ))
            };
            y += body + if compact { 14.0 } else { 22.0 };
            let height = 48.0;
            let setup_width = (width - 10.0) / 3.0;
            let setup = Rect::from_min_size(
                pos2(origin.x + width - setup_width, y),
                vec2(setup_width, height),
            );
            if paint::button_at(
                ui,
                art,
                setup,
                "BATTLE SETUP",
                Some("setup"),
                ButtonStyle::Secondary,
                true,
            )
            .clicked()
            {
                actions.push(Action::BattleSetup);
            }
            if paint::button_at(
                ui,
                art,
                Rect::from_min_size(
                    pos2(origin.x, y),
                    vec2(setup.left() - origin.x - 10.0, height),
                ),
                "PLAY AGAIN",
                Some("again"),
                ButtonStyle::Primary,
                true,
            )
            .clicked()
            {
                actions.push(Action::Start);
            }
            y + height - origin.y
        },
    );
}

/// A phone's battle report (`.phone .menu.results`): the outcome on one line over its
/// score, in a band of the side's light that runs edge to edge over its rule, then
/// PLAY AGAIN and BATTLE SETUP.
fn phone_results(
    context: &egui::Context,
    art: &Art,
    hud: &Hud,
    report: &BattleReport,
    actions: &mut Vec<Action>,
) {
    let solo = hud.game_mode == GameMode::Solo;
    let ended_early = hud.match_state.ended_early.unwrap_or(false);
    dialog_with_padding(
        context,
        "results",
        phone_width(context, PHONE_RESULTS_WIDTH),
        [20.0, 0.0, 18.0],
        |ui, painter, origin, width| {
            let light = banner_light(report.outcome, hud.human_team);
            let title_size = 40.0;
            let title_top = origin.y + 18.0;
            let score_top = title_top + title_size * 0.95 + 10.0;
            let banner_bottom = if solo {
                score_top + 6.0
            } else {
                score_top + 54.0 + 16.0
            };
            let banner = Rect::from_min_max(
                pos2(origin.x - 20.0, origin.y),
                pos2(origin.x + width + 20.0, banner_bottom),
            );
            // The desktop banner's ground and rule, without its edge stripe and hairline.
            banner_ground(painter, banner, report.outcome, light, ended_early);
            banner_rule(painter, banner, report.outcome, light);
            let center = banner.center().x;
            paint::text_fit(
                painter,
                pos2(center, title_top),
                Align2::CENTER_TOP,
                report.title,
                display_font(title_size),
                title_color(report.outcome, hud.human_team),
                2.0,
                width,
            );
            if !solo {
                let scores = hud.match_state.scores;
                let plates = super::menus::score_plates_width(
                    painter,
                    scores,
                    &hud.team_names,
                    hud.human_team,
                );
                score_plates(
                    painter,
                    pos2(center + plates / 2.0, score_top + 27.0),
                    scores,
                    &hud.team_names,
                    hud.human_team,
                );
            }
            let height = super::menus::PHONE_BUTTON_HEIGHT;
            let mut y = banner_bottom + 16.0;
            for (label, icon, style, action) in [
                ("PLAY AGAIN", "again", ButtonStyle::Primary, Action::Start),
                (
                    "BATTLE SETUP",
                    "setup",
                    ButtonStyle::Secondary,
                    Action::BattleSetup,
                ),
            ] {
                let rect = Rect::from_min_size(pos2(origin.x, y), vec2(width, height));
                if paint::button_at(ui, art, rect, label, Some(icon), style, true).clicked() {
                    actions.push(action);
                }
                y += height + 10.0;
            }
            y - 10.0 - origin.y
        },
    );
}

/// A phone's battle report's width.
const PHONE_RESULTS_WIDTH: f32 = 400.0;

/// How the viewer's round lays out: tighter on a short screen, and its stat tiles four or
/// two to a row.
#[derive(Clone, Copy)]
struct RecapLayout {
    compact: bool,
    tile_columns: usize,
}

/// The viewer's part of the report, its hero tile grown so the part is at least `fill`
/// tall (the boards' height beside it); returns the height used.
#[allow(clippy::too_many_arguments)]
fn recap_body(
    painter: &Painter,
    art: &Art,
    origin: Pos2,
    width: f32,
    fill: f32,
    hud: &Hud,
    recap: &RoundRecap,
    layout: RecapLayout,
) -> f32 {
    let report = &recap.report;
    let compact = layout.compact;
    let mut y = origin.y;
    // Feats are little gold report slips, wrapping as a row before the stat tiles.
    let feat_height = if compact { 39.0 } else { 43.0 };
    let mut feat_x = origin.x;
    for feat in &recap.feats {
        let title = feat.title.to_uppercase();
        let chip_width = (paint::text_width(painter, &title, bold_font(11.5), 0.8) + 16.0).max(
            paint::text_width(painter, &feat.detail, egui::FontId::proportional(11.5), 0.0),
        ) + 20.0;
        let chip_width = chip_width.min(width);
        if feat_x > origin.x && feat_x + chip_width > origin.x + width {
            feat_x = origin.x;
            y += feat_height + 6.0;
        }
        let chip = Rect::from_min_size(pos2(feat_x, y), vec2(chip_width, feat_height));
        paint::glass(
            painter,
            chip,
            8.0,
            hexa(0xffda78, 0.15),
            hexa(0xffda78, 0.05),
            hexa(0xffda78, 0.4),
        );
        let title_y = y + if compact { 4.0 } else { 6.0 };
        art.icon(
            painter,
            "star",
            Rect::from_min_size(pos2(feat_x + 10.0, title_y + 1.0), Vec2::splat(11.5)),
            hex(0xffda78),
        );
        paint::text(
            painter,
            pos2(feat_x + 26.0, title_y),
            Align2::LEFT_TOP,
            &title,
            bold_font(11.5),
            hex(0xffda78),
            0.8,
        );
        paint::text_fit(
            painter,
            pos2(feat_x + 10.0, title_y + 15.5),
            Align2::LEFT_TOP,
            &feat.detail,
            egui::FontId::proportional(11.5),
            hex(0xe6dcc0),
            0.0,
            chip_width - 20.0,
        );
        feat_x += chip_width + 6.0;
    }
    if !recap.feats.is_empty() {
        y += feat_height + if compact { 8.0 } else { 10.0 };
    }
    // The hero tile: kills beside the viewer's tank and the measure against the best.
    // Under it the small tiles, the highlights and the note keep their own heights, so
    // the hero takes what the part has to grow (`.with-board .recap-stats`).
    let tile_height = if compact { 78.0 } else { 83.0 };
    let tile_rows = small_tile_rows(report.tiles.len().saturating_sub(1), layout);
    let below = 8.0 + tile_rows as f32 * (tile_height + 8.0) - 8.0
        + if report.details.is_empty() { 0.0 } else { 56.0 }
        + if report.note.is_some() { 28.0 } else { 0.0 };
    let (kills, small_tiles) = match report.tiles.split_first() {
        Some((kills, rest)) => (kills.value.as_str(), rest),
        None => ("0", &[][..]),
    };
    let lines = HeroLines::measure(painter, width, kills, &report.progress, compact);
    let least_hero: f32 = if compact { 88.0 } else { 104.0 };
    let hero_height = least_hero
        .max(lines.height())
        .max(fill - (y - origin.y) - below);
    let hero = Rect::from_min_size(pos2(origin.x, y), vec2(width, hero_height));
    paint::glass(
        painter,
        hero,
        10.0,
        hex(0x172b4a),
        hex(0x0a162c),
        hexa(0xffcf38, 0.33),
    );
    painter.with_clip_rect(hero.shrink(1.0)).add(paint::radial(
        hero.left_center(),
        vec2(hero.width() * 0.7, hero.height() * 0.9),
        &[(0.0, hexa(0xffcf38, 0.15)), (0.7, Color32::TRANSPARENT)],
    ));
    let tank_team = if hud.game_mode == GameMode::Solo {
        Team::Blue
    } else {
        hud.human_team
    };
    // `.recap-tank`: 300 by 158, 8 past the tile's right and 20 under its foot, clipped.
    art.tank(
        &painter.with_clip_rect(hero),
        hud.human.kind,
        tank_team,
        Rect::from_min_size(
            pos2(hero.right() - 292.0, hero.bottom() - 138.0),
            vec2(300.0, 158.0),
        ),
        1.0,
        false,
    );
    lines.paint(painter, art, hero, kills, &report.progress);
    y = hero.bottom() + 8.0;
    // The small tiles.
    let columns = layout.tile_columns;
    let tile_width = (width - (columns - 1) as f32 * 8.0) / columns as f32;
    for (index, tile) in small_tiles.iter().enumerate() {
        let (caption, value, footer) = (tile.label, &tile.value, &tile.footer);
        let (record, standout) = (tile.record, tile.standout);
        let rect = Rect::from_min_size(
            pos2(
                origin.x + (index % columns) as f32 * (tile_width + 8.0),
                y + (index / columns) as f32 * (tile_height + 8.0),
            ),
            vec2(tile_width, tile_height),
        );
        if record {
            paint::glass(
                painter,
                rect,
                10.0,
                hex(0x2a2a2e),
                hex(0x14131a),
                hexa(0xffda78, 0.6),
            );
        } else {
            paint::glass(
                painter,
                rect,
                10.0,
                hex(0x0f2442),
                hex(0x081429),
                if standout {
                    hexa(0xffda78, 0.5)
                } else {
                    hexa(0xa6dfff, 0.13)
                },
            );
        }
        if standout {
            painter.rect_filled(
                Rect::from_min_size(rect.min + vec2(6.0, 0.0), vec2(rect.width() - 12.0, 3.0)),
                1,
                GOLD,
            );
        }
        paint::text_fit(
            painter,
            pos2(
                rect.center().x,
                rect.top() + if compact { 8.0 } else { 11.0 },
            ),
            Align2::CENTER_TOP,
            &caption.to_uppercase(),
            bold_font(12.0),
            hex(0xa6c4d8),
            1.0,
            rect.width() - 8.0,
        );
        paint::shadowed(
            painter,
            pos2(
                rect.center().x,
                rect.top() + if compact { 41.0 } else { 44.0 },
            ),
            Align2::CENTER_CENTER,
            value,
            display_font(28.0),
            if standout { GOLD_HOT } else { Color32::WHITE },
            0.5,
        );
        if !footer.is_empty() {
            let color = if record { hex(0xffda78) } else { hex(0x8fb3cb) };
            let font = if record {
                bold_font(12.0)
            } else {
                egui::FontId::proportional(12.0)
            };
            let width_text = paint::text_width(painter, footer, font.clone(), 0.0)
                + if record { 16.0 } else { 0.0 };
            let mut x = rect.center().x - width_text / 2.0;
            if record {
                art.icon(
                    painter,
                    "star",
                    Rect::from_center_size(pos2(x + 6.0, rect.bottom() - 15.0), Vec2::splat(12.0)),
                    color,
                );
                x += 16.0;
            }
            paint::text(
                painter,
                pos2(x, rect.bottom() - 15.0),
                Align2::LEFT_CENTER,
                footer,
                font,
                color,
                0.0,
            );
        }
    }
    y += small_tile_rows(small_tiles.len(), layout) as f32 * (tile_height + 8.0) - 8.0;
    // The highlights beyond the tiles, new bests first.
    let extras = &report.details;
    if !extras.is_empty() {
        y += 10.0;
        let strip = Rect::from_min_size(pos2(origin.x, y), vec2(width, 46.0));
        painter.rect_filled(strip, 10, hexa(0x9fd0ff, 0.08));
        let cell_width = (width - (extras.len() - 1) as f32) / extras.len() as f32;
        for (index, detail) in extras.iter().enumerate() {
            let record = &detail.record;
            let cell = Rect::from_min_size(
                pos2(origin.x + index as f32 * (cell_width + 1.0), y),
                vec2(cell_width, 46.0),
            );
            let radius = CornerRadius {
                nw: if index == 0 { 10 } else { 0 },
                sw: if index == 0 { 10 } else { 0 },
                ne: if index == extras.len() - 1 { 10 } else { 0 },
                se: if index == extras.len() - 1 { 10 } else { 0 },
            };
            painter.rect_filled(cell, radius, hexa(0x060f1f, 0.9));
            paint::text(
                painter,
                pos2(cell.center().x, cell.top() + 14.0),
                Align2::CENTER_CENTER,
                &detail.label.to_uppercase(),
                bold_font(11.0),
                hex(0xa6c4d8),
                1.0,
            );
            let value = detail.value.to_uppercase();
            let value_width = paint::text_width(painter, &value, display_font(19.0), 0.4)
                + if *record { 16.0 } else { 0.0 };
            let mut x = cell.center().x - value_width / 2.0;
            if *record {
                art.icon(
                    painter,
                    "star",
                    Rect::from_center_size(pos2(x + 6.0, cell.top() + 32.0), Vec2::splat(12.0)),
                    GOLD,
                );
                x += 16.0;
            }
            paint::text(
                painter,
                pos2(x, cell.top() + 32.0),
                Align2::LEFT_CENTER,
                &value,
                display_font(19.0),
                if *record { GOLD_HOT } else { Color32::WHITE },
                0.4,
            );
        }
        y += 46.0;
    }
    // What the round did to the personal bests, or that they could not be kept.
    if let Some(note) = &report.note {
        let record = note.record.is_some();
        let note = match &note.record {
            Some(bests) => format!("{bests} {}", note.text),
            None => note.text.clone(),
        };
        y += 12.0;
        let mut x = origin.x;
        if record {
            art.icon(
                painter,
                "star",
                Rect::from_center_size(pos2(x + 6.0, y + 8.0), Vec2::splat(13.0)),
                GOLD,
            );
            x += 18.0;
        }
        paint::text(
            painter,
            pos2(x, y + 8.0),
            Align2::LEFT_CENTER,
            &note,
            egui::FontId::proportional(11.5),
            if record { GOLD_HOT } else { hex(0xa9c2d8) },
            0.4,
        );
        y += 16.0;
    }
    y - origin.y
}

/// How many rows the small tiles take.
fn small_tile_rows(tiles: usize, layout: RecapLayout) -> usize {
    tiles.div_ceil(layout.tile_columns).max(1)
}

/// The hero tile's kills, KILLS and the measure against the best (`.recap-stat:first-child`):
/// a wrapping row inside 18 points of padding at the left and the tank's 270 at the right.
/// The measure, at most 300 wide, follows KILLS where it fits and otherwise takes a line
/// of its own under the count; the lines stand together in the tile's middle.
struct HeroLines {
    /// The measure follows KILLS on the count's line.
    inline: bool,
    /// The padding over and under the lines, the count's line, and the measure's height
    /// (its margin, the meter and its gap where it has one, then a 13 point line).
    padding: f32,
    count_line: f32,
    measure_height: f32,
    measure_width: f32,
    number_size: f32,
}

impl HeroLines {
    fn measure(
        painter: &Painter,
        width: f32,
        kills: &str,
        progress: &BestProgress,
        compact: bool,
    ) -> Self {
        let number_size = if compact { 58.0 } else { 84.0 };
        let inner_width = width - 2.0 - 18.0 - 270.0;
        let measure_width = HERO_PROGRESS_WIDTH.min(inner_width);
        let number = paint::text_width(painter, kills, display_font(number_size), 0.5);
        let caption = paint::text_width(painter, "KILLS", bold_font(16.0), 3.0);
        Self {
            inline: number + 14.0 + caption + 14.0 + measure_width <= inner_width,
            padding: if compact { 8.0 } else { 12.0 },
            count_line: number_size * 0.9,
            measure_height: 4.0
                + if hero_meter(progress).is_some() {
                    12.0
                } else {
                    0.0
                }
                + 15.6,
            measure_width,
            number_size,
        }
    }

    /// The lines' own height.
    fn lines_height(&self) -> f32 {
        if self.inline {
            self.count_line.max(self.measure_height)
        } else {
            self.count_line + 4.0 + self.measure_height
        }
    }

    /// The least tile that holds the lines.
    fn height(&self) -> f32 {
        2.0 + 2.0 * self.padding + self.lines_height()
    }

    fn paint(
        &self,
        painter: &Painter,
        art: &Art,
        hero: Rect,
        kills: &str,
        progress: &BestProgress,
    ) {
        let left = hero.left() + 1.0 + 18.0;
        let inner_top = hero.top() + 1.0 + self.padding;
        let inner_height = hero.height() - 2.0 - 2.0 * self.padding;
        let top = inner_top + (inner_height - self.lines_height()) / 2.0;
        let (count_y, measure_top) = if self.inline {
            let middle = top + self.lines_height() / 2.0;
            (middle, middle - self.measure_height / 2.0)
        } else {
            (top + self.count_line / 2.0, top + self.count_line + 4.0)
        };
        let number = paint::shadowed(
            painter,
            pos2(left, count_y + 2.0),
            Align2::LEFT_CENTER,
            kills,
            display_font(self.number_size),
            GOLD_HOT,
            0.5,
        );
        let caption = paint::text(
            painter,
            pos2(number.right() + 14.0, count_y),
            Align2::LEFT_CENTER,
            "KILLS",
            bold_font(16.0),
            Color32::WHITE,
            3.0,
        );
        let left = if self.inline {
            caption.right() + 14.0
        } else {
            left
        };
        let width = self.measure_width;
        let mut top = measure_top + 4.0;
        if let Some(share) = hero_meter(progress) {
            let track = Rect::from_min_size(pos2(left, top), vec2(width, 6.0));
            painter.rect_filled(track, 3, hexa(0xffffff, 0.09));
            painter.add(paint::hgradient(
                Rect::from_min_size(
                    track.min,
                    vec2(track.width() * share.clamp(0.0, 1.0) as f32, 6.0),
                ),
                &[(0.0, hex(0xff9d1c)), (0.5, GOLD), (1.0, GOLD_HOT)],
            ));
            top += 12.0;
        }
        let line_y = top + 7.8;
        let text = progress.text.to_uppercase();
        if progress.record {
            art.icon(
                painter,
                "star",
                Rect::from_center_size(pos2(left + 7.0, line_y), Vec2::splat(14.0)),
                GOLD,
            );
            paint::text_fit(
                painter,
                pos2(left + 20.0, line_y),
                Align2::LEFT_CENTER,
                &text,
                bold_font(13.0),
                GOLD_HOT,
                1.0,
                width - 20.0,
            );
        } else {
            paint::text_fit(
                painter,
                pos2(left, line_y),
                Align2::LEFT_CENTER,
                &text,
                bold_font(13.0),
                hex(0xdbe8f4),
                1.0,
                width,
            );
        }
    }
}

/// The hero's meter toward the best: only a gap has one, never a record or a tie.
fn hero_meter(progress: &BestProgress) -> Option<f64> {
    progress.meter.filter(|_| !progress.record)
}

/// The widest the hero's measure grows (`.recap-progress`).
const HERO_PROGRESS_WIDTH: f32 = 300.0;

/// A small tag in a board row.
fn board_tag(painter: &Painter, right: Pos2, text: &str, mvp: bool, yours: bool) -> f32 {
    let width = paint::text_width(painter, text, bold_font(11.0), 0.5) + 10.0;
    let rect = Rect::from_min_size(pos2(right.x - width, right.y - 8.0), vec2(width, 16.0));
    if mvp {
        painter.rect_filled(rect.expand(2.0), 5, hexa(0xffb21c, 0.2));
        painter.add(paint::vgradient(
            rect,
            &[(0.0, GOLD_HOT), (1.0, hex(0xff9d1c))],
        ));
    } else if yours {
        painter.rect_filled(rect, 4, GOLD);
    } else {
        painter.rect_filled(rect, 4, hexa(0xffffff, 0.1));
    }
    paint::text(
        painter,
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        bold_font(11.0),
        if mvp {
            hex(0x2a1a00)
        } else if yours {
            hex(0x1a2233)
        } else {
            hex(0xe6f1f9)
        },
        0.5,
    );
    width
}

/// A team board's header, and the MVP key under the boards.
const BOARD_HEADER: f32 = 34.0;
const BOARD_KEY: f32 = 30.0;

fn largest_team(report: &BattleReport) -> usize {
    let teams = &report.board.teams;
    teams.iter().map(|team| team.tanks.len()).max().unwrap_or(0)
}

/// The boards' own height: a 30 point row a tank (22 past eight), and the MVP key.
fn board_height(report: &BattleReport) -> f32 {
    let largest = largest_team(report);
    let row_height = if largest <= 8 { 30.0 } else { 22.0 };
    let key = if report.board.mvp.is_some() {
        BOARD_KEY
    } else {
        0.0
    };
    BOARD_HEADER + largest.max(1) as f32 * row_height + 6.0 + key
}

/// The team scoreboard: each team's tanks by kills with their wrecks, the viewer's
/// row lit and the MVP tagged, stretched to `minimum_height`; returns the height used.
fn board(
    painter: &Painter,
    origin: Pos2,
    width: f32,
    hud: &Hud,
    report: &BattleReport,
    minimum_height: f32,
) -> f32 {
    let rows: Vec<HudScore> = hud.scoreboard.rows().collect();
    let mvp = report.board.mvp;
    let viewer = hud.human.id;
    let gap = 8.0;
    let team_width = (width - gap) / 2.0;
    let teams = &report.board.teams;
    let largest = largest_team(report);
    let key = if mvp.is_some() { BOARD_KEY } else { 0.0 };
    let body_height =
        (board_height(report) - BOARD_HEADER - key).max(minimum_height - BOARD_HEADER - key);
    let row_height = (body_height - 6.0) / largest.max(1) as f32;
    for team in [Team::Blue, Team::Red] {
        let x = origin.x + team.index() as f32 * (team_width + gap);
        let side = if team == Team::Blue {
            hex(0x3ea6ff)
        } else {
            hex(0xff5367)
        };
        let section = Rect::from_min_size(
            pos2(x, origin.y),
            vec2(team_width, BOARD_HEADER + body_height),
        );
        painter.rect_filled(section, 10, hexa(0x060f1f, 0.9));
        let header = Rect::from_min_size(section.min, vec2(team_width, BOARD_HEADER));
        painter.rect_filled(
            header,
            CornerRadius {
                nw: 10,
                ne: 10,
                sw: 0,
                se: 0,
            },
            hex(0x0b1830),
        );
        painter.add(paint::hgradient(
            header.shrink2(vec2(6.0, 0.0)),
            &[
                (0.0, paint::mix(hex(0x07122a), side, 0.45)),
                (1.0, hex(0x0b1830)),
            ],
        ));
        painter.rect_filled(
            Rect::from_min_size(header.min + vec2(0.0, 10.0), vec2(6.0, 24.0)),
            0,
            paint::mix(hex(0x07122a), side, 0.45),
        );
        painter.rect_filled(
            Rect::from_min_max(pos2(header.left(), header.bottom() - 2.0), header.max),
            0,
            side,
        );
        paint::text(
            painter,
            pos2(header.left() + 12.0, header.center().y),
            Align2::LEFT_CENTER,
            &hud.team_names[team.index()].to_uppercase(),
            bold_font(13.0),
            Color32::WHITE,
            2.0,
        );
        let kills_x = header.right() - 12.0 - 26.0 - 5.0;
        let deaths_x = header.right() - 12.0;
        for (text, right) in [("K", kills_x), ("D", deaths_x)] {
            paint::text(
                painter,
                pos2(right, header.center().y),
                Align2::RIGHT_CENTER,
                text,
                bold_font(13.0),
                Color32::WHITE,
                0.5,
            );
        }
        painter.rect_stroke(
            section,
            10,
            Stroke::new(1.0, hexa(0xa6dfff, 0.12)),
            StrokeKind::Inside,
        );
        let members = teams[team.index()]
            .tanks
            .iter()
            .filter_map(|&id| rows.iter().find(|row| row.id == id));
        for (index, row) in members.enumerate() {
            let line = Rect::from_min_size(
                pos2(x, header.bottom() + 3.0 + index as f32 * row_height),
                vec2(team_width, row_height),
            );
            let yours = row.id == viewer;
            if yours {
                painter.add(paint::hgradient(
                    line,
                    &[
                        (0.0, hexa(0xffcf38, 0.25)),
                        (0.6, hexa(0xffcf38, 0.1)),
                        (1.0, hexa(0xffcf38, 0.07)),
                    ],
                ));
                painter.rect_filled(
                    Rect::from_min_size(line.min, vec2(4.0, row_height)),
                    0,
                    GOLD,
                );
                for edge in [line.top(), line.bottom()] {
                    painter.line_segment(
                        [pos2(line.left(), edge), pos2(line.right(), edge)],
                        Stroke::new(1.0, hexa(0xffcf38, 0.4)),
                    );
                }
            } else if index % 2 == 1 {
                painter.rect_filled(line, 0, hexa(0xffffff, 0.024));
            }
            let center = line.center().y;
            let size = if row_height >= 30.0 { 16.0 } else { 14.0 };
            paint::text(
                painter,
                pos2(kills_x, center),
                Align2::RIGHT_CENTER,
                &format!("{}", row.kills),
                bold_font(size),
                Color32::WHITE,
                0.0,
            );
            paint::text(
                painter,
                pos2(deaths_x, center),
                Align2::RIGHT_CENTER,
                &format!("{}", row.deaths),
                bold_font(size),
                hex(0xb9c4d0),
                0.0,
            );
            let mut name_right = kills_x - 26.0;
            if Some(row.id) == mvp {
                name_right -=
                    board_tag(painter, pos2(name_right, center), "MVP", true, yours) + 5.0;
            } else if yours && row.name != "YOU" {
                name_right -=
                    board_tag(painter, pos2(name_right, center), "YOU", false, true) + 5.0;
            }
            paint::text_fit(
                painter,
                pos2(line.left() + 12.0, center),
                Align2::LEFT_CENTER,
                &row.name,
                bold_font(if row_height >= 30.0 { 14.0 } else { 12.5 }),
                if yours { GOLD_HOT } else { hex(0xdbe8f4) },
                0.0,
                name_right - line.left() - 16.0,
            );
        }
    }
    let mut height = BOARD_HEADER + body_height;
    if mvp.is_some() {
        let y = origin.y + height + 16.0;
        board_tag(painter, pos2(origin.x + 34.0, y), "MVP", true, false);
        paint::text(
            painter,
            pos2(origin.x + 34.0 + 8.0, y),
            Align2::LEFT_CENTER,
            "Most kills · fewest deaths breaks a tie",
            egui::FontId::proportional(12.5),
            hex(0xa9c2d8),
            0.0,
        );
        height += BOARD_KEY;
    }
    height
}

//! The battle report (`round-recap.ts` and the results branch of `menuMarkup`): the
//! outcome and its sentence, the round's feats, the hero kills tile measured against
//! the personal best, the stat tiles with the standout lit, notable highlights, the
//! personal-bests note, and the team scoreboard with the MVP and the viewer's row.

use egui::{
    Align2, Color32, CornerRadius, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2, pos2, vec2,
};

use super::icons::Art;
use super::menus::{
    by_score, count_line, dialog, dialog_head, dim_backdrop, duration, leading_team, team_label,
};
use super::paint::{self, ButtonStyle, fade, hex, hexa};
use super::theme::{GOLD, GOLD_HOT, bold_font, display_font};
use super::{Action, UiState, View};
use crate::engine_api::{HudState, RecapState, ScoreboardRow};

/// Each metric's label and explanation, in report order (`METRICS`).
const METRICS: [(&str, &str, &str); 13] = [
    ("kills", "Kills", "Enemy tanks destroyed"),
    ("damage", "Damage dealt", "Enemy hull damage, no overkill"),
    ("bestLife", "Best spree", "Most kills in a single life"),
    ("rank", "Top rank", "Peak rank across all lives"),
    (
        "busiestMinute",
        "Busiest minute",
        "Most kills within any 60 seconds",
    ),
    (
        "longestLife",
        "Longest life",
        "Longest time alive, pauses excluded",
    ),
    (
        "multikill",
        "Best multikill",
        "Most kills within any 5 seconds",
    ),
    ("clutchKills", "Clutch kills", "Kills at 25% hull or less"),
    ("revengeKills", "Revenge kills", "Kills on your last killer"),
    (
        "posthumousKills",
        "Grave kills",
        "Kills by a previous life's ordnance",
    ),
    ("mineKills", "Mine kills", "Kills from mine explosions"),
    (
        "coverDestroyed",
        "Cover razed",
        "Destructible objects you finished",
    ),
    ("pickups", "Pickups", "Ammo and power-ups collected"),
];
/// The hero tile and the tiles beside it.
const FEATURED: [&str; 4] = ["kills", "damage", "bestLife", "longestLife"];
/// The metrics worth a line when they happened (a multikill from three kills).
const NOTABLE: [&str; 7] = [
    "rank",
    "multikill",
    "clutchKills",
    "revengeKills",
    "posthumousKills",
    "mineKills",
    "coverDestroyed",
];
const NOTABLE_MULTIKILL: f64 = 3.0;
const MAX_DETAILS: usize = 3;
/// Accuracy worth lighting, over enough shots, and a spree worth lighting.
const STANDOUT_ACCURACY: f64 = 0.6;
const STANDOUT_SHOTS: f64 = 5.0;
const STANDOUT_SPREE: f64 = 3.0;

fn label(metric: &str) -> (&'static str, &'static str) {
    METRICS
        .iter()
        .find(|(key, ..)| *key == metric)
        .map_or(("", ""), |(_, label, hint)| (*label, *hint))
}

/// 1234 as "1,234".
fn thousands(value: f64) -> String {
    let digits = format!("{}", value.round().abs() as u64);
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    if value < 0.0 { format!("-{out}") } else { out }
}

fn format_metric(recap: &RecapState, metric: &str, value: f64) -> String {
    match metric {
        "rank" => recap
            .rank_names
            .get((value.max(0.0) as usize).min(recap.rank_names.len().saturating_sub(1)))
            .cloned()
            .unwrap_or_else(|| "Rookie".to_owned()),
        "longestLife" => duration(value),
        _ => thousands(value),
    }
}

/// The one small tile lit as the round's standout (`standoutTile`).
fn standout(recap: &RecapState) -> &'static str {
    let stat = |key: &str| recap.stats.get(key).copied().unwrap_or(0.0);
    if let Some(record) = FEATURED[1..]
        .iter()
        .find(|metric| recap.improved.iter().any(|improved| improved == *metric))
    {
        return record;
    }
    if recap.shots >= STANDOUT_SHOTS && recap.direct_hits / recap.shots >= STANDOUT_ACCURACY {
        return "accuracy";
    }
    if stat("bestLife") >= STANDOUT_SPREE {
        return "bestLife";
    }
    ""
}

/// The outcome's title, its colour, the sentence under it and whether it ended early.
fn outcome(state: &UiState, hud: &HudState) -> (&'static str, Color32, String) {
    let match_state = &hud.match_state;
    let team = hud.human_team;
    let ended_early = match_state.ended_early;
    let lost_color = hex(0xff6b7a);
    if hud.game_mode == "solo" {
        let survived = match_state.winner == Some(team);
        return if ended_early {
            (
                "BATTLE ENDED",
                Color32::WHITE,
                "Run ended early.".to_owned(),
            )
        } else if survived {
            (
                "SURVIVED",
                GOLD_HOT,
                "Ten minutes. One tank. Still standing.".to_owned(),
            )
        } else {
            let death = state
                .death
                .as_ref()
                .map(|report| format!("{} ", report.sentence))
                .unwrap_or_default();
            (
                "TANK DESTROYED",
                lost_color,
                format!("{death}One more run?"),
            )
        };
    }
    let scores = match_state.scores;
    let leader = leading_team(scores);
    let margin = (scores[0] - scores[1]).abs();
    if ended_early {
        // A battle stopped before the limit is called by where the viewer's team stood.
        return match leader {
            None => (
                "STALEMATE",
                Color32::WHITE,
                "The teams were level when you ended the battle.".to_owned(),
            ),
            Some(leader) if leader == team => (
                "LEAD HELD",
                GOLD_HOT,
                format!(
                    "Your team led by {} when you ended the battle.",
                    count_line(margin, "kill")
                ),
            ),
            Some(leader) => (
                "CUT SHORT",
                lost_color,
                format!(
                    "{} led by {} when you ended the battle.",
                    team_label(&hud.team_names[leader as usize]),
                    count_line(margin, "kill")
                ),
            ),
        };
    }
    let winner = match_state.winner.or(leader);
    let side = |index: u8| {
        if index == team {
            "Your team".to_owned()
        } else {
            team_label(&hud.team_names[index.min(1) as usize])
        }
    };
    let how = if match_state.overtime {
        " in overtime".to_owned()
    } else if margin > 0.0 {
        format!(" by {margin}")
    } else {
        String::new()
    };
    let lede = format!(
        "{} took the round{how}.",
        side(match_state.winner.unwrap_or(0))
    );
    match winner {
        None => ("DRAW", Color32::WHITE, lede),
        Some(winner) if winner == team => ("VICTORY", GOLD_HOT, lede),
        Some(_) => ("DEFEAT", lost_color, lede),
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
    if state.settings_draft.is_some() {
        return;
    }
    dim_backdrop(context);
    let solo = hud.game_mode == "solo";
    let (title, title_color, lede) = outcome(state, &hud);
    let mode = if solo { "SOLO ASSAULT" } else { "TEAM BATTLE" };
    let width = if solo { 680.0 } else { 1080.0 };
    let ended_early = hud.match_state.ended_early;
    dialog(context, "results", width, |ui, painter, origin, width| {
        // A wash of the outcome's colour behind the head.
        painter.add(paint::vgradient(
            Rect::from_min_size(origin - vec2(30.0, 26.0), vec2(width + 60.0, 120.0)),
            &[(0.0, fade(title_color, 0.12)), (1.0, Color32::TRANSPARENT)],
        ));
        let eyebrow = format!("{mode} · {}", hud.map_name.to_uppercase());
        let eyebrow_rect = paint::eyebrow(painter, origin, &eyebrow);
        if ended_early && !solo {
            let tag = Rect::from_min_size(
                pos2(eyebrow_rect.right() + 12.0, origin.y - 2.0),
                vec2(
                    paint::text_width(painter, "ENDED EARLY", bold_font(11.0), 1.4) + 14.0,
                    18.0,
                ),
            );
            painter.rect_stroke(
                tag,
                4,
                Stroke::new(1.0, hexa(0xff6f5e, 0.6)),
                StrokeKind::Inside,
            );
            paint::text(
                painter,
                tag.center(),
                Align2::CENTER_CENTER,
                "ENDED EARLY",
                bold_font(11.0),
                hex(0xffb2a7),
                1.4,
            );
        }
        let mut y = origin.y
            + dialog_head(
                painter,
                origin,
                width,
                "",
                title,
                title_color,
                (!solo).then_some((&hud, hud.match_state.scores)),
            );
        let lede_galley = paint::wrapped(
            painter,
            &lede,
            egui::FontId::proportional(14.0),
            hex(0xc4dbee),
            width * 0.6,
        );
        let lede_height = lede_galley.size().y;
        painter.galley(pos2(origin.x, y), lede_galley, Color32::WHITE);
        y += lede_height + 18.0;
        let you_width = if solo { width } else { width * 0.55 };
        let mut body = 0.0f32;
        if let Some(recap) = &hud.recap {
            body = recap_body(painter, art, pos2(origin.x, y), you_width, &hud, recap);
        }
        if !solo {
            let board_left = origin.x + you_width + 18.0;
            body = body.max(board(
                painter,
                pos2(board_left, y),
                origin.x + width - board_left,
                &hud,
            ));
        }
        y += body + 22.0;
        let height = 48.0;
        let setup = Rect::from_min_size(pos2(origin.x + width - 200.0, y), vec2(200.0, height));
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
            Rect::from_min_size(pos2(setup.left() - 12.0 - 200.0, y), vec2(200.0, height)),
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
    });
}

/// The viewer's part of the report; returns the height used.
fn recap_body(
    painter: &Painter,
    art: &Art,
    origin: Pos2,
    width: f32,
    hud: &HudState,
    recap: &RecapState,
) -> f32 {
    let stat = |key: &str| recap.stats.get(key).copied().unwrap_or(0.0);
    let best = |key: &str| recap.best.get(key).copied().unwrap_or(0.0);
    let improved = |key: &str| recap.improved.iter().any(|metric| metric == key);
    let mut y = origin.y;
    // The round's feats.
    for feat in &recap.feats {
        art.icon(
            painter,
            "star",
            Rect::from_center_size(pos2(origin.x + 8.0, y + 9.0), Vec2::splat(15.0)),
            GOLD,
        );
        let title = paint::text(
            painter,
            pos2(origin.x + 22.0, y + 9.0),
            Align2::LEFT_CENTER,
            &feat.title.to_uppercase(),
            bold_font(13.0),
            GOLD_HOT,
            1.0,
        );
        paint::text_fit(
            painter,
            pos2(title.right() + 8.0, y + 9.0),
            Align2::LEFT_CENTER,
            &feat.detail,
            egui::FontId::proportional(13.0),
            hex(0xc4dbee),
            0.0,
            (origin.x + width - title.right() - 8.0).max(40.0),
        );
        y += 24.0;
    }
    if !recap.feats.is_empty() {
        y += 6.0;
    }
    // The hero tile: kills beside the viewer's tank and the measure against the best.
    let hero = Rect::from_min_size(pos2(origin.x, y), vec2(width, 104.0));
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
    let tank_width = (width * 0.38).min(250.0);
    let tank_team = if hud.game_mode == "solo" {
        0
    } else {
        hud.human_team
    };
    art.tank(
        painter,
        &hud.human.kind,
        tank_team,
        Rect::from_min_max(
            pos2(hero.right() - tank_width, hero.top() + 4.0),
            hero.max - vec2(8.0, 4.0),
        ),
        1.0,
    );
    let kills = stat("kills");
    let number = paint::shadowed(
        painter,
        pos2(hero.left() + 18.0, hero.center().y + 2.0),
        Align2::LEFT_CENTER,
        &thousands(kills),
        display_font(80.0),
        GOLD_HOT,
        0.5,
    );
    let text_x = number.right() + 14.0;
    paint::text(
        painter,
        pos2(text_x, hero.top() + 26.0),
        Align2::LEFT_CENTER,
        "KILLS",
        display_font(20.0),
        Color32::WHITE,
        3.0,
    );
    let progress_width = (hero.right() - tank_width - text_x - 8.0).clamp(80.0, 300.0);
    let record_kills = best("kills");
    let line_y = hero.top() + 72.0;
    let meter = |share: f64| {
        let track = Rect::from_min_size(pos2(text_x, hero.top() + 50.0), vec2(progress_width, 6.0));
        painter.rect_filled(track, 3, hexa(0xffffff, 0.1));
        painter.add(paint::hgradient(
            Rect::from_min_size(
                track.min,
                vec2(track.width() * share.clamp(0.0, 1.0) as f32, 6.0),
            ),
            &[(0.0, hex(0xff9d1c)), (1.0, GOLD_HOT)],
        ));
    };
    let star_line = |text: &str| {
        art.icon(
            painter,
            "star",
            Rect::from_center_size(pos2(text_x + 7.0, line_y - 12.0), Vec2::splat(14.0)),
            GOLD,
        );
        paint::text(
            painter,
            pos2(text_x + 20.0, line_y - 12.0),
            Align2::LEFT_CENTER,
            text,
            bold_font(13.0),
            GOLD_HOT,
            1.0,
        );
    };
    if improved("kills") {
        star_line("NEW PERSONAL BEST");
    } else if record_kills > kills {
        meter(kills / record_kills);
        paint::text_fit(
            painter,
            pos2(text_x, line_y),
            Align2::LEFT_CENTER,
            &format!(
                "{} short of your best · {}",
                record_kills - kills,
                thousands(record_kills)
            ),
            bold_font(12.5),
            hex(0xdbe8f4),
            0.2,
            progress_width,
        );
    } else if kills > 0.0 {
        star_line("Ties your personal best");
    } else {
        meter(0.0);
        paint::text(
            painter,
            pos2(text_x, line_y),
            Align2::LEFT_CENTER,
            "No kills this round",
            bold_font(12.5),
            hex(0xdbe8f4),
            0.2,
        );
    }
    y = hero.bottom() + 8.0;
    // The small tiles.
    let lit = standout(recap);
    let tile_width = (width - 3.0 * 8.0) / 4.0;
    let hit_rate = if recap.shots > 0.0 {
        format!("{}%", (recap.direct_hits / recap.shots * 100.0).round())
    } else {
        "—".to_owned()
    };
    let mut tiles: Vec<(&str, &str, String, String, bool)> = FEATURED[1..]
        .iter()
        .map(|metric| {
            let record = improved(metric);
            let footer = if record {
                "NEW BEST".to_owned()
            } else if best(metric) > stat(metric) {
                format!("BEST {}", format_metric(recap, metric, best(metric)))
            } else {
                String::new()
            };
            (
                *metric,
                label(metric).0,
                format_metric(recap, metric, stat(metric)),
                footer,
                record,
            )
        })
        .collect();
    tiles.push((
        "accuracy",
        "Accuracy",
        hit_rate,
        format!("{} / {} HITS", recap.direct_hits, recap.shots),
        false,
    ));
    for (index, (key, caption, value, footer, record)) in tiles.into_iter().enumerate() {
        let rect = Rect::from_min_size(
            pos2(origin.x + index as f32 * (tile_width + 8.0), y),
            vec2(tile_width, 86.0),
        );
        let standout = key == lit;
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
            pos2(rect.center().x, rect.top() + 13.0),
            Align2::CENTER_TOP,
            &caption.to_uppercase(),
            bold_font(12.0),
            hex(0xa6c4d8),
            1.0,
            rect.width() - 8.0,
        );
        paint::shadowed(
            painter,
            pos2(rect.center().x, rect.top() + 46.0),
            Align2::CENTER_CENTER,
            &value,
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
            let width_text = paint::text_width(painter, &footer, font.clone(), 0.0)
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
                &footer,
                font,
                color,
                0.0,
            );
        }
    }
    y += 86.0;
    // The highlights beyond the tiles, new bests first.
    let notable = |metric: &str| {
        improved(metric)
            || (NOTABLE.contains(&metric)
                && stat(metric)
                    >= if metric == "multikill" {
                        NOTABLE_MULTIKILL
                    } else {
                        1.0
                    })
    };
    let mut extras: Vec<(&str, bool)> = METRICS
        .iter()
        .map(|(key, ..)| *key)
        .filter(|metric| !FEATURED.contains(metric) && stat(metric) > 0.0 && notable(metric))
        .map(|metric| (metric, improved(metric)))
        .collect();
    extras.sort_by_key(|(_, record)| !*record);
    extras.truncate(MAX_DETAILS);
    if !extras.is_empty() {
        y += 10.0;
        let strip = Rect::from_min_size(pos2(origin.x, y), vec2(width, 46.0));
        painter.rect_filled(strip, 10, hexa(0x9fd0ff, 0.08));
        let cell_width = (width - (extras.len() - 1) as f32) / extras.len() as f32;
        for (index, (metric, record)) in extras.iter().enumerate() {
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
                pos2(cell.left() + 12.0, cell.top() + 14.0),
                Align2::LEFT_CENTER,
                &label(metric).0.to_uppercase(),
                bold_font(11.0),
                hex(0xa6c4d8),
                1.0,
            );
            let value = format_metric(recap, metric, stat(metric));
            let mut x = cell.left() + 12.0;
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
                bold_font(15.0),
                if *record { GOLD_HOT } else { Color32::WHITE },
                0.2,
            );
        }
        y += 46.0;
    }
    // Records are kept per mode, map and difficulty; mention them only when something
    // happened.
    let bests = recap.improved.len();
    let note = if !recap.persisted {
        Some("Personal bests couldn't be saved on this computer".to_owned())
    } else if bests > 0 {
        let mut difficulty = hud.difficulty.clone();
        if let Some(first) = difficulty.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        Some(format!(
            "{bests} NEW PERSONAL BEST{} on {} · {difficulty}",
            if bests == 1 { "" } else { "S" },
            hud.map_name
        ))
    } else {
        None
    };
    if let Some(note) = note {
        y += 12.0;
        let mut x = origin.x;
        if recap.persisted {
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
            bold_font(12.5),
            if recap.persisted {
                GOLD_HOT
            } else {
                hex(0xa9c2d8)
            },
            0.4,
        );
        y += 16.0;
    }
    y - origin.y
}

/// The round's most valuable tank: most kills, fewest wrecks breaking a tie.
fn most_valuable(rows: &[ScoreboardRow]) -> Option<u32> {
    let mut sorted: Vec<&ScoreboardRow> = rows.iter().collect();
    sorted.sort_by(|a, b| by_score(a, b));
    sorted
        .first()
        .filter(|row| row.kills > 0.0)
        .map(|row| row.id)
}

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

/// The team scoreboard: each team's tanks by kills with their wrecks, the viewer's
/// row lit and the MVP tagged; returns the height used.
fn board(painter: &Painter, origin: Pos2, width: f32, hud: &HudState) -> f32 {
    let mvp = most_valuable(&hud.scoreboard);
    let viewer = hud.human.id;
    let gap = 8.0;
    let team_width = (width - gap) / 2.0;
    let largest = (0..2)
        .map(|team| hud.scoreboard.iter().filter(|row| row.team == team).count())
        .max()
        .unwrap_or(0);
    let row_height = if largest <= 8 { 30.0 } else { 22.0 };
    let body_height = largest.max(1) as f32 * row_height + 6.0;
    for team in 0..2u8 {
        let x = origin.x + team as f32 * (team_width + gap);
        let side = if team == 0 {
            hex(0x3ea6ff)
        } else {
            hex(0xff5367)
        };
        let section = Rect::from_min_size(pos2(x, origin.y), vec2(team_width, 34.0 + body_height));
        painter.rect_filled(section, 10, hexa(0x060f1f, 0.9));
        let header = Rect::from_min_size(section.min, vec2(team_width, 34.0));
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
        let name = paint::text(
            painter,
            pos2(header.left() + 12.0, header.center().y),
            Align2::LEFT_CENTER,
            &hud.team_names[team as usize].to_uppercase(),
            bold_font(13.0),
            Color32::WHITE,
            2.0,
        );
        paint::text(
            painter,
            pos2(name.right() + 10.0, header.center().y),
            Align2::LEFT_CENTER,
            &format!("{}", hud.match_state.scores[team as usize]),
            display_font(22.0),
            Color32::WHITE,
            0.0,
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
        let mut members: Vec<&ScoreboardRow> = hud
            .scoreboard
            .iter()
            .filter(|row| row.team == team)
            .collect();
        members.sort_by(|a, b| by_score(a, b));
        for (index, row) in members.iter().enumerate() {
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
    let mut height = 34.0 + body_height;
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
        height += 30.0;
    }
    height
}

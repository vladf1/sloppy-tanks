//! The battle report and the pause menu's facts in words: the outcome and its sentence,
//! the stat tiles with their standout, the highlights, the personal-bests note, the team
//! board's order and MVP, and pause's standing line and facts. The page lays them out as
//! markup and the desktop paints them; neither derives them.
//!
//! The report rides in the round recap ([`crate::recap::RoundRecap`]), made once a
//! result shows; pause's facts are made when the pause menu asks ([`pause_report`]).

use std::cmp::Ordering;
use std::fmt;

use serde::Serialize;
use sloppy_core::sim::math::js_round;
use sloppy_core::sim::round_recap::{self, Metric};
use sloppy_core::sim::{GameMode, Team};

use crate::Shell;
use crate::catalog::difficulty_name;
use crate::hud::{Hud, HudScore};
use crate::recap::RoundRecap;

/// Each metric's label and, on hover, its explanation, so the report itself reads at a
/// glance. The report lists metrics in `Metric::ALL` order.
fn label(metric: Metric) -> (&'static str, &'static str) {
    match metric {
        Metric::Kills => ("Kills", "Enemy tanks destroyed"),
        Metric::Damage => ("Damage dealt", "Enemy hull damage, no overkill"),
        Metric::BestLife => ("Best spree", "Most kills in a single life"),
        Metric::Rank => ("Top rank", "Peak rank across all lives"),
        Metric::BusiestMinute => ("Busiest minute", "Most kills within any 60 seconds"),
        Metric::LongestLife => ("Longest life", "Longest time alive, pauses excluded"),
        Metric::Multikill => ("Best multikill", "Most kills within any 5 seconds"),
        Metric::ClutchKills => ("Clutch kills", "Kills at 25% hull or less"),
        Metric::RevengeKills => ("Revenge kills", "Kills on your last killer"),
        Metric::PosthumousKills => ("Grave kills", "Kills by a previous life's ordnance"),
        Metric::MineKills => ("Mine kills", "Kills from mine explosions"),
        Metric::CoverDestroyed => ("Cover razed", "Destructible objects you finished"),
        Metric::Pickups => ("Pickups", "Ammo and power-ups collected"),
    }
}

/// The hero tile and the tiles beside it; peak rank shows with the tank, and the other
/// metrics list below only when they happened.
const FEATURED: [Metric; 4] = [
    Metric::Kills,
    Metric::Damage,
    Metric::BestLife,
    Metric::LongestLife,
];

/// The report's details are its highlights, not a ledger: the metrics worth a line when
/// they happened, at most `MAX_DETAILS` of them, and any other metric only when it set a
/// new personal best. A multikill counts from `NOTABLE_MULTIKILL` kills.
const NOTABLE: [Metric; 7] = [
    Metric::Rank,
    Metric::Multikill,
    Metric::ClutchKills,
    Metric::RevengeKills,
    Metric::PosthumousKills,
    Metric::MineKills,
    Metric::CoverDestroyed,
];
const NOTABLE_MULTIKILL: f64 = 3.0;
const MAX_DETAILS: usize = 3;

/// Accuracy worth lighting: at least this share of direct hits over enough shots.
const STANDOUT_ACCURACY: f64 = 0.6;
const STANDOUT_SHOTS: u32 = 5;
/// A spree worth lighting, in kills in one life.
const STANDOUT_SPREE: f64 = 3.0;

/// How the round came out for the viewer; the page's `data-outcome`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    /// A team battle won, or a solo run survived.
    Won,
    /// A team battle lost, or a solo run's tank destroyed.
    Lost,
    #[default]
    Draw,
    /// A solo run the player ended.
    Ended,
    /// A team battle the player ended, called by where the viewer's team stood.
    EndedAhead,
    EndedBehind,
    EndedEven,
}

impl Outcome {
    /// A win or a lead held, which the report crowns in gold.
    pub fn won(self) -> bool {
        matches!(self, Outcome::Won | Outcome::EndedAhead)
    }

    /// A loss or a battle ended behind.
    pub fn lost(self) -> bool {
        matches!(self, Outcome::Lost | Outcome::EndedBehind)
    }
}

/// The battle report of a finished round.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct BattleReport {
    pub outcome: Outcome,
    /// VICTORY, LEAD HELD, TANK DESTROYED …
    pub title: &'static str,
    /// The sentence under the title. A solo run's loss says "One more run?", which the
    /// shell leads with the sentence of the death that ended it.
    pub lede: String,
    /// Kills (the hero tile, beside the tank), the other featured metrics and accuracy.
    pub tiles: Vec<ReportTile>,
    /// The hero tile's measure of the round's kills against the personal best.
    pub progress: BestProgress,
    /// The highlights beyond the tiles, new bests first.
    pub details: Vec<ReportDetail>,
    /// What the round did to the personal bests; none when there is nothing to say.
    pub note: Option<ReportNote>,
    pub board: ReportBoard,
}

/// A stat tile.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ReportTile {
    pub label: &'static str,
    /// The explanation on hover.
    pub hint: &'static str,
    pub value: String,
    /// "NEW BEST" on a record, the best while it is out of reach ("BEST 900"), the hits
    /// behind the hit rate, or empty. Kills says it in `progress` instead.
    pub footer: String,
    /// The round set a new personal best here, starred with the footer.
    pub record: bool,
    /// The one small tile lit as the round's standout.
    pub standout: bool,
}

/// The hero tile's line under the kills.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct BestProgress {
    pub text: String,
    /// A record set or tied, starred in gold.
    pub record: bool,
    /// The bar's fill toward the best, 0 to 1. A meter only measures a gap: a record or a
    /// tie has none.
    pub meter: Option<f64>,
}

/// A highlight below the tiles.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ReportDetail {
    pub label: &'static str,
    pub hint: String,
    pub value: String,
    /// A new personal best, starred.
    pub record: bool,
}

/// The line under the report about personal bests, which are kept per mode, map and
/// difficulty.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ReportNote {
    /// "2 NEW PERSONAL BESTS", starred and set in bold; none for a warning.
    pub record: Option<String>,
    /// What follows the record ("on PINE VILLAGE · Hard"), or the whole warning.
    pub text: String,
}

/// The team scoreboard: each team's tanks by score and the round's most valuable tank.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ReportBoard {
    pub teams: [BoardTeam; 2],
    /// The MVP's tank id; none before anyone scores.
    pub mvp: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct BoardTeam {
    /// "Blue team".
    pub label: String,
    /// The team's tank ids, most kills first.
    pub tanks: Vec<u32>,
}

/// Pause's battle tab and the line under its title.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PauseReport {
    /// A solo run's tally; none in a team battle.
    pub tally: Option<SoloTally>,
    /// Where a team battle stands; none in a solo run.
    pub standing: Option<Standing>,
    pub facts: Vec<PauseFact>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SoloTally {
    pub kills: u32,
    /// The time survived, `m:ss`.
    pub survived: String,
}

/// Who leads and by how much, and the kills the viewer's team still needs.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Standing {
    pub text: String,
    pub tone: StandingTone,
}

/// Whether the viewer's team is ahead, behind or level: the standing line's colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StandingTone {
    Ahead,
    Behind,
    Level,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PauseFact {
    pub label: &'static str,
    pub value: String,
    /// A small line under the value, or empty.
    pub note: &'static str,
}

/// The report of the round `hud` shows finished, with `recap`'s numbers.
pub fn battle_report(hud: &Hud, recap: &RoundRecap, shell: Shell) -> BattleReport {
    let (outcome, title, lede) = outcome(hud);
    let rows: Vec<HudScore> = hud.scoreboard.rows().collect();
    BattleReport {
        outcome,
        title,
        lede,
        tiles: tiles(recap),
        progress: best_progress(
            recap.stats.get(Metric::Kills),
            recap.best.get(Metric::Kills),
            recap.improved.contains(&Metric::Kills),
        ),
        details: details(recap),
        note: note(hud, recap, shell),
        board: ReportBoard {
            teams: [Team::Blue, Team::Red].map(|team| BoardTeam {
                label: team_label(hud.team_names[team.index()]),
                tanks: ranked(&rows, team).iter().map(|row| row.id).collect(),
            }),
            mvp: most_valuable(&rows),
        },
    }
}

/// How the round came out for the viewer's team, its title and the sentence under it. A
/// battle ended early has no winner: it is called by where the viewer's team stood when
/// it ended, never a victory or a defeat the score never decided.
fn outcome(hud: &Hud) -> (Outcome, &'static str, String) {
    let state = &hud.match_state;
    let team = hud.human_team;
    let ended_early = state.ended_early.unwrap_or(false);
    if hud.game_mode == GameMode::Solo {
        return if ended_early {
            (Outcome::Ended, "BATTLE ENDED", "Run ended early.".into())
        } else if state.winner == Some(team) {
            (
                Outcome::Won,
                "SURVIVED",
                "Ten minutes. One tank. Still standing.".into(),
            )
        } else {
            (Outcome::Lost, "TANK DESTROYED", "One more run?".into())
        };
    }
    let scores = state.scores;
    let leader = leading_team(scores);
    let margin = scores[0].abs_diff(scores[1]);
    if ended_early {
        let (outcome, title) = match leader {
            None => (Outcome::EndedEven, "STALEMATE"),
            Some(leader) if leader == team => (Outcome::EndedAhead, "LEAD HELD"),
            Some(_) => (Outcome::EndedBehind, "CUT SHORT"),
        };
        let lede = match leader {
            None => "The teams were level when you ended the battle.".to_owned(),
            Some(leader) => format!(
                "{} led by {} when you ended the battle.",
                side_label(leader, team, &hud.team_names),
                count_line(margin, "kill")
            ),
        };
        return (outcome, title, lede);
    }
    // The plates carry the scores; the sentence names who was ahead and by how much,
    // never the scores in a second order.
    let side = side_label(state.winner.unwrap_or(Team::Blue), team, &hud.team_names);
    let how = if state.overtime {
        " in overtime".to_owned()
    } else if margin > 0 {
        format!(" by {margin}")
    } else {
        String::new()
    };
    let lede = format!("{side} took the round{how}.");
    match state.winner.or(leader) {
        None => (Outcome::Draw, "DRAW", lede),
        Some(winner) if winner == team => (Outcome::Won, "VICTORY", lede),
        Some(_) => (Outcome::Lost, "DEFEAT", lede),
    }
}

/// A metric's value as the report says it: a rank by name, a time as `m:ss`, a count
/// with thousands separated.
fn metric_value(recap: &RoundRecap, metric: Metric, value: f64) -> String {
    match metric {
        Metric::Rank => {
            let last = recap.rank_names.len().saturating_sub(1);
            let index = (value.max(0.0).floor() as usize).min(last);
            recap.rank_names.get(index).unwrap_or(&"Rookie").to_string()
        }
        Metric::LongestLife => duration(value).to_string(),
        _ => thousands(value),
    }
}

/// Kills first, which the report draws as the hero tile, then the featured metrics and
/// the hit rate.
fn tiles(recap: &RoundRecap) -> Vec<ReportTile> {
    let lit = standout(recap);
    let mut tiles: Vec<ReportTile> = FEATURED
        .iter()
        .map(|&metric| {
            let (label, hint) = label(metric);
            let (stat, best) = (recap.stats.get(metric), recap.best.get(metric));
            let record = recap.improved.contains(&metric);
            // A tile only mentions its record when it was beaten or is still out of reach.
            let footer = if metric == Metric::Kills {
                String::new()
            } else if record {
                "NEW BEST".into()
            } else if best > stat {
                format!("BEST {}", metric_value(recap, metric, best))
            } else {
                String::new()
            };
            ReportTile {
                label,
                hint,
                value: metric_value(recap, metric, stat),
                footer,
                record,
                standout: lit == Some(Lit::Metric(metric)),
            }
        })
        .collect();
    let accuracy = Some(recap.shots)
        .filter(|&shots| shots > 0)
        .map(|shots| f64::from(recap.direct_hits) / f64::from(shots));
    tiles.push(ReportTile {
        label: "Accuracy",
        hint: "Direct hits / projectiles fired, splash excluded",
        value: accuracy.map_or_else(|| "—".into(), |rate| format!("{}%", js_round(rate * 100.0))),
        footer: format!("{} / {} HITS", recap.direct_hits, recap.shots),
        record: false,
        standout: lit == Some(Lit::Accuracy),
    });
    tiles
}

/// A small tile the report can light: a metric's, or the hit rate.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Lit {
    Metric(Metric),
    Accuracy,
}

/// The one small tile lit as the round's standout: a personal best first, then sharp
/// shooting, then a long spree; none when nothing stands out. Kills has the hero tile
/// of its own.
fn standout(recap: &RoundRecap) -> Option<Lit> {
    if let Some(&record) = FEATURED[1..]
        .iter()
        .find(|metric| recap.improved.contains(metric))
    {
        return Some(Lit::Metric(record));
    }
    if recap.shots >= STANDOUT_SHOTS
        && f64::from(recap.direct_hits) / f64::from(recap.shots) >= STANDOUT_ACCURACY
    {
        return Some(Lit::Accuracy);
    }
    (recap.stats.get(Metric::BestLife) >= STANDOUT_SPREE).then_some(Lit::Metric(Metric::BestLife))
}

/// The hero tile's measure of the round's kills against the personal best.
fn best_progress(kills: f64, best: f64, improved: bool) -> BestProgress {
    // A meter only measures a gap: a record or a tie is named in gold, with no bar that
    // fills toward nothing.
    let (text, record, meter) = if improved {
        ("NEW PERSONAL BEST".to_owned(), true, None)
    } else if best > kills {
        let short = thousands(best - kills);
        let text = format!("{short} short of your best · {}", thousands(best));
        (text, false, Some((kills / best).clamp(0.0, 1.0)))
    } else if kills > 0.0 {
        // The kills beside it already say the number: the line only names the record.
        ("Ties your personal best".to_owned(), true, None)
    } else {
        ("No kills this round".to_owned(), false, Some(0.0))
    };
    BestProgress {
        text,
        record,
        meter,
    }
}

/// The highlights: notable metrics that happened, and any metric with a new best.
fn details(recap: &RoundRecap) -> Vec<ReportDetail> {
    let notable = |metric: Metric| {
        let threshold = if metric == Metric::Multikill {
            NOTABLE_MULTIKILL
        } else {
            1.0
        };
        recap.improved.contains(&metric)
            || (NOTABLE.contains(&metric) && recap.stats.get(metric) >= threshold)
    };
    // New bests lead; the rest keep the report's order.
    let (mut details, rest): (Vec<ReportDetail>, Vec<ReportDetail>) = Metric::ALL
        .into_iter()
        .filter(|&metric| {
            !FEATURED.contains(&metric) && recap.stats.get(metric) > 0.0 && notable(metric)
        })
        .map(|metric| {
            let (label, hint) = label(metric);
            let record = recap.improved.contains(&metric);
            ReportDetail {
                label,
                hint: if record {
                    format!("{hint} · new personal best")
                } else {
                    hint.to_owned()
                },
                value: metric_value(recap, metric, recap.stats.get(metric)),
                record,
            }
        })
        .partition(|detail| detail.record);
    details.extend(rest);
    details.truncate(MAX_DETAILS);
    details
}

/// Records are kept per mode, map and difficulty: the note mentions them only when
/// something happened, or when they could not be kept.
fn note(hud: &Hud, recap: &RoundRecap, shell: Shell) -> Option<ReportNote> {
    let bests = recap.improved.len();
    let (record, text) = if !recap.persisted {
        let place = match shell {
            Shell::Page => "in this browser",
            Shell::Desktop => "on this computer",
        };
        (None, format!("Personal bests couldn't be saved {place}"))
    } else if bests > 0 {
        let plural = if bests == 1 { "" } else { "S" };
        let record = format!("{bests} NEW PERSONAL BEST{plural}");
        let place = format!("on {} · {}", hud.map_name, difficulty_name(hud.difficulty));
        (Some(record), place)
    } else {
        return None;
    };
    Some(ReportNote { record, text })
}

/// Pause's facts and standing for the battle `hud` shows: a solo run's tally, then the
/// objective, the clock and the viewer's tank and round so far.
pub fn pause_report(hud: &Hud) -> PauseReport {
    let human = &hud.human;
    let solo = hud.game_mode == GameMode::Solo;
    let state = &hud.match_state;
    let clock = if hud.endless_match {
        format!("{} played", duration(hud.elapsed))
    } else if state.overtime {
        "Overtime · next kill wins".to_owned()
    } else {
        format!("{} left", duration(state.time.ceil()))
    };
    let objective = if solo {
        "Survive".to_owned()
    } else if hud.endless_match {
        "Endless battle".to_owned()
    } else {
        format!("First to {}", hud.score_limit)
    };
    let rows: Vec<HudScore> = hud.scoreboard.rows().collect();
    let place = (!solo).then(|| team_place(&rows, human.id)).flatten();
    let last = match place {
        Some((place, of)) => {
            let place = ordinal(place as u64).to_ascii_lowercase();
            ("Team rank", format!("{place} of {of}"), "by kills")
        }
        None => ("Bots", hud.active_enemies.to_string(), "active"),
    };
    let difficulty = difficulty_name(hud.difficulty).to_owned();
    let kills_deaths = format!("{} / {}", human.kills, human.deaths);
    let facts = [
        ("Objective", objective, if solo { "One life" } else { "" }),
        ("Clock", clock, ""),
        ("Difficulty", difficulty, ""),
        ("Your tank", human.vehicle_name.to_owned(), human.rank_name),
        ("Kills / deaths", kills_deaths, "this battle"),
        last,
    ];
    PauseReport {
        tally: solo.then(|| SoloTally {
            kills: human.kills,
            survived: duration(hud.elapsed).to_string(),
        }),
        standing: (!solo).then(|| pause_standing(hud)),
        facts: facts
            .map(|(label, value, note)| PauseFact { label, value, note })
            .into(),
    }
}

/// Who leads, in words, and how many kills the viewer's team still needs (none in an
/// endless battle or overtime, where the next kill decides).
fn pause_standing(hud: &Hud) -> Standing {
    let scores = hud.match_state.scores;
    let team = hud.human_team;
    let mut text = standing_line(scores, team, &hud.team_names);
    if !hud.endless_match && hud.score_limit > 0 && !hud.match_state.overtime {
        let needed = hud.score_limit.saturating_sub(scores[team.index()]);
        text = format!("{text} · {} to win", count_line(needed, "kill"));
    }
    let tone = match leading_team(scores) {
        None => StandingTone::Level,
        Some(leader) if leader == team => StandingTone::Ahead,
        Some(_) => StandingTone::Behind,
    };
    Standing { text, tone }
}

/// `m:ss`, the menus' and the HUD's one clock format. Negative times read 0:00.
pub fn duration(seconds: f64) -> impl fmt::Display {
    round_recap::duration(seconds.max(0.0))
}

/// 1234 as "1,234": one English format on every shell, whatever the browser's locale.
pub fn thousands(value: f64) -> String {
    let rounded = js_round(value);
    let digits = rounded.abs().to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    if rounded < 0.0 {
        grouped.push('-');
    }
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

/// 1ST, 2ND, 3RD, 4TH … 11TH, 12TH, 13TH, 21ST.
pub fn ordinal(value: u64) -> String {
    let suffix = if (11..=13).contains(&(value % 100)) {
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

/// "1 kill", "0 deaths", "3 kills": a count and its noun, one vocabulary across the menus.
pub fn count_line(count: u32, noun: &str) -> String {
    format!("{count} {noun}{}", if count == 1 { "" } else { "s" })
}

/// "BIG RIG" in a sentence: "Big Rig". Team and tank names are ASCII; ASCII case keeps
/// Unicode's case tables out of the Wasm.
pub fn title_case(name: &str) -> String {
    let words = name.split(' ').map(|word| {
        let (first, rest) = word.split_at(word.chars().next().map_or(0, char::len_utf8));
        first.to_ascii_uppercase() + &rest.to_ascii_lowercase()
    });
    words.collect::<Vec<_>>().join(" ")
}

/// "BLUE" as the report says it: "Blue team".
fn team_label(name: &str) -> String {
    format!("{} team", title_case(name))
}

/// The `side` as the viewer on `team` reads it: "Your team", or "Blue team".
fn side_label(side: Team, team: Team, names: &[&str; 2]) -> String {
    if side == team {
        "Your team".to_owned()
    } else {
        team_label(names[side.index()])
    }
}

/// The team ahead, or none when the scores are level.
fn leading_team(scores: [u32; 2]) -> Option<Team> {
    match scores[0].cmp(&scores[1]) {
        Ordering::Equal => None,
        Ordering::Greater => Some(Team::Blue),
        Ordering::Less => Some(Team::Red),
    }
}

/// Where a team battle stands for the viewer's `team`: who leads and by how much.
fn standing_line(scores: [u32; 2], team: Team, names: &[&str; 2]) -> String {
    match leading_team(scores) {
        None => format!("Level at {} each", scores[0]),
        Some(leader) => format!(
            "{} leads by {}",
            side_label(leader, team, names),
            scores[0].abs_diff(scores[1])
        ),
    }
}

/// The scoreboard's order: most kills first, then fewest wrecks, then by name. Names
/// compare as the page's `localeCompare` ordered ASCII names: letters alike whatever
/// their case, then lower case first. Other letters follow by code point, which keeps
/// Unicode's case tables out of the Wasm for a tie on kills and wrecks.
fn by_score(a: &HudScore, b: &HudScore) -> Ordering {
    fn folded(name: &str) -> impl Iterator<Item = u8> + '_ {
        name.bytes().map(|byte| byte.to_ascii_lowercase())
    }
    b.kills
        .cmp(&a.kills)
        .then(a.deaths.cmp(&b.deaths))
        .then_with(|| folded(&a.name).cmp(folded(&b.name)))
        .then_with(|| b.name.cmp(&a.name))
}

/// `team`'s rows in the scoreboard's order, ties in roster order.
fn ranked<'a>(rows: &'a [HudScore<'a>], team: Team) -> Vec<&'a HudScore<'a>> {
    let mut members: Vec<&HudScore> = rows.iter().filter(|row| row.team == team).collect();
    members.sort_by(|a, b| by_score(a, b));
    members
}

/// Where the viewer stands on their own team's board, by kills then fewest wrecks:
/// 1-based place and the team's size, or none when the viewer is not listed.
fn team_place(rows: &[HudScore], viewer: u32) -> Option<(usize, usize)> {
    let row = rows.iter().find(|row| row.id == viewer)?;
    let team = ranked(rows, row.team);
    let place = team.iter().position(|other| other.id == viewer)? + 1;
    Some((place, team.len()))
}

/// The round's most valuable tank: the most kills, fewest wrecks breaking a tie; none
/// before anyone scores.
fn most_valuable(rows: &[HudScore]) -> Option<u32> {
    rows.iter()
        .min_by(|a, b| by_score(a, b))
        .filter(|row| row.kills > 0)
        .map(|row| row.id)
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use sloppy_core::sim::difficulty::Difficulty;
    use sloppy_core::sim::round_recap::RecapStats;
    use sloppy_core::sim::{MatchPhase, VehicleKind};

    use super::*;
    use crate::hud::{HudHuman, Scoreboard};

    fn metrics(values: &[(Metric, f64)]) -> RecapStats {
        let mut stats = RecapStats::default();
        for &(metric, value) in values {
            stats.set(metric, value);
        }
        stats
    }

    fn recap(stats: &[(Metric, f64)], best: &[(Metric, f64)]) -> RoundRecap {
        RoundRecap {
            stats: metrics(stats),
            best: metrics(best),
            persisted: true,
            rank_names: vec!["Rookie", "Veteran", "Elite", "Heroic"],
            ..RoundRecap::default()
        }
    }

    fn row(id: u32, name: &'static str, team: Team, kills: u32, deaths: u32) -> HudScore<'static> {
        HudScore {
            id,
            name: Cow::Borrowed(name),
            kills,
            deaths,
            bot: false,
            team,
            kind: VehicleKind::Balanced,
        }
    }

    /// A team battle on Pine Village, Hard, three kills to four with the viewer on Red.
    fn team_battle(phase: MatchPhase) -> Hud<'static> {
        let mut hud = Hud {
            elapsed: 16.0,
            map_name: "Pine Village".into(),
            difficulty: Difficulty::Hard,
            human_team: Team::Red,
            score_limit: 100,
            active_enemies: 6,
            human: HudHuman {
                id: 1,
                vehicle_name: "BRUISER",
                rank_name: "Rookie",
                kills: 2,
                deaths: 1,
                ..HudHuman::default()
            },
            scoreboard: Scoreboard::Kept(vec![row(1, "YOU", Team::Red, 2, 1)]),
            ..Hud::default()
        };
        hud.match_state.phase = phase;
        hud.match_state.scores = [3, 4];
        hud.match_state.time = 283.2;
        hud.match_state.winner = Some(Team::Red);
        hud
    }

    fn report(hud: &Hud, recap: &RoundRecap) -> BattleReport {
        battle_report(hud, recap, Shell::Page)
    }

    fn tile<'a>(report: &'a BattleReport, label: &str) -> &'a ReportTile {
        report
            .tiles
            .iter()
            .find(|tile| tile.label == label)
            .unwrap()
    }

    #[test]
    fn a_first_round_reports_its_values_without_record_callouts() {
        let stats = [
            (Metric::Kills, 3.0),
            (Metric::Damage, 1234.0),
            (Metric::LongestLife, 95.0),
            (Metric::Rank, 2.0),
        ];
        let report = report(&team_battle(MatchPhase::Results), &recap(&stats, &stats));
        let values: Vec<(&str, &str)> = report
            .tiles
            .iter()
            .map(|tile| (tile.label, tile.value.as_str()))
            .collect();
        assert_eq!(
            values,
            [
                ("Kills", "3"),
                ("Damage dealt", "1,234"),
                ("Best spree", "0"),
                ("Longest life", "1:35"),
                ("Accuracy", "—"),
            ]
        );
        // The kills tile measures the round against the best; the peak rank is a detail.
        assert_eq!(
            report.progress,
            BestProgress {
                text: "Ties your personal best".into(),
                record: true,
                meter: None,
            }
        );
        assert_eq!(report.details.len(), 1);
        assert_eq!(
            (report.details[0].label, report.details[0].value.as_str()),
            ("Top rank", "Elite")
        );
        assert!(report.tiles.iter().all(|tile| !tile.record));
        assert!(report.tiles[1..4].iter().all(|tile| tile.footer.is_empty()));
        assert_eq!(report.note, None);
    }

    #[test]
    fn improved_records_are_starred_and_unreached_records_show_the_best() {
        let mut recap = recap(
            &[
                (Metric::Kills, 5.0),
                (Metric::Damage, 400.0),
                (Metric::MineKills, 2.0),
            ],
            &[
                (Metric::Kills, 5.0),
                (Metric::Damage, 900.0),
                (Metric::MineKills, 2.0),
            ],
        );
        recap.improved = vec![Metric::Kills, Metric::MineKills];
        recap.shots = 20;
        recap.direct_hits = 13;
        let report = report(&team_battle(MatchPhase::Results), &recap);
        assert!(report.tiles[0].record);
        assert_eq!(report.progress.text, "NEW PERSONAL BEST");
        assert!(report.progress.record && report.progress.meter.is_none());
        let damage = tile(&report, "Damage dealt");
        assert_eq!(
            (damage.value.as_str(), damage.footer.as_str()),
            ("400", "BEST 900")
        );
        let accuracy = tile(&report, "Accuracy");
        assert_eq!(
            (accuracy.value.as_str(), accuracy.footer.as_str()),
            ("65%", "13 / 20 HITS")
        );
        assert_eq!(
            report.details,
            [ReportDetail {
                label: "Mine kills",
                hint: "Kills from mine explosions · new personal best".into(),
                value: "2".into(),
                record: true,
            }]
        );
        assert_eq!(
            report.note,
            Some(ReportNote {
                record: Some("2 NEW PERSONAL BESTS".into()),
                text: "on Pine Village · Hard".into(),
            })
        );
    }

    #[test]
    fn a_new_best_on_a_featured_tile_says_so_in_its_footer() {
        let mut recap = recap(&[(Metric::Damage, 1840.0)], &[(Metric::Damage, 1840.0)]);
        recap.improved = vec![Metric::Damage];
        let report = report(&team_battle(MatchPhase::Results), &recap);
        let damage = tile(&report, "Damage dealt");
        assert!(damage.record && damage.standout);
        assert_eq!(damage.footer, "NEW BEST");
        assert_eq!(
            report.note.unwrap().record.as_deref(),
            Some("1 NEW PERSONAL BEST")
        );
    }

    #[test]
    fn the_kills_meter_measures_the_gap_to_the_best() {
        assert_eq!(
            best_progress(7.0, 11.0, false),
            BestProgress {
                text: "4 short of your best · 11".into(),
                record: false,
                meter: Some(7.0 / 11.0),
            }
        );
        assert_eq!(
            best_progress(0.0, 0.0, false),
            BestProgress {
                text: "No kills this round".into(),
                record: false,
                meter: Some(0.0),
            }
        );
        assert_eq!(
            best_progress(1234.0, 2000.0, false).text,
            "766 short of your best · 2,000"
        );
    }

    #[test]
    fn storage_that_refuses_records_says_so_in_each_shells_words() {
        let mut recap = recap(&[], &[]);
        recap.persisted = false;
        recap.improved = vec![Metric::Kills];
        let hud = team_battle(MatchPhase::Results);
        let note = |shell| battle_report(&hud, &recap, shell).note.unwrap();
        assert_eq!(
            note(Shell::Page),
            ReportNote {
                record: None,
                text: "Personal bests couldn't be saved in this browser".into(),
            }
        );
        assert_eq!(
            note(Shell::Desktop).text,
            "Personal bests couldn't be saved on this computer"
        );
    }

    #[test]
    fn the_reports_details_are_highlights_not_a_ledger() {
        let stats = [
            (Metric::Kills, 2.0),
            (Metric::BusiestMinute, 2.0),
            (Metric::Multikill, 2.0),
            (Metric::Pickups, 1.0),
            (Metric::RevengeKills, 1.0),
        ];
        let hud = team_battle(MatchPhase::Results);
        let labels = |recap: &RoundRecap| -> Vec<&'static str> {
            report(&hud, recap)
                .details
                .iter()
                .map(|detail| detail.label)
                .collect()
        };
        assert_eq!(labels(&recap(&stats, &stats)), ["Revenge kills"]);
        let quiet = [(Metric::Kills, 1.0), (Metric::Pickups, 3.0)];
        assert!(labels(&recap(&quiet, &[])).is_empty());
        let triple = [(Metric::Kills, 3.0), (Metric::Multikill, 3.0)];
        assert_eq!(labels(&recap(&triple, &[])), ["Best multikill"]);
        // A new best on any metric earns a line and leads, up to three lines.
        let busy = [
            (Metric::Rank, 1.0),
            (Metric::ClutchKills, 1.0),
            (Metric::RevengeKills, 1.0),
            (Metric::Pickups, 4.0),
        ];
        let mut improved = recap(&busy, &busy);
        improved.improved = vec![Metric::Pickups];
        assert_eq!(labels(&improved), ["Pickups", "Top rank", "Clutch kills"]);
    }

    #[test]
    fn the_report_lights_one_standout_tile_a_record_then_sharp_shooting_then_a_spree() {
        let hud = team_battle(MatchPhase::Results);
        let lit = |recap: &RoundRecap| -> Vec<&'static str> {
            report(&hud, recap)
                .tiles
                .iter()
                .filter(|tile| tile.standout)
                .map(|tile| tile.label)
                .collect()
        };
        let mut damage = recap(&[], &[]);
        damage.improved = vec![Metric::Damage];
        damage.shots = 10;
        damage.direct_hits = 9;
        assert_eq!(lit(&damage), ["Damage dealt"]);
        let mut sharp = recap(&[(Metric::BestLife, 4.0)], &[]);
        sharp.shots = 10;
        sharp.direct_hits = 8;
        assert_eq!(lit(&sharp), ["Accuracy"]);
        // A lucky shot or two is not sharp shooting.
        let mut lucky = recap(&[], &[]);
        lucky.shots = 2;
        lucky.direct_hits = 2;
        assert!(lit(&lucky).is_empty());
        assert_eq!(lit(&recap(&[(Metric::BestLife, 3.0)], &[])), ["Best spree"]);
        assert!(lit(&recap(&[], &[])).is_empty());
    }

    #[test]
    fn a_rank_beyond_the_names_reads_as_the_last_one() {
        let hud = team_battle(MatchPhase::Results);
        let heroic = report(&hud, &recap(&[(Metric::Rank, 9.0)], &[]));
        assert_eq!(heroic.details[0].value, "Heroic");
        let mut nameless = recap(&[(Metric::Rank, 1.0)], &[]);
        nameless.rank_names.clear();
        assert_eq!(report(&hud, &nameless).details[0].value, "Rookie");
    }

    #[test]
    fn a_battle_ended_early_is_called_by_where_the_team_stood_never_won() {
        let mut hud = team_battle(MatchPhase::Results);
        hud.match_state.ended_early = Some(true);
        hud.match_state.winner = None;
        let called = |hud: &Hud| {
            let report = report(hud, &RoundRecap::default());
            (report.outcome, report.title, report.lede)
        };
        assert_eq!(
            called(&hud),
            (
                Outcome::EndedAhead,
                "LEAD HELD",
                "Your team led by 1 kill when you ended the battle.".into()
            )
        );
        hud.human_team = Team::Blue;
        hud.match_state.scores = [3, 6];
        assert_eq!(
            called(&hud),
            (
                Outcome::EndedBehind,
                "CUT SHORT",
                "Red team led by 3 kills when you ended the battle.".into()
            )
        );
        hud.match_state.scores = [0, 0];
        assert_eq!(
            called(&hud),
            (
                Outcome::EndedEven,
                "STALEMATE",
                "The teams were level when you ended the battle.".into()
            )
        );
        assert!(Outcome::EndedAhead.won() && Outcome::EndedBehind.lost());
        assert!(!Outcome::EndedEven.won() && !Outcome::EndedEven.lost());
    }

    #[test]
    fn a_decided_battle_names_the_winning_side_plainly() {
        let mut hud = team_battle(MatchPhase::Results);
        let called = |hud: &Hud| {
            let report = report(hud, &RoundRecap::default());
            (report.outcome, report.title, report.lede)
        };
        assert_eq!(
            called(&hud),
            (
                Outcome::Won,
                "VICTORY",
                "Your team took the round by 1.".into()
            )
        );
        hud.match_state.scores = [100, 80];
        hud.match_state.winner = Some(Team::Blue);
        assert_eq!(
            called(&hud),
            (
                Outcome::Lost,
                "DEFEAT",
                "Blue team took the round by 20.".into()
            )
        );
        hud.match_state.overtime = true;
        assert_eq!(called(&hud).2, "Blue team took the round in overtime.");
        // A level, undecided result is a draw.
        hud.match_state.scores = [5, 5];
        hud.match_state.winner = None;
        hud.match_state.overtime = false;
        assert_eq!(called(&hud).0, Outcome::Draw);
        assert_eq!(called(&hud).1, "DRAW");
    }

    #[test]
    fn a_solo_run_survives_falls_or_ends_early() {
        let mut hud = team_battle(MatchPhase::Results);
        hud.game_mode = GameMode::Solo;
        hud.human_team = Team::Blue;
        hud.match_state.winner = Some(Team::Blue);
        let called = |hud: &Hud| {
            let report = report(hud, &RoundRecap::default());
            (report.outcome, report.title, report.lede)
        };
        assert_eq!(
            called(&hud),
            (
                Outcome::Won,
                "SURVIVED",
                "Ten minutes. One tank. Still standing.".into()
            )
        );
        hud.match_state.winner = Some(Team::Red);
        assert_eq!(
            called(&hud),
            (Outcome::Lost, "TANK DESTROYED", "One more run?".into())
        );
        hud.match_state.ended_early = Some(true);
        assert_eq!(
            called(&hud),
            (Outcome::Ended, "BATTLE ENDED", "Run ended early.".into())
        );
    }

    #[test]
    fn the_board_lists_each_team_by_kills_and_crowns_one_mvp() {
        let rows = vec![
            row(1, "You", Team::Red, 2, 1),
            row(2, "Ace", Team::Blue, 2, 0),
            row(3, "Bob", Team::Blue, 0, 2),
            row(4, "Cat", Team::Red, 3, 4),
        ];
        // Most kills wins; fewer wrecks breaks a tie.
        assert_eq!(most_valuable(&rows), Some(4));
        assert_eq!(
            most_valuable(&[row(5, "A", Team::Blue, 1, 2), row(6, "B", Team::Red, 1, 0)]),
            Some(6)
        );
        assert_eq!(most_valuable(&[row(5, "A", Team::Blue, 0, 0)]), None);
        let mut hud = team_battle(MatchPhase::Results);
        hud.scoreboard = Scoreboard::Kept(rows);
        let board = report(&hud, &RoundRecap::default()).board;
        assert_eq!(board.mvp, Some(4));
        assert_eq!(board.teams[0].label, "Blue team");
        assert_eq!(board.teams[0].tanks, [2, 3]);
        assert_eq!(board.teams[1].tanks, [4, 1]);
    }

    #[test]
    fn ties_on_the_board_order_names_as_the_page_did() {
        let mut rows = [
            row(1, "YOU", Team::Blue, 1, 1),
            row(2, "Yankee", Team::Blue, 1, 1),
            row(3, "ace", Team::Blue, 1, 1),
            row(4, "Ace", Team::Blue, 1, 1),
        ];
        rows.sort_by(by_score);
        let names: Vec<&str> = rows.iter().map(|row| row.name.as_ref()).collect();
        assert_eq!(names, ["ace", "Ace", "Yankee", "YOU"]);
    }

    #[test]
    fn pause_places_the_viewer_on_their_own_team_by_kills_then_fewest_wrecks() {
        let rows = [
            row(1, "T1", Team::Red, 2, 1),
            row(2, "T2", Team::Red, 2, 0),
            row(3, "T3", Team::Red, 0, 0),
            row(4, "T4", Team::Blue, 9, 0),
        ];
        assert_eq!(team_place(&rows, 1), Some((2, 3)));
        assert_eq!(team_place(&rows, 99), None);
    }

    #[test]
    fn pause_names_who_leads_and_by_how_much_from_the_viewers_side() {
        let names = ["BLUE", "RED"];
        assert_eq!(
            standing_line([3, 4], Team::Red, &names),
            "Your team leads by 1"
        );
        assert_eq!(
            standing_line([7, 2], Team::Red, &names),
            "Blue team leads by 5"
        );
        assert_eq!(standing_line([2, 2], Team::Blue, &names), "Level at 2 each");
    }

    #[test]
    fn pause_lists_a_team_battles_facts_and_what_the_team_still_needs() {
        let hud = team_battle(MatchPhase::Paused);
        let pause = pause_report(&hud);
        assert_eq!(pause.tally, None);
        assert_eq!(
            pause.standing,
            Some(Standing {
                text: "Your team leads by 1 · 96 kills to win".into(),
                tone: StandingTone::Ahead,
            })
        );
        let facts: Vec<(&str, &str, &str)> = pause
            .facts
            .iter()
            .map(|fact| (fact.label, fact.value.as_str(), fact.note))
            .collect();
        assert_eq!(
            facts,
            [
                ("Objective", "First to 100", ""),
                ("Clock", "4:44 left", ""),
                ("Difficulty", "Hard", ""),
                ("Your tank", "BRUISER", "Rookie"),
                ("Kills / deaths", "2 / 1", "this battle"),
                ("Team rank", "1st of 1", "by kills"),
            ]
        );
    }

    #[test]
    fn pause_drops_the_kills_needed_where_the_next_kill_decides() {
        let mut hud = team_battle(MatchPhase::Paused);
        hud.match_state.scores = [99, 4];
        let standing = pause_report(&hud).standing.unwrap();
        assert_eq!(standing.text, "Blue team leads by 95 · 96 kills to win");
        assert_eq!(standing.tone, StandingTone::Behind);
        hud.match_state.scores = [4, 99];
        assert!(
            pause_report(&hud)
                .standing
                .unwrap()
                .text
                .ends_with("· 1 kill to win")
        );
        hud.match_state.overtime = true;
        let pause = pause_report(&hud);
        assert_eq!(pause.standing.unwrap().text, "Your team leads by 95");
        assert_eq!(pause.facts[1].value, "Overtime · next kill wins");
        hud.match_state.overtime = false;
        hud.endless_match = true;
        hud.match_state.scores = [4, 4];
        let pause = pause_report(&hud);
        assert_eq!(
            pause.standing,
            Some(Standing {
                text: "Level at 4 each".into(),
                tone: StandingTone::Level,
            })
        );
        assert_eq!(pause.facts[0].value, "Endless battle");
        assert_eq!(pause.facts[1].value, "0:16 played");
    }

    #[test]
    fn a_solo_pause_tallies_the_run_and_counts_the_bots() {
        let mut hud = team_battle(MatchPhase::Paused);
        hud.game_mode = GameMode::Solo;
        hud.elapsed = 312.0;
        let pause = pause_report(&hud);
        assert_eq!(
            pause.tally,
            Some(SoloTally {
                kills: 2,
                survived: "5:12".into(),
            })
        );
        assert_eq!(pause.standing, None);
        assert_eq!(
            (pause.facts[0].value.as_str(), pause.facts[0].note),
            ("Survive", "One life")
        );
        let bots = &pause.facts[5];
        assert_eq!(
            (bots.label, bots.value.as_str(), bots.note),
            ("Bots", "6", "active")
        );
    }

    #[test]
    fn the_menus_count_in_plain_words_and_name_teams_plainly() {
        assert_eq!(
            [(0, "death"), (1, "death"), (2, "kill")].map(|(count, noun)| count_line(count, noun)),
            ["0 deaths", "1 death", "2 kills"]
        );
        assert_eq!(team_label("BLUE"), "Blue team");
        assert_eq!(title_case("BIG RIG"), "Big Rig");
    }

    #[test]
    fn ordinals_read_as_places() {
        let places = [1, 2, 3, 4, 11, 12, 13, 21, 22, 101, 111].map(ordinal);
        assert_eq!(
            places,
            [
                "1ST", "2ND", "3RD", "4TH", "11TH", "12TH", "13TH", "21ST", "22ND", "101ST",
                "111TH"
            ]
        );
    }

    #[test]
    fn clocks_and_counts_have_one_format() {
        let clock = |seconds: f64| duration(seconds).to_string();
        assert_eq!(clock(95.0), "1:35");
        assert_eq!(clock(5.9), "0:05");
        assert_eq!(clock(600.0), "10:00");
        // A clock never reads below zero.
        assert_eq!(clock(-5.0), "0:00");
        assert_eq!(clock(f64::NAN), "0:00");
        assert_eq!(thousands(0.0), "0");
        assert_eq!(thousands(999.0), "999");
        assert_eq!(thousands(1234.0), "1,234");
        assert_eq!(thousands(1234567.4), "1,234,567");
        assert_eq!(thousands(-1234.0), "-1,234");
    }
}

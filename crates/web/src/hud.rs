//! Shared page HUD records and ammo, repair and scoreboard rules for local and room play,
//! with the HUD's text: the hull's tone and colour, the running effects, the rank's
//! benefits, the score pill and Battle Setup's loading line.
//!
//! The page reads them as `hud_json`; the desktop shell reads the same values typed.
//! Read, they borrow the simulation or the displayed scene, so the page's frequent
//! `hud_json` copies nothing on the way to its text; [`Hud::into_owned`] keeps one between
//! reads.

use std::borrow::Cow;
use std::fmt;
use std::sync::LazyLock;

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use sloppy_core::sim::ammunition::{AMMO_ORDER, equipped_weapon_for, has_ammo_for};
use sloppy_core::sim::data::{SCORE_LIMIT, TEAM_NAMES, vehicle};
use sloppy_core::sim::difficulty::Difficulty;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::render_state::RenderTank;
use sloppy_core::sim::simulation::SpeedTuning;
use sloppy_core::sim::veterancy::{RANKS, REPAIR_DELAY, rank_index};
use sloppy_core::sim::{
    AmmoInventory, GameMode, Match, MatchPhase, RenderState, Simulation, Tank, Team, VehicleKind,
    Weapon,
};
use sloppy_render::presentation::hud::{HealthColor, health_bar_state};

use crate::battle_report::duration;
use crate::recap::RoundRecap;

/// `Game.hud_json`: the page HUD's view of the match and the human's tank.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hud<'a> {
    #[serde(rename = "match")]
    pub match_state: Match,
    pub elapsed: f64,
    pub game_mode: GameMode,
    pub endless_match: bool,
    pub map_mode: MapId,
    pub map_name: String,
    pub difficulty: Difficulty,
    pub human_team: Team,
    pub score_limit: u32,
    pub team_names: [&'static str; 2],
    pub active_enemies: usize,
    pub speed_tuning: SpeedTuning,
    pub human: HudHuman,
    pub scoreboard: Scoreboard<'a>,
    /// The finished round's recap, while results show.
    pub recap: Option<Cow<'a, RoundRecap>>,
    /// The score pill's goal line and clock.
    pub score_pill: ScorePill,
}

impl Default for Hud<'_> {
    /// A team battle of the village before its first round, with an empty scoreboard.
    fn default() -> Self {
        let match_state = Match {
            phase: MatchPhase::Ready,
            time: 0.0,
            scores: [0, 0],
            overtime: false,
            ended_early: None,
            winner: None,
            round: 0,
        };
        Hud {
            score_pill: ScorePill::local(&match_state, GameMode::Team, false, 0.0, SCORE_LIMIT),
            match_state,
            elapsed: 0.0,
            game_mode: GameMode::Team,
            endless_match: false,
            map_mode: MapId::Village,
            map_name: String::new(),
            difficulty: Difficulty::Normal,
            human_team: Team::Blue,
            score_limit: SCORE_LIMIT,
            team_names: TEAM_NAMES,
            active_enemies: 0,
            speed_tuning: SpeedTuning::default(),
            human: HudHuman::default(),
            scoreboard: Scoreboard::Kept(Vec::new()),
            recap: None,
        }
    }
}

impl Hud<'_> {
    /// A copy that outlives the game it was read from.
    pub fn into_owned(self) -> Hud<'static> {
        Hud {
            match_state: self.match_state,
            elapsed: self.elapsed,
            game_mode: self.game_mode,
            endless_match: self.endless_match,
            map_mode: self.map_mode,
            map_name: self.map_name,
            difficulty: self.difficulty,
            human_team: self.human_team,
            score_limit: self.score_limit,
            team_names: self.team_names,
            active_enemies: self.active_enemies,
            speed_tuning: self.speed_tuning,
            human: self.human,
            scoreboard: self.scoreboard.into_owned(),
            recap: self.recap.map(|recap| Cow::Owned(recap.into_owned())),
            score_pill: self.score_pill,
        }
    }
}

/// Single player's HUD. The human's tank as the HUD reads it is refilled into `tank`
/// from the simulation, not the last frame's state: debug hooks change tanks between
/// frames. `recap` is the finished round's, while results show; `timers` learn how long
/// the human's effects run.
pub fn simulation_hud<'a>(
    sim: &'a Simulation,
    tank: &mut RenderTank,
    recap: Option<&'a RoundRecap>,
    timers: &mut EffectTimers,
) -> Hud<'a> {
    sim.fill_tank(tank, sim.human());
    Hud {
        match_state: sim.match_state.clone(),
        elapsed: sim.elapsed,
        game_mode: sim.game_mode,
        endless_match: sim.endless_match,
        map_mode: sim.map_mode,
        map_name: sim.map_name(),
        difficulty: sim.difficulty,
        human_team: sim.human_team,
        score_limit: SCORE_LIMIT,
        team_names: TEAM_NAMES,
        active_enemies: sim.tanks.iter().filter(|t| !t.human && t.alive).count(),
        speed_tuning: sim.speed_tuning,
        human: human_json(tank, sim.elapsed, timers),
        scoreboard: Scoreboard::Simulated(&sim.tanks),
        recap: recap.map(Cow::Borrowed),
        score_pill: ScorePill::local(
            &sim.match_state,
            sim.game_mode,
            sim.endless_match,
            sim.elapsed,
            SCORE_LIMIT,
        ),
    }
}

/// `NetGame.hud_json`: a room's HUD for the displayed scene; the room lobby has the
/// rest.
#[derive(Clone, Debug, Serialize)]
pub struct RoomHud<'a> {
    #[serde(rename = "match")]
    pub match_state: &'a Match,
    pub human: HudHuman,
    pub scoreboard: Scoreboard<'a>,
    /// The score pill's clock; a room has no goal line under it.
    #[serde(rename = "scorePill")]
    pub score_pill: ScorePill,
}

/// The room HUD for `state`, or `None` before the scene has its viewer; `timers` learn
/// how long the viewer's effects run.
pub fn room_hud<'a>(state: &'a RenderState, timers: &mut EffectTimers) -> Option<RoomHud<'a>> {
    let viewer = state.viewer()?;
    Some(RoomHud {
        match_state: &state.match_state,
        human: human_json(viewer, state.elapsed, timers),
        scoreboard: Scoreboard::Rendered(&state.tanks),
        score_pill: ScorePill::room(&state.match_state),
    })
}

/// The ammo rack's slots for a `kind` of tank carrying `ammo`, with `equipped` selected.
pub fn hud_ammo(
    kind: VehicleKind,
    ammo: &AmmoInventory,
    equipped: Weapon,
) -> [HudAmmo; AMMO_ORDER.len()] {
    AMMO_ORDER.map(|weapon| HudAmmo {
        weapon,
        count: weapon.special().map(|kind| ammo.get(kind)),
        selected: weapon == equipped,
        available: has_ammo_for(kind, ammo, weapon),
    })
}

fn self_repair_active(alive: bool, repair: f64, hp: f64, max_hp: f64, since_combat: f64) -> bool {
    alive && repair > 0.0 && hp < max_hp && since_combat >= REPAIR_DELAY
}

/// The viewer's HUD block (`hud_json().human`). `elapsed` is the match clock, which
/// decides whether veteran self-repair is running; `timers` learn how long its effects
/// run.
pub fn human_json(tank: &RenderTank, elapsed: f64, timers: &mut EffectTimers) -> HudHuman {
    let rank = rank_index(tank.xp);
    let stats = &RANKS[rank];
    let health = health_bar_state(tank.hp, tank.max_hp);
    let equipped = equipped_weapon_for(tank.kind, &tank.ammo, tank.selected_ammo);
    let ammo = hud_ammo(tank.kind, &tank.ammo, equipped);
    let self_repair = self_repair_active(
        tank.alive,
        stats.repair,
        tank.hp,
        tank.max_hp,
        elapsed - tank.last_combat,
    );
    let mut human = HudHuman {
        id: tank.id,
        kind: tank.kind,
        vehicle_name: vehicle(tank.kind).name,
        alive: tank.alive,
        hp: tank.hp,
        max_hp: tank.max_hp,
        health_ratio: health.ratio,
        rank,
        rank_name: stats.name,
        rank_damage: stats.damage,
        rank_fire_rate: stats.fire_rate,
        rank_health: stats.health,
        rank_repair: stats.repair,
        repair_delay: REPAIR_DELAY,
        ammo,
        mine_cooldown: tank.mine_cooldown,
        protection: tank.protection,
        shield: tank.shield,
        shield_points: tank.shield_points,
        rapid: tank.rapid,
        speed: tank.speed,
        laser: tank.laser,
        respawn: tank.respawn,
        kills: tank.kills,
        deaths: tank.deaths,
        self_repair,
        hull_tone: HullTone::of(health.tone),
        hull_ramp: hull_ramp(health.ratio),
        rank_title: rank_title(rank),
        effects: HudEffects::default(),
    };
    human.effects = timers.effects(&human);
    human
}

/// Every tank's name and kills (`hud_json().scoreboard`), from replicated or simulated
/// tanks, or rows kept between reads.
#[derive(Clone, Debug)]
pub enum Scoreboard<'a> {
    Rendered(&'a [RenderTank]),
    Simulated(&'a [Tank]),
    Kept(Vec<HudScore<'static>>),
}

impl Scoreboard<'_> {
    /// Every tank's row, in roster order.
    pub fn rows(&self) -> impl Iterator<Item = HudScore<'_>> {
        // One chain over the three sources, two of them empty, rather than a boxed
        // iterator: the page serializes this every few frames.
        let (rendered, simulated, kept): (&[RenderTank], &[Tank], &[HudScore]) = match self {
            Self::Rendered(tanks) => (*tanks, &[], &[]),
            Self::Simulated(tanks) => (&[], *tanks, &[]),
            Self::Kept(rows) => (&[], &[], rows.as_slice()),
        };
        rendered
            .iter()
            .map(|tank| HudScore {
                id: tank.id,
                name: Cow::Borrowed(&tank.name),
                kills: tank.kills,
                deaths: tank.deaths,
                team: tank.team,
                kind: tank.kind,
            })
            .chain(simulated.iter().map(|tank| HudScore {
                id: tank.id,
                name: Cow::Borrowed(&tank.name),
                kills: tank.kills,
                deaths: tank.deaths,
                team: tank.team,
                kind: tank.kind,
            }))
            .chain(kept.iter().map(|row| HudScore {
                name: Cow::Borrowed(&row.name),
                ..*row
            }))
    }

    /// The tank `id`'s row.
    pub fn row(&self, id: u32) -> Option<HudScore<'_>> {
        self.rows().find(|row| row.id == id)
    }

    /// Rows that outlive the tanks they were read from.
    pub fn into_owned(self) -> Scoreboard<'static> {
        match self {
            Self::Kept(rows) => Scoreboard::Kept(rows),
            borrowed => Scoreboard::Kept(borrowed.rows().map(HudScore::into_owned).collect()),
        }
    }
}

impl Serialize for Scoreboard<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.rows())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudHuman {
    pub id: u32,
    pub kind: VehicleKind,
    pub vehicle_name: &'static str,
    pub alive: bool,
    pub hp: f64,
    pub max_hp: f64,
    pub health_ratio: f64,
    pub rank: usize,
    pub rank_name: &'static str,
    pub rank_damage: f64,
    pub rank_fire_rate: f64,
    pub rank_health: f64,
    pub rank_repair: f64,
    /// Seconds out of combat before a ranked hull repairs itself.
    pub repair_delay: f64,
    /// One slot per entry of `AMMO_ORDER`.
    pub ammo: [HudAmmo; AMMO_ORDER.len()],
    pub mine_cooldown: f64,
    pub protection: f64,
    pub shield: f64,
    pub shield_points: f64,
    pub rapid: f64,
    pub speed: f64,
    pub laser: f64,
    pub respawn: f64,
    pub kills: u32,
    pub deaths: u32,
    pub self_repair: bool,
    /// How the hull reads at a glance.
    pub hull_tone: HullTone,
    /// The tank panel's hull colour along its ramp ([`hull_ramp`]), sRGB hex.
    pub hull_ramp: u32,
    /// The rank's benefits, for its tooltip ([`rank_title`]).
    pub rank_title: &'static str,
    /// The timed power-ups and protections running, as the tank panel's chips show them.
    pub effects: HudEffects,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct HudAmmo {
    pub weapon: Weapon,
    /// `None` for unlimited Standard shells.
    pub count: Option<f64>,
    pub selected: bool,
    pub available: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HudScore<'a> {
    pub id: u32,
    pub name: Cow<'a, str>,
    pub kills: u32,
    pub deaths: u32,
    pub team: Team,
    /// The tank the death report names; the page never shows it.
    #[serde(skip)]
    pub kind: VehicleKind,
}

impl HudScore<'_> {
    pub fn into_owned(self) -> HudScore<'static> {
        HudScore {
            id: self.id,
            name: Cow::Owned(self.name.into_owned()),
            kills: self.kills,
            deaths: self.deaths,
            team: self.team,
            kind: self.kind,
        }
    }
}

/// How the hull reads at a glance: healthy, damaged or critical, at the overhead bar's
/// thresholds ([`health_bar_state`]), so the panel and the bar over the tank change
/// colour together.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HullTone {
    #[default]
    Healthy,
    Damaged,
    Critical,
}

impl HullTone {
    pub fn of(color: HealthColor) -> Self {
        match color {
            HealthColor::Healthy => HullTone::Healthy,
            HealthColor::Warning => HullTone::Damaged,
            HealthColor::Critical => HullTone::Critical,
        }
    }
}

/// The tank panel's hull colour along its ramp: green while the hull is sound, through
/// amber and orange to red as it drains, so every hit visibly moves the colour toward
/// danger rather than holding one tone between thresholds. Stops are hull shares.
const HULL_RAMP: [(f64, u32); 4] = [
    (0.25, 0xff4d5e),
    (0.4, 0xff8636),
    (0.55, 0xf5c83c),
    (0.7, 0x5fe39a),
];

/// The hull colour at `ratio` of the hull (sRGB hex).
pub fn hull_ramp(ratio: f64) -> u32 {
    let share = ratio.clamp(0.0, 1.0);
    let upper = HULL_RAMP
        .iter()
        .position(|&(stop, _)| share <= stop)
        .unwrap_or(HULL_RAMP.len() - 1);
    let (high_stop, high) = HULL_RAMP[upper];
    let (low_stop, low) = HULL_RAMP[upper.saturating_sub(1)];
    let t = if high_stop == low_stop {
        1.0
    } else {
        ((share - low_stop) / (high_stop - low_stop)).clamp(0.0, 1.0)
    };
    let channel = |shift: u32| {
        let low = f64::from((low >> shift) & 0xff);
        let high = f64::from((high >> shift) & 0xff);
        ((low + (high - low) * t).round() as u32) << shift
    };
    channel(16) | channel(8) | channel(0)
}

/// Each rank's benefits, as its tooltip names them.
static RANK_TITLES: LazyLock<[String; RANKS.len()]> = LazyLock::new(|| {
    std::array::from_fn(|index| {
        if index == 0 {
            return "Earn XP from enemy hull damage and kills. Ranks reset on respawn.".to_owned();
        }
        let rank = &RANKS[index];
        let percent = |value: f64| ((value - 1.0) * 100.0).round();
        let repair = if rank.repair > 0.0 {
            format!(
                " · repairs {}% hull/s after {REPAIR_DELAY}s out of combat",
                (rank.repair * 100.0).round()
            )
        } else {
            String::new()
        };
        format!(
            "+{}% damage · +{}% fire rate · +{}% hull{repair}",
            percent(rank.damage),
            percent(rank.fire_rate),
            percent(rank.health)
        )
    })
});

/// The benefits of rank `rank` (an index of `RANKS`), for its tooltip.
pub fn rank_title(rank: usize) -> &'static str {
    RANK_TITLES.get(rank).map_or("", String::as_str)
}

/// A power-up or protection the tank panel shows as a chip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EffectKind {
    Spawn,
    Shield,
    Rapid,
    Boost,
    Laser,
    Repair,
}

impl EffectKind {
    /// In the chips' order; every kind but self-repair runs on a timer.
    pub const ALL: [EffectKind; 6] = [
        EffectKind::Spawn,
        EffectKind::Shield,
        EffectKind::Rapid,
        EffectKind::Boost,
        EffectKind::Laser,
        EffectKind::Repair,
    ];

    pub fn name(self) -> &'static str {
        match self {
            EffectKind::Spawn => "SPAWN SHIELD",
            EffectKind::Shield => "SHIELD",
            EffectKind::Rapid => "RAPID",
            EffectKind::Boost => "BOOST",
            EffectKind::Laser => "LASER DEFENSE",
            EffectKind::Repair => "SELF-REPAIR",
        }
    }
}

/// The effects that run on a timer, the first of [`EffectKind::ALL`].
const TIMED_EFFECTS: usize = 5;

/// What is left of an effect, as its chip says it after the name.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EffectDetail {
    /// Nothing: a lasting effect, or the spawn shield, whose ring says it.
    None,
    Seconds(f64),
    Shield {
        points: f64,
        seconds: f64,
    },
}

/// A count the HUD shows whole, rounded up as a countdown is.
fn whole(value: f64) -> f64 {
    // Adding zero turns a negative zero, which would print as "-0", into zero.
    value.ceil() + 0.0
}

impl fmt::Display for EffectDetail {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            EffectDetail::None => Ok(()),
            EffectDetail::Seconds(seconds) => write!(formatter, "{}s", whole(seconds)),
            EffectDetail::Shield { points, seconds } => {
                write!(formatter, "{} HP · {}s", whole(points), whole(seconds))
            }
        }
    }
}

impl Serialize for EffectDetail {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// One running effect.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct HudEffect {
    pub kind: EffectKind,
    pub name: &'static str,
    pub detail: EffectDetail,
    /// The share of its time left, for its timer ring; 1 for a lasting effect.
    pub share: f64,
}

/// The running effects in the chips' order, without a list to allocate on each read.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HudEffects([Option<HudEffect>; EffectKind::ALL.len()]);

impl HudEffects {
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &HudEffect> {
        self.0.iter().flatten()
    }
}

impl Serialize for HudEffects {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter())
    }
}

/// The longest time seen on each timed effect since it began, so its ring drains from
/// full whatever the pickup's duration. An effect that ended forgets its length, so the
/// next pickup's ring starts full.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EffectTimers {
    pub longest: [f64; TIMED_EFFECTS],
}

impl EffectTimers {
    /// The effects running on `tank`, learning each timed one's length from this read.
    pub fn effects(&mut self, tank: &HudHuman) -> HudEffects {
        let left = [
            tank.protection,
            tank.shield,
            tank.rapid,
            tank.speed,
            tank.laser,
        ];
        let mut effects = HudEffects::default();
        for (index, (&seconds, longest)) in left.iter().zip(&mut self.longest).enumerate() {
            if seconds <= 0.0 {
                *longest = 0.0;
                continue;
            }
            *longest = longest.max(seconds);
            let kind = EffectKind::ALL[index];
            let detail = match kind {
                EffectKind::Spawn => EffectDetail::None,
                EffectKind::Shield => EffectDetail::Shield {
                    points: tank.shield_points,
                    seconds,
                },
                _ => EffectDetail::Seconds(seconds),
            };
            effects.0[index] = Some(HudEffect {
                kind,
                name: kind.name(),
                detail,
                share: (seconds / *longest).min(1.0),
            });
        }
        if tank.self_repair {
            effects.0[TIMED_EFFECTS] = Some(HudEffect {
                kind: EffectKind::Repair,
                name: EffectKind::Repair.name(),
                detail: EffectDetail::None,
                share: 1.0,
            });
        }
        effects
    }
}

/// What the score pill's clock says instead of the time while overtime waits for a kill.
pub const NEXT_KILL: &str = "NEXT KILL";

/// The goal under the score pill's clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Objective {
    /// A solo run: one life against bots that keep coming. A phone's narrow pill says
    /// less (`src/game/ui.ts`).
    Survive,
    /// An extra level's endless battle.
    EndlessStress,
    /// A race to the score limit.
    FirstTo(u32),
}

impl fmt::Display for Objective {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Objective::Survive => formatter.write_str("SURVIVE · ONE LIFE"),
            Objective::EndlessStress => formatter.write_str("ENDLESS STRESS"),
            Objective::FirstTo(limit) => write!(formatter, "FIRST TO {limit}"),
        }
    }
}

impl Serialize for Objective {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// The score pill between the team plates: its clock and the goal under it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScorePill {
    pub objective: Option<Objective>,
    /// Overtime: the clock says [`NEXT_KILL`] until a kill decides the match.
    pub overtime: bool,
    /// Whole seconds: the time left, or the time survived in an endless battle.
    pub seconds: f64,
}

impl ScorePill {
    /// Single player's pill: a solo run's, an endless level's, or a race to `score_limit`.
    pub fn local(
        match_state: &Match,
        game_mode: GameMode,
        endless: bool,
        elapsed: f64,
        score_limit: u32,
    ) -> Self {
        let objective = if game_mode == GameMode::Solo {
            Objective::Survive
        } else if endless {
            Objective::EndlessStress
        } else {
            Objective::FirstTo(score_limit)
        };
        ScorePill {
            objective: Some(objective),
            overtime: !endless && match_state.overtime,
            seconds: if endless {
                elapsed.max(0.0).floor()
            } else {
                whole(match_state.time).max(0.0)
            },
        }
    }

    /// A room's pill: the time left, and no goal line.
    pub fn room(match_state: &Match) -> Self {
        ScorePill {
            objective: None,
            overtime: match_state.overtime,
            seconds: whole(match_state.time).max(0.0),
        }
    }

    /// What the clock says: [`NEXT_KILL`] in overtime, the time as m:ss otherwise,
    /// written straight into the HUD's text.
    pub fn clock(self) -> impl fmt::Display {
        fmt::from_fn(move |formatter| {
            if self.overtime {
                formatter.write_str(NEXT_KILL)
            } else {
                write!(formatter, "{}", duration(self.seconds))
            }
        })
    }
}

/// The page reads the pill's goal, its clock's text and whether overtime lights it.
impl Serialize for ScorePill {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut pill = serializer.serialize_struct("ScorePill", 3)?;
        pill.serialize_field("objective", &self.objective)?;
        pill.serialize_field("overtime", &self.overtime)?;
        pill.serialize_field("clock", &format_args!("{}", self.clock()))?;
        pill.end()
    }
}

/// Battle Setup's line while the arena prepares: its shaders loading, the GPU compiling
/// them, or its textures loading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Loading {
    Shaders { loaded: u32, total: u32 },
    Compiling,
    Textures { left: u32 },
}

impl Loading {
    /// The line after a preparation step that left the arena unready, with `compiled`
    /// pipelines created since it began preparing, `remaining` still to create,
    /// `textures_pending` to load, and `gpu_pending` while only the GPU works. `None`
    /// while a rebuild that reused every pipeline and texture only waits for its warm-up
    /// frames: that is not loading, so Battle Setup keeps saying it is ready rather than
    /// flicker through these lines.
    pub fn of(
        compiled: u32,
        remaining: u32,
        textures_pending: u32,
        gpu_pending: bool,
    ) -> Option<Self> {
        if compiled == 0 && remaining == 0 && textures_pending == 0 {
            return None;
        }
        Some(if remaining > 0 {
            Loading::Shaders {
                loaded: compiled,
                total: compiled + remaining,
            }
        } else if gpu_pending {
            Loading::Compiling
        } else {
            Loading::Textures {
                left: textures_pending,
            }
        })
    }

    /// The share of the shaders loaded, for a progress bar; `None` while the GPU or the
    /// textures hold the arena up.
    pub fn progress(self) -> Option<f32> {
        match self {
            Loading::Shaders { loaded, total } if total > 0 => {
                Some((loaded as f32 / total as f32).clamp(0.0, 1.0))
            }
            _ => None,
        }
    }
}

impl fmt::Display for Loading {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Loading::Shaders { loaded, total } => {
                write!(formatter, "Shaders loaded: {loaded} of {total}")
            }
            Loading::Compiling => formatter.write_str("Compiling shaders…"),
            Loading::Textures { left } => write!(formatter, "Loading textures… {left} left"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sloppy_core::sim::{RenderState, Simulation, SimulationSetup, Team};

    #[test]
    fn self_repair_requires_a_living_wounded_veteran_past_the_delay() {
        assert!(self_repair_active(true, 1.0, 50.0, 100.0, REPAIR_DELAY));
        assert!(!self_repair_active(
            true,
            1.0,
            50.0,
            100.0,
            REPAIR_DELAY - 0.01
        ));
        assert!(!self_repair_active(false, 1.0, 50.0, 100.0, REPAIR_DELAY));
        assert!(!self_repair_active(true, 0.0, 50.0, 100.0, REPAIR_DELAY));
        assert!(!self_repair_active(true, 1.0, 100.0, 100.0, REPAIR_DELAY));
    }

    /// Single player serializes the simulation's tanks and a room its replicated
    /// copies: both give the same scoreboard, in roster order, with names escaped.
    #[test]
    fn local_and_replicated_scoreboards_match() {
        let mut simulation = Simulation::new(4242.0, SimulationSetup::default());
        let tank = &mut simulation.tanks[0];
        tank.name = "Quote \" slash \\ newline\n火".into();
        tank.team = Team::Red;
        tank.kind = VehicleKind::Humvee;
        tank.human = true;
        tank.kills = 12;
        tank.deaths = 3;
        tank.xp = 500.0;
        tank.alive = false;
        let mut state = RenderState::default();
        simulation.fill_render_state(&mut state, None);
        let text = serde_json::to_string(&Scoreboard::Simulated(&simulation.tanks)).unwrap();
        assert_eq!(
            text,
            serde_json::to_string(&Scoreboard::Rendered(&state.tanks)).unwrap(),
        );
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let tanks = &simulation.tanks;
        assert_eq!(
            value[0],
            json!({
                "id": tanks[0].id,
                "name": tanks[0].name,
                "kills": 12,
                "deaths": 3,
                "team": 1,
            })
        );
        let ids: Vec<_> = value
            .as_array()
            .unwrap()
            .iter()
            .map(|score| score["id"].as_u64().unwrap())
            .collect();
        let roster: Vec<_> = tanks.iter().map(|tank| u64::from(tank.id)).collect();
        assert_eq!(ids, roster);
        assert_eq!(
            serde_json::to_string(&Scoreboard::Simulated(&[])).unwrap(),
            "[]"
        );
        assert_eq!(
            serde_json::to_string(&Scoreboard::Rendered(&[])).unwrap(),
            "[]"
        );
    }

    #[test]
    fn special_ammo_is_equipped_only_while_it_lasts() {
        let mut ammo = AmmoInventory::default();
        let kind = VehicleKind::Balanced;
        assert_eq!(
            equipped_weapon_for(kind, &ammo, Weapon::Rocket),
            Weapon::Standard
        );
        ammo.rocket = 2.0;
        assert_eq!(
            equipped_weapon_for(kind, &ammo, Weapon::Rocket),
            Weapon::Rocket
        );
        let tank = RenderTank {
            kind,
            ammo,
            selected_ammo: Weapon::Rocket,
            hp: 50.0,
            max_hp: 100.0,
            alive: true,
            ..RenderTank::default()
        };
        let human =
            serde_json::to_value(human_json(&tank, 0.0, &mut EffectTimers::default())).unwrap();
        assert_eq!(human["ammo"][2]["selected"], json!(true));
        assert_eq!(human["healthRatio"], json!(0.5));
        assert_eq!(human["repairDelay"], json!(REPAIR_DELAY));
        assert_eq!(human["ammo"][2]["count"], json!(2.0));
        // A Humvee fires its TOW, which no ammo slot offers.
        assert_eq!(
            equipped_weapon_for(
                VehicleKind::Humvee,
                &AmmoInventory::default(),
                Weapon::Standard
            ),
            Weapon::Tow
        );
        let humvee = RenderTank {
            kind: VehicleKind::Humvee,
            alive: false,
            ..RenderTank::default()
        };
        let human =
            serde_json::to_value(human_json(&humvee, 100.0, &mut EffectTimers::default())).unwrap();
        assert!(
            human["ammo"]
                .as_array()
                .unwrap()
                .iter()
                .all(|slot| slot["selected"] == json!(false))
        );
        assert_eq!(human["ammo"][0]["count"], serde_json::Value::Null);
        assert_eq!(human["selfRepair"], json!(false));
    }

    #[test]
    fn the_hull_reads_at_the_overhead_bars_thresholds_never_the_teams() {
        let tone = |ratio: f64| HullTone::of(health_bar_state(ratio * 100.0, 100.0).tone);
        assert_eq!(
            [1.0, 0.61, 0.6, 0.31, 0.3, 0.0].map(tone),
            [
                HullTone::Healthy,
                HullTone::Healthy,
                HullTone::Damaged,
                HullTone::Damaged,
                HullTone::Critical,
                HullTone::Critical,
            ]
        );
        let tank = RenderTank {
            hp: 30.0,
            max_hp: 100.0,
            alive: true,
            ..RenderTank::default()
        };
        let human = serde_json::to_value(human_json(&tank, 0.0, &mut EffectTimers::default()));
        assert_eq!(human.unwrap()["hullTone"], json!("critical"));
    }

    #[test]
    fn the_panels_hull_colour_ramps_from_green_through_orange_to_red() {
        assert_eq!(hull_ramp(1.0), 0x5fe39a);
        assert_eq!(hull_ramp(0.7), 0x5fe39a);
        assert_eq!(hull_ramp(0.4), 0xff8636);
        assert_eq!(hull_ramp(0.25), 0xff4d5e);
        assert_eq!(hull_ramp(0.0), 0xff4d5e);
        assert_eq!(hull_ramp(0.48), 0xfaa939);
        // Each step down the hull moves the colour away from green: less green, never more.
        let green = |ratio: f64| (hull_ramp(ratio) >> 8) & 0xff;
        let mut ratio = 0.7;
        while ratio > 0.25 {
            assert!(green(ratio - 0.05) <= green(ratio), "{ratio}");
            ratio -= 0.05;
        }
    }

    #[test]
    fn rank_titles_name_each_ranks_bonuses_and_repair() {
        assert_eq!(
            rank_title(0),
            "Earn XP from enemy hull damage and kills. Ranks reset on respawn."
        );
        assert_eq!(rank_title(1), "+10% damage · +10% fire rate · +10% hull");
        assert_eq!(
            rank_title(2),
            "+20% damage · +15% fire rate · +15% hull · repairs 1% hull/s after 5s out of combat"
        );
        assert_eq!(rank_title(RANKS.len()), "");
    }

    fn effects(timers: &mut EffectTimers, human: &HudHuman) -> Vec<(EffectKind, String, f64)> {
        timers
            .effects(human)
            .iter()
            .map(|effect| (effect.kind, effect.detail.to_string(), effect.share))
            .collect()
    }

    #[test]
    fn active_effects_show_seconds_left_shield_strength_and_self_repair() {
        let human = HudHuman {
            protection: 0.4,
            shield: 4.1,
            shield_points: 79.1,
            rapid: 3.2,
            speed: 2.3,
            laser: 1.4,
            self_repair: true,
            ..HudHuman::default()
        };
        let list = timers_effects(&human);
        assert_eq!(
            list.iter()
                .map(|effect| format!("{} {}", effect.name, effect.detail))
                .collect::<Vec<_>>(),
            [
                "SPAWN SHIELD ",
                "SHIELD 80 HP · 5s",
                "RAPID 4s",
                "BOOST 3s",
                "LASER DEFENSE 2s",
                "SELF-REPAIR "
            ]
        );
        let rapid = HudHuman {
            rapid: 0.01,
            ..HudHuman::default()
        };
        assert_eq!(
            effects(&mut EffectTimers::default(), &rapid),
            [(EffectKind::Rapid, "1s".to_owned(), 1.0)]
        );
        // Expired effects go, even with shield points left in the state.
        let expired = HudHuman {
            shield_points: 80.0,
            rapid: -0.01,
            ..HudHuman::default()
        };
        assert!(effects(&mut EffectTimers::default(), &expired).is_empty());
        let value = serde_json::to_value(timers_effects(&HudHuman {
            shield: 4.1,
            shield_points: 79.1,
            ..HudHuman::default()
        }))
        .unwrap();
        assert_eq!(
            value,
            json!([{"kind": "shield", "name": "SHIELD", "detail": "80 HP · 5s", "share": 1.0}])
        );
    }

    fn timers_effects(human: &HudHuman) -> HudEffects {
        EffectTimers::default().effects(human)
    }

    #[test]
    fn an_effects_ring_drains_from_the_longest_time_seen_since_it_began() {
        let mut timers = EffectTimers::default();
        let rapid = |seconds: f64| HudHuman {
            rapid: seconds,
            self_repair: true,
            ..HudHuman::default()
        };
        let share = |timers: &mut EffectTimers, seconds: f64| {
            effects(timers, &rapid(seconds))
                .iter()
                .map(|&(kind, _, share)| (kind, share))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            share(&mut timers, 15.0),
            [(EffectKind::Rapid, 1.0), (EffectKind::Repair, 1.0)]
        );
        assert_eq!(share(&mut timers, 6.0)[0], (EffectKind::Rapid, 0.4));
        // A second pickup refills the ring and becomes the new full length.
        assert_eq!(share(&mut timers, 20.0)[0], (EffectKind::Rapid, 1.0));
        assert_eq!(share(&mut timers, 5.0)[0], (EffectKind::Rapid, 0.25));
        // Once it ends, the next pickup starts full whatever it lasts.
        share(&mut timers, 0.0);
        assert_eq!(share(&mut timers, 3.0)[0], (EffectKind::Rapid, 1.0));
    }

    fn match_at(time: f64, overtime: bool) -> Match {
        Match {
            time,
            overtime,
            ..Hud::default().match_state
        }
    }

    #[test]
    fn the_score_pill_names_the_goal_and_counts_down_or_waits_for_the_next_kill() {
        let pill = |game_mode, endless, overtime| {
            let pill = ScorePill::local(&match_at(187.4, overtime), game_mode, endless, 312.6, 100);
            (
                pill.objective.map(|goal| goal.to_string()),
                pill.overtime,
                pill.clock().to_string(),
            )
        };
        assert_eq!(
            pill(GameMode::Team, false, false),
            (Some("FIRST TO 100".into()), false, "3:08".into())
        );
        assert_eq!(
            pill(GameMode::Team, false, true),
            (Some("FIRST TO 100".into()), true, "NEXT KILL".into())
        );
        // An endless level counts the time survived and has no overtime.
        assert_eq!(
            pill(GameMode::Team, true, true),
            (Some("ENDLESS STRESS".into()), false, "5:12".into())
        );
        assert_eq!(
            pill(GameMode::Solo, false, false).0.as_deref(),
            Some("SURVIVE · ONE LIFE")
        );
        let room = ScorePill::room(&match_at(-0.25, true));
        assert_eq!(
            serde_json::to_value(room).unwrap(),
            json!({"objective": null, "overtime": true, "clock": "NEXT KILL"})
        );
        assert!(room.seconds.is_sign_positive());
        let room = ScorePill::room(&match_at(-0.25, false));
        assert_eq!(room.clock().to_string(), "0:00");
    }

    #[test]
    fn the_loading_line_counts_shaders_then_waits_for_the_gpu_and_textures() {
        let line = |compiled, remaining, textures_pending, gpu_pending| {
            Loading::of(compiled, remaining, textures_pending, gpu_pending)
                .map(|line| (line.to_string(), line.progress()))
        };
        assert_eq!(
            line(31, 9, 2, false),
            Some(("Shaders loaded: 31 of 40".into(), Some(0.775)))
        );
        assert_eq!(
            line(40, 0, 2, true),
            Some(("Compiling shaders…".into(), None))
        );
        assert_eq!(
            line(40, 0, 2, false),
            Some(("Loading textures… 2 left".into(), None))
        );
        // A rebuild that reused everything only warms up: Battle Setup stays ready.
        assert_eq!(line(0, 0, 0, true), None);
    }
}

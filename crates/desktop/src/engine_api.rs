//! The engine's JSON reports as the desktop shell reads them: `hud_json`,
//! `drain_events` and `stats_json`, mirroring `src/game/engine-api.ts`. Fields the
//! shell does not show are left out; serde ignores them.

use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    #[default]
    Ready,
    Playing,
    Paused,
    Results,
}

impl Phase {
    /// `frame_slot::PHASE`'s number.
    pub fn from_slot(value: f32) -> Self {
        match value as u32 {
            1 => Phase::Playing,
            2 => Phase::Paused,
            3 => Phase::Results,
            _ => Phase::Ready,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub struct Point {
    pub x: f64,
    pub z: f64,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchState {
    pub phase: Phase,
    pub time: f64,
    pub scores: [f64; 2],
    pub overtime: bool,
    #[serde(default)]
    pub ended_early: bool,
    pub winner: Option<u8>,
    pub round: u32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DamageSource {
    pub cause: String,
}

/// A simulation event as `drain_events` reports it.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineEvent {
    #[serde(rename = "type")]
    pub kind: String,
    pub x: f64,
    pub z: f64,
    pub id: Option<u32>,
    pub owner: Option<u32>,
    pub team: Option<u8>,
    pub weapon: Option<String>,
    pub label: Option<String>,
    pub cover_kind: Option<String>,
    pub damage_source: Option<DamageSource>,
    #[serde(default)]
    pub player_hit: bool,
    #[serde(default)]
    pub own: bool,
    pub damage_angle: Option<f64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventBatch {
    pub listener: Point,
    /// World X/Z of screen right.
    pub listener_right: Point,
    pub events: Vec<EngineEvent>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AmmoSlot {
    pub weapon: String,
    /// `None` for unlimited Standard shells.
    pub count: Option<f64>,
    pub selected: bool,
    pub available: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HumanState {
    pub id: u32,
    pub kind: String,
    pub vehicle_name: String,
    pub alive: bool,
    pub hp: f64,
    pub max_hp: f64,
    pub health_ratio: f64,
    pub rank: u32,
    pub rank_name: String,
    pub rank_damage: f64,
    pub rank_fire_rate: f64,
    pub rank_health: f64,
    pub rank_repair: f64,
    /// Seconds out of combat before a ranked hull repairs itself.
    pub repair_delay: f64,
    pub ammo: Vec<AmmoSlot>,
    pub mine_cooldown: f64,
    pub protection: f64,
    pub shield: f64,
    pub shield_points: f64,
    pub rapid: f64,
    pub speed: f64,
    pub laser: f64,
    pub respawn: f64,
    pub kills: f64,
    pub deaths: f64,
    pub self_repair: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreboardRow {
    pub id: u32,
    pub name: String,
    pub kills: f64,
    pub deaths: f64,
    pub team: u8,
    pub kind: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Feat {
    pub title: String,
    pub detail: String,
}

/// The finished round's report; the engine decides feats and personal bests.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RecapState {
    pub stats: std::collections::BTreeMap<String, f64>,
    pub best: std::collections::BTreeMap<String, f64>,
    pub improved: Vec<String>,
    pub persisted: bool,
    pub feats: Vec<Feat>,
    pub shots: f64,
    pub direct_hits: f64,
    pub damage_taken: f64,
    pub shield_absorbed: f64,
    pub rank_names: Vec<String>,
}

/// `hud_json`: everything the HUD and menus show.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HudState {
    #[serde(rename = "match")]
    pub match_state: MatchState,
    pub elapsed: f64,
    pub game_mode: String,
    pub endless_match: bool,
    pub map_mode: String,
    pub map_name: String,
    pub difficulty: String,
    pub human_team: u8,
    pub score_limit: f64,
    pub team_names: [String; 2],
    pub active_enemies: f64,
    pub human: HumanState,
    pub scoreboard: Vec<ScoreboardRow>,
    pub recap: Option<RecapState>,
}

impl HudState {
    pub fn parse(json: &str) -> Option<Self> {
        serde_json::from_str(json).ok()
    }
}

impl EventBatch {
    pub fn parse(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }
}

//! Round recap statistics, personal bests and feats. The results-screen markup stays in the
//! browser shell; this is the logic it displays.

use serde::{Deserialize, Serialize};

use super::combat_record::longest_life;
use super::math::js_round;
use super::simulation::Simulation;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Metric {
    Kills,
    Damage,
    BestLife,
    Rank,
    BusiestMinute,
    LongestLife,
    Multikill,
    ClutchKills,
    RevengeKills,
    PosthumousKills,
    MineKills,
    CoverDestroyed,
    Pickups,
}

impl Metric {
    pub const ALL: [Metric; 13] = [
        Metric::Kills,
        Metric::Damage,
        Metric::BestLife,
        Metric::Rank,
        Metric::BusiestMinute,
        Metric::LongestLife,
        Metric::Multikill,
        Metric::ClutchKills,
        Metric::RevengeKills,
        Metric::PosthumousKills,
        Metric::MineKills,
        Metric::CoverDestroyed,
        Metric::Pickups,
    ];
    pub const FEATURED: [Metric; 5] = [
        Metric::Kills,
        Metric::Damage,
        Metric::LongestLife,
        Metric::BestLife,
        Metric::Rank,
    ];

    /// The key used in saved personal bests (the TS property name).
    pub const fn key(self) -> &'static str {
        match self {
            Metric::Kills => "kills",
            Metric::Damage => "damage",
            Metric::BestLife => "bestLife",
            Metric::Rank => "rank",
            Metric::BusiestMinute => "busiestMinute",
            Metric::LongestLife => "longestLife",
            Metric::Multikill => "multikill",
            Metric::ClutchKills => "clutchKills",
            Metric::RevengeKills => "revengeKills",
            Metric::PosthumousKills => "posthumousKills",
            Metric::MineKills => "mineKills",
            Metric::CoverDestroyed => "coverDestroyed",
            Metric::Pickups => "pickups",
        }
    }
}

/// One value per metric, in `Metric::ALL` order.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RecapStats(pub [f64; 13]);

impl RecapStats {
    pub fn get(&self, metric: Metric) -> f64 {
        self.0[metric as usize]
    }

    pub fn set(&mut self, metric: Metric, value: f64) {
        self.0[metric as usize] = value;
    }
}

/// Browser storage was unavailable or refused the write (private browsing, quota).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageUnavailable;

/// Browser storage access; errors model private browsing and quota failures.
pub trait RecordStorage {
    fn get_item(&self, key: &str) -> Result<Option<String>, StorageUnavailable>;
    fn set_item(&mut self, key: &str, value: &str) -> Result<(), StorageUnavailable>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct PersonalBests {
    pub best: RecapStats,
    /// Metrics this round improved, in `Metric::ALL` order.
    pub improved: Vec<Metric>,
    /// Whether an earlier record existed.
    pub established: bool,
    /// Whether the new bests were saved.
    pub persisted: bool,
}

pub fn save_personal_bests(
    storage: &mut impl RecordStorage,
    key: &str,
    stats: &RecapStats,
) -> PersonalBests {
    let mut best = *stats;
    let mut improved = Vec::new();
    let mut established = false;
    // Malformed or unavailable storage must not prevent the results screen.
    let saved = storage
        .get_item(key)
        .ok()
        .flatten()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    if let Some(serde_json::Value::Object(saved)) = saved {
        for metric in Metric::ALL {
            let Some(previous) = saved.get(metric.key()).and_then(serde_json::Value::as_f64) else {
                continue;
            };
            if previous.is_finite() && previous >= 0.0 {
                established = true;
                best.set(metric, previous.max(stats.get(metric)));
                if stats.get(metric) > previous {
                    improved.push(metric);
                }
            }
        }
    }
    let record: serde_json::Map<String, serde_json::Value> = Metric::ALL
        .iter()
        .map(|&metric| (metric.key().to_string(), json_number(best.get(metric))))
        .collect();
    // Private browsing and storage quotas can make records session-only.
    let persisted = storage
        .set_item(key, &serde_json::Value::Object(record).to_string())
        .is_ok();
    PersonalBests {
        best,
        improved,
        established,
        persisted,
    }
}

fn json_number(value: f64) -> serde_json::Value {
    if value.fract() == 0.0 && value.abs() < 9e15 {
        serde_json::Value::from(value as i64)
    } else {
        serde_json::Number::from_f64(value)
            .map_or(serde_json::Value::Null, serde_json::Value::Number)
    }
}

pub fn recap_stats(simulation: &Simulation) -> RecapStats {
    let tank = simulation.human();
    let combat = &simulation.combat_record;
    let mut stats = RecapStats::default();
    stats.set(Metric::Kills, tank.kills as f64);
    stats.set(Metric::Damage, js_round(tank.damage_dealt));
    stats.set(Metric::BestLife, tank.best_life_kills as f64);
    stats.set(Metric::Rank, tank.highest_rank as f64);
    stats.set(Metric::BusiestMinute, combat.busiest_minute as f64);
    stats.set(Metric::LongestLife, longest_life(simulation).floor());
    stats.set(Metric::Multikill, combat.multikill as f64);
    stats.set(Metric::ClutchKills, combat.clutch_kills as f64);
    stats.set(Metric::RevengeKills, combat.revenge_kills as f64);
    stats.set(Metric::PosthumousKills, combat.posthumous_kills as f64);
    stats.set(Metric::MineKills, combat.mine_kills as f64);
    stats.set(Metric::CoverDestroyed, combat.cover_destroyed as f64);
    stats.set(Metric::Pickups, combat.pickups as f64);
    stats
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Feat {
    pub title: String,
    pub detail: String,
}

fn feat(title: &str, detail: String) -> Feat {
    Feat {
        title: title.to_string(),
        detail,
    }
}

fn plural(count: f64, suffix: &str) -> &str {
    if count == 1.0 { "" } else { suffix }
}

/// Up to three highlighted accomplishments for the recap.
pub fn combat_feats(stats: &RecapStats, shots: u32, direct_hits: u32) -> Vec<Feat> {
    let mut feats = Vec::new();
    let value = |metric| stats.get(metric);
    if value(Metric::Multikill) >= 3.0 {
        feats.push(feat(
            "ONE-TANK ARMY",
            format!("{} kills in five seconds", value(Metric::Multikill)),
        ));
    }
    if value(Metric::ClutchKills) >= 2.0 {
        feats.push(feat(
            "TOO ANGRY TO DIE",
            format!("{} kills at 25% hull or less", value(Metric::ClutchKills)),
        ));
    }
    let posthumous = value(Metric::PosthumousKills);
    if posthumous > 0.0 {
        feats.push(feat(
            "DEAD BUT DANGEROUS",
            format!(
                "{posthumous} kill{} from a previous life's ordnance",
                plural(posthumous, "s")
            ),
        ));
    }
    if value(Metric::MineKills) >= 2.0 {
        feats.push(feat(
            "MIND YOUR STEP",
            format!("{} mine-blast kills", value(Metric::MineKills)),
        ));
    }
    let revenge = value(Metric::RevengeKills);
    if revenge > 0.0 {
        feats.push(feat(
            "NOTHING PERSONAL",
            format!("{revenge} score{} settled", plural(revenge, "s")),
        ));
    }
    if value(Metric::CoverDestroyed) >= 10.0 {
        feats.push(feat(
            "URBAN REDEVELOPMENT",
            format!(
                "{} pieces of cover demolished",
                value(Metric::CoverDestroyed)
            ),
        ));
    }
    if shots >= 20 && direct_hits as f64 / shots as f64 >= 0.65 {
        feats.push(feat(
            "SURGICAL STRIKES",
            format!(
                "{}% direct hit rate across {shots} projectiles",
                js_round((direct_hits as f64 / shots as f64) * 100.0)
            ),
        ));
    }
    if value(Metric::LongestLife) >= 180.0 {
        feats.push(feat(
            "HARD TO KILL",
            format!(
                "{} without getting wrecked",
                duration(value(Metric::LongestLife))
            ),
        ));
    }
    if value(Metric::BusiestMinute) >= 5.0 {
        feats.push(feat(
            "RUSH HOUR",
            format!(
                "{} kills in your busiest minute",
                value(Metric::BusiestMinute)
            ),
        ));
    }
    if value(Metric::Rank) == 3.0 {
        feats.push(feat("LOCAL LEGEND", "Reached Heroic rank".to_string()));
    }
    feats.truncate(3);
    feats
}

/// `m:ss`.
pub fn duration(seconds: f64) -> String {
    format!(
        "{}:{:02}",
        (seconds / 60.0).floor(),
        (seconds % 60.0).floor() as i64
    )
}

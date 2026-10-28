//! The results screen's round recap, which `hud_json` carries once results show.

use serde::{Serialize, Serializer};
use sloppy_core::sim::Simulation;
use sloppy_core::sim::round_recap::{
    Feat, Metric, RecapStats, RecordStorage, combat_feats, recap_stats, save_personal_bests,
};
use sloppy_core::sim::veterancy::RANKS;

/// The finished round as the results screen reports it: the human's statistics, the
/// personal bests for this mode, map and difficulty, and the feats the engine chose.
///
/// The page's recap was once a JSON value, whose object keys sort, so the fields and the
/// metrics within `stats` and `best` serialize in the order of their names.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundRecap {
    #[serde(serialize_with = "metric_values")]
    pub best: RecapStats,
    pub damage_taken: f64,
    pub direct_hits: u32,
    #[serde(serialize_with = "feats")]
    pub feats: Vec<Feat>,
    /// The metrics this round improved, in `Metric::ALL` order.
    #[serde(serialize_with = "metric_keys")]
    pub improved: Vec<Metric>,
    /// Whether the new bests were saved.
    pub persisted: bool,
    pub rank_names: Vec<&'static str>,
    pub shield_absorbed: f64,
    pub shots: u32,
    #[serde(serialize_with = "metric_values")]
    pub stats: RecapStats,
}

/// The finished round's recap, saving personal bests for this mode, map and difficulty
/// in `storage`.
pub fn round_recap(sim: &Simulation, storage: &mut impl RecordStorage) -> RoundRecap {
    let stats = recap_stats(sim);
    let combat = &sim.combat_record;
    let key = format!(
        "sloppy-records-v1:{}:{}:{}",
        sim.game_mode.as_str(),
        sim.map_name(),
        sim.difficulty.as_str(),
    );
    let records = save_personal_bests(storage, &key, &stats);
    RoundRecap {
        best: records.best,
        damage_taken: combat.damage_taken,
        direct_hits: combat.direct_hits,
        feats: combat_feats(&stats, combat.shots, combat.direct_hits),
        improved: records.improved,
        persisted: records.persisted,
        rank_names: RANKS.iter().map(|rank| rank.name).collect(),
        shield_absorbed: combat.shield_absorbed,
        shots: combat.shots,
        stats,
    }
}

/// Each metric by its key, the keys in alphabetical order.
fn metric_values<S: Serializer>(values: &RecapStats, serializer: S) -> Result<S::Ok, S::Error> {
    let mut metrics = Metric::ALL;
    metrics.sort_by_key(|metric| metric.key());
    serializer.collect_map(
        metrics
            .iter()
            .map(|&metric| (metric.key(), values.get(metric))),
    )
}

fn metric_keys<S: Serializer>(metrics: &[Metric], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(metrics.iter().map(|metric| metric.key()))
}

/// A feat with its keys in alphabetical order.
#[derive(Serialize)]
struct SortedFeat<'a> {
    detail: &'a str,
    title: &'a str,
}

fn feats<S: Serializer>(feats: &[Feat], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(feats.iter().map(|feat| SortedFeat {
        detail: &feat.detail,
        title: &feat.title,
    }))
}

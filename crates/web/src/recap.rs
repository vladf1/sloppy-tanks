//! The results screen's round recap, which `hud_json` carries once results show.

use serde_json::{Value, json};
use sloppy_core::sim::Simulation;
use sloppy_core::sim::round_recap::{
    Metric, RecapStats, RecordStorage, combat_feats, recap_stats, save_personal_bests,
};
use sloppy_core::sim::veterancy::RANKS;

/// The finished round's recap, saving personal bests for this mode, map and difficulty
/// in `storage`.
pub fn round_recap(sim: &Simulation, storage: &mut impl RecordStorage) -> Value {
    let stats = recap_stats(sim);
    let combat = &sim.combat_record;
    let label = |value: Value| value.as_str().map(str::to_owned).unwrap_or_default();
    let key = format!(
        "sloppy-records-v1:{}:{}:{}",
        label(json!(sim.game_mode)),
        sim.map_name(),
        label(json!(sim.difficulty)),
    );
    let records = save_personal_bests(storage, &key, &stats);
    let metrics = |values: &RecapStats| {
        Metric::ALL
            .iter()
            .map(|&metric| (metric.key().to_string(), Value::from(values.get(metric))))
            .collect::<serde_json::Map<String, Value>>()
    };
    json!({
        "stats": metrics(&stats),
        "best": metrics(&records.best),
        "improved": records.improved.iter().map(|metric| metric.key()).collect::<Vec<_>>(),
        "persisted": records.persisted,
        "feats": combat_feats(&stats, combat.shots, combat.direct_hits),
        "shots": combat.shots,
        "directHits": combat.direct_hits,
        "damageTaken": combat.damage_taken,
        "shieldAbsorbed": combat.shield_absorbed,
        "rankNames": RANKS.iter().map(|rank| rank.name).collect::<Vec<_>>(),
    })
}

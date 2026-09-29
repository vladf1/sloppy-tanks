//! Round recap logic: credited damage and peaks, personal-best storage, rolling kill
//! windows, longest life, feats, combat counters, direct hits and ending a paused battle
//! (the former `tests/round-recap.test.ts`; the results-screen markup is not ported).

mod support;

use std::collections::HashMap;

use sloppy_core::sim::combat_record::{longest_life, record_kill};
use sloppy_core::sim::match_state::end_battle;
use sloppy_core::sim::physics::vector;
use sloppy_core::sim::pickups::collect_pickup;
use sloppy_core::sim::projectiles::step_projectiles;
use sloppy_core::sim::round_recap::{
    Metric, RecapStats, RecordStorage, StorageUnavailable, combat_feats, recap_stats,
    save_personal_bests,
};
use sloppy_core::sim::tank_lifecycle::respawn_tank;
use sloppy_core::sim::veterancy::earn_experience;
use sloppy_core::sim::weapons::fire_weapon;
use sloppy_core::sim::{
    CoverKind, DamageCause, DamageSource, MatchPhase, PickupKind, Simulation, VehicleCommand,
};
use support::clear_arena;

fn human(s: &Simulation) -> usize {
    s.human_index().expect("local play has a human")
}

fn first_enemy(s: &Simulation) -> usize {
    let team = s.tanks[human(s)].team;
    s.tanks.iter().position(|t| t.team != team).unwrap()
}

/// `s.damageTank(victim, amount, attacker.id, attacker.team, ownerLife, source)`.
fn damage_from(
    s: &mut Simulation,
    victim: usize,
    amount: f64,
    attacker: usize,
    owner_life: Option<u32>,
    source: Option<DamageSource>,
) {
    let (id, team) = (s.tanks[attacker].id, s.tanks[attacker].team);
    s.damage_tank(victim, amount, id, team, owner_life, source);
}

#[test]
fn recap_counts_actual_enemy_hull_damage_preserves_peaks_and_resets_with_the_round() {
    let mut s = Simulation::with_seed(123.0);
    s.start();
    let player = human(&s);
    let enemy = first_enemy(&s);
    let player_team = s.tanks[player].team;
    let ally = s
        .tanks
        .iter()
        .position(|t| !t.human && t.team == player_team)
        .unwrap();
    for tank in [enemy, ally, player] {
        s.tanks[tank].protection = 0.0;
    }
    damage_from(&mut s, ally, 20.0, player, Some(0), None);
    damage_from(&mut s, player, 10.0, player, Some(0), None);
    assert_eq!(s.tanks[player].damage_dealt, 0.0);
    s.tanks[enemy].shield = 10.0;
    s.tanks[enemy].shield_points = 30.0;
    damage_from(&mut s, enemy, 40.0, player, Some(0), None);
    assert_eq!(s.tanks[player].damage_dealt, 10.0);
    let remaining = s.tanks[enemy].hp;
    damage_from(&mut s, enemy, 999.0, player, Some(0), None);
    assert_eq!(s.tanks[player].damage_dealt, 10.0 + remaining);
    assert_eq!(s.tanks[player].best_life_kills, 1);
    earn_experience(&mut s, player, 1500.0, Some(0));
    assert_eq!(s.tanks[player].highest_rank, 3);
    let enemy_deaths = s.tanks[enemy].deaths;
    damage_from(&mut s, player, 9999.0, enemy, Some(enemy_deaths), None);
    respawn_tank(&mut s, player, None);
    assert_eq!(s.tanks[player].life_kills, 0);
    assert_eq!(s.tanks[player].best_life_kills, 1);
    assert_eq!(s.tanks[player].highest_rank, 3);
    assert_eq!(s.tanks[player].xp, 0.0);
    respawn_tank(&mut s, enemy, None);
    s.tanks[enemy].protection = 0.0;
    damage_from(&mut s, enemy, 9999.0, player, Some(0), None);
    assert_eq!(s.tanks[player].kills, 2);
    assert_eq!(
        s.tanks[player].life_kills, 0,
        "old-life ordnance cannot pad the new life"
    );
    s.reset(None);
    let player = human(&s);
    assert_eq!(s.tanks[player].damage_dealt, 0.0);
    assert_eq!(s.tanks[player].highest_rank, 0);
    assert_eq!(s.tanks[player].best_life_kills, 0);
}

#[derive(Default)]
struct MemoryStorage {
    values: HashMap<String, String>,
}

impl RecordStorage for MemoryStorage {
    fn get_item(&self, key: &str) -> Result<Option<String>, StorageUnavailable> {
        Ok(self.values.get(key).cloned())
    }

    fn set_item(&mut self, key: &str, value: &str) -> Result<(), StorageUnavailable> {
        self.values.insert(key.to_string(), value.to_string());
        Ok(())
    }
}

/// Private browsing or a full quota: every access fails.
struct UnavailableStorage;

impl RecordStorage for UnavailableStorage {
    fn get_item(&self, _key: &str) -> Result<Option<String>, StorageUnavailable> {
        Err(StorageUnavailable)
    }

    fn set_item(&mut self, _key: &str, _value: &str) -> Result<(), StorageUnavailable> {
        Err(StorageUnavailable)
    }
}

fn stats(values: &[(Metric, f64)]) -> RecapStats {
    let mut stats = RecapStats::default();
    for &(metric, value) in values {
        stats.set(metric, value);
    }
    stats
}

#[test]
fn records_distinguish_a_first_round_improvements_ties_and_separate_categories() {
    let mut storage = MemoryStorage::default();
    let first = stats(&[
        (Metric::Kills, 5.0),
        (Metric::Damage, 450.0),
        (Metric::BestLife, 3.0),
        (Metric::Rank, 1.0),
        (Metric::BusiestMinute, 3.0),
        (Metric::LongestLife, 90.0),
        (Metric::Multikill, 2.0),
    ]);
    assert!(!save_personal_bests(&mut storage, "team", &first).established);
    let mut next_stats = first;
    next_stats.set(Metric::Kills, 6.0);
    next_stats.set(Metric::Damage, 400.0);
    next_stats.set(Metric::BestLife, 3.0);
    next_stats.set(Metric::Rank, 2.0);
    let next = save_personal_bests(&mut storage, "team", &next_stats);
    assert_eq!(next.improved, vec![Metric::Kills, Metric::Rank]);
    assert_eq!(next.best.get(Metric::Damage), 450.0);
    assert!(!save_personal_bests(&mut storage, "solo", &first).established);
    storage.values.insert(
        "bad".to_string(),
        r#"{"kills": "oops", "rank": -1}"#.to_string(),
    );
    assert!(!save_personal_bests(&mut storage, "bad", &first).established);
    assert!(!save_personal_bests(&mut UnavailableStorage, "team", &first).persisted);
}

#[test]
fn rolling_kill_windows_cross_clock_minute_boundaries_and_expire_exactly() {
    let mut s = Simulation::with_seed(123.0);
    let victim = first_enemy(&s);
    let player = human(&s);
    for time in [58.0, 59.0, 61.0] {
        s.elapsed = time;
        record_kill(&mut s, player, victim, Some(0), None);
    }
    assert_eq!(s.combat_record.busiest_minute, 3);
    assert_eq!(s.combat_record.multikill, 3);
    s.elapsed = 121.0;
    record_kill(&mut s, player, victim, Some(0), None);
    assert_eq!(s.combat_record.recent_kills, vec![121.0]);
    assert_eq!(s.combat_record.busiest_minute, 3);
    s.reset(None);
    assert_eq!(s.combat_record.busiest_minute, 0);
    let player = human(&s);
    let victim = first_enemy(&s);
    s.elapsed = 1.0;
    record_kill(&mut s, player, victim, Some(0), None);
    s.elapsed = 6.0;
    record_kill(&mut s, player, victim, Some(0), None);
    assert_eq!(
        s.combat_record.multikill, 1,
        "five-second boundary is exclusive"
    );
}

#[test]
fn longest_life_freezes_on_death_excludes_respawn_and_paused_time_includes_unfinished_life() {
    let mut s = Simulation::with_seed(123.0);
    let player = human(&s);
    let enemy = first_enemy(&s);
    s.start();
    s.elapsed = 42.0;
    s.tanks[player].protection = 0.0;
    damage_from(&mut s, player, 9999.0, enemy, None, None);
    assert_eq!(longest_life(&s), 42.0);
    s.elapsed = 45.0;
    respawn_tank(&mut s, player, None);
    s.elapsed = 65.0;
    assert_eq!(longest_life(&s), 42.0);
    s.match_state.phase = MatchPhase::Paused;
    for _ in 0..600 {
        s.step(VehicleCommand::idle(), false);
    }
    assert_eq!(longest_life(&s), 42.0);
    s.elapsed = 100.0;
    assert_eq!(longest_life(&s), 55.0);
}

#[test]
fn revenge_clutch_posthumous_and_mine_feats_use_credited_kills() {
    let mut s = Simulation::with_seed(123.0);
    s.start();
    let p = human(&s);
    let enemy = first_enemy(&s);
    s.tanks[p].protection = 0.0;
    damage_from(&mut s, p, 9999.0, enemy, None, None);
    respawn_tank(&mut s, p, None);
    s.tanks[p].hp = 1.0;
    s.tanks[enemy].protection = 0.0;
    let deaths = s.tanks[p].deaths;
    let mine = DamageSource {
        cause: DamageCause::Mine,
        origin: sloppy_core::sim::math::Vec2::ZERO,
    };
    damage_from(&mut s, enemy, 9999.0, p, Some(deaths), Some(mine));
    assert_eq!(s.combat_record.revenge_kills, 1);
    assert_eq!(s.combat_record.clutch_kills, 1);
    assert_eq!(s.combat_record.mine_kills, 1);
    respawn_tank(&mut s, enemy, None);
    s.tanks[enemy].protection = 0.0;
    let deaths = s.tanks[p].deaths;
    damage_from(&mut s, enemy, 9999.0, p, Some(deaths - 1), None);
    assert_eq!(
        s.combat_record.revenge_kills, 1,
        "revenge is redeemed only once"
    );
    assert_eq!(
        s.combat_record.clutch_kills, 1,
        "old-life shell cannot earn a clutch kill"
    );
    assert_eq!(s.combat_record.posthumous_kills, 1);
    assert!(
        combat_feats(&recap_stats(&s), 0, 0)
            .iter()
            .any(|f| f.title == "DEAD BUT DANGEROUS")
    );
    let mut quiet = recap_stats(&s);
    quiet.set(Metric::PosthumousKills, 0.0);
    quiet.set(Metric::RevengeKills, 0.0);
    assert_eq!(combat_feats(&quiet, 0, 0).len(), 0);
}

#[test]
fn combat_counters_measure_shield_and_hull_loss_actual_pickups_and_attributable_demolition() {
    let mut s = Simulation::with_seed(123.0);
    s.start();
    let p = human(&s);
    let enemy = first_enemy(&s);
    s.tanks[p].protection = 0.0;
    s.tanks[p].shield = 10.0;
    s.tanks[p].shield_points = 30.0;
    damage_from(&mut s, p, 40.0, p, None, None);
    assert_eq!(s.combat_record.shield_absorbed, 30.0);
    assert_eq!(s.combat_record.damage_taken, 10.0);
    let mut pickup = s.pickups[0].clone();
    pickup.kind = PickupKind::Repair;
    pickup.available = true;
    assert!(collect_pickup(&mut s, p, &mut pickup));
    assert!(!collect_pickup(&mut s, p, &mut pickup));
    s.pickups[0] = pickup;
    assert_eq!(s.combat_record.pickups, 1);
    let covers: Vec<usize> = (0..s.covers.len())
        .filter(|&c| s.covers[c].destructible && s.covers[c].kind != CoverKind::Drum)
        .collect();
    let (player_id, player_team) = (s.tanks[p].id, s.tanks[p].team);
    let (enemy_id, enemy_team) = (s.tanks[enemy].id, s.tanks[enemy].team);
    s.damage_cover(covers[0], 9999.0, player_id, player_team, None, None);
    s.damage_cover(covers[0], 9999.0, player_id, player_team, None, None);
    s.damage_cover(covers[1], 9999.0, enemy_id, enemy_team, None, None);
    assert_eq!(s.combat_record.cover_destroyed, 1);
}

#[test]
fn direct_hit_rate_counts_emitted_projectiles_and_enemy_contacts_excluding_protected_hits() {
    let mut s = Simulation::with_seed(123.0);
    s.start();
    let p = human(&s);
    let enemy = first_enemy(&s);
    let all: Vec<usize> = (0..s.tanks.len()).collect();
    // Keeping every tank preserves their indices.
    clear_arena(&mut s, &all);
    for tank in 0..s.tanks.len() {
        let x = if tank == p || tank == enemy {
            0.0
        } else {
            35.0
        };
        let z = if tank == enemy { 8.0 } else { 0.0 };
        let body = s.tanks[tank].body;
        s.world.bodies[body].set_translation(vector(x, 0.65, z), true);
    }
    s.tanks[p].aim = 0.0;
    s.tanks[enemy].protection = 0.0;
    s.world.step();
    fire_weapon(&mut s, p);
    fire_weapon(&mut s, p);
    assert_eq!(
        s.combat_record.shots, 1,
        "rejected cooldown fire isn't a shot"
    );
    for _ in 0..60 {
        step_projectiles(&mut s, 1.0 / 60.0, false);
    }
    assert_eq!(s.combat_record.direct_hits, 1);
    s.tanks[p].cooldown = 0.0;
    s.tanks[enemy].protection = 10.0;
    fire_weapon(&mut s, p);
    for _ in 0..60 {
        step_projectiles(&mut s, 1.0 / 60.0, false);
    }
    assert_eq!(s.combat_record.shots, 2);
    assert_eq!(s.combat_record.direct_hits, 1);
}

#[test]
fn ending_a_paused_battle_preserves_the_round_and_stats_without_claiming_victory() {
    let mut s = Simulation::with_seed(123.0);
    end_battle(&mut s.match_state);
    assert_eq!(s.match_state.phase, MatchPhase::Ready);
    s.start();
    s.elapsed = 42.0;
    let h = human(&s);
    s.tanks[h].kills = 3;
    s.match_state.scores = [7, 4];
    s.match_state.phase = MatchPhase::Paused;
    let time = s.match_state.time;
    end_battle(&mut s.match_state);
    assert_eq!(s.match_state.phase, MatchPhase::Results);
    assert_eq!(s.match_state.ended_early, Some(true));
    assert_eq!(s.match_state.winner, None);
    assert_eq!(s.match_state.time, time);
    assert_eq!(s.match_state.scores, [7, 4]);
    assert_eq!(recap_stats(&s).get(Metric::Kills), 3.0);
    assert_eq!(recap_stats(&s).get(Metric::LongestLife), 42.0);
    s.step(VehicleCommand::idle(), false);
    assert_eq!(s.elapsed, 42.0);
    s.reset(None);
    assert_eq!(s.match_state.ended_early, None);
}

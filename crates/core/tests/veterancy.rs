//! Veterancy: XP from hull damage and kills, excluded sources, promotions that keep the hull
//! percentage, rank bonuses on damage, reload and max hull, life-bound ordnance attribution,
//! quiet-time repair, and resets on death, respawn and round reset. Ported from
//! `tests/veterancy.test.ts`.

mod support;

use sloppy_core::sim::ammunition::{AMMO_ORDER, refill_ammo};
use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::bot_personalities::bot_reload;
use sloppy_core::sim::data::{STEP, vehicle, weapon};
use sloppy_core::sim::math::{Vec2, js_round};
use sloppy_core::sim::physics::vector;
use sloppy_core::sim::simulation::GameMode;
use sloppy_core::sim::veterancy::{RANKS, earn_experience, rank_index, repair_veteran};
use sloppy_core::sim::weapons::{
    collect_pickup, fire_weapon, place_mine, step_mines, step_projectiles, weapon_interval,
};
use sloppy_core::sim::{
    CoverKind, MatchPhase, Mine, PickupKind, Shot, SimEventType, Simulation, VehicleCommand,
    VehicleKind, Weapon,
};
use support::{ALLY, ENEMY, PLAYER, damage_from, player_enemy_ally, supply};

fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8, "{a} != {b}");
}

fn move_tank(s: &mut Simulation, index: usize, x: f64, z: f64) {
    let body = s.tanks[index].body;
    s.world.bodies[body].set_translation(vector(x, 0.65, z), true);
    s.tanks[index].previous = Vec2::new(x, z);
    s.world.bodies[body].set_linvel(vector(0.0, 0.0, 0.0), true);
}

/// The human, one enemy and one ally, 20 m apart along +x.
fn fixture() -> Simulation {
    let mut s = player_enemy_ally();
    for i in 0..s.tanks.len() {
        s.tanks[i].protection = 0.0;
        move_tank(&mut s, i, i as f64 * 20.0, 0.0);
    }
    s.world.step();
    s.start();
    s.events.clear();
    s
}

fn id_team(s: &Simulation, index: usize) -> (u32, sloppy_core::sim::Team) {
    (s.tanks[index].id, s.tanks[index].team)
}

fn promotions(s: &Simulation, id: Option<u32>) -> usize {
    s.events
        .iter()
        .filter(|e| e.kind == SimEventType::Promotion && (id.is_none() || e.id == id))
        .count()
}

#[test]
fn hull_damage_earns_shared_xp_and_only_the_finisher_gets_the_kill_bonus_without_overkill() {
    let mut s = fixture();
    let hp = s.tanks[ENEMY].hp;
    damage_from(&mut s, ENEMY, 30.0, PLAYER, None, None);
    damage_from(&mut s, ENEMY, 10.0, ALLY, None, None);
    assert_eq!(s.tanks[PLAYER].xp, 30.0);
    assert_eq!(s.tanks[ALLY].xp, 10.0);
    damage_from(&mut s, ENEMY, 9999.0, ALLY, None, None);
    assert_eq!(s.tanks[ALLY].xp, hp - 30.0 + 50.0);
    assert_eq!(s.tanks[ALLY].kills, 1);
    damage_from(&mut s, ENEMY, 9999.0, PLAYER, None, None);
    assert_eq!(s.tanks[PLAYER].xp, 30.0);
}

#[test]
fn protection_allies_self_damage_shield_absorption_and_scenery_give_no_xp() {
    let mut s = fixture();
    let (player_id, player_team) = id_team(&s, PLAYER);
    s.tanks[ENEMY].protection = 1.0;
    s.damage_tank(ENEMY, 50.0, player_id, player_team, None, None);
    s.tanks[ENEMY].protection = 0.0;
    s.damage_tank(ALLY, 50.0, player_id, player_team, None, None);
    s.damage_tank(PLAYER, 10.0, player_id, player_team, None, None);
    s.tanks[ENEMY].shield = 12.0;
    s.tanks[ENEMY].shield_points = 60.0;
    s.damage_tank(ENEMY, 50.0, player_id, player_team, None, None);
    let c = s.add_cover(&CoverDef::new(
        CoverKind::Timber,
        0.0,
        20.0,
        2.0,
        1.0,
        1.0,
        10.0,
        0,
    ));
    s.damage_cover(c, 999.0, player_id, player_team, None, None);
    assert_eq!(s.tanks[PLAYER].xp, 0.0);
    s.damage_tank(ENEMY, 25.0, player_id, player_team, None, None);
    assert_eq!(s.tanks[PLAYER].xp, 15.0);
}

#[test]
fn players_and_bots_promote_at_exact_thresholds_keep_hull_percentage_and_cap_at_heroic() {
    let mut s = fixture();
    for t in [PLAYER, ALLY] {
        s.tanks[t].hp = s.max_health(&s.tanks[t]) * 0.4;
        earn_experience(&mut s, t, RANKS[1].xp - 0.5, None);
        assert_eq!(rank_index(s.tanks[t].xp), 0);
        earn_experience(&mut s, t, 0.5, None);
        assert_eq!(rank_index(s.tanks[t].xp), 1);
        near(s.tanks[t].hp, s.max_health(&s.tanks[t]) * 0.4);
        earn_experience(&mut s, t, RANKS[2].xp - RANKS[1].xp, None);
        assert_eq!(rank_index(s.tanks[t].xp), 2);
        earn_experience(&mut s, t, RANKS[3].xp - RANKS[2].xp, None);
        assert_eq!(rank_index(s.tanks[t].xp), 3);
        earn_experience(&mut s, t, 99999.0, None);
        assert_eq!(s.tanks[t].xp, RANKS[3].xp);
        near(s.tanks[t].hp, s.max_health(&s.tanks[t]) * 0.4);
        assert_eq!(promotions(&s, Some(s.tanks[t].id)), 3);
    }
}

#[test]
fn one_large_xp_award_reaches_the_correct_rank_and_emits_one_promotion() {
    let mut s = fixture();
    earn_experience(&mut s, PLAYER, RANKS[2].xp, None);
    assert_eq!(rank_index(s.tanks[PLAYER].xp), 2);
    assert_eq!(promotions(&s, None), 1);
    assert!(s.events[0].label.as_deref().unwrap().contains("ELITE"));
}

#[test]
fn all_five_weapons_snapshot_rank_damage_and_reload_bonuses_stack_with_rapid_fire() {
    let mut s = fixture();
    for t in [PLAYER, ALLY] {
        for fired in AMMO_ORDER {
            s.tanks[t].xp = 0.0;
            s.tanks[t].selected_ammo = fired;
            if let Some(special) = fired.special() {
                refill_ammo(&mut s.tanks[t], special, 1.0);
            }
            let rookie = weapon_interval(&s.tanks[t]);
            let bot_rookie = bot_reload(&s.tanks[t], 0.0, Some(fired));
            s.tanks[t].xp = RANKS[3].xp;
            s.tanks[t].rapid = 12.0;
            s.tanks[t].cooldown = 0.0;
            near(weapon_interval(&s.tanks[t]), rookie / 1.2 / 2.0);
            near(
                bot_reload(&s.tanks[t], 0.0, Some(fired)),
                bot_rookie / 1.2 / 2.0,
            );
            s.shots.clear();
            fire_weapon(&mut s, t);
            assert_eq!(
                s.shots.len(),
                if fired == Weapon::Spread { 3 } else { 1 },
                "{fired:?}"
            );
            for shot in &s.shots {
                near(shot.damage, weapon(fired).damage * 1.3);
                assert_eq!(shot.owner_life, Some(s.tanks[t].deaths));
            }
            s.tanks[t].xp = 0.0;
            near(s.shots[0].damage, weapon(fired).damage * 1.3);
            s.tanks[t].rapid = 0.0;
        }
    }
}

#[test]
fn promotion_scales_a_pending_cannon_reload_and_the_bot_decision_timer() {
    let mut s = fixture();
    s.tanks[ALLY].cooldown = 0.6;
    s.tanks[ALLY].brain.fire_delay = 2.0;
    earn_experience(&mut s, ALLY, RANKS[1].xp, None);
    near(s.tanks[ALLY].cooldown, 0.6 / 1.1);
    near(s.tanks[ALLY].brain.fire_delay, 2.0 / 1.1);
}

#[test]
fn mines_snapshot_damage_and_old_ordnance_never_gives_xp_to_dead_owners_or_replacements() {
    for respawn in [false, true] {
        let mut s = fixture();
        let (player_id, player_team) = id_team(&s, PLAYER);
        s.tanks[PLAYER].xp = RANKS[3].xp;
        place_mine(&mut s, PLAYER);
        let (mine_x, mine_z) = (s.mines[0].x, s.mines[0].z);
        assert_eq!(s.mines[0].damage, Some(130.0));
        assert_eq!(s.mines[0].owner_life, Some(0));
        s.damage_tank(PLAYER, 9999.0, player_id, player_team, None, None);
        if respawn {
            s.respawn(PLAYER, None);
            move_tank(&mut s, PLAYER, -30.0, -30.0);
        }
        let xp = s.tanks[PLAYER].xp;
        move_tank(&mut s, ENEMY, mine_x, mine_z);
        s.tanks[ENEMY].hp = 1.0;
        s.mines[0].arm = 0.0;
        step_mines(&mut s, STEP);
        assert!(!s.tanks[ENEMY].alive);
        assert_eq!(s.tanks[PLAYER].xp, xp);
        // A late shell from that same life also leaves the new tank's progress alone.
        let enemy_team = s.tanks[ENEMY].team;
        let fresh = s.add_tank(enemy_team, false, VehicleKind::Balanced, 0);
        move_tank(&mut s, fresh, 0.0, 10.0);
        s.tanks[fresh].protection = 0.0;
        s.world.step();
        let id = s.next_id;
        s.next_id += 1;
        s.shots.push(Shot {
            id,
            owner: player_id,
            owner_life: Some(0),
            team: player_team,
            x: 0.0,
            z: 5.0,
            vx: 0.0,
            vz: 40.0,
            damage: 30.0,
            life: 2.0,
            ..Shot::default()
        });
        let mut i = 0;
        while i < 12 && !s.shots.is_empty() {
            step_projectiles(&mut s, STEP, false);
            i += 1;
        }
        assert_eq!(
            s.tanks[fresh].hp,
            s.max_health(&s.tanks[fresh]) - 30.0,
            "respawn={respawn}"
        );
        assert_eq!(s.tanks[PLAYER].xp, xp);
    }
}

#[test]
fn mine_and_drum_chains_retain_the_initiating_tanks_xp_and_life_attribution() {
    for old_life in [false, true] {
        let mut s = fixture();
        let (player_id, player_team) = id_team(&s, PLAYER);
        let (enemy_id, enemy_team) = id_team(&s, ENEMY);
        move_tank(&mut s, ENEMY, 0.0, 12.0);
        s.tanks[ENEMY].hp = 10.0;
        let drum = s.add_cover(&CoverDef::new(
            CoverKind::Drum,
            0.0,
            10.0,
            1.0,
            1.0,
            1.0,
            10.0,
            0,
        ));
        let id = s.next_id;
        s.next_id += 1;
        s.mines.push(Mine {
            id,
            owner: enemy_id,
            owner_life: None,
            damage: None,
            team: enemy_team,
            x: 0.0,
            z: 11.0,
            arm: 0.0,
            life: 20.0,
        });
        if old_life {
            s.damage_tank(PLAYER, 9999.0, player_id, player_team, None, None);
            s.respawn(PLAYER, None);
        }
        s.damage_cover(drum, 10.0, player_id, player_team, Some(0), None);
        assert!(!s.tanks[ENEMY].alive);
        assert_eq!(s.tanks[PLAYER].xp, if old_life { 0.0 } else { 60.0 });
        assert_eq!(s.mines.len(), 0);
    }
}

#[test]
fn elite_and_heroic_repair_only_after_five_quiet_seconds_and_shield_hits_or_firing_delay_it() {
    let mut s = fixture();
    let enemy_team = s.tanks[ENEMY].team;
    for bot in [ENEMY, ALLY] {
        let body = s.tanks[bot].body;
        s.remove_body(body);
    }
    s.tanks.truncate(1);
    for rank in RANKS {
        s.tanks[PLAYER].xp = rank.xp;
        s.tanks[PLAYER].hp = 30.0;
        s.tanks[PLAYER].last_combat = 0.0;
        s.elapsed = 4.9;
        repair_veteran(&mut s, PLAYER, 1.0);
        assert_eq!(s.tanks[PLAYER].hp, 30.0);
        s.elapsed = 5.0;
        repair_veteran(&mut s, PLAYER, 1.0);
        near(
            s.tanks[PLAYER].hp,
            30.0 + s.max_health(&s.tanks[PLAYER]) * rank.repair,
        );
    }
    s.tanks[PLAYER].hp = 30.0;
    s.tanks[PLAYER].shield = 10.0;
    s.tanks[PLAYER].shield_points = 100.0;
    // The TypeScript test used owner -1: an id no tank has.
    s.damage_tank(PLAYER, 20.0, u32::MAX, enemy_team, None, None);
    assert_eq!(s.tanks[PLAYER].last_combat, 5.0);
    s.elapsed = 9.9;
    repair_veteran(&mut s, PLAYER, 1.0);
    assert_eq!(s.tanks[PLAYER].hp, 30.0);
    s.elapsed = 11.0;
    s.tanks[PLAYER].cooldown = 0.0;
    s.step(
        VehicleCommand {
            fire: true,
            ..VehicleCommand::idle()
        },
        false,
    );
    assert_eq!(s.tanks[PLAYER].hp, 30.0);
    let time = s.elapsed;
    s.match_state.phase = MatchPhase::Paused;
    for _ in 0..400 {
        s.step(VehicleCommand::idle(), false);
    }
    assert_eq!(s.elapsed, time);
    assert_eq!(s.tanks[PLAYER].hp, 30.0);
    s.start();
    s.shots.clear();
    for _ in 0..361 {
        s.step(VehicleCommand::idle(), false);
    }
    let hp = s.tanks[PLAYER].hp;
    assert!(hp > 32.0 && hp < 33.0, "{hp}");
    s.tanks[PLAYER].hp = s.max_health(&s.tanks[PLAYER]) - 0.01;
    repair_veteran(&mut s, PLAYER, 1.0);
    assert_eq!(s.tanks[PLAYER].hp, s.max_health(&s.tanks[PLAYER]));
    damage_from(&mut s, PLAYER, 9999.0, PLAYER, None, None);
    repair_veteran(&mut s, PLAYER, 10.0);
    assert_eq!(s.tanks[PLAYER].hp, 0.0);
}

#[test]
fn promoted_max_hull_and_repair_pickups_respect_every_chassis_and_solo_scaling() {
    let mut s = fixture();
    for solo in [false, true] {
        for t in [PLAYER, ENEMY] {
            s.game_mode = if solo { GameMode::Solo } else { GameMode::Team };
            for kind in VehicleKind::PLAYABLE {
                s.tanks[t].kind = kind;
                s.tanks[t].xp = RANKS[3].xp;
                s.tanks[t].hp = 1.0;
                let expected = js_round(
                    vehicle(kind).health * if solo && t == ENEMY { 0.4 } else { 1.0 } * 1.2 * 100.0,
                ) / 100.0;
                near(s.max_health(&s.tanks[t]), expected);
                let mut repair = supply(&mut s, PickupKind::Repair, 0.0, 0.0);
                collect_pickup(&mut s, t, &mut repair);
                near(s.tanks[t].hp, expected);
            }
        }
    }
}

#[test]
fn death_stops_xp_respawn_resets_rank_before_new_hull_and_round_reset_clears_all() {
    let mut s = fixture();
    for t in [PLAYER, ALLY] {
        earn_experience(&mut s, t, RANKS[3].xp, None);
        damage_from(&mut s, t, 9999.0, t, None, None);
        earn_experience(&mut s, t, 30.0, None);
        assert_eq!(s.tanks[t].xp, RANKS[3].xp);
        s.human_kind = VehicleKind::Heavy;
        s.respawn(t, None);
        assert_eq!(s.tanks[t].xp, 0.0);
        assert_eq!(s.tanks[t].hp, vehicle(s.tanks[t].kind).health);
    }
    earn_experience(&mut s, PLAYER, RANKS[2].xp, None);
    let mut snapshot = s.snapshot();
    assert_eq!(snapshot.tanks[0].rank, 2);
    assert_eq!(snapshot.tanks[0].max_hp, 161.0);
    snapshot.tanks[0].xp = 0.0;
    assert_eq!(s.tanks[PLAYER].xp, RANKS[2].xp);
    s.reset(None);
    assert!(s.tanks.iter().all(|t| t.xp == 0.0 && rank_index(t.xp) == 0));
}

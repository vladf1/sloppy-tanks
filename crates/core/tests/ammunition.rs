//! Ammunition: per-weapon shots, cost and cooldown, selection cycling and empty feedback,
//! crate refills and caps, power-ups (rapid, shield, repair), inventory clearing, piercing
//! interception rules, and bot ammo choice and crate seeking. Ported from
//! `tests/ammunition.test.ts`; the per-weapon TypeScript tests are loops here.

mod support;

use sloppy_core::sim::ai::bot_command;
use sloppy_core::sim::ammunition::{AMMO_ORDER, empty_ammo, select_ammo};
use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::bot_personalities::{BotPersonality, bot_ammo, preferred_ammo};
use sloppy_core::sim::data::{STEP, pickup as pickup_stats, vehicle, weapon};
use sloppy_core::sim::math::Vec2;
use sloppy_core::sim::physics::{interaction_groups, vector};
use sloppy_core::sim::weapons::{collect_pickup, fire_weapon, step_projectiles};
use sloppy_core::sim::{
    AmmoInventory, AmmoSelection, BotMode, CoverKind, MatchPhase, Pickup, PickupKind, Shot,
    SimEventType, Simulation, SpecialAmmo, Team, VehicleCommand, Weapon,
};
use support::{clear_arena, concrete, decide, event_count, set_translation, shot_by_id, supply};

/// The first `count` roster tanks, all human-driven, in a column 16 m apart along +z.
fn arena(count: usize) -> Simulation {
    let mut s = Simulation::with_seed(123.0);
    let keep: Vec<usize> = (0..count).collect();
    clear_arena(&mut s, &keep);
    for i in 0..s.tanks.len() {
        let z = i as f64 * 16.0;
        let tank = &mut s.tanks[i];
        tank.human = true;
        tank.protection = 0.0;
        tank.aim = 0.0;
        tank.previous = Vec2::new(0.0, z);
        let body = tank.body;
        s.world.bodies[body].set_translation(vector(0.0, 0.65, z), true);
    }
    s.world.step();
    s.start();
    s
}

fn pickup_kind(ammo: SpecialAmmo) -> PickupKind {
    match ammo {
        SpecialAmmo::Spread => PickupKind::Spread,
        SpecialAmmo::Rocket => PickupKind::Rocket,
        SpecialAmmo::Ricochet => PickupKind::Ricochet,
        SpecialAmmo::Piercing => PickupKind::Piercing,
    }
}

fn ammo_crate(s: &mut Simulation, ammo: SpecialAmmo) -> Pickup {
    supply(s, pickup_kind(ammo), 0.0, 0.0)
}

/// Collect a fresh crate of `kind` with the first tank.
fn pickup(s: &mut Simulation, kind: PickupKind) {
    let mut crate_ = supply(s, kind, 0.0, 0.0);
    collect_pickup(s, 0, &mut crate_);
}

/// A shell on the x axis owned by the absent tank `999 + team`.
fn shot(s: &mut Simulation, fired: Weapon, x: f64, vx: f64, team: Team) -> Shot {
    let id = s.next_id;
    s.next_id += 1;
    Shot {
        id,
        weapon: fired,
        x,
        z: 0.0,
        vx,
        vz: 0.0,
        team,
        owner: 999 + team.index() as u32,
        damage: weapon(fired).damage,
        bounces: 0,
        life: 3.5,
        piercing: if fired == Weapon::Piercing { 1 } else { 0 },
        ..Shot::default()
    }
}

fn command(ammo_selection: Option<AmmoSelection>, fire: bool) -> VehicleCommand {
    VehicleCommand {
        fire,
        ammo_selection,
        ..VehicleCommand::idle()
    }
}

#[test]
fn each_weapon_emits_the_correct_shot_costs_one_unit_and_respects_cooldown() {
    for fired in AMMO_ORDER {
        let mut s = arena(1);
        if let Some(special) = fired.special() {
            pickup(&mut s, pickup_kind(special));
        }
        select_ammo(&mut s.tanks[0], Some(AmmoSelection::Weapon(fired)));
        let before = s.tanks[0].ammo;
        fire_weapon(&mut s, 0);
        assert_eq!(
            s.shots.len(),
            if fired == Weapon::Spread { 3 } else { 1 },
            "{fired:?}"
        );
        let event = s
            .events
            .iter()
            .find(|e| e.kind == SimEventType::Shot)
            .unwrap();
        assert_eq!(event.weapon, Some(fired));
        assert_eq!(event.id, Some(s.tanks[0].id));
        let stats = weapon(fired);
        for p in &s.shots {
            assert_eq!(p.weapon, fired);
            assert_eq!(p.damage, stats.damage);
            assert_eq!(p.bounces, stats.bounces);
            assert_eq!(p.piercing, if fired == Weapon::Piercing { 1 } else { 0 });
            assert!((p.vx.hypot(p.vz) - stats.speed).abs() < 1e-9, "{fired:?}");
        }
        match fired.special() {
            Some(special) => assert_eq!(s.tanks[0].ammo.get(special), before.get(special) - 1.0),
            None => assert_eq!(s.tanks[0].ammo, empty_ammo()),
        }
        let after = s.tanks[0].ammo;
        let cooldown = s.tanks[0].cooldown;
        let shots = s.shots.len();
        fire_weapon(&mut s, 0);
        assert_eq!(s.shots.len(), shots);
        assert_eq!(s.tanks[0].ammo, after);
        assert_eq!(s.tanks[0].cooldown, cooldown);
        s.tanks[0].cooldown = 0.0;
        s.tanks[0].rapid = 12.0;
        fire_weapon(&mut s, 0);
        assert_eq!(s.tanks[0].cooldown, stats.interval / 2.0 / 1.2, "{fired:?}");
    }
}

#[test]
fn standard_remains_unlimited_over_sustained_firing() {
    let mut s = arena(1);
    for _ in 0..500 {
        s.tanks[0].cooldown = 0.0;
        fire_weapon(&mut s, 0);
        s.shots.clear();
    }
    assert_eq!(s.shots_fired, 500);
    assert_eq!(s.tanks[0].ammo, empty_ammo());
    assert_eq!(s.tanks[0].selected_ammo, Weapon::Standard);
}

#[test]
fn stress_multipliers_extend_power_ups_and_weapon_crate_payloads_tenfold() {
    let mut s = arena(1);
    s.power_up_duration_multiplier = 10.0;
    s.ammo_crate_multiplier = 10.0;
    for kind in [
        PickupKind::Rapid,
        PickupKind::Speed,
        PickupKind::Shield,
        PickupKind::Laser,
    ] {
        let mut c = supply(&mut s, kind, 0.0, 0.0);
        assert!(collect_pickup(&mut s, 0, &mut c), "{kind:?}");
        let tank = &s.tanks[0];
        let value = match kind {
            PickupKind::Rapid => tank.rapid,
            PickupKind::Speed => tank.speed,
            PickupKind::Shield => tank.shield,
            _ => tank.laser,
        };
        assert_eq!(value, pickup_stats(kind).duration * 10.0, "{kind:?}");
    }
    for ammo in SpecialAmmo::ALL {
        let mut c = ammo_crate(&mut s, ammo);
        assert!(collect_pickup(&mut s, 0, &mut c));
        assert_eq!(
            s.tanks[0].ammo.get(ammo),
            weapon(ammo.weapon()).per_crate * 10.0,
            "{ammo:?}"
        );
    }
}

#[test]
fn selection_cycles_both_directions_skips_empty_slots_wraps_and_never_changes_cooldown() {
    let mut s = arena(1);
    let t = &mut s.tanks[0];
    t.cooldown = 0.72;
    select_ammo(t, Some(AmmoSelection::Step(1)));
    assert_eq!(t.selected_ammo, Weapon::Standard);
    t.ammo.spread = 2.0;
    t.ammo.piercing = 1.0;
    for expected in [Weapon::Spread, Weapon::Piercing, Weapon::Standard] {
        select_ammo(t, Some(AmmoSelection::Step(1)));
        assert_eq!(t.selected_ammo, expected);
    }
    for expected in [Weapon::Piercing, Weapon::Spread, Weapon::Standard] {
        select_ammo(t, Some(AmmoSelection::Step(-1)));
        assert_eq!(t.selected_ammo, expected);
    }
    select_ammo(t, Some(AmmoSelection::Weapon(Weapon::Rocket)));
    assert_eq!(t.selected_ammo, Weapon::Standard);
    assert_eq!(t.cooldown, 0.72);
}

#[test]
fn selection_precedes_held_fire_depletion_falls_back_and_switching_cannot_bypass_reload() {
    let mut s = arena(1);
    s.tanks[0].ammo.rocket = 1.0;
    s.tanks[0].ammo.spread = 2.0;
    s.step(
        command(Some(AmmoSelection::Weapon(Weapon::Rocket)), true),
        false,
    );
    assert_eq!(s.shots[0].weapon, Weapon::Rocket);
    assert_eq!(s.tanks[0].ammo.rocket, 0.0);
    assert_eq!(s.tanks[0].selected_ammo, Weapon::Standard);
    let cooldown = s.tanks[0].cooldown;
    for selection in [Weapon::Spread, Weapon::Standard, Weapon::Spread] {
        s.step(command(Some(AmmoSelection::Weapon(selection)), true), false);
    }
    assert_eq!(s.shots_fired, 1);
    assert_eq!(s.tanks[0].ammo.spread, 2.0);
    assert!((s.tanks[0].cooldown - (cooldown - 3.0 * STEP)).abs() < 1e-9);
    select_ammo(
        &mut s.tanks[0],
        Some(AmmoSelection::Weapon(Weapon::Standard)),
    );
    // The TypeScript loop was unbounded; a reload is far shorter than this cap.
    let mut ticks = 0;
    while s.shots_fired == 1 {
        s.step(command(None, true), false);
        ticks += 1;
        assert!(ticks < 600, "the reload never finished");
    }
    assert_eq!(s.shots.last().unwrap().weapon, Weapon::Standard);
    assert_eq!(s.tanks[0].cooldown, weapon(Weapon::Standard).interval / 1.2);
}

#[test]
fn death_respawn_and_reset_clear_inventories_and_snapshots_own_their_inventory_copy() {
    let mut s = arena(1);
    for ammo in SpecialAmmo::ALL {
        pickup(&mut s, pickup_kind(ammo));
    }
    select_ammo(
        &mut s.tanks[0],
        Some(AmmoSelection::Weapon(Weapon::Piercing)),
    );
    let snapshot = s.snapshot().tanks[0].clone();
    assert_eq!(snapshot.selected_ammo, Weapon::Piercing);
    let (id, team) = (s.tanks[0].id, s.tanks[0].team);
    s.damage_tank(0, 999.0, id, team, None, None);
    assert_eq!(s.tanks[0].ammo, empty_ammo());
    assert_eq!(s.tanks[0].selected_ammo, Weapon::Standard);
    assert_eq!(s.pickups.len(), 0);
    assert_eq!(snapshot.ammo.piercing, 24.0);
    s.tanks[0].ammo.spread = 1.0;
    select_ammo(&mut s.tanks[0], Some(AmmoSelection::Step(1)));
    assert_eq!(s.tanks[0].selected_ammo, Weapon::Standard);
    s.respawn(0, None);
    assert_eq!(s.tanks[0].ammo, empty_ammo());
    s.tanks[0].ammo.rocket = 7.0;
    s.tanks[0].selected_ammo = Weapon::Rocket;
    s.reset(None);
    assert!(s.tanks.iter().all(|t| t.selected_ammo == Weapon::Standard));
    assert!(
        s.tanks
            .iter()
            .all(|t| t.ammo.values().iter().all(|&n| n == 0.0))
    );
}

#[test]
fn paused_and_results_simulations_ignore_selection() {
    let mut s = arena(1);
    s.tanks[0].ammo.spread = 1.0;
    for phase in [MatchPhase::Ready, MatchPhase::Paused, MatchPhase::Results] {
        s.match_state.phase = phase;
        s.step(command(Some(AmmoSelection::Step(1)), true), false);
        assert_eq!(s.tanks[0].selected_ammo, Weapon::Standard, "{phase:?}");
        assert_eq!(s.tanks[0].ammo.spread, 1.0, "{phase:?}");
    }
}

#[test]
fn crates_equip_the_first_advanced_ammo_and_report_actual_receipt_without_clearing_cooldown() {
    for ammo in SpecialAmmo::ALL {
        let mut s = arena(1);
        let stats = weapon(ammo.weapon());
        s.tanks[0].cooldown = 0.6;
        let mut p = ammo_crate(&mut s, ammo);
        assert!(collect_pickup(&mut s, 0, &mut p));
        assert_eq!(s.tanks[0].ammo.get(ammo), stats.per_crate);
        assert_eq!(s.tanks[0].selected_ammo, ammo.weapon());
        assert_eq!(s.tanks[0].cooldown, 0.6);
        assert_eq!(p.cooldown, 13.0);
        *s.tanks[0].ammo.get_mut(ammo) = stats.carry_limit - 3.0;
        let mut top_up = ammo_crate(&mut s, ammo);
        assert!(collect_pickup(&mut s, 0, &mut top_up));
        assert_eq!(s.tanks[0].ammo.get(ammo), stats.carry_limit);
        assert_eq!(
            s.events.last().unwrap().label.as_deref(),
            Some(format!("+3 {}", stats.unit).as_str()),
            "{ammo:?}"
        );
        let mut full = ammo_crate(&mut s, ammo);
        let events = s.events.len();
        assert!(!collect_pickup(&mut s, 0, &mut full));
        assert!(full.available);
        assert_eq!(full.cooldown, 0.0);
        assert_eq!(s.events.len(), events);
    }
}

#[test]
fn collecting_another_ammo_type_preserves_selection_when_advanced_ammo_is_already_stocked() {
    let mut s = arena(1);
    pickup(&mut s, PickupKind::Spread);
    select_ammo(
        &mut s.tanks[0],
        Some(AmmoSelection::Weapon(Weapon::Standard)),
    );
    pickup(&mut s, PickupKind::Rocket);
    assert_eq!(s.tanks[0].selected_ammo, Weapon::Standard);
    assert_eq!(s.tanks[0].ammo.spread, weapon(Weapon::Spread).per_crate);
    assert_eq!(s.tanks[0].ammo.rocket, weapon(Weapon::Rocket).per_crate);
}

#[test]
fn rapid_fire_modifies_only_selected_ammunition_and_expires_independently() {
    let mut s = arena(1);
    pickup(&mut s, PickupKind::Spread);
    pickup(&mut s, PickupKind::Rapid);
    pickup(&mut s, PickupKind::Ricochet);
    s.tanks[0].selected_ammo = Weapon::Spread;
    fire_weapon(&mut s, 0);
    assert_eq!(s.shots.len(), 3);
    assert_eq!(
        s.tanks[0].cooldown,
        weapon(Weapon::Spread).interval / 2.0 / 1.2
    );
    assert!(s.shots.iter().all(|p| p.damage == 27.0 && p.bounces == 0));
    let cooldown = s.tanks[0].cooldown;
    pickup(&mut s, PickupKind::Rapid);
    pickup(&mut s, PickupKind::Ricochet);
    assert_eq!(s.tanks[0].cooldown, cooldown);
    assert_eq!(s.tanks[0].rapid, 20.0);
    assert_eq!(s.tanks[0].ammo.ricochet, 48.0);
    s.tanks[0].rapid = STEP;
    s.tanks[0].cooldown = 0.0;
    s.step(VehicleCommand::idle(), false);
    assert_eq!(s.tanks[0].rapid, 0.0);
    assert_eq!(s.tanks[0].ammo.ricochet, 48.0);
    fire_weapon(&mut s, 0);
    assert_eq!(s.tanks[0].cooldown, weapon(Weapon::Spread).interval / 1.2);
    s.tanks[0].hp = 1.0;
    pickup(&mut s, PickupKind::Repair);
    let health = vehicle(s.tanks[0].kind).health;
    assert_eq!(s.tanks[0].hp, health);
    pickup(&mut s, PickupKind::Repair);
    assert_eq!(s.tanks[0].hp, health, "repair never overheals");
}

#[test]
fn shield_absorbs_three_shells_spills_excess_damage_expires_and_resets_on_respawn() {
    let mut s = arena(1);
    pickup(&mut s, PickupKind::Shield);
    let hp = s.tanks[0].hp;
    for _ in 0..3 {
        s.damage_tank(0, 40.0, 999, Team::Red, None, None);
    }
    assert_eq!(s.tanks[0].hp, hp);
    assert_eq!(s.tanks[0].shield, 0.0);
    assert_eq!(s.tanks[0].shield_points, 0.0);
    s.damage_tank(0, 40.0, 999, Team::Red, None, None);
    assert_eq!(s.tanks[0].hp, hp - 40.0);
    pickup(&mut s, PickupKind::Shield);
    s.damage_tank(0, 130.0, 999, Team::Red, None, None);
    assert_eq!(s.tanks[0].hp, hp - 50.0);
    pickup(&mut s, PickupKind::Shield);
    s.tanks[0].shield = STEP;
    s.step(VehicleCommand::idle(), false);
    assert_eq!(s.tanks[0].shield_points, 0.0);
    pickup(&mut s, PickupKind::Rapid);
    pickup(&mut s, PickupKind::Ricochet);
    pickup(&mut s, PickupKind::Shield);
    s.respawn(0, None);
    let t = &s.tanks[0];
    assert_eq!(
        [t.rapid, t.ammo.ricochet, t.shield, t.shield_points],
        [0.0; 4]
    );
}

#[test]
fn empty_selection_emits_feedback_without_switching_and_final_special_shot_announces_fallback_once()
{
    let mut s = arena(1);
    s.tanks[0].ammo.rocket = 1.0;
    s.tanks[0].selected_ammo = Weapon::Rocket;
    s.step(
        command(Some(AmmoSelection::Weapon(Weapon::Piercing)), false),
        false,
    );
    assert_eq!(s.tanks[0].selected_ammo, Weapon::Rocket);
    let notice = |s: &Simulation| {
        s.events
            .iter()
            .find(|e| e.kind == SimEventType::Notice)
            .and_then(|e| e.label.clone())
            .unwrap()
    };
    assert!(notice(&s).contains("PIERCING EMPTY"), "{}", notice(&s));
    s.events.clear();
    fire_weapon(&mut s, 0);
    assert_eq!(s.tanks[0].selected_ammo, Weapon::Standard);
    assert!(
        notice(&s).contains("switched to STANDARD"),
        "{}",
        notice(&s)
    );
    fire_weapon(&mut s, 0);
    assert_eq!(event_count(&s, SimEventType::Notice), 1);
    s.events.clear();
    s.match_state.phase = MatchPhase::Paused;
    s.step(
        command(Some(AmmoSelection::Weapon(Weapon::Piercing)), false),
        false,
    );
    assert_eq!(s.events.len(), 0);
}

#[test]
fn simultaneous_collection_skips_full_tanks_awards_one_recipient_and_refills_after_13_seconds() {
    let mut s = arena(3);
    for i in 0..3 {
        set_translation(&mut s, i, (i as f64 - 1.0) * 0.5, 0.0);
        // Disable physical separation for this single shared pickup-contact fixture.
        let colliders = s.world.bodies[s.tanks[i].body].colliders().to_vec();
        for collider in colliders {
            s.world.colliders[collider].set_collision_groups(interaction_groups(0));
        }
    }
    s.tanks[0].ammo.rocket = 24.0;
    let p = ammo_crate(&mut s, SpecialAmmo::Rocket);
    s.pickups.push(p);
    s.step(VehicleCommand::idle(), false);
    assert_eq!(
        [
            s.tanks[0].ammo.rocket,
            s.tanks[1].ammo.rocket,
            s.tanks[2].ammo.rocket
        ],
        [24.0, 12.0, 0.0]
    );
    assert_eq!(event_count(&s, SimEventType::Pickup), 1);
    for i in 0..3 {
        set_translation(&mut s, i, 20.0, 20.0);
    }
    for _ in 0..779 {
        s.step(VehicleCommand::idle(), false);
    }
    assert!(!s.pickups[0].available);
    for _ in 0..2 {
        s.step(VehicleCommand::idle(), false);
    }
    assert!(s.pickups[0].available);
}

#[test]
fn piercing_intercepts_standard_rocket_and_piercing_shells_in_either_order() {
    for opposing in [Weapon::Standard, Weapon::Rocket, Weapon::Piercing] {
        for reversed in [false, true] {
            let label = format!("{opposing:?} reversed={reversed}");
            let mut s = arena(0);
            let a = shot(&mut s, Weapon::Piercing, -2.0, 20.0, Team::Blue);
            let b = shot(&mut s, opposing, 2.0, -20.0, Team::Red);
            let (a_id, b_id) = (a.id, b.id);
            s.shots = if reversed { vec![b, a] } else { vec![a, b] };
            step_projectiles(&mut s, 0.081, false);
            let both = opposing == Weapon::Piercing;
            assert_eq!(s.shots.len(), if both { 2 } else { 1 }, "{label}");
            assert_eq!(shot_by_id(&s, a_id).piercing, 0, "{label}");
            if both {
                assert_eq!(shot_by_id(&s, b_id).piercing, 0, "{label}");
            }
            assert_eq!(event_count(&s, SimEventType::Impact), 1, "{label}");
            assert_eq!(event_count(&s, SimEventType::Explosion), 0, "{label}");
            // Start the next tick still within interception radius; do not resolve this pair twice.
            step_projectiles(&mut s, 0.01, false);
            assert_eq!(s.events.len(), 1, "{label}");
            assert_eq!(s.shots.len(), if both { 2 } else { 1 }, "{label}");
        }
    }
}

#[test]
fn a_spent_piercing_shell_uses_normal_interception_and_opposing_damage_ownership() {
    let mut s = arena(2);
    set_translation(&mut s, 0, 5.0, -2.6);
    set_translation(&mut s, 1, 5.0, 2.6);
    s.tanks[0].hp = 40.0;
    s.tanks[1].hp = 40.0;
    s.world.step();
    let (a_owner, b_owner) = (s.tanks[0].id, s.tanks[1].id);
    let mut a = shot(&mut s, Weapon::Piercing, 0.0, 20.0, Team::Blue);
    a.owner = a_owner;
    let mut first = shot(&mut s, Weapon::Rocket, 2.0, 0.0, Team::Red);
    first.owner = b_owner;
    let mut second = shot(&mut s, Weapon::Standard, 6.0, 0.0, Team::Red);
    second.owner = b_owner;
    s.shots = vec![second, first, a];
    step_projectiles(&mut s, 0.4, false);
    assert_eq!(s.shots.len(), 0);
    assert_eq!(event_count(&s, SimEventType::Explosion), 1);
    assert!(!s.tanks[0].alive);
    assert!(!s.tanks[1].alive);
    assert_eq!(s.tanks[0].kills, 1);
    assert_eq!(s.tanks[1].kills, 1);
}

#[test]
fn piercing_stops_on_tanks_and_cover_and_cannot_intercept_through_thin_cover() {
    let mut s = arena(1);
    set_translation(&mut s, 0, 2.0, 0.0);
    s.world.step();
    let hp = s.tanks[0].hp;
    let p = shot(&mut s, Weapon::Piercing, -2.0, 20.0, Team::Red);
    s.shots = vec![p];
    step_projectiles(&mut s, 0.3, false);
    assert_eq!(s.shots.len(), 0);
    assert_eq!(s.tanks[0].hp, hp - 40.0);
    s.add_cover(&concrete(0.0, 0.0, 0.1, 10.0));
    s.world.step();
    s.events.clear();
    let a = shot(&mut s, Weapon::Piercing, -0.3, 20.0, Team::Blue);
    let b = shot(&mut s, Weapon::Piercing, 0.3, -20.0, Team::Red);
    s.shots = vec![a, b];
    step_projectiles(&mut s, 0.05, false);
    assert_eq!(s.shots.len(), 0);
    assert_eq!(
        s.events
            .iter()
            .filter(|e| e.kind == SimEventType::Explosion || e.kind == SimEventType::Ricochet)
            .count(),
        0
    );
    assert!(
        s.events.iter().all(|e| e.size == Some(0.6)),
        "only cover impacts, no shell interception"
    );
}

#[test]
fn bot_roles_select_stocked_ammo_use_standard_on_cover_and_fall_back_after_depletion() {
    let mut s = arena(2);
    let (bot, enemy) = (0, 1);
    s.tanks[bot].human = false;
    s.tanks[enemy].human = true;
    s.tanks[bot].ammo = AmmoInventory {
        spread: 5.0,
        rocket: 5.0,
        ricochet: 5.0,
        piercing: 5.0,
    };
    // The key order of the TypeScript BOT_AMMO record.
    for role in [
        BotPersonality::Artillery,
        BotPersonality::Heavy,
        BotPersonality::Scout,
        BotPersonality::Minelayer,
        BotPersonality::Sniper,
        BotPersonality::Guard,
        BotPersonality::Support,
    ] {
        s.tanks[bot].brain.personality = role;
        s.tanks[bot].brain.decision = 0.0;
        assert_eq!(preferred_ammo(&s.tanks[bot]), bot_ammo(role), "{role:?}");
        assert_eq!(
            bot_command(&mut s, bot, STEP).ammo_selection,
            Some(AmmoSelection::Weapon(bot_ammo(role))),
            "{role:?}"
        );
    }
    set_translation(&mut s, enemy, 0.0, 50.0);
    let brain = &mut s.tanks[bot].brain;
    brain.target = 0;
    brain.memory = 0.0;
    brain.decision = 99.0;
    brain.goal = Vec2::new(0.0, 10.0);
    s.add_cover(&CoverDef::new(
        CoverKind::Timber,
        0.0,
        7.0,
        4.0,
        0.6,
        2.0,
        60.0,
        0,
    ));
    s.world.step();
    assert_eq!(
        bot_command(&mut s, bot, STEP).ammo_selection,
        Some(AmmoSelection::Weapon(Weapon::Standard))
    );
    s.tanks[bot].ammo = empty_ammo();
    s.tanks[bot].ammo.rocket = 1.0;
    s.tanks[bot].selected_ammo = Weapon::Rocket;
    fire_weapon(&mut s, bot);
    assert_eq!(s.tanks[bot].selected_ammo, Weapon::Standard);
    s.tanks[bot].cooldown = 0.0;
    fire_weapon(&mut s, bot);
    assert_eq!(s.shots.last().unwrap().weapon, Weapon::Standard);
}

#[test]
fn bots_drive_to_useful_crates_consume_ammo_and_ignore_a_full_crate() {
    let mut s = arena(1);
    let bot = 0;
    s.tanks[bot].human = false;
    s.tanks[bot].brain.personality = BotPersonality::Artillery;
    let mut p = ammo_crate(&mut s, SpecialAmmo::Rocket);
    p.z = 6.0;
    s.pickups.push(p);
    decide(&mut s, bot);
    assert_eq!(s.tanks[bot].brain.mode, BotMode::Pickup);
    let mut i = 0;
    while i < 180 && s.pickups[0].available {
        s.step(VehicleCommand::idle(), false);
        i += 1;
    }
    assert!(!s.pickups[0].available);
    assert_eq!(s.tanks[bot].ammo.rocket, 12.0);
    let preferred = preferred_ammo(&s.tanks[bot]);
    select_ammo(&mut s.tanks[bot], Some(AmmoSelection::Weapon(preferred)));
    fire_weapon(&mut s, bot);
    assert_eq!(s.tanks[bot].ammo.rocket, 11.0);
    s.tanks[bot].ammo.rocket = 24.0;
    s.pickups[0].available = true;
    decide(&mut s, bot);
    assert_ne!(s.tanks[bot].brain.mode, BotMode::Pickup);
}

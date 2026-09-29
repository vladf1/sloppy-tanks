//! Bot roles: stable assignment, role movement, hunters and cover, sniper and minelayer
//! behavior, ammunition preference, cadence against the player, and the bot name decks
//! (the former `tests/personalities.test.ts`).

mod support;

use std::collections::HashSet;
use std::f64::consts::PI;

use sloppy_core::sim::ai::bot_command;
use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::bot_personalities::{
    BotPersonality, bot_assignment, bot_profile_for, bot_reload, combat_movement, preferred_ammo,
    shuffled_bot_names,
};
use sloppy_core::sim::data::{STEP, weapon};
use sloppy_core::sim::math::Vec2;
use sloppy_core::sim::physics::vector;
use sloppy_core::sim::pickups::collect_pickup;
use sloppy_core::sim::weapons::{fire_weapon, weapon_interval};
use sloppy_core::sim::{CoverKind, Pickup, PickupKind, Simulation, Team, Weapon};
use support::clear_arena;

struct Duel {
    s: Simulation,
    bot: usize,
    human: usize,
}

fn set_translation(s: &mut Simulation, index: usize, x: f64, z: f64) {
    let body = s.tanks[index].body;
    s.world.bodies[body].set_translation(vector(x, 0.65, z), true);
}

/// The human and the first enemy bot, alone on an empty map, `range` metres apart.
fn duel(range: f64) -> Duel {
    let mut s = Simulation::with_seed(123.0);
    let human = s.human_index().expect("human");
    let human_team = s.tanks[human].team;
    let bot = s
        .tanks
        .iter()
        .position(|t| t.team != human_team)
        .expect("enemy bot");
    clear_arena(&mut s, &[human, bot]);
    let (human, bot) = (0, 1);
    set_translation(&mut s, bot, 0.0, 0.0);
    set_translation(&mut s, human, 0.0, range);
    s.tanks[bot].aim = 0.0;
    s.tanks[bot].brain.decision = 0.0;
    s.tanks[bot].brain.ultra_aggressive = false;
    s.world.step();
    s.start();
    Duel { s, bot, human }
}

fn roster(s: &Simulation) -> Vec<(BotPersonality, bool)> {
    s.tanks
        .iter()
        .filter(|t| !t.human)
        .map(|t| (t.brain.personality, t.brain.ultra_aggressive))
        .collect()
}

fn bot_names(s: &Simulation) -> Vec<String> {
    s.tanks
        .iter()
        .filter(|t| !t.human)
        .map(|t| t.name.clone())
        .collect()
}

#[test]
fn all_roles_appear_assignment_is_stable_and_only_every_tenth_bot_is_a_hunter() {
    let rare: Vec<usize> = (0..100)
        .filter(|&i| bot_assignment(i, Team::from_index(i % 2), i).ultra_aggressive)
        .collect();
    assert_eq!(rare, vec![9, 19, 29, 39, 49, 59, 69, 79, 89, 99]);
    let mut s = Simulation::with_seed(123.0);
    let initial = roster(&s);
    assert_eq!(initial.iter().filter(|(_, rare)| *rare).count(), 1);
    let roles: HashSet<BotPersonality> = initial.iter().map(|(role, _)| *role).collect();
    assert_eq!(roles.len(), BotPersonality::ALL.len());
    s.reset(None);
    assert_eq!(roster(&s), initial);
}

#[test]
fn scouts_close_guards_strafe_snipers_hold_or_retreat_and_hunters_pursue_close_range() {
    let Duel { mut s, bot, .. } = duel(18.0);
    s.tanks[bot].brain.personality = BotPersonality::Scout;
    assert!(combat_movement(&s.tanks[bot], 0.0, 20.0, 1.0).z > 0.0);
    s.tanks[bot].brain.personality = BotPersonality::Guard;
    assert!(combat_movement(&s.tanks[bot], 0.0, 18.0, 1.0).x > 0.0);
    s.tanks[bot].brain.personality = BotPersonality::Sniper;
    assert_eq!(combat_movement(&s.tanks[bot], 0.0, 24.0, 1.0), Vec2::ZERO);
    assert!(combat_movement(&s.tanks[bot], 0.0, 10.0, 1.0).z < 0.0);
    s.tanks[bot].brain.ultra_aggressive = true;
    assert!(combat_movement(&s.tanks[bot], 0.0, 24.0, 1.0).z > 0.0);
}

#[test]
fn hunters_seek_through_cover_but_cannot_shoot_through_it_ordinary_bots_require_sight() {
    let Duel { mut s, bot, human } = duel(18.0);
    s.add_cover(&CoverDef::new(
        CoverKind::Concrete,
        0.0,
        9.0,
        20.0,
        2.0,
        3.0,
        f64::INFINITY,
        0,
    ));
    s.nav.rebuild(&s.covers, None);
    s.world.step();
    s.tanks[bot].brain.personality = BotPersonality::Guard;
    bot_command(&mut s, bot, STEP);
    assert_eq!(s.tanks[bot].brain.target, 0);
    s.tanks[bot].brain.ultra_aggressive = true;
    s.tanks[bot].brain.decision = 0.0;
    let command = bot_command(&mut s, bot, STEP);
    assert_eq!(s.tanks[bot].brain.target, s.tanks[human].id);
    assert!(!command.fire);
    assert!(!s.tanks[bot].brain.path.is_empty());
}

#[test]
fn snipers_actually_hold_their_firing_lane_and_minelayers_deliberately_deploy_near_enemies() {
    let Duel { mut s, bot, human } = duel(24.0);
    s.tanks[bot].brain.personality = BotPersonality::Sniper;
    let hold = bot_command(&mut s, bot, STEP);
    assert!(hold.move_x.hypot(hold.move_z) < 0.01);
    assert!(
        !hold.fire,
        "acquiring a target still requires reaction time"
    );
    s.tanks[bot].brain.personality = BotPersonality::Minelayer;
    set_translation(&mut s, human, 0.0, 12.0);
    s.world.step();
    s.tanks[bot].brain.decision = 0.0;
    assert!(bot_command(&mut s, bot, STEP).mine);
    s.tanks[bot].mine_cooldown = 1.0;
    assert!(!bot_command(&mut s, bot, STEP).mine);
}

#[test]
fn artillery_needs_crates_and_every_role_preserves_the_player_cadence_edge() {
    let Duel { mut s, bot, human } = duel(18.0);
    s.tanks[bot].brain.personality = BotPersonality::Artillery;
    fire_weapon(&mut s, bot);
    assert_eq!(s.shots.last().expect("shot").weapon, Weapon::Standard);
    let mut crate_ = Pickup {
        id: 9999,
        kind: PickupKind::Rocket,
        x: 0.0,
        z: 0.0,
        available: true,
        cooldown: 0.0,
        cooldown_duration: 0.0,
    };
    collect_pickup(&mut s, bot, &mut crate_);
    assert_eq!(s.tanks[bot].selected_ammo, Weapon::Standard);
    s.tanks[bot].selected_ammo = preferred_ammo(&s.tanks[bot]);
    s.tanks[bot].cooldown = 0.0;
    fire_weapon(&mut s, bot);
    assert_eq!(s.shots.last().expect("shot").weapon, Weapon::Rocket);
    assert_eq!(s.tanks[bot].ammo.rocket, 11.0);
    for role in BotPersonality::ALL {
        for ultra in [false, true] {
            for rapid in [0.0, 12.0] {
                s.tanks[bot].brain.personality = role;
                s.tanks[bot].brain.ultra_aggressive = ultra;
                for chosen in [
                    Weapon::Standard,
                    Weapon::Spread,
                    Weapon::Rocket,
                    Weapon::Ricochet,
                    Weapon::Piercing,
                ] {
                    s.tanks[bot].selected_ammo = chosen;
                    s.tanks[human].selected_ammo = chosen;
                    if let Some(special) = chosen.special() {
                        *s.tanks[bot].ammo.get_mut(special) = 5.0;
                        *s.tanks[human].ammo.get_mut(special) = 5.0;
                    }
                    s.tanks[bot].rapid = rapid;
                    s.tanks[human].rapid = rapid;
                    assert!(
                        bot_reload(&s.tanks[bot], 0.0, None)
                            > weapon_interval(&s.tanks[human]) * 1.2,
                        "{role:?} {chosen:?} cadence"
                    );
                }
            }
        }
    }
    assert!(
        bot_profile_for(BotPersonality::Artillery).reload
            > bot_profile_for(BotPersonality::Sniper).reload
    );
}

#[test]
fn bots_pause_between_shots_even_when_breaching_the_human_fires_faster_with_and_without_rapid_fire()
{
    for rapid in [false, true] {
        let Duel { mut s, bot, human } = duel(18.0);
        set_translation(&mut s, bot, -8.0, 0.0);
        set_translation(&mut s, human, -30.0, 30.0);
        s.world.step();
        s.tanks[bot].aim = PI / 2.0;
        let rapid_seconds = if rapid { 12.0 } else { 0.0 };
        s.tanks[bot].rapid = rapid_seconds;
        s.tanks[human].rapid = rapid_seconds;
        s.tanks[bot].brain.goal = Vec2::new(4.0, 0.0);
        s.tanks[bot].brain.decision = 100.0;
        s.tanks[bot].brain.memory = 100.0;
        s.add_cover(&CoverDef::new(
            CoverKind::Concrete,
            0.0,
            0.0,
            1.0,
            5.0,
            2.0,
            10000.0,
            0,
        ));
        let mut bot_shots = 0;
        let mut human_shots = 0;
        for _ in 0..600 {
            s.tanks[bot].cooldown = 0f64.max(s.tanks[bot].cooldown - STEP);
            s.tanks[human].cooldown = 0f64.max(s.tanks[human].cooldown - STEP);
            if bot_command(&mut s, bot, STEP).fire && s.tanks[bot].cooldown == 0.0 {
                fire_weapon(&mut s, bot);
                bot_shots += 1;
            }
            if s.tanks[human].cooldown == 0.0 {
                fire_weapon(&mut s, human);
                human_shots += 1;
            }
        }
        assert!(
            bot_shots > 0 && (bot_shots as f64) < human_shots as f64 * 0.8,
            "rapid {rapid}: bot {bot_shots}, human {human_shots}"
        );
        let interval = (weapon(Weapon::Standard).interval * if rapid { 0.5 } else { 1.0 }) / 1.2;
        assert!(human_shots as f64 >= (10.0 / (interval + STEP)).floor());
    }
}

#[test]
fn bot_name_decks_are_derived_from_the_round_seed_unique_stable_per_seed_new_each_round_kept_on_respawn()
 {
    let deck = shuffled_bot_names(123.0);
    assert!(deck.len() >= 80);
    assert_eq!(deck.iter().collect::<HashSet<_>>().len(), deck.len());
    assert_eq!(deck, shuffled_bot_names(123.0));
    assert_ne!(deck, shuffled_bot_names(124.0));
    let mut s = Simulation::with_seed(123.0);
    let t = s.tanks.iter().position(|t| !t.human).expect("bot");
    let initial = bot_names(&s);
    let name = s.tanks[t].name.clone();
    assert_eq!(initial.iter().collect::<HashSet<_>>().len(), initial.len());
    s.tanks[t].protection = 0.0;
    let opponent = s.tanks[t].team.opponent();
    s.damage_tank(t, 1000.0, 999, opponent, None, None);
    s.respawn(t, None);
    assert_eq!(s.tanks[t].name, name);
    s.reset(None);
    assert_ne!(bot_names(&s), initial);
}

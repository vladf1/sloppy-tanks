//! Menu difficulty: enemy damage scaling (allied and self damage keep the baseline), Solo
//! damage ordering before shield absorption, and monotonic enemy aim, reaction and cadence
//! while allies stay unchanged. Ported from `tests/difficulty.test.ts`.

mod support;

use sloppy_core::sim::ai::bot_command;
use sloppy_core::sim::difficulty::{Difficulty, difficulty_tuning, parse_difficulty};
use sloppy_core::sim::simulation::GameMode;
use sloppy_core::sim::simulation_rules::SOLO;
use support::{ALLY, ENEMY, PLAYER, damage_from, squad};

#[test]
fn enemy_damage_scales_while_allied_and_self_damage_keep_baseline_and_reset_keeps_choice() {
    for level in Difficulty::ALL {
        let mut s = squad();
        s.difficulty = level;
        let hp = s.tanks[PLAYER].hp;
        damage_from(&mut s, PLAYER, 20.0, ENEMY, None, None);
        assert_eq!(
            s.tanks[PLAYER].hp,
            hp - 20.0 * difficulty_tuning(level).damage,
            "{level:?}"
        );
        let enemy_hp = s.tanks[ENEMY].hp;
        damage_from(&mut s, ENEMY, 20.0, ALLY, None, None);
        assert_eq!(s.tanks[ENEMY].hp, enemy_hp - 20.0, "{level:?}");
        let own_hp = s.tanks[PLAYER].hp;
        damage_from(&mut s, PLAYER, 10.0, PLAYER, None, None);
        assert_eq!(s.tanks[PLAYER].hp, own_hp - 10.0, "{level:?}");
        let safe_hp = s.tanks[PLAYER].hp;
        damage_from(&mut s, PLAYER, 20.0, ALLY, None, None);
        assert_eq!(s.tanks[PLAYER].hp, safe_hp, "{level:?}");
        s.reset(None);
        assert_eq!(s.difficulty, level);
    }
}

#[test]
fn difficulty_applies_to_solo_damage_before_shield_absorption() {
    let mut s = squad();
    s.game_mode = GameMode::Solo;
    s.difficulty = Difficulty::Easy;
    s.tanks[PLAYER].shield = 10.0;
    s.tanks[PLAYER].shield_points = 3.0;
    let hp = s.tanks[PLAYER].hp;
    damage_from(&mut s, PLAYER, 40.0, ENEMY, None, None);
    assert_eq!(
        s.tanks[PLAYER].hp,
        hp - (40.0 * 0.9 * SOLO.enemy_damage_multiplier - 3.0)
    );
    assert_eq!(s.tanks[PLAYER].shield_points, 0.0);
}

#[derive(Debug, PartialEq)]
struct Decision {
    reaction: f64,
    error: f64,
    reload: f64,
}

/// A bot's first reaction and aim error, and the reload of its first real firing decision.
fn decision(level: Difficulty, friendly: bool) -> Decision {
    let mut s = squad();
    s.difficulty = level;
    let tank = if friendly { ALLY } else { ENEMY };
    s.tanks[tank].brain.decision = 0.0;
    s.tanks[tank].brain.target = 0;
    bot_command(&mut s, tank, 1.0 / 60.0);
    let reaction = s.tanks[tank].brain.reaction;
    let error = s.tanks[tank].brain.aim_error;
    s.tanks[tank].brain.reaction = 0.0;
    s.tanks[tank].brain.decision = 10.0;
    s.tanks[tank].brain.fire_delay = 0.0;
    s.tanks[tank].aim = if friendly { std::f64::consts::PI } else { 0.0 };
    // Let the turret align, then capture a real firing decision.
    let mut i = 0;
    while i < 240 && s.tanks[tank].brain.fire_delay == 0.0 {
        s.tanks[tank].aim = bot_command(&mut s, tank, 1.0 / 60.0).aim;
        i += 1;
    }
    assert!(s.tanks[tank].brain.fire_delay > 0.0);
    Decision {
        reaction,
        error: error.abs(),
        reload: s.tanks[tank].brain.fire_delay,
    }
}

#[test]
fn enemy_aim_reaction_and_cadence_vary_monotonically_while_allies_are_unchanged() {
    let easy = decision(Difficulty::Easy, false);
    let normal = decision(Difficulty::Normal, false);
    let hard = decision(Difficulty::Hard, false);
    for (key, pick) in [
        (
            "reaction",
            (|d: &Decision| d.reaction) as fn(&Decision) -> f64,
        ),
        ("error", |d: &Decision| d.error),
        ("reload", |d: &Decision| d.reload),
    ] {
        assert!(pick(&easy) > pick(&normal), "Easy {key}");
        assert!(pick(&normal) > pick(&hard), "Hard {key}");
    }
    assert_eq!(
        decision(Difficulty::Easy, true),
        decision(Difficulty::Normal, true)
    );
    assert_eq!(
        decision(Difficulty::Hard, true),
        decision(Difficulty::Normal, true)
    );
    assert_eq!(parse_difficulty(Some("corrupt")), Difficulty::Normal);
    assert_eq!(parse_difficulty(None), Difficulty::Normal);
}

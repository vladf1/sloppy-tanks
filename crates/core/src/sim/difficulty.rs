//! Enemy tuning per menu difficulty.

use serde::{Deserialize, Serialize};

use super::simulation::Simulation;
use super::types::Tank;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Difficulty {
    Easy,
    #[default]
    Normal,
    Hard,
}

impl Difficulty {
    pub const ALL: [Difficulty; 3] = [Difficulty::Easy, Difficulty::Normal, Difficulty::Hard];

    pub const fn as_str(self) -> &'static str {
        match self {
            Difficulty::Easy => "easy",
            Difficulty::Normal => "normal",
            Difficulty::Hard => "hard",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DifficultyTuning {
    pub label: &'static str,
    pub description: &'static str,
    pub reaction: f64,
    pub aim_error: f64,
    pub reload: f64,
    pub damage: f64,
}

const EASY: DifficultyTuning = DifficultyTuning {
    label: "Easy",
    description: "Slightly more forgiving enemies · 10% less enemy damage",
    reaction: 1.2,
    aim_error: 1.2,
    reload: 1.1,
    damage: 0.9,
};
const NORMAL: DifficultyTuning = DifficultyTuning {
    label: "Normal",
    description: "The original combat balance",
    reaction: 1.0,
    aim_error: 1.0,
    reload: 1.0,
    damage: 1.0,
};
const HARD: DifficultyTuning = DifficultyTuning {
    label: "Hard",
    description: "Faster, sharper enemies · 15% more enemy damage",
    reaction: 0.7,
    aim_error: 0.65,
    reload: 0.85,
    damage: 1.15,
};

pub const fn difficulty_tuning(difficulty: Difficulty) -> &'static DifficultyTuning {
    match difficulty {
        Difficulty::Easy => &EASY,
        Difficulty::Normal => &NORMAL,
        Difficulty::Hard => &HARD,
    }
}

pub fn parse_difficulty(value: Option<&str>) -> Difficulty {
    match value {
        Some("easy") => Difficulty::Easy,
        Some("hard") => Difficulty::Hard,
        _ => Difficulty::Normal,
    }
}

/// Allies retain the original behavior; mode-specific Solo tuning still applies.
pub fn enemy_difficulty(simulation: &Simulation, tank: &Tank) -> &'static DifficultyTuning {
    difficulty_tuning(
        if !tank.human && (simulation.multiplayer() || tank.team != simulation.human_team) {
            simulation.difficulty
        } else {
            Difficulty::Normal
        },
    )
}

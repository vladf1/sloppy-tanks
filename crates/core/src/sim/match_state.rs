//! Round clock, team scores and the victory rules.

use super::data::{ROUND_TIME, SCORE_LIMIT};
use super::types::{Match, MatchPhase, Team};

pub fn new_match(round: u32) -> Match {
    Match {
        phase: MatchPhase::Ready,
        time: ROUND_TIME,
        scores: [0, 0],
        overtime: false,
        ended_early: None,
        winner: None,
        round,
    }
}

pub fn award_kill(
    state: &mut Match,
    victim: Team,
    killer: Team,
    self_kill: bool,
    allow_victory: bool,
) {
    if self_kill || victim == killer || state.phase != MatchPhase::Playing {
        return;
    }
    state.scores[killer.index()] += 1;
    if allow_victory && (state.overtime || state.scores[killer.index()] >= SCORE_LIMIT) {
        finish(state, killer);
    }
}

pub fn tick_match(state: &mut Match, dt: f64) {
    if state.phase != MatchPhase::Playing || state.overtime {
        return;
    }
    state.time = 0f64.max(state.time - dt);
    if state.time == 0.0 {
        if state.scores[0] == state.scores[1] {
            state.overtime = true;
        } else {
            finish(
                state,
                if state.scores[0] > state.scores[1] {
                    Team::Blue
                } else {
                    Team::Red
                },
            );
        }
    }
}

fn finish(state: &mut Match, team: Team) {
    state.winner = Some(team);
    state.phase = MatchPhase::Results;
}

/// Finish the current round for its recap without declaring an unearned winner.
pub fn end_battle(state: &mut Match) {
    if state.phase != MatchPhase::Paused && state.phase != MatchPhase::Playing {
        return;
    }
    state.ended_early = Some(true);
    state.winner = None;
    state.phase = MatchPhase::Results;
}

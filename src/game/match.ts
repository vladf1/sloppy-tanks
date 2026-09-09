import { ROUND_TIME, SCORE_LIMIT } from "./data";
import type { Match, Team } from "./types";
export function newMatch(round = 1): Match {
  return {
    phase: "ready",
    time: ROUND_TIME,
    scores: [0, 0],
    overtime: false,
    winner: null,
    round,
  };
}
export function awardKill(match: Match, victim: Team, killer: Team, self: boolean): void {
  if (self || victim === killer || match.phase !== "playing") {
    return;
  }
  match.scores[killer]++;
  if (match.overtime || match.scores[killer] >= SCORE_LIMIT) {
    finish(match, killer);
  }
}
export function tickMatch(match: Match, dt: number): void {
  if (match.phase !== "playing" || match.overtime) {
    return;
  }
  match.time = Math.max(0, match.time - dt);
  if (match.time === 0) {
    if (match.scores[0] === match.scores[1]) {
      match.overtime = true;
    } else {
      finish(match, match.scores[0] > match.scores[1] ? 0 : 1);
    }
  }
}
function finish(match: Match, team: Team): void {
  match.winner = team;
  match.phase = "results";
}

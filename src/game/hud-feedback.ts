import type { DamageCause, EngineEvent, HumanState } from "./engine-api";

const DAMAGE_LABELS: Record<DamageCause, string> = {
  standard: "Standard shell",
  spread: "Spread shot",
  rocket: "Rocket blast",
  tow: "TOW missile",
  ricochet: "Ricochet shell",
  piercing: "Piercing shell",
  mine: "Mine explosion",
  drum: "Exploding barrel",
  interception: "Shell collision blast",
  explosion: "Explosion",
};

/** Both HUDs describe the authoritative damage source, including self and yard damage. */
export function deathCause(
  event: EngineEvent,
  scoreboard: readonly { id: number; name: string }[],
): string {
  const killer = scoreboard.find((tank) => tank.id === event.owner);
  const cause = event.damageSource ? DAMAGE_LABELS[event.damageSource.cause] : "Unknown weapon";
  const weapon = `${/^[aeiou]/i.test(cause) ? "an" : "a"} ${cause.toLowerCase()}`;
  return event.owner === event.id
    ? `You destroyed yourself with ${weapon}.`
    : killer
      ? `${killer.name} killed you with ${weapon}.`
      : `You were destroyed by ${weapon}.`;
}

export function effectsLabel(
  tank: Pick<
    HumanState,
    "protection" | "shield" | "shieldPoints" | "rapid" | "speed" | "laser" | "selfRepair"
  >,
): string {
  return [
    tank.protection > 0 ? "SPAWN SHIELD" : null,
    tank.shield > 0
      ? `◇ SHIELD ${Math.ceil(tank.shieldPoints)} HP · ${Math.ceil(tank.shield)}s`
      : null,
    tank.rapid > 0 ? `» RAPID ${Math.ceil(tank.rapid)}s` : null,
    tank.speed > 0 ? `ϟ BOOST ${Math.ceil(tank.speed)}s` : null,
    tank.laser > 0 ? `✧ LASER DEFENSE ${Math.ceil(tank.laser)}s` : null,
    tank.selfRepair ? "SELF-REPAIR" : null,
  ]
    .filter(Boolean)
    .join("  ");
}

export function rankTitle(
  tank: Pick<
    HumanState,
    "rank" | "rankDamage" | "rankFireRate" | "rankHealth" | "rankRepair" | "repairDelay"
  >,
): string {
  return tank.rank === 0
    ? "Earn XP from enemy hull damage and kills. Ranks reset on respawn."
    : `+${Math.round((tank.rankDamage - 1) * 100)}% damage · +${Math.round((tank.rankFireRate - 1) * 100)}% fire rate · +${Math.round((tank.rankHealth - 1) * 100)}% hull${tank.rankRepair ? ` · repairs ${tank.rankRepair * 100}% hull/s after ${tank.repairDelay}s out of combat` : ""}`;
}

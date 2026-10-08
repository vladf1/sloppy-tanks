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

/** The killer and victim of a kill-feed row, as both HUDs name them. */
export function killFeedNames(
  event: Pick<EngineEvent, "id" | "owner">,
  viewerId: number,
  scoreboard: readonly { id: number; name: string }[],
): string[] {
  const name = (id: number | undefined) =>
    id === viewerId ? "YOU" : (scoreboard.find((tank) => tank.id === id)?.name ?? "YARD");
  return [name(event.owner), name(event.id)];
}

/** Shows a feed row: one name for a notice, or killer and victim with an arrow between. The
 * arrow is drawn in CSS because small font sets, such as Tesla's browser, lack symbol glyphs
 * like U+25B8 `▸` and draw a missing-glyph box instead. The viewer's own kills (`YOU` first,
 * and not also the victim) are highlighted; `entering` is the newest row, which plays the pop-in
 * once rather than again each time older rows shift down. */
export function showFeedRow(row: HTMLElement, names: readonly string[], entering = false): void {
  const key = names.join("\n");
  if (row.dataset.names === key) {
    return;
  }
  row.dataset.names = key;
  const ownKill = names.length > 1 && names[0] === "YOU" && names[1] !== "YOU";
  row.classList.toggle("own-kill", ownKill);
  row.classList.remove("own-kill-new");
  if (ownKill && entering) {
    // Reading offsetWidth commits the removal so a reused row restarts the animation.
    void row.offsetWidth;
    row.classList.add("own-kill-new");
  }
  row.replaceChildren(names[0]);
  for (const name of names.slice(1)) {
    const arrow = document.createElement("i");
    arrow.className = "feed-arrow";
    arrow.setAttribute("role", "img");
    arrow.setAttribute("aria-label", "destroyed");
    row.append(arrow, name);
  }
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

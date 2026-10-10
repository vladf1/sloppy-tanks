import { AMMO_OPTIONS } from "./ammo-options";
import type { DamageCause, EngineEvent, HumanState } from "./engine-api";

/** The kill feed shows this many of its newest rows. */
export const FEED_ROWS = 4;

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

/** Whether the viewer destroyed someone else. Decided by tank identity, not by the displayed
 * names, since another player may be called "YOU". */
export function isOwnKill(event: Pick<EngineEvent, "id" | "owner">, viewerId: number): boolean {
  return event.owner === viewerId && event.id !== viewerId;
}

/** One kill-feed row: the names to show and how long it stays (seconds). `seq` tells two
 * otherwise identical rows apart, such as the same kill twice in one window. */
export interface FeedRow {
  names: string[];
  ownKill: boolean;
  time: number;
  seq: number;
}

let lastFeedSeq = 0;

export function newFeedRow(names: string[], ownKill = false): FeedRow {
  return { names, ownKill, time: 5, seq: ++lastFeedSeq };
}

/** Shows a feed row: one name for a notice, or killer and victim with an arrow between. The
 * arrow is drawn in CSS because small font sets, such as Tesla's browser, lack symbol glyphs
 * like U+25B8 `▸` and draw a missing-glyph box instead. The viewer's own kills are highlighted;
 * `entering` is the newest row, which plays the pop-in once rather than again each time older
 * rows shift down. */
export function showFeedRow(
  row: HTMLElement,
  feedRow: Pick<FeedRow, "names" | "ownKill" | "seq">,
  entering = false,
): void {
  const { names, ownKill } = feedRow;
  const key = `${feedRow.seq}\n${names.join("\n")}`;
  if (row.dataset.names === key) {
    return;
  }
  row.dataset.names = key;
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

/** Ages the kill feed by `dt` seconds and shows its newest live rows in `feed`, reusing
 * row elements. Returns the rows still showing. */
export function showFeed(feed: Element, rows: FeedRow[], dt: number): FeedRow[] {
  const live = rows.filter((row) => (row.time -= dt) > 0);
  live.length = Math.min(FEED_ROWS, live.length);
  while (feed.childElementCount > live.length) {
    feed.lastElementChild!.remove();
  }
  live.forEach((row, index) => {
    let node = feed.children[index] as HTMLElement | undefined;
    if (!node) {
      node = document.createElement("div");
      feed.append(node);
    }
    showFeedRow(node, row, index === 0);
  });
  return live;
}

/** Writes `text` into the element `#id` under `root`, unless it already shows it. */
export function setText(root: ParentNode, id: string, text: string): void {
  const node = root.querySelector(`#${id}`);
  if (node && node.textContent !== text) {
    node.textContent = text;
  }
}

/** Both HUDs' tank status (`hudMarkup` under `root`): hull, tank and rank, the health bar,
 * the ammo slots, which `slotsDisabled` makes unusable, the mine, power-ups and the low
 * hull warning. */
export function showTankStatus(root: ParentNode, tank: HumanState, slotsDisabled: boolean): void {
  setText(root, "hp", String(Math.max(0, Math.ceil(tank.hp))));
  setText(root, "vehicle-name", tank.vehicleName);
  setText(root, "rank", tank.rankName.toUpperCase());
  const rank = root.querySelector<HTMLElement>("#rank")!;
  rank.dataset.rank = String(tank.rank);
  rank.title = rankTitle(tank);
  const hpbar = root.querySelector<HTMLElement>("#hpbar")!;
  hpbar.style.width = `${tank.healthRatio * 100}%`;
  hpbar.style.backgroundColor = `#${tank.healthColor.toString(16).padStart(6, "0")}`;
  AMMO_OPTIONS.forEach(({ weapon, label: name }, index) => {
    const slotState = tank.ammo.find((slot) => slot.weapon === weapon);
    const selected = !!slotState?.selected;
    const count = slotState?.count === null ? "∞" : String(slotState?.count ?? 0);
    setText(root, `ammo-count-${weapon}`, count);
    const slot = root.querySelector<HTMLButtonElement>(`#ammo-${weapon}`)!;
    slot.disabled = slotsDisabled;
    slot.setAttribute("aria-pressed", String(selected));
    slot.classList.toggle("selected", selected);
    slot.classList.toggle("empty", !slotState?.available);
    const label = `${index + 1}: ${name}, ${count === "0" ? "empty, collect an ammo crate" : count === "∞" ? "unlimited" : count + " remaining"}${selected ? ", selected" : ""}`;
    if (slot.getAttribute("aria-label") !== label) {
      slot.setAttribute("aria-label", label);
    }
  });
  setText(
    root,
    "mine",
    tank.mineCooldown > 0 ? `MINE ${tank.mineCooldown.toFixed(1)}s` : "MINE READY · RMB",
  );
  setText(root, "effects", effectsLabel(tank));
  root
    .querySelector(".status")!
    .classList.toggle("critical-health", tank.alive && tank.healthRatio < 0.25);
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

import { AMMO_OPTIONS } from "./ammo-options";
import type { DamageCause, EngineEvent, HumanState } from "./engine-api";
import { CAUSE_ICONS } from "./ui-markup";

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

/** What wrecked the viewer's tank, for the respawn card: the killer's name (null for
 * the yard, a departed player or the viewer's own ordnance) and the weapon, when the
 * engine reported one. */
export interface DeathReport {
  killer: string | null;
  self: boolean;
  cause: DamageCause | null;
  /** The weapon's label, such as "Rocket blast", or null when it is unknown. */
  weapon: string | null;
}

export function deathReport(
  event: EngineEvent,
  scoreboard: readonly { id: number; name: string }[],
): DeathReport {
  const self = event.owner !== undefined && event.owner === event.id;
  const killer = self ? undefined : scoreboard.find((tank) => tank.id === event.owner);
  const cause = event.damageSource?.cause ?? null;
  return {
    killer: killer?.name ?? null,
    self,
    cause,
    weapon: cause ? DAMAGE_LABELS[cause] : null,
  };
}

/** Both HUDs describe the authoritative damage source, including self and yard damage.
 * An unreported weapon is left out rather than called unknown. */
export function deathCause(
  event: EngineEvent,
  scoreboard: readonly { id: number; name: string }[],
): string {
  const { killer, self, weapon: label } = deathReport(event, scoreboard);
  const weapon = label ? `${/^[aeiou]/i.test(label) ? "an" : "a"} ${label.toLowerCase()}` : "";
  if (self) {
    return weapon ? `You destroyed yourself with ${weapon}.` : "You destroyed yourself.";
  }
  if (killer) {
    return weapon ? `${killer} killed you with ${weapon}.` : `${killer} destroyed you.`;
  }
  return weapon ? `You were destroyed by ${weapon}.` : "You were destroyed.";
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

/** Which side a kill-feed name is drawn in: a team's colour, or neutral for the yard. */
export type FeedSide = "blue" | "red" | "neutral";

/** The killer's and victim's sides of a death, for colouring the feed. The victim's team
 * comes with the event; allies take no hull damage from each other, so the killer is on
 * the other side unless the tank destroyed itself or the yard did it. */
export function killFeedSides(event: Pick<EngineEvent, "id" | "owner" | "team">): FeedSide[] {
  const side = (team: number | undefined): FeedSide =>
    team === 0 ? "blue" : team === 1 ? "red" : "neutral";
  const victim = side(event.team);
  const killer =
    event.owner === undefined
      ? "neutral"
      : event.owner === event.id
        ? victim
        : side(event.team === undefined ? undefined : 1 - event.team);
  return [killer, victim];
}

/** One kill-feed row: the names to show and how long it stays (seconds). `seq` tells two
 * otherwise identical rows apart, such as the same kill twice in one window. `cause`
 * picks the weapon icon between the names and `sides` their colours. */
export interface FeedRow {
  names: string[];
  ownKill: boolean;
  time: number;
  seq: number;
  cause?: DamageCause;
  sides?: FeedSide[];
}

let lastFeedSeq = 0;

export function newFeedRow(
  names: string[],
  ownKill = false,
  details: Pick<FeedRow, "cause" | "sides"> = {},
): FeedRow {
  return { names, ownKill, time: 5, seq: ++lastFeedSeq, ...details };
}

/** Shows a feed row: one name for a notice, or killer and victim with an arrow between. The
 * arrow is drawn in CSS because small font sets, such as Tesla's browser, lack symbol glyphs
 * like U+25B8 `▸` and draw a missing-glyph box instead. The viewer's own kills are highlighted;
 * `entering` is the newest row, which plays the pop-in once rather than again each time older
 * rows shift down. */
export function showFeedRow(
  row: HTMLElement,
  feedRow: Pick<FeedRow, "names" | "ownKill" | "seq" | "cause" | "sides">,
  entering = false,
): void {
  const { names, ownKill, cause, sides = [] } = feedRow;
  const key = `${feedRow.seq}\n${names.join("\n")}\n${cause ?? ""}\n${sides.join(" ")}`;
  if (row.dataset.names === key) {
    return;
  }
  row.dataset.names = key;
  row.classList.toggle("own-kill", ownKill);
  row.classList.remove("own-kill-new", "feed-new");
  if (entering) {
    // Reading offsetWidth commits the removal so a reused row restarts the animation.
    void row.offsetWidth;
    row.classList.add(ownKill ? "own-kill-new" : "feed-new");
  }
  const name = (text: string, index: number) => {
    const span = document.createElement("span");
    span.className = `feed-name ${sides[index] ?? ""}`.trim();
    span.classList.toggle("you", text === "YOU" && ownKill === (index === 0));
    span.textContent = text;
    return span;
  };
  row.replaceChildren(name(names[0], 0));
  names.slice(1).forEach((victim, index) => {
    const icon = CAUSE_ICONS[cause ?? ""];
    let mark: Element;
    if (icon) {
      mark = document.createElementNS("http://www.w3.org/2000/svg", "svg");
      mark.setAttribute("class", "feed-weapon");
      const use = document.createElementNS("http://www.w3.org/2000/svg", "use");
      use.setAttribute("href", `#${icon}`);
      mark.append(use);
    } else {
      mark = document.createElement("i");
      mark.className = "feed-arrow";
    }
    mark.setAttribute("role", "img");
    mark.setAttribute("aria-label", "destroyed");
    row.append(mark, name(victim, index + 1));
  });
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
  setText(root, "hp-max", `/ ${Math.round(tank.maxHp)}`);
  const hpbar = root.querySelector<HTMLElement>("#hpbar")!;
  const width = `${tank.healthRatio * 100}%`;
  hpbar.style.width = width;
  hpbar.style.backgroundColor = `#${tank.healthColor.toString(16).padStart(6, "0")}`;
  // The ghost behind the bar trails it down (a CSS transition), showing the chunk a hit
  // took; it sits under the bar, so a repair hides it until it catches up.
  const ghost = root.querySelector<HTMLElement>("#hpghost");
  if (ghost && ghost.style.width !== width) {
    ghost.style.width = width;
  }
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
  root.querySelector(".mine-line")?.classList.toggle("cooling", tank.mineCooldown > 0);
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

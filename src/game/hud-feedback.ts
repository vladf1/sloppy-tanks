import { AMMO_OPTIONS } from "./ammo-options";
import type { DamageCause, EngineEvent, HumanState, Team, VehicleKind } from "./engine-api";
import { CAUSE_ICONS } from "./ui-markup";

/** The kill feed shows this many of its newest rows. */
export const FEED_ROWS = 3;

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

/** The vehicles' names, as Battle Setup and the HUD show them. */
export const VEHICLE_NAMES: Record<VehicleKind, string> = {
  scout: "SKIPPER",
  balanced: "BRUISER",
  heavy: "BIG RIG",
  humvee: "HUNTER",
};

/** A scoreboard row as the death report reads it; rooms may leave out the details. */
interface KillerRow {
  id: number;
  name: string;
  kills?: number;
  team?: Team;
  kind?: VehicleKind;
}

/** What wrecked the viewer's tank, for the respawn card: the killer's name (null for
 * the yard, a departed player or the viewer's own ordnance), what they drive and how
 * many kills they have, and the weapon, when the engine reported one. */
export interface DeathReport {
  killer: string | null;
  self: boolean;
  cause: DamageCause | null;
  /** The weapon's label, such as "Rocket blast", or null when it is unknown. */
  weapon: string | null;
  /** The killer's vehicle name, team and kills, when the scoreboard knows them. */
  killerVehicle?: string;
  killerTeam?: Team;
  killerKills?: number;
}

export function deathReport(event: EngineEvent, scoreboard: readonly KillerRow[]): DeathReport {
  const self = event.owner !== undefined && event.owner === event.id;
  const killer = self ? undefined : scoreboard.find((tank) => tank.id === event.owner);
  const cause = event.damageSource?.cause ?? null;
  return {
    killer: killer?.name ?? null,
    self,
    cause,
    weapon: cause ? DAMAGE_LABELS[cause] : null,
    ...(killer?.kind ? { killerVehicle: VEHICLE_NAMES[killer.kind] } : {}),
    ...(killer?.team !== undefined ? { killerTeam: killer.team } : {}),
    ...(killer?.kills !== undefined ? { killerKills: killer.kills } : {}),
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
    return weapon ? `${killer} destroyed you with ${weapon}.` : `${killer} destroyed you.`;
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
 * picks the weapon icon between the names and `sides` their colours; `ownDeath` marks
 * the viewer's own wreck, which the feed highlights like the viewer's kills. */
export interface FeedRow {
  names: string[];
  ownKill: boolean;
  time: number;
  seq: number;
  cause?: DamageCause;
  sides?: FeedSide[];
  ownDeath?: boolean;
}

/** The weapon icon between a kill-feed row's names. Every kill gets one, so every row
 * reads the same way; a kill without a reported weapon shows the generic wreck mark. */
export function feedIcon(cause: DamageCause | undefined): string {
  return (cause && CAUSE_ICONS[cause]) || "hud-skull";
}

let lastFeedSeq = 0;

export function newFeedRow(
  names: string[],
  ownKill = false,
  details: Pick<FeedRow, "cause" | "sides" | "ownDeath"> = {},
): FeedRow {
  return { names, ownKill, time: 5, seq: ++lastFeedSeq, ...details };
}

/** Shows a feed row: one name for a notice, or killer and victim with the weapon's icon
 * between. Icons are SVG shapes because small font sets, such as Tesla's browser, lack
 * symbol glyphs and draw a missing-glyph box instead. Every row has one plate; the
 * viewer's own kills and wreck carry its highlight. `entering` is the newest row, which
 * plays the pop-in once rather than again each time older rows shift down. */
export function showFeedRow(
  row: HTMLElement,
  feedRow: Pick<FeedRow, "names" | "ownKill" | "seq" | "cause" | "sides" | "ownDeath">,
  entering = false,
): void {
  const { names, ownKill, cause, sides = [], ownDeath = false } = feedRow;
  const key = `${feedRow.seq}\n${names.join("\n")}\n${cause ?? ""}\n${sides.join(" ")}\n${ownDeath}`;
  if (row.dataset.names === key) {
    return;
  }
  row.dataset.names = key;
  row.classList.toggle("own-kill", ownKill);
  row.classList.toggle("own-death", ownDeath);
  // The plate's ends take the killer's and the victim's team colours.
  row.dataset.from = sides[0] ?? "neutral";
  row.dataset.to = sides[1] ?? "neutral";
  row.classList.remove("own-kill-new", "feed-new");
  if (entering) {
    // Reading offsetWidth commits the removal so a reused row restarts the animation.
    void row.offsetWidth;
    row.classList.add(ownKill ? "own-kill-new" : "feed-new");
  }
  const name = (text: string, index: number) => {
    const span = document.createElement("span");
    span.className = `feed-name ${sides[index] ?? ""}`.trim();
    span.classList.toggle("you", index === 0 ? ownKill : ownDeath);
    span.textContent = text;
    return span;
  };
  row.replaceChildren(name(names[0], 0));
  names.slice(1).forEach((victim, index) => {
    const mark = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    mark.setAttribute("class", "feed-weapon");
    mark.dataset.cause = cause ?? "wreck";
    const use = document.createElementNS("http://www.w3.org/2000/svg", "use");
    use.setAttribute("href", `#${feedIcon(cause)}`);
    mark.append(use);
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

/** A point on screen in normalized device coordinates (y up). */
export interface ScreenPoint {
  x: number;
  y: number;
}

/** Where the damage ring sits: around the viewer's tank overhead, or opened out to the
 * screen's edges round its middle in first person (or whenever the tank is off screen). */
export function damageRingPlacement(viewer: ScreenPoint | null): {
  left: string;
  top: string;
  edge: boolean;
} {
  if (!viewer) {
    return { left: "50%", top: "50%", edge: true };
  }
  const percent = (value: number) => `${(Math.min(1, Math.max(0, value)) * 100).toFixed(2)}%`;
  return { left: percent((viewer.x + 1) / 2), top: percent((1 - viewer.y) / 2), edge: false };
}

/** The point on the screen's edge (CSS percentages) toward a hit `angle` radians
 * clockwise from straight up. */
export function damageEdgePoint(angle: number): { x: string; y: string } {
  const dx = Math.sin(angle);
  const dy = -Math.cos(angle);
  const reach = 0.5 / Math.max(Math.abs(dx), Math.abs(dy));
  const percent = (value: number) => `${((0.5 + value * reach) * 100).toFixed(1)}%`;
  return { x: percent(dx), y: percent(dy) };
}

/** Moves the damage ring onto the viewer (`damageRingPlacement`). */
export function placeDamageRing(indicator: HTMLElement, viewer: ScreenPoint | null): void {
  const { left, top, edge } = damageRingPlacement(viewer);
  if (indicator.style.left !== left) {
    indicator.style.left = left;
  }
  if (indicator.style.top !== top) {
    indicator.style.top = top;
  }
  indicator.classList.toggle("edge", edge);
}

/** Turns the damage ring toward a hit (`angle`: clockwise screen radians) and pulses it. */
export function showDamageRing(
  indicator: HTMLElement,
  angle: number,
  viewer: ScreenPoint | null,
): void {
  placeDamageRing(indicator, viewer);
  indicator.style.setProperty("--angle", `${angle}rad`);
  // In first person the glow sits on the screen's edge toward the shot.
  const { x, y } = damageEdgePoint(angle);
  indicator.style.setProperty("--edge-x", x);
  indicator.style.setProperty("--edge-y", y);
  indicator.classList.remove("pulse");
  // Commit the removal so a second hit restarts the pulse.
  void indicator.offsetWidth;
  indicator.classList.add("pulse");
  indicator.hidden = false;
}

/** Writes `text` into the element `#id` under `root`, unless it already shows it. */
export function setText(root: ParentNode, id: string, text: string): void {
  const node = root.querySelector(`#${id}`);
  if (node && node.textContent !== text) {
    node.textContent = text;
  }
}

/** How the hull reads at a glance: healthy, damaged or critical, at the overhead bar's
 * thresholds (`health_bar_state` in the renderer), so both bars change colour together. */
export type HullTone = "healthy" | "damaged" | "critical";

export function hullTone(ratio: number): HullTone {
  return ratio <= 0.3 ? "critical" : ratio <= 0.6 ? "damaged" : "healthy";
}

/** One timed power-up or protection, as the tank panel's chips show it. */
export interface ActiveEffect {
  kind: "spawn" | "shield" | "rapid" | "boost" | "laser" | "repair";
  name: string;
  /** What is left: seconds, shield points, or nothing for a lasting effect. */
  detail: string;
}

export function activeEffects(
  tank: Pick<
    HumanState,
    "protection" | "shield" | "shieldPoints" | "rapid" | "speed" | "laser" | "selfRepair"
  >,
): ActiveEffect[] {
  const seconds = (time: number) => `${Math.ceil(time)}s`;
  const effects: (ActiveEffect | null)[] = [
    tank.protection > 0 ? { kind: "spawn", name: "SPAWN SHIELD", detail: "" } : null,
    tank.shield > 0
      ? {
          kind: "shield",
          name: "SHIELD",
          detail: `${Math.ceil(tank.shieldPoints)} HP · ${seconds(tank.shield)}`,
        }
      : null,
    tank.rapid > 0 ? { kind: "rapid", name: "RAPID", detail: seconds(tank.rapid) } : null,
    tank.speed > 0 ? { kind: "boost", name: "BOOST", detail: seconds(tank.speed) } : null,
    tank.laser > 0 ? { kind: "laser", name: "LASER DEFENSE", detail: seconds(tank.laser) } : null,
    tank.selfRepair ? { kind: "repair", name: "SELF-REPAIR", detail: "" } : null,
  ];
  return effects.filter((effect): effect is ActiveEffect => effect !== null);
}

/** Each effect's mark in the text line. The chips draw their own colour dot instead (small
 * font sets lack these symbols), so the marks stay in the text for readers only. */
const EFFECT_MARKS: Record<ActiveEffect["kind"], string> = {
  spawn: "",
  shield: "◇ ",
  rapid: "» ",
  boost: "ϟ ",
  laser: "✧ ",
  repair: "",
};

/** The effect chips in the tank panel's footer, rebuilt only when their text changes. Their
 * text, separators included, reads as `effectsLabel`. */
function showEffects(root: ParentNode, tank: HumanState): void {
  const list = root.querySelector<HTMLElement>("#effects");
  if (!list) {
    return;
  }
  const effects = activeEffects(tank);
  const key = effects.map(({ kind, detail }) => `${kind}:${detail}`).join("|");
  if (list.dataset.key === key) {
    return;
  }
  list.dataset.key = key;
  list.replaceChildren(
    ...effects.flatMap(({ kind, name, detail }, index) => {
      const chip = document.createElement("span");
      chip.className = "effect";
      chip.dataset.kind = kind;
      if (EFFECT_MARKS[kind]) {
        const mark = document.createElement("i");
        mark.textContent = EFFECT_MARKS[kind];
        chip.append(mark);
      }
      const label = document.createElement("b");
      label.textContent = name;
      chip.append(label);
      if (detail) {
        chip.append(` ${detail}`);
      }
      return index ? ["  ", chip] : [chip];
    }),
  );
}

/** Both HUDs' tank status (`hudMarkup` under `root`): hull, tank and rank, the health bar,
 * the ammo slots, which `slotsDisabled` makes unusable, the mine, power-ups and the low
 * hull warning. */
export function showTankStatus(root: ParentNode, tank: HumanState, slotsDisabled: boolean): void {
  setText(root, "hp", String(Math.max(0, Math.ceil(tank.hp))));
  setText(root, "vehicle-name", tank.vehicleName);
  // The phone's hull pill names the tank too.
  setText(root, "hull-pill-name", tank.vehicleName);
  setText(root, "rank", tank.rankName.toUpperCase());
  const rank = root.querySelector<HTMLElement>("#rank")!;
  rank.dataset.rank = String(tank.rank);
  rank.title = rankTitle(tank);
  setText(root, "hp-max", `/ ${Math.round(tank.maxHp)}`);
  // The bar's colour follows the hull's state, not the team: a full hull never reads as
  // danger. The phone's hull pill shows the same reading.
  const width = `${tank.healthRatio * 100}%`;
  const tone = hullTone(tank.healthRatio);
  for (const bar of root.querySelectorAll<HTMLElement>(".hull-bar")) {
    if (bar.dataset.tone !== tone) {
      bar.dataset.tone = tone;
    }
    const fill = bar.querySelector<HTMLElement>(".hull-fill");
    if (fill && fill.style.width !== width) {
      fill.style.width = width;
    }
    // The ghost behind the bar trails it down (a CSS transition), showing the chunk a hit
    // took; it sits under the bar, so a repair hides it until it catches up.
    const ghost = bar.querySelector<HTMLElement>(".hull-ghost");
    if (ghost && ghost.style.width !== width) {
      ghost.style.width = width;
    }
  }
  setText(root, "hull-pill-hp", String(Math.max(0, Math.ceil(tank.hp))));
  root.querySelector<HTMLElement>(".hull-readout")?.setAttribute("data-tone", tone);
  AMMO_OPTIONS.forEach(({ weapon, label: name }, index) => {
    const slotState = tank.ammo.find((slot) => slot.weapon === weapon);
    const selected = !!slotState?.selected;
    const count = slotState?.count === null ? "∞" : String(slotState?.count ?? 0);
    setText(root, `ammo-count-${weapon}`, count);
    const slot = root.querySelector<HTMLButtonElement>(`#ammo-${weapon}`)!;
    slot.disabled = slotsDisabled;
    slot.setAttribute("aria-pressed", String(selected));
    // The loaded round's slot widens to name it while the others narrow, so the rack, and
    // the panel with it, keeps one width whichever round is loaded.
    slot.classList.toggle("selected", selected);
    slot.classList.toggle("empty", !slotState?.available);
    const label = `${index + 1}: ${name}, ${count === "0" ? "empty, collect an ammo crate" : count === "∞" ? "unlimited" : count + " remaining"}${selected ? ", selected" : ""}`;
    if (slot.getAttribute("aria-label") !== label) {
      slot.setAttribute("aria-label", label);
    }
  });
  showMine(root, tank.mineCooldown);
  showEffects(root, tank);
  root
    .querySelector(".status")!
    .classList.toggle("critical-health", tank.alive && tank.healthRatio < 0.25);
}

/** The longest mine cooldown seen, so the mine dial drains from full whatever the rules. */
let mineCooldownTotal = 0;

/** The mine socket: armed, or a dial that refills while the seconds count down. */
function showMine(root: ParentNode, cooldown: number): void {
  const cooling = cooldown > 0;
  mineCooldownTotal = cooling ? Math.max(mineCooldownTotal, cooldown) : mineCooldownTotal;
  setText(root, "mine", cooling ? `Mine re-arming, ${Math.ceil(cooldown)} seconds` : "Mine ready");
  setText(root, "mine-time", cooling ? String(Math.ceil(cooldown)) : "");
  const slot = root.querySelector<HTMLElement>(".mine-slot");
  if (!slot) {
    return;
  }
  slot.classList.toggle("cooling", cooling);
  const armed = cooling && mineCooldownTotal > 0 ? 1 - cooldown / mineCooldownTotal : 1;
  const value = armed.toFixed(2);
  const dial = slot.querySelector<HTMLElement>(".mine-dial");
  if (dial && dial.style.getPropertyValue("--armed") !== value) {
    dial.style.setProperty("--armed", value);
  }
}

/** The active effects as one line of text, for screen readers and logs. */
export function effectsLabel(
  tank: Pick<
    HumanState,
    "protection" | "shield" | "shieldPoints" | "rapid" | "speed" | "laser" | "selfRepair"
  >,
): string {
  return activeEffects(tank)
    .map(({ kind, name, detail }) => `${EFFECT_MARKS[kind]}${name}${detail ? " " + detail : ""}`)
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

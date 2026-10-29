import { AMMO_OPTIONS } from "./ammo-options";
import type { ActiveEffect, EngineFeedRow, FeedSide, HumanState } from "./engine-api";

// How much HUD feedback shows and for how many seconds: the desktop app times it the
// same (`crates/web/src/feedback.rs`, pinned by `tests/page-catalog.test.ts`).
/** The kill feed shows this many of its newest rows, each this long. */
export const FEED_ROWS = 3;
export const FEED_TIME = 5;
/** The red screen-edge flash fades over this long after the player's hull is hit. */
export const HURT_FLASH_TIME = 0.6;
/** The damage ring round the player's tank shows this long. */
export const DAMAGE_RING_TIME = 1.2;
/** An ammunition notice shows above the tank panel this long. */
export const AMMO_NOTICE_TIME = 3;
/** A pickup's toast shows this long, and a promotion's or the player's death's. */
export const PICKUP_TOAST_TIME = 1.5;
export const TOAST_TIME = 2;
/** Below this share of its hull the tank's HUD warns, like the status panel's pulse. */
export const CRITICAL_HEALTH = 0.25;

/** One kill-feed row: the engine's names, sides, weapon icon and highlights
 * (`EngineFeedRow`), or a notice's one name, and how long it stays (seconds). `seq`
 * tells two otherwise identical rows apart, such as the same kill twice in one window. */
export interface FeedRow {
  names: string[];
  ownKill: boolean;
  time: number;
  seq: number;
  cause?: EngineFeedRow["cause"];
  icon?: string;
  sides?: FeedSide[];
  ownDeath?: boolean;
}

let lastFeedSeq = 0;

export function newFeedRow(
  names: string[],
  ownKill = false,
  details: Pick<FeedRow, "cause" | "icon" | "sides" | "ownDeath"> = {},
): FeedRow {
  return { names, ownKill, time: FEED_TIME, seq: ++lastFeedSeq, ...details };
}

/** Shows a feed row: one name for a notice, or killer and victim with the weapon's icon
 * between (the wreck mark if a row names none). Icons are SVG shapes because small font
 * sets, such as Tesla's browser, lack symbol glyphs and draw a missing-glyph box
 * instead. Every row has one plate; the viewer's own kills and wreck carry its highlight. `entering` marks the newest row,
 * which plays the pop-in; its animation starts when the row's element is new to the
 * page, which `showFeed` makes sure of. */
export function showFeedRow(
  row: HTMLElement,
  feedRow: Pick<FeedRow, "names" | "ownKill" | "seq" | "cause" | "icon" | "sides" | "ownDeath">,
  entering = false,
): void {
  const { names, ownKill, cause, icon = "skull", sides = [], ownDeath = false } = feedRow;
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
    use.setAttribute("href", `#hud-${icon}`);
    mark.append(use);
    mark.setAttribute("role", "img");
    mark.setAttribute("aria-label", "destroyed");
    row.append(mark, name(victim, index + 1));
  });
}

/** Ages the kill feed by `dt` seconds and shows its newest live rows in `feed`. Returns
 * the rows still showing. Each row keeps its element while it shows: rows only enter at
 * the top, so older rows stay as they are while a new one pushes them down, and a new
 * row enters as a fresh element whose pop-in starts by itself. Restarting the animation
 * on a reused element instead needed a forced style and layout pass mid-frame (10 ms of
 * a death frame on a slow device); moving an element would restart it too. */
export function showFeed(feed: Element, rows: FeedRow[], dt: number): FeedRow[] {
  const live = rows.filter((row) => (row.time -= dt) > 0);
  live.length = Math.min(FEED_ROWS, live.length);
  const showing = new Set(live.map((row) => String(row.seq)));
  for (const node of Array.from(feed.children) as HTMLElement[]) {
    if (!showing.has(node.dataset.seq ?? "")) {
      node.remove();
    }
  }
  // The elements left show live rows in order; new rows go in above them.
  let next = feed.firstElementChild as HTMLElement | null;
  live.forEach((row, index) => {
    const seq = String(row.seq);
    if (next?.dataset.seq === seq) {
      next = next.nextElementSibling as HTMLElement | null;
      return;
    }
    const node = document.createElement("div");
    node.dataset.seq = seq;
    showFeedRow(node, row, index === 0);
    feed.insertBefore(node, next);
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
  // A fresh glow element restarts the pulse at once; re-adding the class to the old one
  // would need a forced style and layout pass to see it go first.
  indicator.replaceChildren(document.createElement("i"));
  indicator.classList.add("pulse");
  indicator.hidden = false;
}

/** Set an attribute unless it already holds `value`. */
function setAttributeIfChanged(element: Element, name: string, value: string): void {
  if (element.getAttribute(name) !== value) {
    element.setAttribute(name, value);
  }
}

/** Writes `text` into the element `#id` under `root`, unless it already shows it. */
export function setText(root: ParentNode, id: string, text: string): void {
  const node = root.querySelector(`#${id}`);
  if (node && node.textContent !== text) {
    node.textContent = text;
  }
}

/** The CSS colour of an sRGB hex number, such as the engine's hull ramp. */
export function cssColor(rgb: number): string {
  return `#${rgb.toString(16).padStart(6, "0")}`;
}

/** Each effect's icon in its timer ring (`HUD_ICONS`) and mark in its chip's text. The
 * chips draw their own colour dot instead of the mark (small font sets lack these
 * symbols), so the marks stay in the text for readers only. */
const EFFECT_ART: Record<ActiveEffect["kind"], { icon: string; mark: string }> = {
  spawn: { icon: "hud-shield", mark: "" },
  shield: { icon: "hud-shield", mark: "◇ " },
  rapid: { icon: "hud-rapid", mark: "» " },
  boost: { icon: "hud-boost", mark: "ϟ " },
  laser: { icon: "hud-target", mark: "✧ " },
  repair: { icon: "hud-repair", mark: "" },
};

/** The effect chips docked on the tank panel, rebuilt only when their text changes: each
 * a timer ring round its icon (stepping once a second with the countdown), its name and
 * what is left. */
function showEffects(root: ParentNode, effects: readonly ActiveEffect[]): void {
  const list = root.querySelector<HTMLElement>("#effects");
  if (!list) {
    return;
  }
  const key = effects.map(({ kind, detail }) => `${kind}:${detail}`).join("|");
  if (list.dataset.key === key) {
    return;
  }
  list.dataset.key = key;
  list.replaceChildren(
    ...effects.flatMap(({ kind, name, detail, share }, index) => {
      const chip = document.createElement("span");
      chip.className = "effect";
      chip.dataset.kind = kind;
      chip.style.setProperty("--left", share.toFixed(3));
      const ring = document.createElementNS("http://www.w3.org/2000/svg", "svg");
      ring.setAttribute("class", "effect-ring");
      ring.setAttribute("aria-hidden", "true");
      const use = document.createElementNS("http://www.w3.org/2000/svg", "use");
      use.setAttribute("href", `#${EFFECT_ART[kind].icon}`);
      ring.append(use);
      chip.append(ring);
      if (EFFECT_ART[kind].mark) {
        const mark = document.createElement("i");
        mark.textContent = EFFECT_ART[kind].mark;
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
  setText(root, "rank", tank.rankName.toUpperCase());
  const rank = root.querySelector<HTMLElement>("#rank")!;
  // The panel updates every few frames: attributes are written only when they change,
  // since every write, even of the same value, invalidates the panel's style.
  setAttributeIfChanged(rank, "data-rank", String(tank.rank));
  setAttributeIfChanged(rank, "title", tank.rankTitle);
  setText(root, "hp-max", `/ ${Math.round(tank.maxHp)}`);
  // The bar's colour follows the hull's state, not the team: a full hull never reads as
  // danger.
  const width = `${tank.healthRatio * 100}%`;
  const tone = tank.hullTone;
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
  // Every hull readout under `root` takes the ramp's colour from one property.
  const ramp = cssColor(tank.hullRamp);
  if (root instanceof HTMLElement && root.style.getPropertyValue("--hull-ramp") !== ramp) {
    root.style.setProperty("--hull-ramp", ramp);
  }
  const readout = root.querySelector<HTMLElement>(".hull-readout");
  if (readout) {
    setAttributeIfChanged(readout, "data-tone", tone);
  }
  AMMO_OPTIONS.forEach(({ weapon, label: name }, index) => {
    const slotState = tank.ammo.find((slot) => slot.weapon === weapon);
    const selected = !!slotState?.selected;
    const count = slotState?.count === null ? "∞" : String(slotState?.count ?? 0);
    setText(root, `ammo-count-${weapon}`, count);
    const slot = root.querySelector<HTMLButtonElement>(`#ammo-${weapon}`)!;
    if (slot.disabled !== slotsDisabled) {
      slot.disabled = slotsDisabled;
    }
    setAttributeIfChanged(slot, "aria-pressed", String(selected));
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
  showEffects(root, tank.effects);
  root
    .querySelector(".status")!
    .classList.toggle("critical-health", tank.alive && tank.healthRatio < CRITICAL_HEALTH);
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

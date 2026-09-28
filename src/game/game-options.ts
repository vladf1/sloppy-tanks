import { DIFFICULTIES, parseDifficulty } from "./difficulty";
import { isExtraLevel, mapOption, showsExtraLevels, type MapId } from "./map-options";
import { bindMapChoice, setMapChoice } from "./map-picker";
import { Random } from "./math";
import type { Simulation } from "./simulation";
import type { VehicleKind } from "./types";

export type GameOptions = Pick<
  Simulation,
  "humanKind" | "humanTeam" | "gameMode" | "mapMode" | "difficulty"
>;

/** A link's `?map=` wins; otherwise the player's last map is the one prepared
 * behind the menu, so a returning player's GO needs no rebuild. Extra levels are
 * offered only with `?extralevels`. */
export function initialGameOptions(
  seed: number,
  search: string,
  difficulty: string | null,
  lastMap: string | null = null,
): GameOptions {
  const requestedMap = new URLSearchParams(search).get("map");
  const extraLevels = showsExtraLevels(search);
  const map = (id: string | null) => {
    const option = mapOption(id);
    return option && (extraLevels || !isExtraLevel(option.id)) ? option.id : undefined;
  };
  return {
    humanKind: "balanced",
    humanTeam: new Random(seed).next() < 0.5 ? 0 : 1,
    gameMode: "team",
    mapMode: map(requestedMap) ?? map(lastMap) ?? "village",
    difficulty: parseDifficulty(difficulty),
  };
}

/** Only the menu choices, for copying from a simulation that carries them. */
export function gameChoices(source: GameOptions): GameOptions {
  const { humanKind, humanTeam, gameMode, mapMode, difficulty } = source;
  return { humanKind, humanTeam, gameMode, mapMode, difficulty };
}

export function sameGameOptions(a: GameOptions, b: GameOptions): boolean {
  return (
    a.humanKind === b.humanKind &&
    a.humanTeam === b.humanTeam &&
    a.gameMode === b.gameMode &&
    a.mapMode === b.mapMode &&
    a.difficulty === b.difficulty
  );
}

/** Apply per-visit choices to the build-time menu without replacing its DOM. */
export function syncGameOptions(overlay: HTMLElement, options: GameOptions): void {
  showTank(overlay, options.humanKind);
  for (const key of ["gameMode", "difficulty"] as const) {
    overlay.querySelectorAll<HTMLInputElement>(`input[name="${key}"]`).forEach((input) => {
      input.checked = input.value === options[key];
    });
  }
  setMapChoice(overlay, "mapMode", options.mapMode);
  showBattleFormat(overlay, options.mapMode);
  // Each difficulty explains itself in a tooltip, so choosing one never reflows the menu.
  overlay.querySelectorAll<HTMLInputElement>('input[name="difficulty"]').forEach((input) => {
    const description = DIFFICULTIES[parseDifficulty(input.value)].description;
    input.closest<HTMLElement>(".segment")?.setAttribute("data-tip", description);
    input.setAttribute("aria-description", description);
  });
  showTankTeam(overlay, options.humanTeam);
}

/** An extra level plays its own team battle: Team Battle describes its roster, and Solo
 * Assault waits until a standard map is chosen again. */
function showBattleFormat(overlay: HTMLElement, mapMode: MapId): void {
  const level = mapOption(mapMode);
  const teamTanks = level && "teamTanks" in level ? level.teamTanks : undefined;
  overlay.querySelectorAll<HTMLInputElement>('input[name="gameMode"]').forEach((input) => {
    const card = input.closest<HTMLElement>(".choice-card");
    const note = card?.querySelector<HTMLElement>("small");
    if (input.value === "team" && note) {
      note.dataset.standard ??= note.textContent ?? "";
      note.textContent = teamTanks
        ? `${teamTanks} vs ${teamTanks} · Endless respawns and scoring`
        : note.dataset.standard;
    } else if (input.value === "solo") {
      input.disabled = teamTanks !== undefined;
      if (teamTanks !== undefined) {
        card?.setAttribute("data-tip", `Not available on ${level!.name}`);
      } else {
        card?.removeAttribute("data-tip");
      }
    }
  });
}

/** Mark one tank card as the player's choice. */
export function showTank(overlay: HTMLElement, kind: string): void {
  overlay.querySelectorAll<HTMLButtonElement>("[data-kind]").forEach((card) => {
    const selected = card.dataset.kind === kind;
    card.classList.toggle("selected", selected);
    card.setAttribute("aria-pressed", String(selected));
  });
}

/** Tank previews are one sprite sheet with a row per team colour. */
export function showTankTeam(overlay: HTMLElement, team: GameOptions["humanTeam"]): void {
  overlay.querySelectorAll(".tank-preview image").forEach((image) => {
    image.setAttribute("y", String(-team * 400));
  });
}

/** The team colour the tank previews currently show. */
export function shownTankTeam(overlay: HTMLElement): GameOptions["humanTeam"] {
  return overlay.querySelector(".tank-preview image")?.getAttribute("y") === "-400" ? 1 : 0;
}

/** Both the lightweight startup menu and later rounds edit the same choices. */
export function bindGameOptions(overlay: HTMLElement, options: GameOptions): void {
  overlay.querySelectorAll<HTMLButtonElement>("[data-kind]").forEach((button) => {
    button.addEventListener("click", () => {
      options.humanKind = button.dataset.kind as VehicleKind;
      showTank(overlay, options.humanKind);
    });
  });
  for (const key of ["gameMode", "difficulty"] as const) {
    overlay.querySelectorAll<HTMLInputElement>(`input[name="${key}"]`).forEach((input) => {
      input.addEventListener("change", () => {
        if (key === "difficulty") {
          options.difficulty = parseDifficulty(input.value);
          localStorage.setItem("sloppy-difficulty", options.difficulty);
        } else {
          options.gameMode = input.value as GameOptions["gameMode"];
        }
      });
    });
  }
  // Runs before the menu's own change listeners, which the event reaches as it bubbles.
  bindMapChoice(overlay, "mapMode", (value) => {
    const mapMode = mapOption(value)?.id;
    if (!mapMode) {
      return;
    }
    options.mapMode = mapMode;
    localStorage.setItem("sloppy-map", mapMode);
    if (isExtraLevel(mapMode)) {
      options.gameMode = "team";
      overlay.querySelectorAll<HTMLInputElement>('input[name="gameMode"]').forEach((input) => {
        input.checked = input.value === "team";
      });
    }
    showBattleFormat(overlay, mapMode);
  });
}

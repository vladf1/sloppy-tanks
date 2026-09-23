import { DIFFICULTIES, parseDifficulty } from "./difficulty";
import { MAP_OPTIONS } from "./map-options";
import { Random } from "./math";
import type { Simulation } from "./simulation";
import type { VehicleKind } from "./types";

export type GameOptions = Pick<
  Simulation,
  "humanKind" | "humanTeam" | "gameMode" | "mapMode" | "difficulty"
>;

export function initialGameOptions(
  seed: number,
  search: string,
  difficulty: string | null,
): GameOptions {
  const requestedMap = new URLSearchParams(search).get("map");
  return {
    humanKind: "balanced",
    humanTeam: new Random(seed).next() < 0.5 ? 0 : 1,
    gameMode: "team",
    mapMode: MAP_OPTIONS.find((map) => map.id === requestedMap)?.id ?? "village",
    difficulty: parseDifficulty(difficulty),
  };
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
  overlay.querySelectorAll<HTMLButtonElement>("[data-kind]").forEach((button) => {
    const selected = button.dataset.kind === options.humanKind;
    button.classList.toggle("selected", selected);
    button.setAttribute("aria-pressed", String(selected));
  });
  for (const key of ["gameMode", "mapMode", "difficulty"] as const) {
    overlay.querySelectorAll<HTMLInputElement>(`input[name="${key}"]`).forEach((input) => {
      input.checked = input.value === options[key];
    });
  }
  const help = overlay.querySelector("#difficulty-help");
  if (help) {
    help.textContent = DIFFICULTIES[options.difficulty].description;
  }
  overlay.querySelectorAll(".tank-preview image").forEach((image) => {
    image.setAttribute("y", String(-options.humanTeam * 400));
  });
}

/** Both the lightweight startup menu and later rounds edit the same choices. */
export function bindGameOptions(overlay: HTMLElement, options: GameOptions): void {
  overlay.querySelectorAll<HTMLButtonElement>("[data-kind]").forEach((button) => {
    button.addEventListener("click", () => {
      options.humanKind = button.dataset.kind as VehicleKind;
      overlay.querySelectorAll<HTMLButtonElement>("[data-kind]").forEach((card) => {
        const selected = card === button;
        card.classList.toggle("selected", selected);
        card.setAttribute("aria-pressed", String(selected));
      });
    });
  });
  for (const key of ["gameMode", "mapMode", "difficulty"] as const) {
    overlay.querySelectorAll<HTMLInputElement>(`input[name="${key}"]`).forEach((input) => {
      input.addEventListener("change", () => {
        if (key === "difficulty") {
          options.difficulty = parseDifficulty(input.value);
          localStorage.setItem("sloppy-difficulty", options.difficulty);
          overlay.querySelector("#difficulty-help")!.textContent =
            DIFFICULTIES[options.difficulty].description;
        } else if (key === "gameMode") {
          options.gameMode = input.value as GameOptions["gameMode"];
        } else {
          options.mapMode = input.value as GameOptions["mapMode"];
        }
      });
    });
  }
}

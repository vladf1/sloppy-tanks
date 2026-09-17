import { DIFFICULTIES, parseDifficulty } from "./difficulty";
import { MAP_OPTIONS } from "./map-options";
import { Random } from "./math";
import type { Simulation } from "./simulation";
import type { VehicleKind } from "./types";

export type GameOptions = Pick<
  Simulation,
  "humanKind" | "humanTeam" | "gameMode" | "mapMode" | "difficulty"
>;
export type MenuOptions = GameOptions & Pick<Simulation, "customMap">;

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
    mapMode:
      MAP_OPTIONS.find((map) => map.id === requestedMap)?.id ??
      (requestedMap === "random" || requestedMap === "surprise" ? "surprise" : "village"),
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

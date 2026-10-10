import type { GameMode, PlayerVehicleKind } from "./engine-api";
import { isPhone } from "./phone-mode";

type Preference = "tank" | "game-mode" | "map" | "difficulty" | "camera" | "zoom" | Setting;
/** The Settings dialog's choices, and the name a player joins rooms with. */
type Setting = "volume" | "tank-speed" | "bullet-speed" | "touch" | "player-name";

/** Storage can be disabled or full; the choices still work for this visit. */
export function savedPreference(key: Preference): string | null {
  try {
    return localStorage.getItem("sloppy-" + key);
  } catch {
    return null;
  }
}

export function savePreference(key: Preference, value: string): void {
  try {
    localStorage.setItem("sloppy-" + key, value);
  } catch {
    /* Session-only preference. */
  }
}

export function preferredTank(value: string | null): PlayerVehicleKind {
  return value === "scout" || value === "heavy" ? value : "balanced";
}

export function preferredGameMode(value: string | null): GameMode {
  return value === "solo" ? "solo" : "team";
}

/** Camera bounds belong to the renderer; only discard malformed storage here. */
export function savedCameraPreferences(): { firstPerson: boolean; zoom?: number } {
  const storedZoom = savedPreference("zoom");
  const zoom = storedZoom?.trim() ? Number(storedZoom) : NaN;
  return {
    firstPerson: savedPreference("camera") === "first-person",
    ...(Number.isFinite(zoom) ? { zoom } : {}),
  };
}

/** Phones have a small screen, so until the player zooms their overhead camera starts
 * farther out than the renderer's default (34) to show more of the arena. */
const PHONE_ZOOM = 40;

/** The camera an engine starts with: the saved one, on a phone farther out until it
 * zooms and without the reticle, since phones aim by touching the arena. */
export function startingCamera(): { firstPerson: boolean; zoom?: number; hideReticle?: true } {
  const saved = savedCameraPreferences();
  return isPhone() ? { ...saved, zoom: saved.zoom ?? PHONE_ZOOM, hideReticle: true } : saved;
}

/** Read the chosen view and clamped zoom after a user action. The current drawn
 * camera may temporarily differ while a menu, transition or wreck is visible. */
export function saveCameraPreferences(game: { camera_preferences(): Float64Array }): void {
  const [firstPerson, zoom] = game.camera_preferences();
  savePreference("camera", firstPerson === 1 ? "first-person" : "overhead");
  savePreference("zoom", String(zoom));
}

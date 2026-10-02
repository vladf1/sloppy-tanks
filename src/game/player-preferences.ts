import type { GameMode, PlayerVehicleKind } from "./engine-api";

type Preference = "tank" | "game-mode" | "map" | "difficulty" | "camera" | "zoom";

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

/** Read the chosen view and clamped zoom after a user action. The current drawn
 * camera may temporarily differ while a menu, transition or wreck is visible. */
export function saveCameraPreferences(game: { camera_preferences(): Float64Array }): void {
  const [firstPerson, zoom] = game.camera_preferences();
  savePreference("camera", firstPerson === 1 ? "first-person" : "overhead");
  savePreference("zoom", String(zoom));
}

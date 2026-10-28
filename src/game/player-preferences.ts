import type { GameMode, PlayerVehicleKind } from "./engine-api";
import { isPhone } from "./phone-mode";

type Preference = "tank" | "game-mode" | "map" | "difficulty" | "camera" | "zoom" | Setting;
/** The Settings dialog's choices, and the name a player joins rooms with. */
type Setting =
  "volume" | "tank-speed" | "bullet-speed" | "touch" | "graphics" | "player-name" | "debug";

/** The Settings dialog's Graphics choices: Auto adapts the engine's quality tier and
 * render scale to the frame rate; the others fix the tier at full resolution. */
export const GRAPHICS_SETTINGS = [
  ["auto", "Auto"],
  ["high", "High"],
  ["medium", "Medium"],
  ["low", "Low"],
] as const;

/** The saved Graphics setting, Auto unless the player chose another. */
export function savedGraphics(): string {
  const saved = savedPreference("graphics");
  return GRAPHICS_SETTINGS.some(([value]) => value === saved) ? saved! : "auto";
}

/** The kind of device, from which the engine picks Auto's starting tier: the phone
 * edition, a touch tablet (a coarse primary pointer, or iPadOS Safari, which presents
 * itself as a Mac with touch points) or a desktop. */
function deviceClass(): "desktop" | "tablet" | "phone" {
  if (isPhone()) {
    return "phone";
  }
  const iPad = /Macintosh/.test(navigator.userAgent) && navigator.maxTouchPoints > 1;
  return iPad || matchMedia("(pointer: coarse)").matches ? "tablet" : "desktop";
}

/** The graphics an engine starts with: the saved setting and the device class. */
export function startingGraphics(): { graphics: string; device: string } {
  return { graphics: savedGraphics(), device: deviceClass() };
}

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
 * zooms. Touch screens (phones, tablets, the Tesla) draw no overhead reticle: they aim
 * where the finger is, so it sat hidden under the finger and then stuck where the
 * finger lifted. First person keeps its gun sight everywhere. */
export function startingCamera(): { firstPerson: boolean; zoom?: number; hideReticle?: true } {
  const saved = savedCameraPreferences();
  const camera = isPhone() ? { ...saved, zoom: saved.zoom ?? PHONE_ZOOM } : saved;
  return deviceClass() === "desktop" ? camera : { ...camera, hideReticle: true };
}

/** Read the chosen view and clamped zoom after a user action. The current drawn
 * camera may temporarily differ while a menu, transition or wreck is visible. */
export function saveCameraPreferences(game: { camera_preferences(): Float64Array }): void {
  const [firstPerson, zoom] = game.camera_preferences();
  savePreference("camera", firstPerson === 1 ? "first-person" : "overhead");
  savePreference("zoom", String(zoom));
}

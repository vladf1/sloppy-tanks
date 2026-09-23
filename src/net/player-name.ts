import { BOT_NAMES } from "../game/bot-personalities";

export function preferredPlayerName(): string {
  try {
    const saved = localStorage.getItem("sloppy-player-name")?.trim().slice(0, 24);
    if (saved) {
      return saved;
    }
  } catch {
    /* Optional preference. */
  }
  return BOT_NAMES[crypto.getRandomValues(new Uint32Array(1))[0] % BOT_NAMES.length];
}
export function rememberPlayerName(name: string): void {
  try {
    localStorage.setItem("sloppy-player-name", name);
  } catch {
    /* Optional preference. */
  }
}

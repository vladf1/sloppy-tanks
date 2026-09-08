import { WEAPONS } from "./data";
import type { AmmoInventory, AmmoSelection, SpecialAmmo, Tank, Weapon } from "./types";

export const AMMO_ORDER: readonly Weapon[] = ["standard", "spread", "rocket", "ricochet", "piercing"];
export const AMMO_RESPAWN_SECONDS = 13;
export const AMMO_SCROLL_INTERVAL_MS = 120;
export const emptyAmmo = (): AmmoInventory => ({ spread: 0, rocket: 0, ricochet: 0, piercing: 0 });
export function isSpecialAmmo(kind: string): kind is SpecialAmmo {
  return kind !== "standard" && AMMO_ORDER.includes(kind as Weapon);
}
export function hasAmmo(t: Tank, weapon: Weapon) {
  return weapon === "standard" || t.ammo[weapon] > 0;
}
export function canCollectAmmo(t: Tank, kind: SpecialAmmo) {
  return t.ammo[kind] < WEAPONS[kind].carryLimit;
}
export function equippedWeapon(t: Tank): Weapon {
  return hasAmmo(t, t.selectedAmmo) ? t.selectedAmmo : "standard";
}
export function selectAmmo(t: Tank, selection?: AmmoSelection) {
  if (!t.alive) return;
  t.selectedAmmo = equippedWeapon(t);
  if (typeof selection === "string") {
    if (hasAmmo(t, selection)) t.selectedAmmo = selection;
  } else if (selection) {
    const start = AMMO_ORDER.indexOf(t.selectedAmmo);
    for (let offset = 1; offset <= AMMO_ORDER.length; offset++) {
      const weapon = AMMO_ORDER[(start + selection * offset + AMMO_ORDER.length) % AMMO_ORDER.length];
      if (hasAmmo(t, weapon)) { t.selectedAmmo = weapon; break; }
    }
  }
}
export function consumeAmmo(t: Tank, weapon: Weapon) {
  if (weapon === "standard") return;
  t.ammo[weapon]--;
  if (!hasAmmo(t, weapon)) t.selectedAmmo = "standard";
}
export function refillAmmo(t: Tank, kind: SpecialAmmo) {
  const received = Math.min(WEAPONS[kind].perCrate, WEAPONS[kind].carryLimit - t.ammo[kind]);
  t.ammo[kind] += received;
  return received;
}
export function clearAmmo(t: Tank) {
  t.ammo = emptyAmmo();
  t.selectedAmmo = "standard";
  t.command.ammoSelection = undefined;
}

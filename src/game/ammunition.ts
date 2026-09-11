import { WEAPONS } from "./data";
import type { AmmoInventory, AmmoSelection, SpecialAmmo, Tank, Weapon } from "./types";

export const AMMO_ORDER: readonly Weapon[] = [
  "standard",
  "spread",
  "rocket",
  "ricochet",
  "piercing",
];
export const AMMO_HELP: Record<Weapon, string> = {
  standard: "Unlimited shells · one bounce",
  spread: "Three shells per volley · best up close",
  rocket: "Accelerates in flight · explosive blast · can hurt you",
  ricochet: "High damage · bounces up to three times",
  piercing: "Passes through one enemy shell · stops at tanks and cover",
};
export const AMMO_RESPAWN_SECONDS = 13;
export const AMMO_SCROLL_INTERVAL_MS = 120;
export const emptyAmmo = (): AmmoInventory => ({ spread: 0, rocket: 0, ricochet: 0, piercing: 0 });
export function isSpecialAmmo(kind: string): kind is SpecialAmmo {
  return kind !== "standard" && AMMO_ORDER.includes(kind as Weapon);
}
export function hasAmmo(tank: Tank, weapon: Weapon): boolean {
  return weapon === "standard" || tank.ammo[weapon] > 0;
}
export function hasAdvancedAmmo(tank: Tank): boolean {
  return Object.values(tank.ammo).some((count) => count > 0);
}
export function canCollectAmmo(tank: Tank, kind: SpecialAmmo): boolean {
  return tank.ammo[kind] < WEAPONS[kind].carryLimit;
}
export function equippedWeapon(tank: Tank): Weapon {
  return hasAmmo(tank, tank.selectedAmmo) ? tank.selectedAmmo : "standard";
}
export function selectAmmo(tank: Tank, selection?: AmmoSelection): void {
  if (!tank.alive) {
    return;
  }
  tank.selectedAmmo = equippedWeapon(tank);
  if (typeof selection === "string") {
    if (hasAmmo(tank, selection)) {
      tank.selectedAmmo = selection;
    }
  } else if (selection) {
    const start = AMMO_ORDER.indexOf(tank.selectedAmmo);
    for (let offset = 1; offset <= AMMO_ORDER.length; offset++) {
      const weapon =
        AMMO_ORDER[(start + selection * offset + AMMO_ORDER.length) % AMMO_ORDER.length];
      if (hasAmmo(tank, weapon)) {
        tank.selectedAmmo = weapon;
        break;
      }
    }
  }
}
export function consumeAmmo(tank: Tank, weapon: Weapon): void {
  if (weapon === "standard") {
    return;
  }
  tank.ammo[weapon]--;
  if (!hasAmmo(tank, weapon)) {
    tank.selectedAmmo = "standard";
  }
}
export function refillAmmo(tank: Tank, kind: SpecialAmmo): number {
  const received = Math.min(WEAPONS[kind].perCrate, WEAPONS[kind].carryLimit - tank.ammo[kind]);
  tank.ammo[kind] += received;
  return received;
}
export function clearAmmo(tank: Tank): void {
  tank.ammo = emptyAmmo();
  tank.selectedAmmo = "standard";
  tank.command.ammoSelection = undefined;
}

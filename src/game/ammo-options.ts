/** The ammunition the HUD offers, in selection order (keys 1–5). Like `map-options.ts`
 * this is interface data: the engine owns the ammunition rules and reports stock and
 * selection in its HUD state. */
export const AMMO_OPTIONS = [
  {
    weapon: "standard",
    label: "STANDARD",
    name: "Standard shells",
    color: 0xffdf00,
    help: "Unlimited shells · stops at walls",
  },
  {
    weapon: "spread",
    label: "SPREAD",
    name: "Spread shot",
    color: 0xff38d4,
    help: "Three shells per volley · best up close",
  },
  {
    weapon: "rocket",
    label: "ROCKET",
    name: "Breaching rockets",
    color: 0xff591c,
    help: "Accelerates in flight · explosive blast · can hurt you",
  },
  {
    weapon: "ricochet",
    label: "RICOCHET",
    name: "Ricochet shells",
    color: 0xb19afc,
    help: "High damage · bounces up to three times",
  },
  {
    weapon: "piercing",
    label: "PIERCING",
    name: "Piercing shells",
    color: 0x54e6dc,
    help: "Passes through one enemy shell · stops at tanks and cover",
  },
] as const;

export type AmmoWeapon = (typeof AMMO_OPTIONS)[number]["weapon"];
export const AMMO_ORDER: readonly AmmoWeapon[] = AMMO_OPTIONS.map((option) => option.weapon);

/** Stable cosmetic/aftermath choice without consuming the combat RNG stream. */
export function tankBurnout(seed: number, id: number, deaths: number): boolean {
  let hash = (seed ^ Math.imul(id, 0x9e3779b1) ^ Math.imul(deaths, 0x85ebca6b)) >>> 0;
  hash = Math.imul(hash ^ (hash >>> 16), 0x7feb352d);
  hash = Math.imul(hash ^ (hash >>> 15), 0x846ca68b);
  hash = (hash ^ (hash >>> 16)) >>> 0;
  return hash % 5 === 0;
}

import { earnExperience, KILL_XP } from "./veterancy";
import { distance } from "./data";
import { clearAmmo } from "./ammunition";
import { breakTank } from "./wrecks";
import { awardKill } from "./match";
import type { Simulation } from "./simulation";
import type { Tank, Cover, Team, Vec2 } from "./types";
export function damageTank(
  s: Simulation,
  t: Tank,
  amount: number,
  owner: number,
  team: Team,
  ownerLife?: number,
) {
  if (!t.alive || t.protection > 0 || (t.team === team && t.id !== owner))
    return;
  if (s.gameMode === "solo" && team !== s.humanTeam) amount *= 0.4;
  if (amount > 0) t.lastCombat = s.elapsed;
  if (t.shield > 0 && t.shieldPoints > 0) {
    const absorbed = Math.min(amount, t.shieldPoints);
    t.shieldPoints -= absorbed;
    amount -= absorbed;
    if (t.shieldPoints === 0) t.shield = 0;
  }
  const hullDamage = Math.min(t.hp, Math.max(0, amount));
  t.hp -= amount;
  const attacker = s.tanks.find((a) => a.id === owner);
  if (attacker && attacker.team === team && attacker.team !== t.team && hullDamage > 0)
    earnExperience(s, attacker, hullDamage + (t.hp <= 0 ? KILL_XP : 0), ownerLife);
  const p = t.body.translation();
  if (t.hp > 0) {
    if (amount > 0) s.events.push({
      type: "hurt",
      x: p.x,
      z: p.z,
      id: t.id,
      owner,
      team: t.team,
      size: amount,
    });
    return;
  }
  t.hp = 0;
  t.alive = false;
  t.laser = 0;
  clearAmmo(t);
  t.deaths++;
  t.respawn = 3;
  t.previous = { x: p.x, z: p.z };
  const killer = s.tanks.find((a) => a.id === owner);
  if (killer && killer !== t && killer.team !== t.team) killer.kills++;
  if (s.gameMode === "team") awardKill(s.match, t.team, team, owner === t.id);
  s.checkSoloResult();
  breakTank(s, t);
  s.events.push({
    type: "death",
    x: p.x,
    z: p.z,
    id: t.id,
    owner,
    team: t.team,
    size: 3,
    label: `${killer?.human ? "YOU" : killer?.name ?? "YARD"}  ▸  ${t.human ? "YOU" : t.name}`,
  });
}
export function damageCover(
  s: Simulation,
  c: Cover,
  amount: number,
  owner: number,
  team: Team,
  ownerLife?: number,
) {
  if (!c.alive || !c.destructible) return;
  c.hp -= amount;
  if (c.hp > 0) return;
  c.alive = false;
  s.destroyed++;
  s.coverByCollider.delete(c.collider.handle);
  s.world.removeRigidBody(c.body);
  s.nav.rebuild(s.covers, c);
  s.events.push({
    type: "destroy",
    coverKind: c.kind,
    height: c.h,
    x: c.x,
    z: c.z,
    id: c.id,
    size: c.kind === "tower" ? 7 : 2,
    color: c.color,
  });
  for (let i = 0; i < (c.kind === "tower" ? 10 : c.kind === "tree" ? 9 : 3); i++)
    s.fragment(
      c.x + s.rng.range(-c.w / 2, c.w / 2),
      c.z + s.rng.range(-c.d / 2, c.d / 2),
      c.kind === "tree" ? (i % 3 === 0 ? 0x825333 : c.color) : c.color,
      s.rng.range(0.3, 0.7),
      c.kind === "shed" ||
        c.kind === "fence" ||
        c.kind === "house" ||
        c.kind === "tree"
        ? "track"
        : c.kind === "drum"
          ? "armor"
          : "shard",
    );
  if (c.kind === "tower") {
    // One authored support object; its destruction leaves two flank foundations and an open middle.
    for (const side of [-1, 1])
      s.addCover({
        kind: "rubble",
        x: c.x + side * 2.55,
        z: c.z,
        w: 1.3,
        d: 3,
        h: 1.25,
        hp: Infinity,
        color: 0x9b9481,
      });
    s.nav.rebuild(s.covers, c);
  }
  if (c.kind === "drum") explode(s, c, 6, 75, owner, team, ownerLife);
}
export function explode(
  s: Simulation,
  p: Vec2,
  radius: number,
  damage: number,
  owner: number,
  team: Team,
  ownerLife?: number,
) {
  s.events.push({ type: "explosion", ...p, size: radius });
  // Blast-triggered mines retain the initiator of this chain, like drums.
  const chained = s.mines.filter((m) => distance(p, m) < radius);
  s.mines = s.mines.filter((m) => distance(p, m) >= radius);
  for (const m of chained) explode(s, m, 5.7, m.damage ?? 100, owner, team, ownerLife);
  for (const t of s.tanks) {
    if (!t.alive) continue;
    const q = t.body.translation(),
      d = distance(p, q);
    if (d > radius) continue;
    const factor = Math.max(0.25, 1 - d / radius);
    damageTank(s, t, damage * factor, owner, team, ownerLife);
    if (t.alive && (t.team !== team || t.id === owner)) {
      const m = Math.max(0.1, d);
      t.body.applyImpulse(
        {
          x: ((q.x - p.x) / m) * 9 * factor,
          y: 0,
          z: ((q.z - p.z) / m) * 9 * factor,
        },
        true,
      );
    }
  }
  // alive is cleared before recursion, so drums and chains are exactly-once and keep the original owner.
  for (const c of [...s.covers])
    if (
      c.alive &&
      c.destructible &&
      distance(p, c) < radius + Math.max(c.w, c.d) * 0.35
    )
      damageCover(s, c, damage, owner, team, ownerLife);
}

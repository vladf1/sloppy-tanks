import { GROUP, VEHICLES, TEAM_COLORS, distance } from "./data";
import { awardKill } from "./match";
import type { Simulation } from "./simulation";
import type { Tank, Cover, Team, Vec2 } from "./types";
export function damageTank(
  s: Simulation,
  t: Tank,
  amount: number,
  owner: number,
  team: Team,
) {
  if (!t.alive || t.protection > 0 || (t.team === team && t.id !== owner))
    return;
  t.hp -= amount * (t.shield > 0 ? 0.4 : 1);
  const p = t.body.translation();
  s.events.push({
    type: "hurt",
    x: p.x,
    z: p.z,
    id: t.id,
    team: t.team,
    size: amount,
  });
  if (t.hp > 0) return;
  t.hp = 0;
  t.alive = false;
  t.deaths++;
  t.respawn = 3;
  t.previous = { x: p.x, z: p.z };
  const killer = s.tanks.find((a) => a.id === owner);
  if (killer && killer !== t && killer.team !== t.team) killer.kills++;
  awardKill(s.match, t.team, team, owner === t.id);
  t.body.setEnabledRotations(true, true, true, true);
  t.collider.setCollisionGroups(GROUP.fragment);
  t.body.applyImpulse(
    {
      x: s.rng.range(-7, 7),
      y: 12 * VEHICLES[t.kind].mass,
      z: s.rng.range(-7, 7),
    },
    true,
  );
  t.body.applyTorqueImpulse({ x: 5, y: 3, z: 4 }, true);
  s.fragments.push({
    id: s.nextId++,
    body: t.body,
    life: 2.4,
    size: 1,
    color: 0x46534c,
    wreck: t.kind,
    team: t.team,
  });
  s.events.push({
    type: "death",
    x: p.x,
    z: p.z,
    id: t.id,
    team: t.team,
    size: 3,
    label: `${killer?.human ? "YOU" : killer ? `BOT ${killer.id}` : "YARD"}  ▸  ${t.human ? "YOU" : `BOT ${t.id}`}`,
  });
  s.fragment(p.x, p.z, TEAM_COLORS[t.team], 0.65, "armor");
  s.fragment(p.x, p.z, 0x263447, 0.5, "wheel");
  s.fragment(p.x, p.z, 0x43566a, 0.55, "track");
}
export function damageCover(
  s: Simulation,
  c: Cover,
  amount: number,
  owner: number,
  team: Team,
) {
  if (!c.alive || !c.destructible) return;
  c.hp -= amount;
  if (c.hp > 0) return;
  c.alive = false;
  s.destroyed++;
  s.world.removeRigidBody(c.body);
  s.nav.rebuild(s.covers, c);
  s.events.push({
    type: "destroy",
    x: c.x,
    z: c.z,
    id: c.id,
    size: c.kind === "tower" ? 7 : 2,
    color: c.color,
    label: c.kind === "tower" ? "SHORTCUT OPEN" : "COVER BREACHED",
  });
  for (let i = 0; i < (c.kind === "tower" ? 10 : 3); i++)
    s.fragment(
      c.x + s.rng.range(-c.w / 2, c.w / 2),
      c.z + s.rng.range(-c.d / 2, c.d / 2),
      c.color,
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
  if (c.kind === "drum") explode(s, c, 6, 75, owner, team);
}
export function explode(
  s: Simulation,
  p: Vec2,
  radius: number,
  damage: number,
  owner: number,
  team: Team,
) {
  s.events.push({ type: "explosion", ...p, size: radius });
  // Blast-triggered mines retain the initiator of this chain, like drums.
  const chained = s.mines.filter((m) => distance(p, m) < radius);
  s.mines = s.mines.filter((m) => distance(p, m) >= radius);
  for (const m of chained) explode(s, m, 5.7, 100, owner, team);
  for (const t of s.tanks) {
    if (!t.alive) continue;
    const q = t.body.translation(),
      d = distance(p, q);
    if (d > radius) continue;
    const factor = Math.max(0.25, 1 - d / radius);
    damageTank(s, t, damage * factor, owner, team);
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
      damageCover(s, c, damage, owner, team);
}

import RAPIER from "@dimforge/rapier3d-compat";
import { writeFileSync } from "node:fs";
import { Simulation } from "../src/game/simulation";
import { idleCommand, type Tank } from "../src/game/types";
import { distance } from "../src/game/data";
await RAPIER.init();
const label = process.argv[2] ?? "after";
if (!["before", "after"].includes(label)) throw new Error("Use before or after");
function fixture() {
  const s = new Simulation(123);
  for (const c of s.covers) s.world.removeRigidBody(c.body);
  for (const t of s.tanks) s.world.removeRigidBody(t.body);
  s.covers = [];
  s.coverByCollider.clear();
  s.tanks = [];
  s.pickups = [];
  s.nav.rebuild([]);
  return s;
}
function place(t: Tank, x: number, z: number) {
  t.body.setTranslation({ x, y: 0.65, z }, true);
  t.previous = { x, z };
  t.protection = 1000;
}
function motion(s: Simulation, tank: Tank, seconds: number) {
  const points = [],
    start = { ...tank.body.translation() };
  let reversals = 0,
    previous = { x: 0, z: 0 },
    minZ = start.z;
  for (let i = 0; i < seconds * 60; i++) {
    s.step(idleCommand());
    const p = tank.body.translation(),
      c = tank.command;
    const length = Math.hypot(c.moveX, c.moveZ);
    if (length > 0.15) {
      const next = { x: c.moveX / length, z: c.moveZ / length };
      if (next.x * previous.x + next.z * previous.z < -0.5) reversals++;
      previous = next;
    }
    minZ = Math.min(minZ, p.z);
    if (i % 30 === 0) points.push({ x: +p.x.toFixed(2), z: +p.z.toFixed(2) });
    s.events = [];
  }
  return { reversals, displacement: distance(start, tank.body.translation()), minZ, points };
}
const scenarios = [];
{
  const s = fixture();
  const bot = s.addTank(0, false, "balanced", 1),
    target = s.addTank(1, true, "balanced");
  place(bot, 0, -8);
  place(target, 0, 0);
  bot.brain.personality = "guard";
  s.addCover({ kind: "concrete", x: 0, z: -12, w: 16, d: 2, h: 3, hp: Infinity, color: 0 });
  s.nav.rebuild(s.covers);
  s.world.step();
  s.start();
  scenarios.push({ name: "retreat-at-wall", ...motion(s, bot, 10) });
  s.dispose();
}
{
  const s = fixture(),
    bot = s.addTank(0, false, "balanced", 1);
  place(bot, 0, 0);
  bot.brain.decision = 999;
  bot.brain.goal = { x: 0, z: 0.7 };
  bot.brain.path = [];
  s.world.step();
  s.start();
  scenarios.push({ name: "arrive-at-goal", ...motion(s, bot, 4) });
  s.dispose();
}
{
  const s = fixture(),
    a = s.addTank(0, false, "balanced", 1),
    b = s.addTank(0, false, "balanced", 1);
  place(a, 0, -6);
  place(b, 0, 6);
  for (const [t, z] of [
    [a, 16],
    [b, -16],
  ] as const) {
    t.brain.decision = 999;
    t.brain.goal = { x: 0, z };
    t.brain.path = s.nav.find(t.previous, t.brain.goal);
  }
  s.world.step();
  s.start();
  scenarios.push({ name: "head-on-allies", ...motion(s, a, 10), other: b.body.translation() });
  s.dispose();
}
const rounds = [];
for (const mapMode of ["village", "random"] as const)
  for (const seed of [123, 456, 789]) {
    const s = new Simulation(seed);
    s.mapMode = mapMode;
    s.reset();
    s.start();
    const windows = new Map<
      number,
      {
        x: number;
        z: number;
        samples: number;
        reversals: number;
        intent: number;
        dx: number;
        dz: number;
      }
    >();
    let stalledWindows = 0,
      twitchWindows = 0,
      activeWindows = 0,
      reversals = 0;
    const start = performance.now();
    for (let i = 0; i < 60 * 90; i++) {
      s.step(idleCommand(), true);
      s.events = [];
      for (const t of s.tanks) {
        if (!t.alive) {
          windows.delete(t.id);
          continue;
        }
        const p = t.body.translation(),
          c = t.command,
          length = Math.hypot(c.moveX, c.moveZ);
        const w = windows.get(t.id) ?? {
          x: p.x,
          z: p.z,
          samples: 0,
          reversals: 0,
          intent: 0,
          dx: 0,
          dz: 0,
        };
        if (length > 0.15) {
          const dx = c.moveX / length,
            dz = c.moveZ / length;
          if (dx * w.dx + dz * w.dz < -0.5) {
            w.reversals++;
            reversals++;
          }
          w.dx = dx;
          w.dz = dz;
          w.intent++;
        }
        w.samples++;
        if (w.samples >= 120) {
          if (w.intent > 80) {
            activeWindows++;
            if (distance(p, w) < 1) {
              stalledWindows++;
              if (w.reversals >= 8) twitchWindows++;
            }
          }
          windows.delete(t.id);
        } else windows.set(t.id, w);
      }
    }
    rounds.push({
      mapMode,
      seed,
      stalledWindows,
      twitchWindows,
      activeWindows,
      reversals,
      simulationMs: (performance.now() - start) / 5400,
      scores: s.match.scores,
    });
    s.dispose();
  }
const result = { label, scenarios, rounds };
writeFileSync(`artifacts/bot-movement-${label}.json`, JSON.stringify(result, null, 2));
console.log(JSON.stringify(result));

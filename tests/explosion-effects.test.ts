import { test } from "node:test";
import assert from "node:assert/strict";
import {
  EXPLOSION_LIFETIME,
  ExplosionEffects,
  MAX_EXPLOSIONS,
  TANK_EXPLOSION_LIFETIME,
} from "../src/game/explosion-effects";
import { ParticleEffects } from "../src/game/particle-effects";

test("blast sequence expands into smoke, then completely retires", () => {
  const effects = new ExplosionEffects();
  effects.event({ type: "explosion", x: 4, z: -2, size: 5 });
  effects.update(0.06);
  assert.equal(effects.puffs.count, 3);
  assert.equal(effects.rings.count, 1);
  effects.update(0.34);
  assert.equal(effects.puffs.count, 5);
  effects.update(0.25);
  assert.equal(effects.rings.count, 0);
  assert.equal(effects.puffs.count, 5);
  effects.update(EXPLOSION_LIFETIME);
  assert.equal(effects.puffs.count, 0);
  assert.equal(effects.rings.count, 0);
});

test("chain reactions reuse bounded buffers and reset without leftover effects", () => {
  const effects = new ExplosionEffects();
  const buffer = effects.puffs.instanceMatrix.array;
  for (let i = 0; i < 1000; i++) effects.event({ type: "explosion", x: i, z: 0, size: 100 });
  effects.update(0.18);
  assert.equal(effects.rings.count, MAX_EXPLOSIONS);
  assert.equal(effects.puffs.count, MAX_EXPLOSIONS * 8);
  assert.ok(Array.from(buffer).every(Number.isFinite));
  effects.reset();
  effects.update(0);
  assert.equal(effects.puffs.count, 0);
  assert.equal(effects.rings.count, 0);
  assert.equal(effects.puffs.instanceMatrix.array, buffer);
});

test("barrel destruction is not counted twice, and wood collapse has no fireball", () => {
  const particles = new ParticleEffects();
  assert.equal(particles.event({ type: "destroy", coverKind: "drum", x: 0, z: 0 }), false);
  particles.update(0.06, 0.06);
  assert.equal(particles.explosions.puffs.count, 0);
  assert.equal(particles.particles.length, 0);
  assert.equal(particles.event({ type: "explosion", x: 0, z: 0, size: 5 }), true);
  particles.update(0.06, 0.12);
  assert.equal(particles.explosions.rings.count, 1);
  assert.equal(particles.particles.length, 8);
  particles.reset();
  assert.equal(particles.event({ type: "destroy", coverKind: "timber", x: 0, z: 0 }), false);
  particles.update(0.06, 0.06);
  const tint = particles.explosions.puffs.geometry.getAttribute("puffColor");
  assert.ok(tint.getX(0) < 1, "collapse produces dust, not the bright fire core");
  assert.ok(particles.particles.every((p) => p.shape === "splinter"));
});

test("tank deaths have taller, darker, longer smoke and upward embers", (context) => {
  context.mock.method(Math, "random", () => 0.5);
  const tank = new ExplosionEffects();
  const shell = new ExplosionEffects();
  tank.event({ type: "death", x: 0, z: 0, size: 3 });
  shell.event({ type: "explosion", x: 0, z: 0, size: 3 });
  tank.update(0.75);
  shell.update(0.75);
  const tankMatrix = tank.puffs.instanceMatrix.array;
  const shellMatrix = shell.puffs.instanceMatrix.array;
  assert.ok(tankMatrix[4 * 16 + 13] > shellMatrix[4 * 16 + 13] + 1);
  assert.ok(
    tank.puffs.geometry.getAttribute("puffColor").getX(0) <
      shell.puffs.geometry.getAttribute("puffColor").getX(0),
  );
  const shellCleanup = EXPLOSION_LIFETIME - 0.75 + 0.01;
  tank.update(shellCleanup);
  shell.update(shellCleanup);
  assert.equal(shell.puffs.count, 0);
  assert.equal(tank.puffs.count, 5);
  tank.update(TANK_EXPLOSION_LIFETIME);
  assert.equal(tank.puffs.count, 0);
  const particles = new ParticleEffects();
  particles.event({ type: "death", x: 0, z: 0, size: 3 });
  assert.equal(particles.particles.length, 8);
  assert.ok(particles.particles.every((p) => p.vy >= 3.5 && p.life >= 0.8));
});

test("mixed tank deaths and blasts still fit the shared pool", () => {
  const effects = new ExplosionEffects();
  for (let i = 0; i < 100; i++) effects.event({ type: i % 2 ? "death" : "explosion", x: i, z: 0 });
  effects.update(0.37);
  assert.ok(effects.puffs.count <= MAX_EXPLOSIONS * 8);
  assert.ok(effects.puffs.count >= MAX_EXPLOSIONS * 5);
  assert.equal(effects.rings.count, MAX_EXPLOSIONS);
  effects.reset();
  effects.event({ type: "explosion", x: 0, z: 0 });
  effects.update(EXPLOSION_LIFETIME);
  assert.equal(effects.puffs.count, 0, "reused tank slot must not retain its long lifetime");
});

test("burnout uses thin smoke with no shockwave or bright explosion light", () => {
  const particles = new ParticleEffects();
  assert.equal(
    particles.event({ type: "death", deathStyle: "burnout", x: 0, z: 0, size: 3 }),
    false,
  );
  assert.equal(particles.particles.length, 3);
  particles.update(0.5, 0.5);
  assert.equal(particles.explosions.rings.count, 0);
  assert.equal(particles.explosions.puffs.count, 3);
});

test("consecutive tank and barrel blasts have different silhouettes without growing buffers", () => {
  for (const type of ["death", "explosion"] as const) {
    const effects = new ExplosionEffects();
    const signatures = [];
    for (let i = 0; i < 3; i++) {
      effects.event({
        type,
        x: 0,
        z: 0,
        size: 3,
        coverKind: type === "explosion" ? "drum" : undefined,
      });
      effects.update(0.45);
      signatures.push(
        Array.from(effects.puffs.instanceMatrix.array.slice(0, effects.puffs.count * 16)),
      );
      effects.update(4);
    }
    assert.notDeepEqual(signatures[0], signatures[1]);
    assert.notDeepEqual(signatures[1], signatures[2]);
    assert.equal(effects.puffs.instanceMatrix.count, MAX_EXPLOSIONS * 8);
  }
});

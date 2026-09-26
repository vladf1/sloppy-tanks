import { before, test } from "node:test";
import assert from "node:assert/strict";
import * as THREE from "three";
import RAPIER from "@dimforge/rapier3d-compat";
import { tankModel } from "../src/game/models";
import { SHELL_HIT_RADIUS, tankHitTime } from "../src/game/hitboxes";
import { Simulation } from "../src/game/simulation";
import { fireWeapon, interceptionTime, stepProjectiles } from "../src/game/weapons";
import { stepMines } from "../src/game/mines";
import { MINE } from "../src/game/combat-rules";
import { VEHICLES, WEAPONS, STEP } from "../src/game/data";
import type { Shot, Team, VehicleKind, Weapon } from "../src/game/types";
import { clearArena, placeTank } from "./fixtures";

before(async () => {
  await RAPIER.init();
});

/** One enemy target at the origin, facing `heading`. */
function fixture(kind: VehicleKind = "balanced", heading = 0) {
  const s = new Simulation(123);
  const target = s.tanks.find((t) => !t.human && t.kind === kind)!;
  clearArena(s, [target]);
  target.team = 1;
  target.protection = 0;
  placeTank(target, 0, 0, heading);
  s.world.step();
  s.start();
  return { s, target };
}

/** The first `count` roster tanks in a column along +z; the first one is the human. */
function column(count = 0) {
  const s = new Simulation(123);
  clearArena(s, s.tanks.slice(0, count));
  for (const [i, t] of s.tanks.entries()) {
    t.human = i === 0;
    t.protection = 0;
    placeTank(t, 0, i * 12);
  }
  s.humanTeam = 0;
  s.world.step();
  s.start();
  return s;
}

/** The human, one enemy and one ally, 14 m apart along +z. */
function squad() {
  const s = new Simulation(123);
  const player = s.human;
  const enemy = s.tanks.find((t) => t.team !== player.team)!;
  const ally = s.tanks.find((t) => !t.human && t.team === player.team)!;
  clearArena(s, [player, enemy, ally]);
  for (const [i, tank] of s.tanks.entries()) {
    tank.protection = 0;
    tank.body.setTranslation({ x: 0, y: 0.65, z: i * 14 }, true);
    tank.previous = { x: 0, z: i * 14 };
  }
  s.world.step();
  s.start();
  return { s, player, enemy, ally };
}

function shot(
  s: Simulation,
  x: number,
  z: number,
  vx: number,
  vz: number,
  team: Team = 0,
  extra: Partial<Shot> = {},
): Shot {
  return {
    id: s.nextId++,
    x,
    z,
    vx,
    vz,
    team,
    owner: s.tanks.find((t) => t.team === team)?.id ?? 999,
    damage: 40,
    bounces: 0,
    life: 3.5,
    piercing: 0,
    weapon: "standard",
    ...extra,
  };
}

/** A standard shell fired by an absent team-0 shooter. */
function shell(s: Simulation, x: number, z: number, vx: number, vz: number) {
  s.shots.push(shot(s, x, z, vx, vz, 0, { owner: 999, life: 2 }));
}

function incoming(s: Simulation, weapon: Weapon, owner: number, team: Team): Shot {
  return shot(s, -5, 0, 30, 0, team, {
    weapon,
    owner,
    life: 3,
    piercing: weapon === "piercing" ? 1 : 0,
  });
}

test("hit boundaries match rendered hull bounds plus shell radius on every side of rotated hulls", () => {
  for (const kind of ["scout", "balanced", "heavy"] as const) {
    const model = tankModel(kind, 1);
    model.updateMatrixWorld(true);
    const bounds = new THREE.Box3().setFromObject(model.userData.hull);
    for (const angle of [0, Math.PI / 3]) {
      const { s, target } = fixture(kind, angle);
      const sin = Math.sin(angle),
        cos = Math.cos(angle);
      // Shots are authored in hull space, then rotated with the tank.
      const localShell = (x: number, z: number, vx: number, vz: number) =>
        shell(s, cos * x + sin * z, -sin * x + cos * z, cos * vx + sin * vz, -sin * vx + cos * vz);
      for (const axis of ["x", "z"] as const)
        for (const side of [-1, 1]) {
          const edge = (side < 0 ? bounds.min[axis] : bounds.max[axis]) + side * SHELL_HIT_RADIUS;
          for (const delta of [-0.001, 0.001]) {
            const offset = edge + side * delta;
            if (axis === "x") localShell(offset, -5, 0, 600);
            else localShell(-5, offset, 600, 0);
            const hit = tankHitTime(s.shots.pop()!, target, STEP);
            assert.equal(hit !== null, delta < 0, `${kind} ${angle} ${axis} ${side} ${delta}`);
          }
        }
      // A real shell grazing the outer tread, beyond the old narrower physics box, deals damage.
      localShell(bounds.max.x + SHELL_HIT_RADIUS - 0.01, -5, 0, 600);
      stepProjectiles(s, STEP);
      assert.equal(target.hp, VEHICLES[kind].health - 40, `${kind} at ${angle}`);
      assert.equal(s.shots.length, 0);
      const physical = target.collider.halfExtents();
      assert.ok(physical);
      assert.ok(Math.abs(physical.x - (bounds.max.x - bounds.min.x) / 2) < 1e-6);
      s.dispose();
    }
  }
});

test("cover still blocks shots at the widened hull, and protected targets do not lose health", () => {
  const { s, target } = fixture();
  s.addCover({ kind: "concrete", x: 0, z: -3.8, w: 5, d: 0.25, h: 3, hp: Infinity, color: 0 });
  s.world.step();
  shell(s, 1.1 * VEHICLES.balanced.scale, -5, 0, 600);
  stepProjectiles(s, STEP);
  assert.equal(target.hp, 100);
  assert.equal(s.shots.length, 0);
  clearArena(s, [target]);
  target.protection = 1;
  shell(s, 0, -5, 0, 600);
  stepProjectiles(s, STEP);
  assert.equal(target.hp, 100);
  target.protection = 0;
  target.shield = 10;
  target.shieldPoints = 40;
  shell(s, 0, -5, 0, 600);
  stepProjectiles(s, STEP);
  assert.equal(target.hp, 100);
  assert.equal(target.shieldPoints, 0);
  s.dispose();
});

test("moving tanks are hit at the crossing time, not just their end-of-tick location", () => {
  for (const [start, end, expected] of [
    [-5, 5, 60],
    [-12, 0, 100],
  ]) {
    const { s, target } = fixture();
    target.body.setTranslation({ x: 0, y: 0.65, z: end }, true);
    target.previous = { x: 0, z: start };
    shell(s, -5, 0, 600, 0);
    stepProjectiles(s, STEP, true);
    assert.equal(target.hp, expected, `travel ${start} to ${end}`);
    s.dispose();
  }
});

test("damage events credit the owner on surviving and lethal hits, excluding protected hits", () => {
  const { s, target } = fixture();
  s.events = [];
  target.protection = 1;
  s.damageTank(target, 40, 999, 0);
  target.protection = 0;
  target.shield = 10;
  target.shieldPoints = 40;
  s.damageTank(target, 40, 999, 0);
  s.damageTank(target, 40, 999, target.team);
  assert.equal(s.events.length, 0);
  s.damageTank(target, 40, 999, 0);
  assert.equal(s.events.at(-1)!.type, "hurt");
  assert.equal(s.events.at(-1)!.owner, 999);
  assert.equal(s.events.at(-1)!.team, target.team);
  s.damageTank(target, 100, 999, 0);
  const death = s.events.find((e) => e.type === "death")!;
  assert.equal(death.owner, 999);
  assert.equal(death.id, target.id);
  s.dispose();
});

test("shells and spread pellets emerge from the model muzzle for all chassis and aim directions", () => {
  for (const kind of ["scout", "balanced", "heavy"] as const)
    for (const angle of [0, 1.2]) {
      const s = new Simulation(123);
      clearArena(s);
      const t = placeTank(s.addTank(0, true, kind), 0, 0);
      t.aim = angle;
      t.ammo.spread = 18;
      t.selectedAmmo = "spread";
      const model = tankModel(kind, 0);
      model.userData.turret.rotation.y = angle;
      model.position.y = 0.25;
      model.updateMatrixWorld(true);
      const muzzle = model.userData.muzzle.getWorldPosition(new THREE.Vector3());
      fireWeapon(s, t);
      assert.equal(s.shots.length, 3);
      for (const p of s.shots) {
        assert.ok(Math.abs(p.x - muzzle.x) < 1e-5);
        assert.ok(Math.abs(p.z - muzzle.z) < 1e-5);
        assert.ok(Math.abs(p.y! - muzzle.y) < 1e-5);
      }
      s.dispose();
    }
});

test("a protruding barrel cannot spawn shots beyond nearby cover or an enemy", () => {
  for (const obstruction of ["cover", "enemy"] as const) {
    const s = new Simulation(123);
    clearArena(s);
    const t = placeTank(s.addTank(0, true, "balanced"), 0, 0);
    t.aim = 0;
    if (obstruction === "cover")
      s.addCover({ kind: "timber", x: 0, z: 1, w: 3, d: 0.2, h: 2, hp: 100, color: 0 });
    else {
      const target = placeTank(s.addTank(1, true, "balanced"), 0, 2);
      target.protection = 0;
    }
    s.world.step();
    t.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
    if (obstruction === "enemy") s.tanks[1].body.setTranslation({ x: 0, y: 0.65, z: 2 }, true);
    s.world.propagateModifiedBodyPositionsToColliders();
    fireWeapon(s, t);
    stepProjectiles(s, STEP);
    assert.equal(obstruction === "cover" ? s.covers[0].hp : s.tanks[1].hp, 60, obstruction);
    s.dispose();
  }
});

for (const weapon of ["standard", "spread", "rocket", "ricochet", "piercing"] as const) {
  test(`${weapon} hit records actual impact direction and death cause`, () => {
    const { s, player, enemy } = squad();
    try {
      player.hp = 1;
      s.shots = [incoming(s, weapon, enemy.id, enemy.team)];
      stepProjectiles(s, 0.3);
      const death = s.events.find((e) => e.type === "death" && e.id === player.id)!;
      assert.ok(death);
      assert.equal(death.damageSource?.cause, weapon);
      assert.ok(
        death.damageSource!.origin.x < death.x,
        "hit from left despite attacker standing below",
      );
      assert.equal(death.owner, enemy.id);
    } finally {
      s.dispose();
    }
  });
}

test("mines, barrels and shell collisions preserve distinct death causes", () => {
  for (const cause of ["mine", "drum", "interception"] as const) {
    const { s, player, enemy, ally } = squad();
    try {
      player.hp = 1;
      if (cause === "mine") {
        s.mines.push({
          id: s.nextId++,
          x: 1,
          z: 0,
          owner: enemy.id,
          team: enemy.team,
          arm: 0,
          life: 10,
          damage: MINE.damage,
        });
        stepMines(s, 1 / 60);
      } else if (cause === "drum") {
        const drum = s.addCover({ kind: "drum", x: 2, z: 0, w: 1, d: 1, h: 2, hp: 1, color: 0 });
        s.damageCover(drum, 2, enemy.id, enemy.team);
      } else {
        s.shots = [
          { ...incoming(s, "standard", enemy.id, enemy.team), x: -1, z: 2.8, vx: 10 },
          { ...incoming(s, "standard", ally.id, ally.team), x: 1, z: 2.8, vx: -10 },
        ];
        stepProjectiles(s, 0.2);
      }
      const death = s.events.find((e) => e.type === "death" && e.id === player.id)!;
      assert.ok(death, cause);
      assert.equal(death.damageSource?.cause, cause);
      assert.equal(death.owner, enemy.id);
    } finally {
      s.dispose();
    }
  }
});

test("protected and fully shielded hits do not emit hull damage direction", () => {
  const { s, player, enemy } = squad();
  try {
    player.protection = 2;
    s.damageTank(player, 40, enemy.id, enemy.team, undefined, {
      cause: "standard",
      origin: { x: -1, z: 0 },
    });
    assert.equal(s.events.length, 0);
    player.protection = 0;
    player.shield = 10;
    player.shieldPoints = 100;
    s.damageTank(player, 40, enemy.id, enemy.team, undefined, {
      cause: "standard",
      origin: { x: -1, z: 0 },
    });
    assert.equal(s.events.length, 0);
    assert.equal(player.shieldPoints, 60);
  } finally {
    s.dispose();
  }
});

test("a reflected shell points toward its bounce, not the original shooter", () => {
  const { s, player, enemy } = squad();
  try {
    s.addCover({ kind: "concrete", x: 5, z: 0, w: 1, d: 10, h: 3, hp: Infinity, color: 0 });
    s.world.step();
    s.shots = [{ ...incoming(s, "ricochet", enemy.id, enemy.team), x: 3, bounces: 1 }];
    stepProjectiles(s, 0.4);
    const hit = s.events.find((e) => e.type === "hurt" && e.id === player.id)!;
    assert.ok(hit);
    assert.equal(hit.damageSource?.cause, "ricochet");
    assert.ok(hit.damageSource!.origin.x > hit.x);
  } finally {
    s.dispose();
  }
});

test("opposing fast shells intercept between endpoints; allies and asynchronous crossing paths pass", () => {
  const s = column();
  const a = shot(s, -5, 0, 1000, 0, 0),
    b = shot(s, 5, 0, -1000, 0, 1);
  s.shots = [a, b];
  stepProjectiles(s, STEP);
  assert.equal(s.shots.length, 0);
  assert.equal(s.events.filter((e) => e.type === "explosion").length, 1);
  assert.equal(interceptionTime(a, { ...b, team: 0 }, 1), null);
  const crossing = shot(s, 0, -8, 0, 100, 1);
  assert.equal(interceptionTime(shot(s, -2, 0, 100, 0, 0), crossing, 0.1), null);
  s.dispose();
});

test("the earliest interception consumes each bullet once, independent of array order", () => {
  for (const reversed of [false, true]) {
    const s = column();
    const a = shot(s, 0, 0, 100, 0, 0),
      near = shot(s, 2, 0, 0, 0, 1),
      far = shot(s, 4, 0, 0, 0, 1);
    s.shots = reversed ? [far, near, a] : [a, near, far];
    stepProjectiles(s, 0.1);
    assert.deepEqual(
      s.shots.map((p) => p.id),
      [far.id],
    );
    assert.equal(s.events.filter((e) => e.type === "explosion").length, 1);
    s.dispose();
  }
});

test("a wall blocks interception, while a reflected shell can intercept on its new path", () => {
  const s = column();
  s.addCover({ kind: "concrete", x: 0, z: 0, w: 0.5, d: 8, h: 3, hp: Infinity, color: 0 });
  s.world.step();
  s.shots = [shot(s, -2, 0, 100, 0, 0), shot(s, 2, 0, -100, 0, 1)];
  stepProjectiles(s, 0.1);
  assert.equal(s.events.filter((e) => e.type === "explosion").length, 0);
  assert.equal(s.shots.length, 0);
  s.events = [];
  s.shots = [
    shot(s, -1, 0, 60, 0, 0, { weapon: "ricochet", bounces: 1 }),
    shot(s, -3, 0, 60, 0, 1),
  ];
  stepProjectiles(s, 0.05);
  assert.equal(s.events.filter((e) => e.type === "ricochet").length, 1);
  assert.equal(s.events.filter((e) => e.type === "explosion").length, 1);
  assert.equal(s.shots.length, 0);
  s.dispose();
});

test("earlier tank impacts and lifetime expiry take precedence over later interception", () => {
  const s = column(1);
  const a = shot(s, -4, 0, 100, 0, 1),
    b = shot(s, 4, 0, -10, 0, 0);
  s.shots = [a, b];
  const hp = s.human.hp;
  stepProjectiles(s, 0.1);
  assert.equal(s.human.hp, hp - 40);
  assert.equal(s.shots.length, 1);
  s.shots = [shot(s, -4, 5, 100, 0, 0, { life: 0.001 }), shot(s, 4, 5, -100, 0, 1)];
  s.events = [];
  stepProjectiles(s, 0.1);
  assert.equal(s.shots.length, 1);
  assert.equal(s.events.filter((e) => e.type === "explosion").length, 0);
  s.dispose();
});

test("interception blast hurts both teams once and credits the opposing shooter", () => {
  const s = column(2);
  for (const [i, t] of s.tanks.entries()) {
    t.body.setTranslation({ x: 0, y: 0.65, z: i ? 1.8 : -1.8 }, true);
    t.hp = 40;
  }
  s.world.step();
  s.shots = [shot(s, -3, 0, 100, 0, 0), shot(s, 3, 0, -100, 0, 1)];
  stepProjectiles(s, 0.05);
  assert.ok(s.tanks.every((t) => !t.alive));
  assert.ok(s.tanks.every((t) => t.kills === 1));
  assert.deepEqual(s.match.scores, [1, 1]);
  s.dispose();
});

test("rockets accelerate along their flight path, reach a cap, and outpace their launch speed", () => {
  const s = column();
  try {
    const base = WEAPONS.rocket.speed;
    const rocket = shot(s, 0, 0, base * 0.6, base * 0.8, 0, { weapon: "rocket" });
    const standard = shot(s, 0, 10, WEAPONS.standard.speed, 0, 0);
    s.shots = [rocket, standard];
    let previous = base;
    for (let i = 0; i < 60; i++) {
      stepProjectiles(s, STEP);
      const speed = Math.hypot(rocket.vx, rocket.vz);
      assert.ok(speed > previous - 1e-9);
      assert.ok(Math.abs(rocket.vx / rocket.vz - 0.75) < 1e-9);
      previous = speed;
    }
    assert.ok(Math.abs(previous - base * 2.5) < 1e-9);
    assert.ok(Math.hypot(rocket.x, rocket.z) > base * 1.7);
    assert.equal(standard.vx, WEAPONS.standard.speed);
    for (let i = 0; i < 60; i++) stepProjectiles(s, STEP);
    assert.ok(Math.abs(Math.hypot(rocket.vx, rocket.vz) - base * 2.5) < 1e-9);
  } finally {
    s.dispose();
  }
});

test("accelerated rockets still hit thin cover and intercept crossing enemy shells", () => {
  for (const obstacle of ["wall", "shell"] as const) {
    const s = column();
    try {
      s.shots = [shot(s, -0.4, 0, WEAPONS.rocket.speed * 2.49, 0, 0, { weapon: "rocket" })];
      if (obstacle === "wall") {
        s.addCover({ kind: "concrete", x: 0, z: 0, w: 0.1, d: 4, h: 3, hp: Infinity, color: 0 });
        s.world.step();
      } else {
        s.shots.push(shot(s, 0.7, 0, -WEAPONS.standard.speed, 0, 1));
      }
      stepProjectiles(s, STEP);
      assert.equal(s.shots.length, 0, obstacle);
      assert.equal(s.events.filter((event) => event.type === "explosion").length, 1, obstacle);
      if (obstacle === "wall")
        assert.ok(s.events.find((event) => event.type === "explosion")!.x < 0);
    } finally {
      s.dispose();
    }
  }
});

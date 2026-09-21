import { tankBurnout } from "../src/game/tank-destruction";
import {
  updateHumveeGoal,
  steadyHumveeShot,
  humveeHoldingPosition,
  withdrawHumvee,
  HUMVEE_DEPARTURE_SECONDS,
} from "../src/game/humvee-tactics";
import { before, test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import RAPIER from "@dimforge/rapier3d-compat";
import { preferredAmmo } from "../src/game/bot-personalities";
import { angleDelta, distance, GROUP, STEP, VEHICLES, WEAPONS } from "../src/game/data";
import { COMBAT } from "../src/game/combat-rules";
import { canCollectAmmo, equippedWeapon } from "../src/game/ammunition";
import { botCommand } from "../src/game/ai";
import { steerBot } from "../src/game/bot-movement";
import { driveTank } from "../src/game/tank-driving";
import { collectPickup, fireWeapon } from "../src/game/weapons";
import { Simulation } from "../src/game/simulation";
import { stepProjectiles } from "../src/game/projectiles";
import { HUMVEE_TRACK_STRENGTH, TrackTrails } from "../src/game/tracks";

before(async () => {
  await RAPIER.init();
});

test("team rounds include fast TOW HMMWVs while solo rounds do not", () => {
  const team = new Simulation(123);
  try {
    const hunters = team.tanks.filter((tank) => tank.kind === "humvee");
    assert.equal(hunters.length, 2);
    assert.ok(hunters.every((tank) => !tank.human));
    assert.ok(hunters.every((tank) => equippedWeapon(tank) === "tow"));
    assert.ok(hunters.every((tank) => preferredAmmo(tank) === "tow"));
    assert.ok(VEHICLES.humvee.health < WEAPONS.standard.damage);
    assert.ok(VEHICLES.humvee.health < VEHICLES.scout.health);
    assert.ok(VEHICLES.humvee.speed > VEHICLES.scout.speed);
  } finally {
    team.dispose();
  }

  const solo = new Simulation(123);
  solo.gameMode = "solo";
  solo.reset();
  try {
    assert.equal(solo.tanks.filter((tank) => tank.kind === "humvee").length, 0);
  } finally {
    solo.dispose();
  }
});

test("a HMMWV fires an unlimited TOW that leaves a fresh Bruiser alive", () => {
  const simulation = new Simulation(123);
  try {
    for (const cover of simulation.covers) simulation.world.removeRigidBody(cover.body);
    simulation.covers = [];
    simulation.movableCovers = [];
    simulation.coverByCollider.clear();
    const hunter = simulation.tanks.find((tank) => tank.kind === "humvee")!;
    const target = simulation.tanks.find((tank) => tank.team !== hunter.team)!;
    for (const tank of simulation.tanks) {
      if (tank !== hunter && tank !== target) simulation.world.removeRigidBody(tank.body);
    }
    simulation.tanks = [hunter, target];
    hunter.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
    target.body.setTranslation({ x: 0, y: 0.65, z: 12 }, true);
    hunter.previous = { x: 0, z: 0 };
    target.previous = { x: 0, z: 12 };
    hunter.aim = 0;
    hunter.brain.target = target.id;
    hunter.cooldown = 0;
    hunter.protection = 0;
    target.protection = 0;
    simulation.world.step();
    simulation.start();

    fireWeapon(simulation, hunter);
    assert.equal(simulation.shots[0].weapon, "tow");
    assert.equal(simulation.shots[0].damage, WEAPONS.tow.damage);
    assert.ok((simulation.shots[0].visualY ?? 0) > (simulation.shots[0].y ?? 0));
    assert.equal(simulation.shots[0].targetId, target.id);
    assert.equal(hunter.ammo.rocket, 0);
    for (let i = 0; i < 40; i++) stepProjectiles(simulation, STEP, true);
    assert.equal(target.alive, true);
    assert.equal(target.hp, VEHICLES[target.kind].health - WEAPONS.tow.damage);
    assert.equal(
      simulation.events.some((event) => event.type === "explosion"),
      false,
    );
  } finally {
    simulation.dispose();
  }
});

test("HMMWVs drive forward except during recovery and guide TOWs with a capped turn", () => {
  const simulation = new Simulation(123);
  try {
    for (const cover of simulation.covers) simulation.world.removeRigidBody(cover.body);
    simulation.covers = [];
    simulation.movableCovers = [];
    simulation.coverByCollider.clear();
    const hunter = simulation.tanks.find((tank) => tank.kind === "humvee")!;
    const target = simulation.tanks.find((tank) => tank.team !== hunter.team)!;
    for (const tank of simulation.tanks) {
      if (tank !== hunter && tank !== target) simulation.world.removeRigidBody(tank.body);
    }
    simulation.tanks = [hunter, target];
    hunter.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
    target.body.setTranslation({ x: 0, y: 0.65, z: 24 }, true);
    hunter.previous = { x: 0, z: 0 };
    target.previous = { x: 0, z: 24 };
    hunter.aim = 0;
    hunter.heading = 0;
    hunter.brain.target = target.id;
    hunter.cooldown = 0;
    hunter.protection = 0;
    target.protection = 0;
    simulation.world.step();
    simulation.start();

    driveTank(hunter, { moveX: 0, moveZ: -1, aim: 0, fire: false, mine: false }, STEP);
    assert.ok(hunter.body.linvel().z >= 0, "normal retreat never drives backward");
    assert.notEqual(hunter.heading, 0, "normal retreat starts a forward-facing pivot");
    hunter.brain.recovery = 1;
    hunter.body.setLinvel({ x: 0, y: 0, z: 0 }, true);
    driveTank(hunter, { moveX: 0, moveZ: -1, aim: 0, fire: false, mine: false }, STEP);
    assert.ok(hunter.body.linvel().z < 0, "stuck recovery may use reverse gear");
    hunter.brain.recovery = 0;

    fireWeapon(simulation, hunter);
    const shot = simulation.shots[0];
    const before = Math.atan2(shot.vx, shot.vz);
    target.body.setTranslation({ x: 12, y: 0.65, z: 24 }, true);
    stepProjectiles(simulation, 0.1);
    const after = Math.atan2(shot.vx, shot.vz);
    assert.ok(Math.abs(angleDelta(before, after)) <= COMBAT.towTurnRate * 0.1 + 1e-9);
    assert.ok(shot.vx > 0, "the TOW bends toward the marked target");
  } finally {
    simulation.dispose();
  }
});

test("HMMWVs never spend TOWs on cover or hidden tanks", () => {
  for (const kind of ["tree", "timber"] as const) {
    const simulation = new Simulation(123);
    try {
      const hunter = simulation.tanks.find((tank) => tank.kind === "humvee")!;
      const target = simulation.tanks.find((tank) => tank.team !== hunter.team)!;
      for (const tank of simulation.tanks) {
        if (tank !== hunter && tank !== target) simulation.world.removeRigidBody(tank.body);
      }
      for (const cover of simulation.covers) simulation.world.removeRigidBody(cover.body);
      simulation.tanks = [hunter, target];
      simulation.covers = [];
      simulation.movableCovers = [];
      simulation.coverByCollider.clear();
      simulation.addCover({
        kind,
        x: 0,
        z: 5,
        w: 2,
        d: 2,
        h: kind === "tree" ? 6 : 2,
        hp: 80,
        color: 0,
      });
      simulation.nav.rebuild(simulation.covers);
      hunter.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
      target.body.setTranslation({ x: 0, y: 0.65, z: 10 }, true);
      hunter.previous = { x: 0, z: 0 };
      target.previous = { x: 0, z: 10 };
      hunter.aim = 0;
      hunter.brain.target = target.id;
      hunter.brain.memory = 1;
      hunter.cooldown = 0;
      target.protection = 0;
      simulation.world.step();

      assert.equal(simulation.visible(hunter.body.translation(), target.body.translation()), false);
      fireWeapon(simulation, hunter);
      assert.equal(simulation.shots.length, 0, `${kind}: hidden tank must not attract a TOW`);

      hunter.brain.target = 0;
      hunter.brain.memory = 0;
      hunter.brain.decision = 999;
      hunter.brain.path = [];
      hunter.brain.goal = { x: 0, z: 10 };
      const command = botCommand(simulation, hunter, STEP);
      assert.equal(command.fire, false, `${kind}: HMMWV must not use TOWs as breach shots`);
    } finally {
      simulation.dispose();
    }
  }
});

test("HMMWVs steer around substantial debris instead of driving through it", () => {
  const simulation = new Simulation(123);
  try {
    const hunter = simulation.tanks.find((tank) => tank.kind === "humvee")!;
    for (const tank of simulation.tanks) {
      if (tank !== hunter) simulation.world.removeRigidBody(tank.body);
    }
    for (const cover of simulation.covers) simulation.world.removeRigidBody(cover.body);
    simulation.tanks = [hunter];
    simulation.covers = [];
    simulation.movableCovers = [];
    simulation.coverByCollider.clear();
    simulation.nav.rebuild([]);
    hunter.body.setTranslation({ x: 0, y: 0.65, z: -5 }, true);
    hunter.previous = { x: 0, z: -5 };
    hunter.heading = 0;
    simulation.world.createCollider(
      RAPIER.ColliderDesc.cuboid(1.1, 0.6, 1.1).setCollisionGroups(GROUP.pushableDebris).setMass(6),
      simulation.world.createRigidBody(RAPIER.RigidBodyDesc.fixed().setTranslation(0, 0.65, -2.5)),
    );
    simulation.world.step();

    const steer = steerBot(simulation, hunter, { x: 0, z: 1 }, STEP);
    assert.ok(Math.abs(steer.x) > 0.1 || steer.z < 0.5, "HMMWV must choose an open side");
  } finally {
    simulation.dispose();
  }
});

test("HMMWVs leave denser continuous wheel trails than tracked vehicles", () => {
  const trailCount = (kind: "humvee" | "scout") => {
    const simulation = new Simulation(123);
    const tank = simulation.human;
    for (const other of simulation.tanks) {
      if (other !== tank) simulation.world.removeRigidBody(other.body);
    }
    simulation.tanks = [tank];
    tank.kind = kind;
    tank.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
    tank.previous = { x: 0, z: 0 };
    const trails = new TrackTrails();
    for (let i = 0; i <= 60; i++) {
      const x = i * 0.5;
      tank.body.setTranslation({ x, y: 0.65, z: 0 }, true);
      tank.previous = { x: x - 0.5, z: 0 };
      tank.heading = 0;
      simulation.elapsed = i / 60;
      trails.update(simulation, 1);
    }
    const count = trails.mesh.count;
    trails.dispose();
    simulation.dispose();
    return count;
  };

  assert.ok(trailCount("humvee") > trailCount("scout") * 1.8);
});

test("HMMWV wheel trails use reduced visual strength", () => {
  const simulation = new Simulation(123);
  try {
    const tank = simulation.human;
    for (const other of simulation.tanks) {
      if (other !== tank) simulation.world.removeRigidBody(other.body);
    }
    simulation.tanks = [tank];
    tank.kind = "humvee";
    tank.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
    tank.previous = { x: 0, z: 0 };
    const trails = new TrackTrails();
    try {
      trails.update(simulation, 1);
      tank.body.setTranslation({ x: 0.5, y: 0.65, z: 0 }, true);
      tank.previous = { x: 0, z: 0 };
      simulation.elapsed = 1 / 60;
      trails.update(simulation, 1);
      const strength = trails.mesh.geometry.getAttribute("trackStrength");
      assert.ok(strength);
      assert.ok(Math.abs(strength.getX(0) - HUMVEE_TRACK_STRENGTH) < 1e-6);
    } finally {
      trails.dispose();
    }
  } finally {
    simulation.dispose();
  }
});

test("the human setup does not expose the team-only HMMWV", async () => {
  const html = await readFile(new URL("../src/game/battle-setup.html", import.meta.url), "utf8");
  assert.equal(html.includes('data-kind="humvee"'), false);
});

test("a rejected TOW launch preserves protection, reload, recoil and combat time", () => {
  const simulation = new Simulation(123);
  try {
    const hunter = simulation.tanks.find((tank) => tank.kind === "humvee")!;
    hunter.brain.target = 0;
    const before = [hunter.protection, hunter.cooldown, hunter.recoil, hunter.lastCombat];
    simulation.elapsed = 10;
    fireWeapon(simulation, hunter);
    assert.equal(simulation.shots.length, 0);
    assert.deepEqual(
      [hunter.protection, hunter.cooldown, hunter.recoil, hunter.lastCombat],
      before,
    );
  } finally {
    simulation.dispose();
  }
});

test("HMMWVs leave unusable ammo crates for other vehicles", () => {
  const simulation = new Simulation(123);
  try {
    const hunter = simulation.tanks.find((tank) => tank.kind === "humvee")!;
    const pickup = { id: 999, kind: "ricochet" as const, x: 0, z: 0, available: true, cooldown: 0 };
    assert.equal(canCollectAmmo(hunter, pickup.kind), false);
    assert.equal(collectPickup(simulation, hunter, pickup), false);
    assert.equal(pickup.available, true);
    assert.equal(hunter.ammo.ricochet, 0);
  } finally {
    simulation.dispose();
  }
});

test("TOW guidance cannot transfer to a new life of the marked target", () => {
  const simulation = new Simulation(123);
  try {
    const hunter = simulation.tanks.find((tank) => tank.kind === "humvee")!;
    const target = simulation.tanks.find((tank) => tank.team !== hunter.team)!;
    for (const cover of simulation.covers) simulation.world.removeRigidBody(cover.body);
    simulation.covers = [];
    for (const tank of simulation.tanks) {
      if (tank !== hunter && tank !== target) simulation.world.removeRigidBody(tank.body);
    }
    simulation.tanks = [hunter, target];
    hunter.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
    target.body.setTranslation({ x: 0, y: 0.65, z: 24 }, true);
    hunter.aim = 0;
    hunter.brain.target = target.id;
    simulation.world.step();
    fireWeapon(simulation, hunter);
    const shot = simulation.shots[0];
    assert.ok(shot);
    target.deaths++;
    target.body.setTranslation({ x: 12, y: 0.65, z: 24 }, true);
    stepProjectiles(simulation, STEP);
    assert.equal(shot.vx, 0);
    assert.equal(shot.targetId, undefined);
  } finally {
    simulation.dispose();
  }
});

test("Humvees plan an escape, fire once, withdraw and wait before attacking again", () => {
  const simulation = new Simulation(123);
  try {
    const hunter = simulation.tanks.find((tank) => tank.kind === "humvee")!;
    const target = simulation.tanks.find((tank) => tank.team !== hunter.team)!;
    for (const cover of simulation.covers) simulation.world.removeRigidBody(cover.body);
    simulation.covers = [];
    simulation.movableCovers = [];
    simulation.coverByCollider.clear();
    simulation.pickups = [];
    for (const tank of simulation.tanks) {
      if (tank !== hunter && tank !== target) simulation.world.removeRigidBody(tank.body);
    }
    simulation.tanks = [hunter, target];
    simulation.nav.rebuild([]);
    hunter.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
    target.body.setTranslation({ x: 0, y: 0.65, z: 28 }, true);
    hunter.aim = 0;
    hunter.brain.target = target.id;
    hunter.brain.memory = 5;
    hunter.brain.lastSeen = { x: 0, z: 28 };
    hunter.brain.decision = 0;
    hunter.brain.reaction = 0;
    simulation.world.step();
    botCommand(simulation, hunter, STEP);
    const tactics = hunter.brain.humvee!;
    assert.equal(tactics.phase, "attack");
    assert.ok(distance(tactics.escape, target.body.translation()) > 28);
    const firingPoint = { ...hunter.brain.goal };
    hunter.brain.decision = 0;
    botCommand(simulation, hunter, STEP);
    assert.deepEqual(
      hunter.brain.goal,
      firingPoint,
      "target acquisition must not overwrite the firing position",
    );
    fireWeapon(simulation, hunter);
    assert.equal(tactics.phase, "withdraw");
    assert.deepEqual(hunter.brain.goal, tactics.escape);
    assert.ok(tactics.readyAt > hunter.cooldown);
    hunter.cooldown = 0;
    hunter.brain.fireDelay = 0;
    hunter.brain.decision = 0;
    assert.equal(botCommand(simulation, hunter, STEP).fire, false);
    hunter.body.setTranslation({ ...tactics.escape, y: 0.65 }, true);
    hunter.brain.decision = 0;
    botCommand(simulation, hunter, STEP);
    assert.notEqual(tactics.phase, "attack", "arrival does not bypass the withdrawal pause");
    simulation.elapsed = Math.max(tactics.readyAt, tactics.deadline) + 0.1;
    hunter.brain.decision = 0;
    botCommand(simulation, hunter, STEP);
    assert.equal(tactics.phase, "attack", "open terrain withdrawal remains bounded");
    assert.ok(
      distance(hunter.brain.goal, tactics.lastShot!) >= 5,
      "next attack uses a different position",
    );
  } finally {
    simulation.dispose();
  }
});

test("Humvees prefer a concealed escape and do not escort idle allies", () => {
  const simulation = new Simulation(123);
  try {
    const hunter = simulation.tanks.find((tank) => tank.kind === "humvee")!;
    const target = simulation.tanks.find((tank) => tank.team !== hunter.team)!;
    for (const cover of simulation.covers) simulation.world.removeRigidBody(cover.body);
    simulation.covers = [];
    simulation.movableCovers = [];
    simulation.coverByCollider.clear();
    simulation.pickups = [];
    for (const tank of simulation.tanks) {
      if (tank !== hunter && tank !== target) simulation.world.removeRigidBody(tank.body);
    }
    simulation.tanks = [hunter, target];
    simulation.addCover({ kind: "concrete", x: 7, z: 3, w: 5, d: 2, h: 3, hp: Infinity, color: 0 });
    simulation.nav.rebuild(simulation.covers);
    hunter.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
    target.body.setTranslation({ x: 0, y: 0.65, z: 28 }, true);
    hunter.brain.decision = 0;
    simulation.world.step();
    botCommand(simulation, hunter, STEP);
    const tactics = hunter.brain.humvee!;
    assert.equal(tactics.phase, "attack");
    assert.equal(simulation.visible(tactics.escape, target.body.translation()), false);
    assert.equal(simulation.nav.clearLine(hunter.brain.goal, tactics.escape), true);
    target.team = hunter.team;
    hunter.brain.target = 0;
    hunter.brain.memory = 0;
    hunter.brain.decision = 0;
    botCommand(simulation, hunter, STEP);
    assert.notEqual(hunter.brain.mode, "escort");
  } finally {
    simulation.dispose();
  }
});

test("Humvees must settle to fire and remain exposed briefly after launch", () => {
  const simulation = new Simulation(123);
  try {
    const hunter = simulation.tanks.find((tank) => tank.kind === "humvee")!;
    updateHumveeGoal(simulation, hunter);
    hunter.brain.target = 1234;
    for (let i = 0; i < 30; i++) assert.equal(steadyHumveeShot(hunter, true, STEP), false);
    assert.equal(humveeHoldingPosition(simulation, hunter), true);
    assert.equal(steadyHumveeShot(hunter, false, STEP), false);
    assert.equal(humveeHoldingPosition(simulation, hunter), false);
    for (let i = 0; i < 60; i++) steadyHumveeShot(hunter, true, STEP);
    assert.equal(steadyHumveeShot(hunter, true, STEP), true);
    withdrawHumvee(simulation, hunter);
    assert.equal(humveeHoldingPosition(simulation, hunter), true);
    simulation.elapsed += HUMVEE_DEPARTURE_SECONDS + STEP;
    assert.equal(humveeHoldingPosition(simulation, hunter), false);
  } finally {
    simulation.dispose();
  }
});

test("Humvee deaths alternate between quiet burnouts and bounded whole-vehicle tumbles", () => {
  for (const quiet of [false, true]) {
    const simulation = new Simulation(123);
    try {
      const hunter = simulation.tanks.find((tank) => tank.kind === "humvee")!;
      const attacker = simulation.tanks.find((tank) => tank.team !== hunter.team)!;
      hunter.protection = 0;
      while (tankBurnout(simulation.seed, hunter.id, hunter.deaths + 1) !== quiet)
        simulation.seed++;
      const bodies = simulation.world.bodies.len();
      simulation.damageTank(hunter, 1000, attacker.id, attacker.team);
      const death = simulation.events.find(
        (event) => event.type === "death" && event.id === hunter.id,
      )!;
      assert.ok(death);
      assert.equal(death.deathStyle === "burnout", quiet);
      const pieces = simulation.fragments.filter((fragment) => fragment.wreck === "humvee");
      assert.equal(pieces.length, 1);
      assert.equal(pieces[0].part, "intact");
      const lift = pieces[0].body.linvel().y;
      const spin = Math.hypot(...Object.values(pieces[0].body.angvel()));
      assert.ok(quiet ? lift > 6 && lift < 7.1 : lift > 8);
      assert.ok(quiet ? spin < 1 : spin > 4);
      assert.equal(simulation.world.bodies.len(), bodies);
    } finally {
      simulation.dispose();
    }
  }
});

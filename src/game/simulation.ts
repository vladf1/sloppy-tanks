import RAPIER from "@dimforge/rapier3d-compat";
import { arenaLayout, pickupLayout, spawnPositions } from "./arena";
import {
  Random,
  STEP,
  VEHICLES,
  GROUP,
  ARENA,
  distance,
  angleDelta,
} from "./data";
import { Navigation } from "./navigation";
import { newMatch, tickMatch } from "./match";
import { botCommand } from "./ai";
import {
  fireWeapon,
  stepProjectiles,
  stepMines,
  placeMine,
  collectPickup,
} from "./weapons";
import { damageTank, damageCover, explode } from "./damage";
import {
  idleCommand,
  type Tank,
  type Team,
  type VehicleKind,
  type Cover,
  type Shot,
  type Mine,
  type Pickup,
  type Fragment,
  type SimEvent,
  type Vec2,
  type VehicleCommand,
} from "./types";
export class Simulation {
  world!: RAPIER.World;
  rng: Random;
  tanks: Tank[] = [];
  covers: Cover[] = [];
  shots: Shot[] = [];
  mines: Mine[] = [];
  pickups: Pickup[] = [];
  fragments: Fragment[] = [];
  events: SimEvent[] = [];
  nav = new Navigation();
  match = newMatch();
  nextId = 1;
  elapsed = 0;
  seed: number;
  humanTeam: Team;
  humanKind: VehicleKind = "balanced";
  maxFragments = 80;
  wreckView?: { minX: number; maxX: number; minZ: number; maxZ: number };
  destroyed = 0;
  shotsFired = 0;
  botBreachShots = 0;
  botReroutes = 0;
  roundCount = 12;
  constructor(seed = 12345) {
    this.seed = seed;
    this.rng = new Random(seed);
    this.humanTeam = this.rng.next() < 0.5 ? 0 : 1;
    this.reset();
  }
  reset(count = this.roundCount) {
    this.world?.free();
    this.rng = new Random(this.seed);
    this.world = new RAPIER.World({ x: 0, y: -22, z: 0 });
    this.world.timestep = STEP;
    this.nextId = 1;
    this.tanks = [];
    this.covers = [];
    this.shots = [];
    this.mines = [];
    this.pickups = [];
    this.fragments = [];
    this.events = [];
    this.elapsed = 0;
    this.wreckView = undefined;
    this.destroyed = 0;
    this.shotsFired = 0;
    this.botBreachShots = 0;
    this.botReroutes = 0;
    this.roundCount = count;
    this.match = newMatch(this.match.round + 1);
    const ground = this.world.createRigidBody(
      RAPIER.RigidBodyDesc.fixed().setTranslation(0, -0.5, 0),
    );
    this.world.createCollider(
      RAPIER.ColliderDesc.cuboid(ARENA + 2, 0.5, ARENA + 2).setCollisionGroups(
        GROUP.ground,
      ),
      ground,
    );
    for (const c of arenaLayout()) this.addCover(c);
    this.pickups = pickupLayout.map((p) => ({
      ...p,
      id: this.nextId++,
      available: true,
      cooldown: 0,
    }));
    this.nav = new Navigation();
    this.nav.rebuild(this.covers);
    for (let i = 0; i < count; i++) {
      const team = (i % 2) as Team;
      this.addTank(
        team,
        i === this.humanTeam,
        i === this.humanTeam
          ? this.humanKind
          : (["scout", "balanced", "heavy"] as VehicleKind[])[
              Math.floor(i / 2) % 3
            ],
        Math.floor(i / 2),
      );
    }
    this.world.step();
  }
  addCover(c: {
    kind: Cover["kind"];
    x: number;
    z: number;
    w: number;
    d: number;
    h: number;
    hp: number;
    color: number;
  }): Cover {
    const body = this.world.createRigidBody(
      RAPIER.RigidBodyDesc.fixed().setTranslation(c.x, c.h / 2, c.z),
    );
    const collider = this.world.createCollider(
      RAPIER.ColliderDesc.cuboid(c.w / 2, c.h / 2, c.d / 2)
        .setCollisionGroups(GROUP.cover)
        .setFriction(0.4),
      body,
    );
    const cover: Cover = {
      ...c,
      id: this.nextId++,
      maxHp: c.hp,
      destructible: Number.isFinite(c.hp),
      alive: true,
      body,
      collider,
    };
    this.covers.push(cover);
    return cover;
  }
  addTank(team: Team, human: boolean, kind: VehicleKind, slot = 0) {
    const p = spawnPositions(team)[slot % 5];
    const offset = slot >= 5 ? 2 : 0;
    const desc = VEHICLES[kind];
    const body = this.world.createRigidBody(
      RAPIER.RigidBodyDesc.dynamic()
        .setTranslation(p.x + (team === 0 ? offset : -offset), 0.65, p.z)
        .enabledRotations(false, true, false)
        .setLinearDamping(0.35)
        .setAngularDamping(8)
        .setCcdEnabled(true),
    );
    const collider = this.world.createCollider(
      RAPIER.ColliderDesc.cuboid(0.83 * desc.scale, 0.6, 1.03 * desc.scale)
        .setMass(desc.mass)
        .setCollisionGroups(GROUP.tank)
        .setFriction(0.05)
        .setRestitution(0.1),
      body,
    );
    const tank: Tank = {
      id: this.nextId++,
      team,
      human,
      kind,
      body,
      collider,
      hp: desc.health,
      alive: true,
      respawn: 0,
      protection: 2,
      weapon: "standard",
      weaponTime: 0,
      shield: 0,
      speed: 0,
      cooldown: 0,
      mineCooldown: 0,
      aim: team === 0 ? Math.PI / 2 : -Math.PI / 2,
      heading: 0,
      previous: { ...p },
      recoil: 0,
      kills: 0,
      deaths: 0,
      command: idleCommand(),
      brain: {
        preference: (["rusher", "cautious", "hunter"] as const)[slot % 3],
        decision: slot * 0.05,
        target: 0,
        memory: 0,
        reaction: 0.3,
        fireDelay: 0,
        aimError: 0,
        path: [],
        goal: { x: 0, z: 0 },
        last: { ...p },
        stuck: 0,
        navVersion: 0,
        mode: "advance",
      },
    };
    this.tanks.push(tank);
    return tank;
  }
  get human() {
    return this.tanks.find((t) => t.human)!;
  }
  start() {
    this.match.phase = "playing";
  }
  step(command: VehicleCommand = idleCommand(), autoplay = false) {
    if (this.match.phase !== "playing") return;
    this.elapsed += STEP;
    tickMatch(this.match, STEP);
    if (this.match.phase !== "playing") return;
    for (const t of this.tanks) {
      if (!t.alive) {
        t.respawn -= STEP;
        if (t.respawn <= 0) this.respawn(t);
        continue;
      }
      const p = t.body.translation();
      t.previous = { x: p.x, z: p.z };
      t.protection = Math.max(0, t.protection - STEP);
      t.cooldown = Math.max(0, t.cooldown - STEP);
      t.mineCooldown = Math.max(0, t.mineCooldown - STEP);
      t.shield = Math.max(0, t.shield - STEP);
      t.speed = Math.max(0, t.speed - STEP);
      t.recoil = Math.max(0, t.recoil - STEP * 6);
      t.weaponTime = Math.max(0, t.weaponTime - STEP);
      if (t.weaponTime === 0) t.weapon = "standard";
      const c = t.human && !autoplay ? command : botCommand(this, t, STEP);
      t.command = c;
      t.aim = c.aim;
      const mag = Math.hypot(c.moveX, c.moveZ);
      const speed = VEHICLES[t.kind].speed * (t.speed > 0 ? 1.4 : 1);
      const dx = (c.moveX / Math.max(1, mag)) * speed,
        dz = (c.moveZ / Math.max(1, mag)) * speed;
      const v = t.body.linvel(),
        ax = dx - v.x,
        az = dz - v.z,
        amount = Math.min(1, (27 * STEP) / (Math.hypot(ax, az) || 1));
      // Bounded impulses preserve knockback; no per-frame velocity overwrite.
      t.body.applyImpulse(
        {
          x: ax * amount * t.body.mass(),
          y: 0,
          z: az * amount * t.body.mass(),
        },
        true,
      );
      if (mag > 0.05)
        t.heading +=
          angleDelta(t.heading, Math.atan2(c.moveX, c.moveZ)) *
          Math.min(1, STEP * 9);
      t.body.setRotation(
        { x: 0, y: Math.sin(t.heading / 2), z: 0, w: Math.cos(t.heading / 2) },
        true,
      );
      if (c.fire) fireWeapon(this, t);
      if (c.mine) placeMine(this, t);
    }
    this.world.step();
    stepProjectiles(this, STEP);
    stepMines(this, STEP);
    for (const p of this.pickups) {
      if (!p.available) {
        p.cooldown -= STEP;
        if (p.cooldown <= 0) p.available = true;
        continue;
      }
      for (const t of this.tanks)
        if (t.alive && distance(t.body.translation(), p) < 1.8) {
          collectPickup(this, t, p);
          break;
        }
    }
    for (let i = this.fragments.length - 1; i >= 0; i--) {
      const f = this.fragments[i];
      f.life -= STEP;
      if (f.life <= 0) {
        this.world.removeRigidBody(f.body);
        this.fragments.splice(i, 1);
      }
    }
    // Consumers drain every rendered frame; headless sessions remain bounded too.
    if (this.events.length > 400)
      this.events.splice(0, this.events.length - 400);
  }
  respawn(t: Tank) {
    const kind = t.human ? this.humanKind : t.kind;
    const stats = VEHICLES[kind];
    t.kind = kind;
    const enemies = this.tanks.filter((e) => e.alive && e.team !== t.team);
    const friends = this.tanks.filter(
      (e) => e.alive && e.team === t.team && e !== t,
    );
    const points = spawnPositions(t.team);
    points.sort(
      (a, b) =>
        this.spawnScore(b, enemies, friends) -
        this.spawnScore(a, enemies, friends),
    );
    const p = points[0];
    const body = this.world.createRigidBody(
      RAPIER.RigidBodyDesc.dynamic()
        .setTranslation(p.x, 0.65, p.z)
        .enabledRotations(false, true, false)
        .setLinearDamping(0.35)
        .setAngularDamping(8)
        .setCcdEnabled(true),
    );
    const collider = this.world.createCollider(
      RAPIER.ColliderDesc.cuboid(0.83 * stats.scale, 0.6, 1.03 * stats.scale)
        .setMass(stats.mass)
        .setCollisionGroups(GROUP.tank)
        .setFriction(0.05),
      body,
    );
    t.body = body;
    t.collider = collider;
    t.hp = stats.health;
    t.alive = true;
    t.protection = 2;
    t.weapon = "standard";
    t.weaponTime = 0;
    t.shield = 0;
    t.speed = 0;
    t.cooldown = 0;
    t.mineCooldown = 0;
    t.previous = { ...p };
    t.brain.path = [];
    t.brain.decision = 0;
    t.brain.fireDelay = 0;
    this.events.push({ type: "respawn", ...p, id: t.id });
  }
  spawnScore(p: Vec2, enemies: Tank[], friends: Tank[]) {
    let score = 50;
    for (const t of enemies) {
      const q = t.body.translation();
      score = Math.min(score, distance(p, q) - (this.visible(p, q) ? 12 : 0));
    }
    for (const f of friends)
      score -= Math.max(0, 5 - distance(p, f.body.translation())) * 8;
    return score;
  }
  visible(a: Vec2, b: Vec2) {
    const len = distance(a, b);
    if (len < 0.01) return true;
    const ray = new RAPIER.Ray(
      { x: a.x, y: 1, z: a.z },
      { x: (b.x - a.x) / len, y: 0, z: (b.z - a.z) / len },
    );
    return !this.world.castRay(
      ray,
      len,
      true,
      undefined,
      undefined,
      undefined,
      undefined,
      (c) => this.covers.some((o) => o.alive && o.collider.handle === c.handle),
    );
  }
  reserveFragments(count: number) {
    while (this.fragments.length + count > this.maxFragments) {
      const old = this.fragments.shift()!;
      this.world.removeRigidBody(old.body);
    }
  }
  fragment(
    x: number,
    z: number,
    color: number,
    size = 0.5,
    shape: NonNullable<Fragment["shape"]> = "shard",
  ) {
    size = Math.round(size * 5) / 5;
    this.reserveFragments(1);
    const body = this.world.createRigidBody(
      RAPIER.RigidBodyDesc.dynamic()
        .setTranslation(x, this.rng.range(1, 3), z)
        .setLinvel(
          this.rng.range(-7, 7),
          this.rng.range(5, 14),
          this.rng.range(-7, 7),
        )
        .setAngvel({
          x: this.rng.range(-6, 6),
          y: this.rng.range(-6, 6),
          z: this.rng.range(-6, 6),
        }),
    );
    this.world.createCollider(
      RAPIER.ColliderDesc.cuboid(
        size / 2,
        size * (shape === "armor" || shape === "track" ? 0.12 : 0.4),
        size / 2,
      )
        .setCollisionGroups(GROUP.fragment)
        .setRestitution(0.25)
        .setMass(0.1),
      body,
    );
    this.fragments.push({
      id: this.nextId++,
      body,
      life: this.rng.range(1.6, 2.6),
      shape,
      size,
      color,
    });
  }
  damageTank = (t: Tank, amount: number, owner: number, team: Team) =>
    damageTank(this, t, amount, owner, team);
  damageCover = (c: Cover, amount: number, owner: number, team: Team) =>
    damageCover(this, c, amount, owner, team);
  explode = (
    p: Vec2,
    radius: number,
    damage: number,
    owner: number,
    team: Team,
  ) => explode(this, p, radius, damage, owner, team);
  snapshot() {
    return {
      seed: this.seed,
      elapsed: this.elapsed,
      match: { ...this.match, scores: [...this.match.scores] },
      tanks: this.tanks.map((t) => ({
        id: t.id,
        team: t.team,
        kind: t.kind,
        alive: t.alive,
        hp: t.hp,
        x: t.alive ? t.body.translation().x : t.previous.x,
        z: t.alive ? t.body.translation().z : t.previous.z,
        aim: t.aim,
        kills: t.kills,
        deaths: t.deaths,
        mode: t.brain.mode,
      })),
      counts: {
        bodies: this.world.bodies.len(),
        colliders: this.world.colliders.len(),
        shots: this.shots.length,
        mines: this.mines.length,
        fragments: this.fragments.length,
        covers: this.covers.filter((c) => c.alive).length,
      },
      destroyed: this.destroyed,
      navVersion: this.nav.version,
      botReroutes: this.botReroutes,
      botBreachShots: this.botBreachShots,
    };
  }
  dispose() {
    this.world.free();
  }
}

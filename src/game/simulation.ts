import { botAssignment, shuffledBotNames, BOT_PROFILES } from "./bot-personalities";
import RAPIER from "@dimforge/rapier3d-compat";
import { arenaLayout, randomArenaLayout, pickupLayout, spawnPositions } from "./arena";
import {
  Random,
  MOVE_ACCELERATION,
  HULL_TURN_SPEED,
  STEP,
  VEHICLES,
  GROUP,
  ARENA,
  distance,
  angleDelta,
} from "./data";
import { Navigation } from "./navigation";
import { tankContactCollider } from "./hitboxes";
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
  gameMode: "team" | "solo" = "team";
  mapMode: "village" | "random" = "village";
  mapSeed = 0;
  readonly enemyCount = 20;
  readonly activeEnemyLimit = 6;
  reinforcementDelay = 0;
  get enemiesEliminated() { return this.tanks.filter(t => !t.human && !t.alive).length; }
  isEasyEnemy(t: Tank) { return this.gameMode === "solo" && !t.human; }
  maxHealth(t: Tank) { return VEHICLES[t.kind].health * (this.isEasyEnemy(t) ? 0.4 : 1); }
  get mapName() { return this.mapMode === "random" ? "RANDOM MAP" : "PINE VILLAGE"; }
  maxFragments = 80;
  wreckView?: { minX: number; maxX: number; minZ: number; maxZ: number };
  destroyed = 0;
  shotsFired = 0;
  botBreachShots = 0;
  botReroutes = 0;
  roundCount = 12;
  private botNames: string[] = [];
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
    this.reinforcementDelay = 0;
    this.wreckView = undefined;
    this.destroyed = 0;
    this.shotsFired = 0;
    this.botBreachShots = 0;
    this.botReroutes = 0;
    this.roundCount = count;
    this.match = newMatch(this.match.round + 1);
    this.botNames = shuffledBotNames((this.seed + this.match.round * 0x9e3779b9) >>> 0);
    const ground = this.world.createRigidBody(
      RAPIER.RigidBodyDesc.fixed().setTranslation(0, -0.5, 0),
    );
    this.world.createCollider(
      RAPIER.ColliderDesc.cuboid(ARENA + 2, 0.5, ARENA + 2).setCollisionGroups(
        GROUP.ground,
      ),
      ground,
    );
    this.mapSeed = (this.seed + this.match.round * 0x9e3779b9) >>> 0;
    for (const c of this.mapMode === "random" ? randomArenaLayout(this.mapSeed) : arenaLayout()) this.addCover(c);
    this.pickups = pickupLayout.map((p) => ({
      ...p,
      id: this.nextId++,
      available: true,
      cooldown: 0,
    }));
    this.nav = new Navigation();
    this.nav.rebuild(this.covers);
    if (this.gameMode === "solo") {
      this.addTank(this.humanTeam, true, this.humanKind, 2);
      for (let i = 0; i < this.activeEnemyLimit; i++)
        this.addTank((1 - this.humanTeam) as Team, false, "scout", i);
    } else for (let i = 0; i < count; i++) {
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
    const p = this.gameMode === "solo" && !human
      ? { x: team === 0 ? -53 : 53, z: -46 + (slot % this.activeEnemyLimit) * 92 / (this.activeEnemyLimit - 1) }
      : spawnPositions(team)[slot % 5];
    const offset = this.gameMode === "solo" ? 0 : Math.floor(slot / 5) * 3;
    const ordinal = this.tanks.filter((t) => !t.human).length;
    const assignment = botAssignment(slot, team, ordinal);
    if (!human) kind = BOT_PROFILES[assignment.personality].chassis;
    const desc = VEHICLES[kind];
    const body = this.world.createRigidBody(
      RAPIER.RigidBodyDesc.dynamic()
        .setTranslation(p.x + (team === 0 ? offset : -offset), 0.65, p.z)
        .enabledRotations(false, true, false)
        .setLinearDamping(0.35)
        .setAngularDamping(8)
        .setCcdEnabled(true)
        .setSoftCcdPrediction(desc.speed * 1.5 * STEP * 2),
    );
    const collider = this.world.createCollider(
      tankContactCollider(kind)
        .setMass(desc.mass)
        .setCollisionGroups(GROUP.tank)
        .setFriction(0.05)
        .setRestitution(0.1),
      body,
    );
    this.world.createCollider(tankContactCollider(kind), body);
    const tank: Tank = {
      id: this.nextId++,
      name: human ? "YOU" : this.botNames[ordinal % this.botNames.length] +
        (ordinal >= this.botNames.length ? ` ${Math.floor(ordinal / this.botNames.length) + 1}` : ""),
      team,
      human,
      kind,
      body,
      collider,
      hp: desc.health,
      alive: true,
      respawn: 0,
      protection: 2,
      spread: 0,
      rocket: 0,
      shield: 0,
      shieldPoints: 0,
      rapid: 0,
      ricochet: 0,
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
        ...assignment,
        lastSeen: { ...p },
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
    tank.hp = this.maxHealth(tank);
    if (this.isEasyEnemy(tank)) tank.brain.ultraAggressive = false;
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
    if (this.gameMode === "solo") {
      this.match.time = Math.max(0, this.match.time - STEP);
      this.checkSoloResult();
      if (this.match.phase === "playing") this.reinforceSolo();
    } else tickMatch(this.match, STEP);
    if (this.match.phase !== "playing") return;
    for (const t of this.tanks) {
      if (!t.alive) {
        if (this.gameMode === "solo") continue;
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
      if (t.shield === 0) t.shieldPoints = 0;
      t.rapid = Math.max(0, t.rapid - STEP);
      t.ricochet = Math.max(0, t.ricochet - STEP);
      t.speed = Math.max(0, t.speed - STEP);
      t.recoil = Math.max(0, t.recoil - STEP * 6);
      t.spread = Math.max(0, t.spread - STEP);
      t.rocket = Math.max(0, t.rocket - STEP);
      const c = t.human && !autoplay ? command : botCommand(this, t, STEP);
      t.command = c;
      t.aim = c.aim;
      const mag = Math.hypot(c.moveX, c.moveZ);
      const speed = VEHICLES[t.kind].speed * (t.speed > 0 ? 1.5 : 1);
      const dx = (c.moveX / Math.max(1, mag)) * speed,
        dz = (c.moveZ / Math.max(1, mag)) * speed;
      const v = t.body.linvel(),
        ax = dx - v.x,
        az = dz - v.z,
        amount = Math.min(1, (MOVE_ACCELERATION * STEP) / (Math.hypot(ax, az) || 1));
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
          Math.max(-HULL_TURN_SPEED * STEP, Math.min(HULL_TURN_SPEED * STEP,
            angleDelta(t.heading, Math.atan2(c.moveX, c.moveZ))));
      t.body.setRotation(
        { x: 0, y: Math.sin(t.heading / 2), z: 0, w: Math.cos(t.heading / 2) },
        true,
      );
      if (c.fire) fireWeapon(this, t);
      if (c.mine) placeMine(this, t);
    }
    this.world.step();
    stepProjectiles(this, STEP, true);
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
  reinforceSolo() {
    this.reinforcementDelay = Math.max(0, this.reinforcementDelay - STEP);
    const enemies = this.tanks.filter(t => !t.human);
    if (enemies.length >= this.enemyCount || this.reinforcementDelay > 0) return;
    const living = enemies.filter(t => t.alive);
    if (living.length >= this.activeEnemyLimit) return;
    const team = (1 - this.humanTeam) as Team;
    const slots = Array.from({ length: this.activeEnemyLimit }, (_, slot) => ({
      slot, x: team === 0 ? -53 : 53, z: -46 + slot * 92 / (this.activeEnemyLimit - 1),
    })).filter(p => this.tanks.every(t => !t.alive || distance(p, t.body.translation()) > 4));
    slots.sort((a, b) => this.spawnScore(b, [this.human], living) - this.spawnScore(a, [this.human], living));
    if (!slots.length) return;
    this.addTank(team, false, "scout", slots[0].slot);
    this.reinforcementDelay = 1;
  }
  checkSoloResult() {
    if (this.gameMode !== "solo" || this.match.phase !== "playing") return;
    if (!this.human.alive || this.match.time === 0) {
      this.match.winner = (1 - this.humanTeam) as Team;
      this.match.phase = "results";
    } else if (this.enemiesEliminated === this.enemyCount) {
      this.match.winner = this.humanTeam;
      this.match.phase = "results";
    }
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
        .setCcdEnabled(true)
        .setSoftCcdPrediction(stats.speed * 1.5 * STEP * 2),
    );
    const collider = this.world.createCollider(
      tankContactCollider(kind)
        .setMass(stats.mass)
        .setCollisionGroups(GROUP.tank)
        .setFriction(0.05),
      body,
    );
    this.world.createCollider(tankContactCollider(kind), body);
    t.body = body;
    t.collider = collider;
    t.hp = this.maxHealth(t);
    t.alive = true;
    t.protection = 2;
    t.spread = 0;
    t.rocket = 0;
    t.shield = 0;
    t.shieldPoints = 0;
    t.rapid = 0;
    t.ricochet = 0;
    t.speed = 0;
    t.cooldown = 0;
    t.mineCooldown = 0;
    t.previous = { ...p };
    t.brain.path = [];
    t.brain.decision = 0;
    t.brain.fireDelay = 0;
    t.brain.target = 0;
    t.brain.memory = 0;
    t.brain.reaction = 0.3;
    t.brain.lastSeen = { ...p };
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
        name: t.name,
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
        personality: t.human ? "player" : t.brain.personality,
        ultraAggressive: !t.human && t.brain.ultraAggressive,
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

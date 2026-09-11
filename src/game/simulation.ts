import RAPIER from "@dimforge/rapier3d-compat";
import { botCommand } from "./ai";
import { hasAmmo, selectAmmo } from "./ammunition";
import { arenaLayout, pickupLayout, randomArenaLayout } from "./arena";
import { shuffledBotNames } from "./bot-personalities";
import { damageCover, damageTank, explode } from "./damage";
import {
  ARENA,
  bestBy,
  distance,
  GROUP,
  LASER_DEFENSE,
  Random,
  SOLO_TIME,
  STEP,
  VEHICLES,
} from "./data";
import type { Difficulty } from "./difficulty";
import { createFragment } from "./fragments";
import { newMatch, tickMatch } from "./match";
import { Navigation } from "./navigation";
import { GRAVITY, MAX_FRAGMENTS, SIMULATION_RULES, SOLO, SPAWN_SCORING } from "./simulation-rules";
import { driveTank } from "./tank-driving";
import { respawnTank, spawnTank } from "./tank-lifecycle";
import {
  idleCommand,
  type Cover,
  type DamageSource,
  type DamageCause,
  type Fragment,
  type Mine,
  type Pickup,
  type Shot,
  type SimEvent,
  type Tank,
  type Team,
  type Vec2,
  type VehicleCommand,
  type VehicleKind,
} from "./types";
import { rankIndex, rankStats, repairVeteran } from "./veterancy";
import { collectPickup, fireWeapon, placeMine, stepMines, stepProjectiles } from "./weapons";
export class Simulation {
  world!: RAPIER.World;
  rng: Random;
  tanks: Tank[] = [];
  covers: Cover[] = [];
  coverByCollider = new Map<number, Cover>();
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
  difficulty: Difficulty = "normal";
  gameMode: "team" | "solo" = "team";
  mapMode: "village" | "random" = "village";
  mapSeed = 0;
  readonly activeEnemyLimit = SOLO.activeEnemies;
  reinforcementDelay = 0;
  isEasyEnemy(tank: Tank): boolean {
    return this.gameMode === "solo" && !tank.human;
  }
  maxHealth(tank: Tank): number {
    return (
      Math.round(
        VEHICLES[tank.kind].health *
          (this.isEasyEnemy(tank) ? SOLO.enemyHealthMultiplier : 1) *
          rankStats(tank).health *
          100,
      ) / 100
    );
  }
  get mapName() {
    return this.mapMode === "random" ? "RANDOM MAP" : "PINE VILLAGE";
  }
  maxFragments: number = MAX_FRAGMENTS;
  wreckView?: { minX: number; maxX: number; minZ: number; maxZ: number };
  destroyed = 0;
  shotsFired = 0;
  botBreachShots = 0;
  botReroutes = 0;
  roundCount: number = SIMULATION_RULES.defaultTankCount;
  private botNames: string[] = [];
  constructor(seed: number = SIMULATION_RULES.defaultSeed) {
    this.seed = seed;
    this.rng = new Random(seed);
    this.humanTeam = this.rng.next() < 0.5 ? 0 : 1;
    this.reset();
  }
  reset(count = this.roundCount): void {
    this.world?.free();
    this.rng = new Random(this.seed);
    this.world = new RAPIER.World({ x: 0, y: -GRAVITY, z: 0 });
    this.world.timestep = STEP;
    this.nextId = 1;
    this.tanks = [];
    this.covers = [];
    this.coverByCollider.clear();
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
    if (this.gameMode === "solo") {
      this.match.time = SOLO_TIME;
    }
    this.botNames = shuffledBotNames(
      (this.seed + this.match.round * SIMULATION_RULES.roundSeedStride) >>> 0,
    );
    const ground = this.world.createRigidBody(
      RAPIER.RigidBodyDesc.fixed().setTranslation(0, -0.5, 0),
    );
    this.world.createCollider(
      RAPIER.ColliderDesc.cuboid(ARENA + 2, 0.5, ARENA + 2).setCollisionGroups(GROUP.ground),
      ground,
    );
    this.mapSeed = (this.seed + this.match.round * SIMULATION_RULES.roundSeedStride) >>> 0;
    for (const c of this.mapMode === "random" ? randomArenaLayout(this.mapSeed) : arenaLayout()) {
      this.addCover(c);
    }
    this.pickups = pickupLayout.map((p) => ({
      ...p,
      id: this.nextId++,
      available: p.kind !== "laser",
      cooldown: p.kind === "laser" ? LASER_DEFENSE.initialDelay : 0,
      cooldownDuration: p.kind === "laser" ? LASER_DEFENSE.initialDelay : 0,
    }));
    this.nav = new Navigation();
    this.nav.rebuild(this.covers);
    if (this.gameMode === "solo") {
      this.addTank(this.humanTeam, true, this.humanKind, 2);
      for (let i = 0; i < this.activeEnemyLimit; i++) {
        this.addTank((1 - this.humanTeam) as Team, false, "scout", i);
      }
    } else {
      for (let i = 0; i < count; i++) {
        const team = (i % 2) as Team;
        this.addTank(
          team,
          i === this.humanTeam,
          i === this.humanTeam
            ? this.humanKind
            : (["scout", "balanced", "heavy"] as VehicleKind[])[Math.floor(i / 2) % 3],
          Math.floor(i / 2),
        );
      }
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
    debrisSeed?: number;
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
    this.coverByCollider.set(collider.handle, cover);
    return cover;
  }
  addTank(team: Team, human: boolean, kind: VehicleKind, slot = 0): Tank {
    return spawnTank(this, this.botNames, team, human, kind, slot);
  }
  get human() {
    return this.tanks.find((tank) => tank.human)!;
  }
  start(): void {
    this.match.phase = "playing";
  }
  step(command: VehicleCommand = idleCommand(), autoplay = false): void {
    if (this.match.phase !== "playing") {
      return;
    }
    this.elapsed += STEP;
    if (this.gameMode === "solo") {
      this.match.time = Math.max(0, this.match.time - STEP);
      this.checkSoloResult();
      if (this.match.phase === "playing") {
        this.reinforceSolo();
      }
    } else {
      tickMatch(this.match, STEP);
    }
    if (this.match.phase !== "playing") {
      return;
    }
    for (const tank of this.tanks) {
      if (!tank.alive) {
        if (this.gameMode === "solo") {
          continue;
        }
        tank.respawn -= STEP;
        if (tank.respawn <= 0) {
          this.respawn(tank);
        }
        continue;
      }
      const position = tank.body.translation();
      tank.previous = { x: position.x, z: position.z };
      tank.protection = Math.max(0, tank.protection - STEP);
      tank.cooldown = Math.max(0, tank.cooldown - STEP);
      tank.mineCooldown = Math.max(0, tank.mineCooldown - STEP);
      tank.shield = Math.max(0, tank.shield - STEP);
      if (tank.shield === 0) {
        tank.shieldPoints = 0;
      }
      tank.rapid = Math.max(0, tank.rapid - STEP);
      tank.speed = Math.max(0, tank.speed - STEP);
      tank.laser = Math.max(0, tank.laser - STEP);
      tank.recoil = Math.max(0, tank.recoil - STEP * SIMULATION_RULES.recoilRecoveryPerSecond);
      const c = tank.human && !autoplay ? command : botCommand(this, tank, STEP);
      tank.command = c;
      if (tank.human && typeof c.ammoSelection === "string" && !hasAmmo(tank, c.ammoSelection)) {
        this.events.push({
          type: "notice",
          id: tank.id,
          x: position.x,
          z: position.z,
          label: `${c.ammoSelection.toUpperCase()} EMPTY — collect an ammo crate`,
        });
      }
      selectAmmo(tank, c.ammoSelection);
      tank.aim = c.aim;
      driveTank(tank, c, STEP);
      if (c.fire) {
        fireWeapon(this, tank);
      }
      if (c.mine) {
        placeMine(this, tank);
      }
    }
    this.world.step();
    stepProjectiles(this, STEP, true);
    stepMines(this, STEP);
    for (const tank of this.tanks) {
      repairVeteran(this, tank, STEP);
    }
    for (const pickup of this.pickups) {
      if (!pickup.available) {
        pickup.cooldown -= STEP;
        if (pickup.cooldown <= 0) {
          pickup.available = true;
        }
        continue;
      }
      for (const tank of this.tanks) {
        if (
          tank.alive &&
          distance(tank.body.translation(), pickup) < SIMULATION_RULES.pickupRadius
        ) {
          if (collectPickup(this, tank, pickup)) {
            break;
          }
        }
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
    if (this.events.length > SIMULATION_RULES.maxPendingEvents) {
      this.events.splice(0, this.events.length - SIMULATION_RULES.maxPendingEvents);
    }
  }
  reinforceSolo(): void {
    this.reinforcementDelay = Math.max(0, this.reinforcementDelay - STEP);
    const enemies = this.tanks.filter((tank) => !tank.human);
    if (this.reinforcementDelay > 0) {
      return;
    }
    const living = enemies.filter((tank) => tank.alive);
    if (living.length >= this.activeEnemyLimit) {
      return;
    }
    const team = (1 - this.humanTeam) as Team;
    const slots = Array.from({ length: this.activeEnemyLimit }, (_, slot) => ({
      slot,
      x: team === 0 ? -SOLO.spawnX : SOLO.spawnX,
      z: -SOLO.spawnHalfSpanZ + (slot * (SOLO.spawnHalfSpanZ * 2)) / (this.activeEnemyLimit - 1),
    })).filter((p) =>
      this.tanks.every((tank) => !tank.alive || distance(p, tank.body.translation()) > 4),
    );
    const spawn = bestBy(slots, (p) => this.spawnScore(p, [this.human], living));
    if (!spawn) {
      return;
    }
    // Reuse the six enemy slots so long runs do not accumulate tanks or HUD meshes.
    const replacement = enemies.find((tank) => !tank.alive);
    if (!replacement) {
      return;
    }
    this.respawn(replacement, spawn);
    this.reinforcementDelay = SOLO.reinforcementSeconds;
  }
  checkSoloResult(): void {
    if (this.gameMode !== "solo" || this.match.phase !== "playing") {
      return;
    }
    if (!this.human.alive) {
      this.match.winner = (1 - this.humanTeam) as Team;
      this.match.phase = "results";
    } else if (this.match.time === 0) {
      this.match.winner = this.humanTeam;
      this.match.phase = "results";
    }
  }
  respawn(tank: Tank, position?: Vec2): void {
    respawnTank(this, tank, position);
  }
  spawnScore(position: Vec2, enemies: Tank[], friends: Tank[]): number {
    let score: number = SPAWN_SCORING.maximumEnemyDistance;
    for (const tank of enemies) {
      const q = tank.body.translation();
      score = Math.min(
        score,
        distance(position, q) - (this.visible(position, q) ? SPAWN_SCORING.visibleEnemyPenalty : 0),
      );
    }
    for (const f of friends) {
      score -=
        Math.max(0, SPAWN_SCORING.allyClearance - distance(position, f.body.translation())) *
        SPAWN_SCORING.allyProximityPenalty;
    }
    return score;
  }
  visible(a: Vec2, b: Vec2): boolean {
    const len = distance(a, b);
    if (len < 0.01) {
      return true;
    }
    const ray = new RAPIER.Ray(
      { x: a.x, y: 1, z: a.z },
      { x: (b.x - a.x) / len, y: 0, z: (b.z - a.z) / len },
    );
    return !this.world.castRay(ray, len, true, undefined, GROUP.coverQuery);
  }
  reserveFragments(count: number): void {
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
    lifetimeScale = 1,
  ): void {
    createFragment(this, x, z, color, size, shape, lifetimeScale);
  }
  damageTank = (
    tank: Tank,
    amount: number,
    owner: number,
    team: Team,
    ownerLife?: number,
    source?: DamageSource,
  ) => damageTank(this, tank, amount, owner, team, ownerLife, source);
  damageCover = (cover: Cover, amount: number, owner: number, team: Team, ownerLife?: number) =>
    damageCover(this, cover, amount, owner, team, ownerLife);
  explode = (
    position: Vec2,
    radius: number,
    damage: number,
    owner: number,
    team: Team,
    ownerLife?: number,
    cause?: DamageCause,
  ) => explode(this, position, radius, damage, owner, team, ownerLife, cause);
  snapshot() {
    return {
      seed: this.seed,
      difficulty: this.difficulty,
      elapsed: this.elapsed,
      match: { ...this.match, scores: [...this.match.scores] },
      tanks: this.tanks.map((tank) => ({
        id: tank.id,
        name: tank.name,
        team: tank.team,
        kind: tank.kind,
        alive: tank.alive,
        hp: tank.hp,
        maxHp: this.maxHealth(tank),
        xp: tank.xp,
        rank: rankIndex(tank),
        selectedAmmo: tank.selectedAmmo,
        ammo: { ...tank.ammo },
        laser: tank.laser,
        x: tank.alive ? tank.body.translation().x : tank.previous.x,
        z: tank.alive ? tank.body.translation().z : tank.previous.z,
        aim: tank.aim,
        kills: tank.kills,
        deaths: tank.deaths,
        mode: tank.brain.mode,
        recovering: tank.brain.recovery > 0,
        recoveries: tank.brain.recoveries,
        personality: tank.human ? "player" : tank.brain.personality,
        ultraAggressive: !tank.human && tank.brain.ultraAggressive,
      })),
      counts: {
        bodies: this.world.bodies.len(),
        colliders: this.world.colliders.len(),
        shots: this.shots.length,
        mines: this.mines.length,
        fragments: this.fragments.length,
        covers: this.covers.filter((cover) => cover.alive).length,
      },
      destroyed: this.destroyed,
      navVersion: this.nav.version,
      botReroutes: this.botReroutes,
      botBreachShots: this.botBreachShots,
    };
  }
  dispose(): void {
    this.world.free();
  }
}

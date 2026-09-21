import { recordDeath, recordKill } from "./combat-record";
import { blastDebris } from "./debris-physics";
import { movedCoverRegion } from "./movable-cover";
import { breakScenery } from "./scenery-pieces";
import { DIFFICULTIES } from "./difficulty";
import { clearAmmo } from "./ammunition";
import { COMBAT, MINE } from "./combat-rules";
import RAPIER from "@dimforge/rapier3d-compat";
import { distance, GROUP } from "./data";
import { treeProportions } from "./tree-proportions";
import { awardKill } from "./match";
import type { Simulation } from "./simulation";
import { SIMULATION_RULES, SOLO } from "./simulation-rules";
import { TOWER_BASE } from "./tower-layout";
import type { Cover, DamageCause, DamageSource, Tank, Team, Vec2 } from "./types";
import { earnExperience, KILL_XP } from "./veterancy";
import { tankBurnout } from "./tank-destruction";
import { breakTank } from "./wrecks";
export function damageTank(
  simulation: Simulation,
  tank: Tank,
  amount: number,
  owner: number,
  team: Team,
  ownerLife?: number,
  source?: DamageSource,
): void {
  if (!tank.alive || tank.protection > 0 || (tank.team === team && tank.id !== owner)) {
    return;
  }
  if (team !== simulation.humanTeam && tank.team === simulation.humanTeam) {
    amount *= DIFFICULTIES[simulation.difficulty].damage;
  }
  if (simulation.gameMode === "solo" && team !== simulation.humanTeam) {
    amount *= SOLO.enemyDamageMultiplier;
  }
  if (amount > 0) {
    tank.lastCombat = simulation.elapsed;
  }
  if (tank.shield > 0 && tank.shieldPoints > 0) {
    const absorbed = Math.min(amount, tank.shieldPoints);
    tank.shieldPoints -= absorbed;
    if (tank.human) {
      simulation.combatRecord.shieldAbsorbed += absorbed;
    }
    amount -= absorbed;
    if (tank.shieldPoints === 0) {
      tank.shield = 0;
    }
  }
  const hullDamage = Math.min(tank.hp, Math.max(0, amount));
  tank.hp -= amount;
  if (tank.human) {
    simulation.combatRecord.damageTaken += hullDamage;
  }
  const attacker = simulation.tanks.find((candidate) => candidate.id === owner);
  if (attacker && attacker.team === team && attacker.team !== tank.team && hullDamage > 0) {
    attacker.damageDealt += hullDamage;
    earnExperience(simulation, attacker, hullDamage + (tank.hp <= 0 ? KILL_XP : 0), ownerLife);
  }
  const position = tank.body.translation();
  if (tank.hp > 0) {
    if (amount > 0) {
      simulation.events.push({
        type: "hurt",
        damageSource: source,
        x: position.x,
        z: position.z,
        id: tank.id,
        owner,
        team: tank.team,
        size: amount,
      });
    }
    return;
  }
  tank.hp = 0;
  recordDeath(simulation, tank, owner);
  tank.alive = false;
  tank.laser = 0;
  clearAmmo(tank);
  tank.deaths++;
  tank.respawn = SIMULATION_RULES.respawnSeconds;
  tank.previous = { x: position.x, z: position.z };
  const killer = simulation.tanks.find((candidate) => candidate.id === owner);
  if (killer && killer !== tank && killer.team !== tank.team) {
    killer.kills++;
    recordKill(simulation, killer, tank, ownerLife, source);
    // Old ordnance counts toward the round, never toward a replacement life.
    if (killer.alive && (ownerLife === undefined || ownerLife === killer.deaths)) {
      killer.lifeKills++;
      killer.bestLifeKills = Math.max(killer.bestLifeKills, killer.lifeKills);
    }
  }
  if (simulation.gameMode === "team") {
    awardKill(simulation.match, tank.team, team, owner === tank.id, !simulation.endlessMatch);
  }
  simulation.checkSoloResult();
  const burnout = tankBurnout(simulation.seed, tank.id, tank.deaths);
  if (!burnout) {
    blastDebris(simulation, position, 3, 60);
  }
  breakTank(simulation, tank, burnout);
  simulation.events.push({
    type: "death",
    deathStyle: burnout ? "burnout" : undefined,
    damageSource: source,
    x: position.x,
    z: position.z,
    id: tank.id,
    owner,
    team: tank.team,
    size: tank.kind === "scout" ? 2.6 : tank.kind === "heavy" ? 3.6 : 3,
    label: `${killer?.human ? "YOU" : (killer?.name ?? "YARD")}  ▸  ${tank.human ? "YOU" : tank.name}`,
  });
}
export function damageCover(
  simulation: Simulation,
  cover: Cover,
  amount: number,
  owner: number,
  team: Team,
  ownerLife?: number,
  impact?: { x: number; y: number; z: number },
): void {
  if (!cover.alive || !cover.destructible) {
    return;
  }
  const previousHp = cover.hp;
  cover.hp -= amount;
  if (cover.hp > 0) {
    if (cover.kind === "timber" && amount > 0) {
      const marks = (cover.timberHits ??= []);
      if (marks.length < 6) {
        marks.push({
          x: impact ? impact.x - cover.x : 0,
          y: impact?.y ?? Math.min(1, cover.h * 0.5),
          z: impact ? impact.z - cover.z : -cover.d / 2,
          size: marks.length === 0 ? 1 : Math.min(1.8, 1.5 + (marks.length - 1) * 0.15),
        });
      }
    }
    return;
  }
  const pose = cover.motion
    ? { position: cover.body.translation(), rotation: cover.body.rotation() }
    : undefined;
  if (pose) {
    cover.x = pose.position.x;
    cover.z = pose.position.z;
  }
  cover.alive = false;
  simulation.destroyed++;
  if (simulation.tanks.some((tank) => tank.human && tank.id === owner && tank.team === team)) {
    simulation.combatRecord.coverDestroyed++;
  }
  for (let i = 0; i < cover.body.numColliders(); i++) {
    simulation.coverByCollider.delete(cover.body.collider(i).handle);
  }
  if (cover.kind === "tree") {
    // A tank-only upright footprint stops planar hulls climbing or crossing the
    // stump while shells can still fly through the space left by the crown.
    cover.collider.setShape(new RAPIER.Cylinder(0.8, treeProportions(cover).stumpRadius));
    cover.collider.setTranslationWrtParent({ x: 0, y: 0.8 - cover.h / 2, z: 0 });
    cover.collider.setCollisionGroups(GROUP.stumpContact);
  } else {
    simulation.world.removeRigidBody(cover.body);
  }
  simulation.nav.rebuild(simulation.covers, movedCoverRegion(cover));
  simulation.events.push({
    type: "destroy",
    coverKind: cover.kind,
    height: cover.h,
    x: cover.x,
    z: cover.z,
    id: cover.id,
    size: cover.kind === "tower" ? 7 : 2,
    color: cover.color,
  });
  if (!breakScenery(simulation, cover, pose, previousHp)) {
    for (let i = 0; i < 3; i++) {
      simulation.fragment(
        cover.x + simulation.rng.range(-cover.w / 2, cover.w / 2),
        cover.z + simulation.rng.range(-cover.d / 2, cover.d / 2),
        cover.color,
        simulation.rng.range(0.3, 0.7),
        cover.kind === "house" ? "track" : "shard",
      );
    }
  }
  if (cover.kind === "tower") {
    // One authored support object; its destruction leaves two flank foundations and an open middle.
    for (const side of [-1, 1]) {
      simulation.addCover({
        kind: "rubble",
        x: cover.x + side * TOWER_BASE.offset,
        z: cover.z,
        w: TOWER_BASE.width,
        d: TOWER_BASE.depth,
        h: TOWER_BASE.rubbleHeight,
        hp: Infinity,
        color: cover.color,
        debrisSeed: Math.floor(simulation.rng.next() * 0x100000000),
      });
    }
    simulation.nav.rebuild(simulation.covers, movedCoverRegion(cover));
  }
  if (cover.kind === "drum") {
    explode(
      simulation,
      cover,
      COMBAT.drumBlastRadius,
      COMBAT.drumDamage,
      owner,
      team,
      ownerLife,
      "drum",
    );
  }
}
export function explode(
  simulation: Simulation,
  position: Vec2,
  radius: number,
  damage: number,
  owner: number,
  team: Team,
  ownerLife?: number,
  cause: DamageCause = "explosion",
): void {
  simulation.events.push({
    type: "explosion",
    ...position,
    size: radius,
    coverKind: cause === "drum" ? "drum" : undefined,
  });
  blastDebris(simulation, position, radius, damage);
  // Blast-triggered mines retain the initiator of this chain, like drums.
  const chained = simulation.mines.filter((m) => distance(position, m) < radius);
  simulation.mines = simulation.mines.filter((m) => distance(position, m) >= radius);
  for (const m of chained) {
    explode(
      simulation,
      m,
      MINE.blastRadius,
      m.damage ?? MINE.damage,
      owner,
      team,
      ownerLife,
      "mine",
    );
  }
  for (const tank of simulation.tanks) {
    if (!tank.alive) {
      continue;
    }
    const q = tank.body.translation();
    const d = distance(position, q);
    if (d > radius) {
      continue;
    }
    const factor = Math.max(COMBAT.minimumBlastDamageFraction, 1 - d / radius);
    damageTank(simulation, tank, damage * factor, owner, team, ownerLife, {
      cause,
      origin: { x: position.x, z: position.z },
    });
    if (tank.alive && (tank.team !== team || tank.id === owner)) {
      const m = Math.max(COMBAT.minimumBlastDistance, d);
      tank.body.applyImpulse(
        {
          x: ((q.x - position.x) / m) * COMBAT.blastImpulse * factor,
          y: 0,
          z: ((q.z - position.z) / m) * COMBAT.blastImpulse * factor,
        },
        true,
      );
    }
  }
  // alive is cleared before recursion, so drums and chains are exactly-once and keep the original owner.
  for (const cover of [...simulation.covers]) {
    if (
      cover.alive &&
      cover.destructible &&
      distance(position, cover) < radius + Math.max(cover.w, cover.d) * COMBAT.coverBlastAllowance
    ) {
      if (cover.kind === "timber") {
        const dx = cover.x - position.x;
        const dz = cover.z - position.z;
        const distance = Math.hypot(dx, dz);
        cover.timberKick = distance > 0.001 ? { x: dx / distance, z: dz / distance } : undefined;
      }
      damageCover(simulation, cover, damage, owner, team, ownerLife, {
        x: position.x,
        y: 1,
        z: position.z,
      });
    }
  }
}

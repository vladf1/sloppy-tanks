import { clearAmmo } from "./ammunition";
import { COMBAT, MINE } from "./combat-rules";
import { distance } from "./data";
import { awardKill } from "./match";
import type { Simulation } from "./simulation";
import { SIMULATION_RULES, SOLO } from "./simulation-rules";
import { TOWER_BASE } from "./tower-layout";
import type { Cover, Tank, Team, Vec2 } from "./types";
import { earnExperience, KILL_XP } from "./veterancy";
import { breakTank } from "./wrecks";
export function damageTank(
  simulation: Simulation,
  tank: Tank,
  amount: number,
  owner: number,
  team: Team,
  ownerLife?: number,
): void {
  if (!tank.alive || tank.protection > 0 || (tank.team === team && tank.id !== owner)) {
    return;
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
    amount -= absorbed;
    if (tank.shieldPoints === 0) {
      tank.shield = 0;
    }
  }
  const hullDamage = Math.min(tank.hp, Math.max(0, amount));
  tank.hp -= amount;
  const attacker = simulation.tanks.find((candidate) => candidate.id === owner);
  if (attacker && attacker.team === team && attacker.team !== tank.team && hullDamage > 0) {
    earnExperience(simulation, attacker, hullDamage + (tank.hp <= 0 ? KILL_XP : 0), ownerLife);
  }
  const position = tank.body.translation();
  if (tank.hp > 0) {
    if (amount > 0) {
      simulation.events.push({
        type: "hurt",
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
  tank.alive = false;
  tank.laser = 0;
  clearAmmo(tank);
  tank.deaths++;
  tank.respawn = SIMULATION_RULES.respawnSeconds;
  tank.previous = { x: position.x, z: position.z };
  const killer = simulation.tanks.find((candidate) => candidate.id === owner);
  if (killer && killer !== tank && killer.team !== tank.team) {
    killer.kills++;
  }
  if (simulation.gameMode === "team") {
    awardKill(simulation.match, tank.team, team, owner === tank.id);
  }
  simulation.checkSoloResult();
  breakTank(simulation, tank);
  simulation.events.push({
    type: "death",
    x: position.x,
    z: position.z,
    id: tank.id,
    owner,
    team: tank.team,
    size: 3,
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
): void {
  if (!cover.alive || !cover.destructible) {
    return;
  }
  cover.hp -= amount;
  if (cover.hp > 0) {
    return;
  }
  cover.alive = false;
  simulation.destroyed++;
  simulation.coverByCollider.delete(cover.collider.handle);
  simulation.world.removeRigidBody(cover.body);
  simulation.nav.rebuild(simulation.covers, cover);
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
  for (
    let i = 0;
    i < (cover.kind === "tower" ? 10 : cover.kind === "tree" ? 9 : cover.kind === "timber" ? 7 : 3);
    i++
  ) {
    simulation.fragment(
      cover.x + simulation.rng.range(-cover.w / 2, cover.w / 2),
      cover.z + simulation.rng.range(-cover.d / 2, cover.d / 2),
      cover.kind === "tree" ? 0x825333 : cover.color,
      simulation.rng.range(0.3, 0.7),
      cover.kind === "tower" || cover.kind === "tree"
        ? "wood"
        : cover.kind === "shed" ||
            cover.kind === "fence" ||
            cover.kind === "timber" ||
            cover.kind === "house"
          ? "track"
          : cover.kind === "drum"
            ? "armor"
            : "shard",
      cover.kind === "tree" ? 3 : cover.kind === "timber" || cover.kind === "fence" ? 2 : 1,
    );
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
    simulation.nav.rebuild(simulation.covers, cover);
  }
  if (cover.kind === "drum") {
    explode(simulation, cover, COMBAT.drumBlastRadius, COMBAT.drumDamage, owner, team, ownerLife);
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
): void {
  simulation.events.push({ type: "explosion", ...position, size: radius });
  // Blast-triggered mines retain the initiator of this chain, like drums.
  const chained = simulation.mines.filter((m) => distance(position, m) < radius);
  simulation.mines = simulation.mines.filter((m) => distance(position, m) >= radius);
  for (const m of chained) {
    explode(simulation, m, MINE.blastRadius, m.damage ?? MINE.damage, owner, team, ownerLife);
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
    damageTank(simulation, tank, damage * factor, owner, team, ownerLife);
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
      damageCover(simulation, cover, damage, owner, team, ownerLife);
    }
  }
}

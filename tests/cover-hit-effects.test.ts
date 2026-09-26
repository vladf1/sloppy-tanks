import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { ParticleEffects } from "../src/game/particle-effects";
import { Simulation } from "../src/game/simulation";
import { stepProjectiles } from "../src/game/weapons";
import type { Cover } from "../src/game/types";

before(async () => {
  await RAPIER.init();
});

/** Fire one standard shell into the cover's near face and route its events to particles. */
function shoot(s: Simulation, particles: ParticleEffects, cover: Cover, damage: number) {
  s.shots.length = 0;
  s.events.length = 0;
  particles.reset();
  s.shots.push({
    id: s.nextId++,
    x: cover.x,
    z: cover.z - cover.d / 2 - 0.5,
    vx: 0,
    vz: 40,
    owner: s.human.id,
    team: s.humanTeam,
    damage,
    bounces: 0,
    life: 1,
    piercing: 0,
    weapon: "standard",
  });
  stepProjectiles(s, 0.05);
  // Particle counts use cosmetic Math.random; fix it only around presentation.
  const random = Math.random;
  try {
    Math.random = () => 0.5;
    for (const event of s.events) particles.event(event);
  } finally {
    Math.random = random;
  }
  const shapes = particles.particles.map((particle) => particle.shape);
  return {
    alive: cover.alive,
    hp: cover.hp,
    impacts: s.events.filter((e) => e.type === "impact" && e.coverKind === cover.kind).length,
    destroys: s.events.filter((e) => e.type === "destroy").length,
    leaves: shapes.filter((shape) => shape === "leaf").length,
    chips: shapes.filter((shape) => shape === "splinter").length,
    fragments: s.fragments.length,
    treeParts: s.fragments.filter((piece) => piece.treeCoverId === cover.id).length,
  };
}

test("shell hits chip trees, timber and cargo; a fatal hit bursts once and leaves debris", () => {
  const s = new Simulation(123);
  const particles = new ParticleEffects();
  try {
    assert.equal(s.mapMode, "village");
    const covers = [
      s.covers.find((c) => c.kind === "tree")!,
      s.covers.find((c) => c.kind === "timber")!,
      s.addCover({ kind: "cargo", x: 0, z: 30, w: 4, d: 0.9, h: 1.5, hp: 80, color: 0xb47a49 }),
    ];
    s.world.step();
    for (const cover of covers) {
      const { kind } = cover;
      const hp = cover.hp;
      const hit = shoot(s, particles, cover, 20);
      assert.equal(hit.alive, true, kind);
      assert.equal(hit.hp, hp - 20, kind);
      assert.equal(hit.impacts, 1, kind);
      assert.equal(hit.destroys, 0, kind);
      assert.ok(hit.chips > 0, `${kind} hit chips`);
      assert.equal(hit.leaves > 0, kind === "tree", `${kind} leaves`);
      const destroyed = shoot(s, particles, cover, 999);
      assert.equal(destroyed.alive, false, kind);
      assert.equal(destroyed.destroys, 1, kind);
      assert.equal(destroyed.impacts, 0, `${kind}: fatal impacts must not double the burst`);
      assert.ok(destroyed.fragments > hit.fragments, `${kind}: destruction creates debris`);
      if (kind === "tree") {
        assert.equal(destroyed.leaves + destroyed.chips, 0, "falling tree parts replace the burst");
        assert.ok(destroyed.treeParts > 0);
      } else {
        assert.ok(destroyed.chips > hit.chips, `${kind}: collapse adds more chips than a hit`);
      }
    }
  } finally {
    s.dispose();
  }
});

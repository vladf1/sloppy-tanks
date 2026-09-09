import * as THREE from "three";
import { AMMO_RESPAWN_SECONDS, isSpecialAmmo } from "./ammunition";
import { batch, freezeStatic } from "./batching";
import { ARENA, MINE_RADIUS, PICKUPS, TEAM_COLORS, VEHICLES } from "./data";
import { healthBarState } from "./health-bar";
import { sidingBox } from "./house-surfaces";
import { LaserVisuals } from "./laser-visuals";
import {
  box,
  coverModel,
  cylinder,
  material,
  put,
  tankModel,
  wreckModel,
  type TankModel,
} from "./models";
import { ParticleEffects, type Particle } from "./particle-effects";
import { pickupCube } from "./pickup-visuals";
import { ProjectileVisuals } from "./projectile-visuals";
import { disposeOwned, isMesh, updateInstances } from "./render-resources";
import { createReticle } from "./reticle";
import { createLighting, createTerrain } from "./scenery";
import type { Simulation } from "./simulation";
import { MAX_FRAGMENTS } from "./simulation-rules";
import { createTankBar, type TankBar } from "./tank-bars";
import { TrackTrails } from "./tracks";
import { setTreeDestroyed } from "./tree-models";
import type { Fragment, SimEvent } from "./types";
import { rankIndex } from "./veterancy";
import { CAMERA, FEEDBACK } from "./view-settings";
import { weaponInterval } from "./weapons";
interface PickupModel extends THREE.Group {
  userData: {
    gem: THREE.Object3D;
    ring?: THREE.Mesh<THREE.TorusGeometry, THREE.MeshStandardMaterial>;
    refill?: THREE.Mesh<THREE.RingGeometry, THREE.MeshBasicMaterial>;
  };
}
function batchTank(model: TankModel): void {
  const d = model.userData;
  batch(d.trackGroup);
  batch(d.hull);
  batch(d.turret);
  batch(d.barrel);
}
export class Presentation {
  renderer: THREE.WebGLRenderer;
  scene = new THREE.Scene();
  camera = new THREE.PerspectiveCamera(CAMERA.fieldOfView, 1, CAMERA.near, CAMERA.far);
  raycaster = new THREE.Raycaster();
  groundPlane = new THREE.Plane(new THREE.Vector3(0, 1, 0), -1);
  private floorPlane = new THREE.Plane(new THREE.Vector3(0, 1, 0), 0);
  private pointer = new THREE.Vector2();
  private aimPoint = new THREE.Vector3();
  private corners = Array.from({ length: 4 }, () => new THREE.Vector3());
  worldGroup = new THREE.Group();
  tankMeshes = new Map<number, TankModel>();
  coverMeshes = new Map<number, THREE.Group>();
  fragmentMeshes = new Map<number, THREE.Object3D>();
  pickupMeshes = new Map<number, PickupModel>();
  mineMeshes = new Map<number, THREE.Group>();
  bars = new Map<number, TankBar>();
  projectiles = new ProjectileVisuals();
  laserVisuals = new LaserVisuals();
  private particleEffects = new ParticleEffects();
  debrisMeshes = new Map<NonNullable<Fragment["shape"]>, THREE.InstancedMesh>();
  playerRing = new THREE.Group();
  spawnPulse: THREE.Mesh;
  spawnCue = 0;
  playerWasAlive = false;
  debrisColor = new THREE.Color();
  get particles(): readonly Particle[] {
    return this.particleEffects.particles;
  }
  hitUntil = new Map<number, number>();
  pickupEffects: { group: THREE.Group; age: number; tankId?: number }[] = [];
  pickupRingGeometry = new THREE.RingGeometry(0.88, 1, 48);
  pickupGlowGeometry = new THREE.SphereGeometry(1, 16, 10);
  tracks = new TrackTrails();
  dummy = new THREE.Object3D();
  follow = new THREE.Vector3();
  zoom: number = CAMERA.defaultZoom;
  time = 0;
  flash = new THREE.PointLight(0xffc178, 0, 20, 2);
  crosshair: THREE.Group;
  reticleInk: THREE.MeshBasicMaterial;
  reticleCenter: THREE.MeshBasicMaterial;
  hitConfirmUntil = 0;
  constructor(public canvas: HTMLCanvasElement) {
    this.renderer = new THREE.WebGLRenderer({
      canvas,
      antialias: true,
      powerPreference: "high-performance",
    });
    this.renderer.setPixelRatio(Math.min(devicePixelRatio, CAMERA.maxPixelRatio));
    this.renderer.shadowMap.enabled = true;
    this.renderer.shadowMap.type = THREE.PCFShadowMap;
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.renderer.toneMappingExposure = 1.0;
    createLighting(this.scene);
    this.scene.add(this.flash);
    this.scene.add(this.worldGroup);
    this.scene.add(this.tracks.mesh);
    createTerrain(this.scene, this.renderer);
    const woodFragment = sidingBox(1.5, 0.18, 0.45, 0xffffff);
    const fragmentGeometry = {
      wood: woodFragment.geometry,
      armor: box(1.25, 0.16, 0.85, 0xffffff).geometry,
      wheel: new THREE.CylinderGeometry(0.48, 0.48, 0.28, 10),
      track: box(0.5, 0.2, 1.5, 0xffffff).geometry,
      shard: new THREE.TetrahedronGeometry(0.75),
    };
    for (const shape of Object.keys(fragmentGeometry) as NonNullable<Fragment["shape"]>[]) {
      const mesh = new THREE.InstancedMesh(
        fragmentGeometry[shape],
        shape === "wood" ? woodFragment.material : material(0xffffff),
        MAX_FRAGMENTS,
      );
      mesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
      mesh.frustumCulled = false;
      mesh.castShadow = mesh.receiveShadow = true;
      mesh.count = 0;
      this.debrisMeshes.set(shape, mesh);
      this.scene.add(mesh);
    }
    this.scene.add(this.projectiles.group);
    this.scene.add(this.laserVisuals.group);
    // A thin dark rim reads on sand; the yellow ring identifies the player on either team.
    for (const [inner, outer, color, y] of [
      [1.48, 1.9, 0x172f4a, 0.1],
      [1.56, 1.8, 0xffe522, 0.12],
    ]) {
      const ring = new THREE.Mesh(
        new THREE.RingGeometry(inner, outer, 48),
        new THREE.MeshBasicMaterial({
          color,
          side: THREE.DoubleSide,
          toneMapped: false,
        }),
      );
      ring.rotation.x = -Math.PI / 2;
      put(this.playerRing, ring, 0, y, 0);
    }
    this.scene.add(this.playerRing);
    this.spawnPulse = new THREE.Mesh(
      new THREE.RingGeometry(1.8, 1.96, 48),
      new THREE.MeshBasicMaterial({
        color: 0xffe522,
        transparent: true,
        depthWrite: false,
        side: THREE.DoubleSide,
        toneMapped: false,
      }),
    );
    this.spawnPulse.rotation.x = -Math.PI / 2;
    this.scene.add(this.spawnPulse);
    this.scene.add(this.particleEffects.mesh);
    const reticle = createReticle();
    this.crosshair = reticle.crosshair;
    this.reticleInk = reticle.ink;
    this.reticleCenter = reticle.center;
    this.scene.add(this.crosshair);
    this.resize();
  }
  reset(simulation: Simulation): void {
    disposeOwned(this.worldGroup);
    this.worldGroup.clear();
    this.tankMeshes.clear();
    this.coverMeshes.clear();
    this.fragmentMeshes.clear();
    this.pickupMeshes.clear();
    this.mineMeshes.clear();
    this.bars.clear();
    this.particleEffects.reset();
    this.hitUntil.clear();
    this.hitConfirmUntil = 0;
    this.projectiles.reset();
    this.laserVisuals.reset();
    for (const effect of this.pickupEffects) {
      this.scene.remove(effect.group);
      disposeOwned(effect.group);
    }
    this.pickupEffects = [];
    this.tracks.reset();
    for (const mesh of this.debrisMeshes.values()) {
      mesh.count = 0;
    }
    this.playerWasAlive = false;
    for (const cover of simulation.covers) {
      const group = coverModel(cover);
      batch(group);
      this.coverMeshes.set(cover.id, group);
      this.worldGroup.add(group);
      freezeStatic(group);
    }
    for (const tank of simulation.tanks) {
      const model = tankModel(tank.kind, tank.team);
      batchTank(model);
      this.tankMeshes.set(tank.id, model);
      this.worldGroup.add(model);
      this.makeBar(tank.id, tank.team, tank.human);
    }
    for (const pickup of simulation.pickups) {
      const group = new THREE.Group() as PickupModel;
      const def = PICKUPS[pickup.kind];
      put(group, cylinder(1.05, 0.12, 0x25435f, 24), 0, 0.08, 0);
      const ring = new THREE.Mesh(new THREE.TorusGeometry(0.94, 0.045, 5, 24), material(def.color));
      ring.geometry.userData.owned = true;
      if (isSpecialAmmo(pickup.kind)) {
        ring.material = ring.material.clone();
        ring.material.transparent = true;
        ring.material.userData.owned = true;
        const refill = new THREE.Mesh(
          new THREE.RingGeometry(0.89, 1.02, 48, 1, Math.PI / 2),
          new THREE.MeshBasicMaterial({
            color: def.color,
            transparent: true,
            opacity: 0.9,
            side: THREE.DoubleSide,
            depthWrite: false,
            toneMapped: false,
          }),
        );
        refill.geometry.userData.owned = true;
        refill.material.userData.owned = true;
        refill.rotation.x = -Math.PI / 2;
        refill.visible = false;
        put(group, refill, 0, 0.23, 0);
        group.userData.refill = refill;
        group.userData.ring = ring;
      }
      ring.rotation.x = Math.PI / 2;
      put(group, ring, 0, 0.2, 0);
      const gem = pickupCube(pickup.kind);
      gem.rotation.y = Math.PI / 4;
      put(group, gem, 0, 1.25, 0);
      group.userData.gem = gem;
      group.position.set(pickup.x, 0, pickup.z);
      this.worldGroup.add(group);
      this.pickupMeshes.set(pickup.id, group);
    }
    const position = simulation.human.body.translation();
    this.follow.set(position.x, 0, position.z);
  }
  makeBar(id: number, team: number, human: boolean): void {
    const bar = createTankBar(team, human);
    this.bars.set(id, bar);
    this.worldGroup.add(bar);
  }
  resize(width = innerWidth, height = innerHeight, exact = false): void {
    this.renderer.setPixelRatio(exact ? 1 : Math.min(devicePixelRatio, CAMERA.maxPixelRatio));
    this.renderer.setSize(width, height, !exact);
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
  }
  aim(nx: number, ny: number) {
    this.raycaster.setFromCamera(this.pointer.set(nx, ny), this.camera);
    const position = this.aimPoint;
    this.raycaster.ray.intersectPlane(this.groundPlane, position);
    this.crosshair.position.x = position.x;
    this.crosshair.position.z = position.z;
    return position;
  }
  damageAngle(event: SimEvent): number | null {
    const origin = event.damageSource?.origin;
    if (!origin || Math.hypot(origin.x - event.x, origin.z - event.z) < 0.001) {
      return null;
    }
    const direction = new THREE.Vector3(
      origin.x - event.x,
      0,
      origin.z - event.z,
    ).transformDirection(this.camera.matrixWorldInverse);
    return Math.atan2(direction.x, direction.y);
  }
  event(event: SimEvent, playerHit = false): void {
    if (event.type === "notice") {
      return;
    }
    this.laserVisuals.event(event);
    if (playerHit) {
      this.hitConfirmUntil = this.time + FEEDBACK.hitConfirmationSeconds;
    }
    if ((event.type === "death" || event.type === "respawn") && event.id !== undefined) {
      this.hitUntil.delete(event.id);
    }
    const hurt = event.type === "hurt";
    if (hurt) {
      if (event.id === undefined || (event.size ?? 0) <= 0) {
        return;
      }
      this.hitUntil.set(event.id, this.time + FEEDBACK.recoilSeconds);
    }
    if (event.type === "respawn") {
      return;
    }
    const pickup = event.type === "pickup" || event.type === "promotion";
    if (pickup) {
      if (this.pickupEffects.length >= FEEDBACK.maxPickupEffects) {
        const oldest = this.pickupEffects.shift()!;
        this.scene.remove(oldest.group);
        disposeOwned(oldest.group);
      }
      const group = new THREE.Group();
      const ringMaterial = new THREE.MeshBasicMaterial({
        color: event.color ?? 0xffffff,
        transparent: true,
        opacity: 0.9,
        depthWrite: false,
        side: THREE.DoubleSide,
        blending: THREE.AdditiveBlending,
      });
      ringMaterial.userData.owned = true;
      const glowMaterial = ringMaterial.clone();
      glowMaterial.opacity = 0.2;
      glowMaterial.side = THREE.BackSide;
      glowMaterial.userData.owned = true;
      const ring = new THREE.Mesh(this.pickupRingGeometry, ringMaterial);
      ring.rotation.x = -Math.PI / 2;
      ring.position.y = 0.08;
      group.add(ring, new THREE.Mesh(this.pickupGlowGeometry, glowMaterial));
      group.position.set(event.x, 0, event.z);
      this.scene.add(group);
      this.pickupEffects.push({ group, age: 0, tankId: event.id });
    }
    if (this.particleEffects.event(event)) {
      this.flash.position.set(event.x, 3, event.z);
      this.flash.intensity = 45;
    }
  }
  private updatePickupEffects(simulation: Simulation, alpha: number, dt: number): void {
    for (let i = this.pickupEffects.length - 1; i >= 0; i--) {
      const effect = this.pickupEffects[i];
      effect.age += dt;
      const progress = effect.age / FEEDBACK.pickupSeconds;
      if (progress >= 1) {
        this.scene.remove(effect.group);
        disposeOwned(effect.group);
        this.pickupEffects.splice(i, 1);
        continue;
      }
      const [ring, glow] = effect.group.children as THREE.Mesh<
        THREE.BufferGeometry,
        THREE.MeshBasicMaterial
      >[];
      ring.scale.setScalar(1 + progress * 3);
      ring.material.opacity = 0.85 * (1 - progress) ** 2;
      const tank = simulation.tanks.find((tank) => tank.id === effect.tankId && tank.alive);
      glow.visible = !!tank;
      if (tank) {
        const pos = tank.body.translation();
        glow.position.set(
          THREE.MathUtils.lerp(tank.previous.x, pos.x, alpha) - effect.group.position.x,
          1.1,
          THREE.MathUtils.lerp(tank.previous.z, pos.z, alpha) - effect.group.position.z,
        );
        glow.scale
          .set(1.65, 1.25, 1.9)
          .multiplyScalar(VEHICLES[tank.kind].scale * (1 + progress * 0.15));
        glow.material.opacity = 0.2 * (1 - progress) ** 2;
      }
    }
  }
  private updateCamera(simulation: Simulation, alpha: number, overview: boolean): void {
    const position = simulation.human.alive
      ? simulation.human.body.translation()
      : simulation.human.previous;
    // Follow the same interpolated pose as the tank, with no edge clamp or trailing lag.
    this.follow.set(
      overview ? 0 : THREE.MathUtils.lerp(simulation.human.previous.x, position.x, alpha),
      overview ? 0 : 0.7,
      overview ? 0 : THREE.MathUtils.lerp(simulation.human.previous.z, position.z, alpha),
    );
    const zoom = overview ? ARENA * 1.8 : this.zoom;
    this.camera.position.set(
      this.follow.x,
      this.follow.y + zoom * 0.93,
      this.follow.z + zoom * 0.72,
    );
    this.camera.lookAt(this.follow);
    this.camera.updateMatrixWorld();
    const corners = this.corners;
    for (let i = 0; i < corners.length; i++) {
      this.raycaster.setFromCamera(
        this.pointer.set(i % 2 ? 0.8 : -0.8, i < 2 ? -0.7 : 0.65),
        this.camera,
      );
      this.raycaster.ray.intersectPlane(this.floorPlane, corners[i]);
    }
    const bounds = (simulation.wreckView ??= { minX: 0, maxX: 0, minZ: 0, maxZ: 0 });
    bounds.minX = Math.max(corners[0].x, corners[2].x);
    bounds.maxX = Math.min(corners[1].x, corners[3].x);
    bounds.minZ = corners[2].z;
    bounds.maxZ = corners[0].z;
  }
  private updatePlayerIndicators(
    simulation: Simulation,
    alpha: number,
    dt: number,
    overview: boolean,
  ): void {
    const position = simulation.human.alive
      ? simulation.human.body.translation()
      : simulation.human.previous;
    this.flash.intensity *= Math.exp(-dt * FEEDBACK.flashDecay);
    const confirmed = this.hitConfirmUntil > this.time;
    const ready = simulation.human.cooldown <= 0;
    this.reticleInk.opacity = confirmed || ready ? 1 : 0.3;
    this.reticleCenter.opacity = confirmed || ready ? 1 : 0.3;
    this.reticleInk.color.setHex(confirmed ? 0xffffff : 0xfff9da);
    this.reticleCenter.color.setHex(confirmed ? 0xffffff : 0xffdf38);
    this.crosshair.scale.setScalar(confirmed ? 1.2 : 1);
    if (simulation.human.alive && !this.playerWasAlive) {
      this.spawnCue = FEEDBACK.spawnCueSeconds;
    }
    this.playerWasAlive = simulation.human.alive;
    this.spawnCue = Math.max(0, this.spawnCue - dt);
    this.playerRing.visible = simulation.human.alive && !overview;
    this.playerRing.position.set(
      THREE.MathUtils.lerp(simulation.human.previous.x, position.x, alpha),
      0,
      THREE.MathUtils.lerp(simulation.human.previous.z, position.z, alpha),
    );
    this.playerRing.scale.setScalar(VEHICLES[simulation.human.kind].scale);
    this.spawnPulse.visible = this.playerRing.visible && this.spawnCue > 0;
    this.spawnPulse.position.copy(this.playerRing.position);
    this.spawnPulse.position.y = 0.14;
    this.spawnPulse.scale.setScalar(
      1 + ((FEEDBACK.spawnCueSeconds - this.spawnCue) % FEEDBACK.spawnPulseSeconds) * 2,
    );
    (this.spawnPulse.material as THREE.MeshBasicMaterial).opacity =
      Math.min(1, this.spawnCue) *
      (1 -
        ((FEEDBACK.spawnCueSeconds - this.spawnCue) % FEEDBACK.spawnPulseSeconds) /
          FEEDBACK.spawnPulseSeconds);
  }
  private updateTanks(simulation: Simulation, alpha: number): void {
    for (const tank of simulation.tanks) {
      let group = this.tankMeshes.get(tank.id);
      // Reinforcements arrive after reset, so create their visuals on first render.
      if (!group || group.userData.kind !== tank.kind) {
        if (group) {
          disposeOwned(group);
          this.worldGroup.remove(group);
        }
        group = tankModel(tank.kind, tank.team);
        batchTank(group);
        this.tankMeshes.set(tank.id, group);
        this.worldGroup.add(group);
      }
      group.visible = tank.alive;
      if (!this.bars.has(tank.id)) {
        this.makeBar(tank.id, tank.team, tank.human);
      }
      const bar = this.bars.get(tank.id)!;
      bar.visible = tank.alive;
      if (!tank.alive) {
        this.hitUntil.delete(tank.id);
        continue;
      }
      const pos = tank.body.translation();
      group.position.set(
        THREE.MathUtils.lerp(tank.previous.x, pos.x, alpha),
        pos.y - 0.4,
        THREE.MathUtils.lerp(tank.previous.z, pos.z, alpha),
      );
      const hitRemaining = Math.max(0, (this.hitUntil.get(tank.id) ?? 0) - this.time);
      const hitFade = hitRemaining / FEEDBACK.recoilSeconds;
      const hitAge = FEEDBACK.recoilSeconds - hitRemaining;
      // Render-only recoil: physics, steering and the camera keep their true pose.
      group.position.x += Math.cos(hitAge * 70) * 0.12 * hitFade;
      group.position.z += Math.sin(hitAge * 55) * 0.09 * hitFade;
      group.rotation.x = Math.sin(hitAge * 60) * 0.035 * hitFade;
      group.rotation.z = Math.cos(hitAge * 65) * 0.045 * hitFade;
      if (hitRemaining === 0) {
        this.hitUntil.delete(tank.id);
      }
      group.userData.hull.rotation.y = tank.heading;
      group.userData.turret.rotation.y = tank.aim;
      group.userData.barrel.position.z = -tank.recoil * 0.2;
      const velocity = tank.body.linvel();
      group.userData.trackGroup.position.z =
        (this.time * Math.hypot(velocity.x, velocity.z) * 0.4) % 0.25;
      group.scale.setScalar(VEHICLES[tank.kind].scale);
      bar.position.set(group.position.x, tank.human ? 3.2 : 2.5, group.position.z);
      bar.quaternion.copy(this.camera.quaternion);
      const health = healthBarState(tank.hp, simulation.maxHealth(tank), tank.team);
      bar.userData.fg.scale.x = health.ratio;
      bar.userData.fg.visible = health.ratio > 0;
      bar.userData.ammo.scale.x = Math.max(0, 1 - tank.cooldown / weaponInterval(tank));
      bar.userData.fg.material.color.setHex(health.color);
      const rank = rankIndex(tank);
      (bar.userData.ranks as THREE.Mesh[]).forEach((chevron, i) => {
        chevron.visible = i < rank;
      });
    }
  }
  private updateCover(simulation: Simulation): void {
    for (const cover of simulation.covers) {
      let group = this.coverMeshes.get(cover.id);
      const stump = cover.kind === "tree" && !cover.alive;
      const damageStage =
        cover.kind === "timber"
          ? Math.min(2, Math.floor(((cover.maxHp - cover.hp) * 3) / cover.maxHp))
          : 0;
      if (!group || (cover.alive && (group.userData.damageStage ?? 0) !== damageStage)) {
        if (group) {
          disposeOwned(group);
          this.worldGroup.remove(group);
        }
        group = coverModel(cover, "full", damageStage);
        batch(group);
        this.coverMeshes.set(cover.id, group);
        this.worldGroup.add(group);
        freezeStatic(group);
      }
      if (cover.kind === "tree") {
        setTreeDestroyed(group, stump);
      }
      group.visible = cover.alive || stump;
    }
  }
  private updatePickups(simulation: Simulation, dt: number): void {
    for (const pickup of simulation.pickups) {
      const group = this.pickupMeshes.get(pickup.id)!;
      const refill = group.userData.refill as THREE.Mesh<THREE.RingGeometry> | undefined;
      group.visible = pickup.available || !!refill;
      group.userData.gem.visible = pickup.available;
      if (refill) {
        group.userData.ring!.material.opacity = pickup.available ? 1 : 0.2;
        refill.visible = !pickup.available;
        const progress = THREE.MathUtils.clamp(1 - pickup.cooldown / AMMO_RESPAWN_SECONDS, 0, 1);
        refill.geometry.setDrawRange(0, Math.floor(progress * 48) * 6);
      }
      group.userData.gem.rotation.y += dt;
      group.userData.gem.position.y = 1.2 + Math.sin(this.time * 2 + pickup.id) * 0.18;
    }
  }
  private updateFragments(simulation: Simulation): void {
    const fragIds = new Set(simulation.fragments.map((f) => f.id));
    for (const [id, g] of this.fragmentMeshes) {
      if (!fragIds.has(id)) {
        disposeOwned(g);
        this.worldGroup.remove(g);
        this.fragmentMeshes.delete(id);
      }
    }
    for (const mesh of this.debrisMeshes.values()) {
      mesh.count = 0;
    }
    for (const f of simulation.fragments) {
      const pos = f.body.translation();
      const q = f.body.rotation();
      if (!f.wreck) {
        const mesh = this.debrisMeshes.get(f.shape ?? "shard")!;
        if (mesh.count >= MAX_FRAGMENTS) {
          continue;
        }
        this.dummy.position.set(pos.x, pos.y, pos.z);
        this.dummy.quaternion.set(q.x, q.y, q.z, q.w);
        this.dummy.scale.setScalar(f.size * Math.min(1, f.life * 2));
        this.dummy.updateMatrix();
        mesh.setMatrixAt(mesh.count, this.dummy.matrix);
        mesh.setColorAt(mesh.count++, this.debrisColor.set(f.color));
        continue;
      }
      let g = this.fragmentMeshes.get(f.id);
      if (!g) {
        g = wreckModel(f.wreck, f.team ?? 0, f.part ?? "hull");
        if (f.cleanup === "fade") {
          g.traverse((o) => {
            if (!isMesh(o)) {
              return;
            }
            const clone = (material: THREE.Material) => {
              const copy = material.clone();
              copy.transparent = true;
              copy.userData.owned = true;
              return copy;
            };
            o.material = Array.isArray(o.material) ? o.material.map(clone) : clone(o.material);
          });
        }
        this.fragmentMeshes.set(f.id, g);
        this.worldGroup.add(g);
      }
      g.position.set(pos.x, pos.y, pos.z);
      g.quaternion.set(q.x, q.y, q.z, q.w);
      const remaining = Math.min(1, f.life * 2);
      g.scale.setScalar(VEHICLES[f.wreck].scale * (f.cleanup === "fade" ? 1 : remaining));
      if (f.cleanup === "fade") {
        g.traverse((o) => {
          if (!isMesh(o)) {
            return;
          }
          const materials = Array.isArray(o.material) ? o.material : [o.material];
          for (const material of materials) {
            material.opacity = remaining;
          }
          o.castShadow = remaining === 1;
        });
      }
    }
    for (const mesh of this.debrisMeshes.values()) {
      updateInstances(mesh);
    }
  }
  private updateMines(simulation: Simulation): void {
    const mineIds = new Set(simulation.mines.map((m) => m.id));
    for (const [id, g] of this.mineMeshes) {
      if (!mineIds.has(id)) {
        this.worldGroup.remove(g);
        this.mineMeshes.delete(id);
      }
    }
    for (const m of simulation.mines) {
      let group = this.mineMeshes.get(m.id);
      if (!group) {
        group = new THREE.Group();
        put(group, cylinder(MINE_RADIUS, 0.17, 0x384f47), 0, 0.12, 0);
        put(group, cylinder(0.17, 0.07, TEAM_COLORS[m.team]), 0, 0.24, 0);
        group.position.set(m.x, 0, m.z);
        this.mineMeshes.set(m.id, group);
        this.worldGroup.add(group);
      }
      group.children[1].visible = m.arm > 0 || Math.sin(this.time * 10) > 0;
    }
  }
  /** Synchronize entity visuals before drawing; alpha blends the previous and current physics poses. */
  render(simulation: Simulation, alpha: number, dt: number, overview = false): void {
    this.time += dt;
    this.updatePickupEffects(simulation, alpha, dt);
    this.tracks.update(simulation, alpha);
    this.updateCamera(simulation, alpha, overview);
    this.updatePlayerIndicators(simulation, alpha, dt, overview);
    this.updateTanks(simulation, alpha);
    this.updateCover(simulation);
    this.updatePickups(simulation, dt);
    this.updateFragments(simulation);
    this.updateMines(simulation);
    this.projectiles.update(simulation.shots, this.time);
    this.laserVisuals.update(simulation, alpha, dt);
    this.particleEffects.update(dt, this.time);
    this.crosshair.visible = simulation.match.phase === "playing";
    this.renderer.render(this.scene, this.camera);
  }
}

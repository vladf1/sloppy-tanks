import { timberPartModel } from "./timber-model";
import * as THREE from "three";
import { barrelScrapGeometry } from "./barrel-debris";
import { AMMO_RESPAWN_SECONDS } from "./ammunition";
import { batch, freezeStatic } from "./batching";
import { coverDamageStage } from "./cover-model";
import { debrisCleanupProgress } from "./debris-cleanup";
import { addDebrisFade } from "./debris-fade";
import { ARENA, LASER_DEFENSE, MINE_RADIUS, PICKUPS, TEAM_COLORS, VEHICLES } from "./data";
import { healthBarState } from "./health-bar";
import { HarborScenery } from "./harbor-scenery";
import { QuarryDust } from "./quarry-dust";
import { QuarryScenery } from "./quarry-scenery";
import { Flags } from "./flags";
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
import { createArenaFloor, createLighting, createSpawnPads } from "./scenery";
import { VillageScenery } from "./village-scenery";
import type { Simulation } from "./simulation";
import { MAX_FRAGMENTS } from "./simulation-rules";
import { createTankBar, updateTankProtection, type TankBar } from "./tank-bars";
import { TrackTrails } from "./tracks";
import { TrackDust } from "./track-dust";
import { TankSuspension } from "./tank-suspension";
import { setTreeDamage, setTreeDestroyed, trunkFragment } from "./tree-models";
import { TreeDebris } from "./tree-debris";
import type { Cover, Fragment, SimEvent } from "./types";
import { ageWreckMaterial } from "./wreck-aging";
import { rankIndex } from "./veterancy";
import { CAMERA, FEEDBACK } from "./view-settings";
interface PickupModel extends THREE.Group {
  userData: {
    gem: THREE.Object3D;
    ring: THREE.Mesh<THREE.TorusGeometry, THREE.MeshStandardMaterial>;
    refill: THREE.Mesh<THREE.RingGeometry, THREE.MeshBasicMaterial>;
  };
}
function batchTank(model: TankModel): void {
  const d = model.userData;
  batch(d.trackGroup);
  batch(d.hull);
  batch(d.turret);
  batch(d.barrel);
}
/** Move only the cover root; batched children keep their cached local matrices. */
function physicalCoverModel(cover: Cover): THREE.Group {
  const m = cover.motion;
  const group = coverModel(
    m ? { ...cover, x: m.originX, z: m.originZ, w: m.w, d: m.d } : cover,
    "full",
    coverDamageStage(cover),
  );
  if (m) {
    for (const child of group.children) {
      child.position.y -= cover.h / (2 * group.scale.y);
    }
  }
  batch(group);
  if (!m) {
    freezeStatic(group);
  }
  return group;
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
  private suspensions = new Map<number, TankSuspension>();
  coverMeshes = new Map<number, THREE.Group>();
  fragmentMeshes = new Map<number, THREE.Object3D>();
  pickupMeshes = new Map<number, PickupModel>();
  mineMeshes = new Map<number, THREE.Group>();
  bars = new Map<number, TankBar>();
  projectiles = new ProjectileVisuals();
  laserVisuals = new LaserVisuals();
  treeDebris = new TreeDebris();
  private flags = new Flags();
  private villageScenery?: VillageScenery;
  private harborScenery?: HarborScenery;
  private quarryScenery?: QuarryScenery;
  private stressSpawnPads?: THREE.Group;
  private customFloor?: THREE.Mesh;
  private customOuterFloor?: THREE.Mesh;
  private lighting: ReturnType<typeof createLighting>;
  private particleEffects = new ParticleEffects();
  debrisMeshes = new Map<NonNullable<Fragment["shape"]>, THREE.InstancedMesh>();
  playerRing = new THREE.Group();
  spawnPulse: THREE.Mesh;
  spawnCue = 0;
  playerWasAlive = false;
  debrisColor = new THREE.Color();
  private debrisBounds = new THREE.Box3();
  get particles(): readonly Particle[] {
    return this.particleEffects.particles;
  }
  hitUntil = new Map<number, number>();
  pickupEffects: { group: THREE.Group; age: number; tankId?: number }[] = [];
  pickupRingGeometry = new THREE.RingGeometry(0.88, 1, 48);
  pickupGlowGeometry = new THREE.SphereGeometry(1, 16, 10);
  tracks = new TrackTrails();
  trackDust = new TrackDust();
  quarryDust = new QuarryDust();
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
    // Count the main view and water reflection together, including their draw calls.
    this.renderer.info.autoReset = false;
    this.lighting = createLighting(this.scene);
    this.scene.add(this.flash);
    this.scene.add(this.worldGroup);
    this.scene.add(this.tracks.mesh);
    this.scene.add(this.trackDust.mesh, this.trackDust.gravel.mesh);
    this.scene.add(this.quarryDust.mesh);
    this.scene.add(this.flags.group);
    const woodFragment = sidingBox(1.5, 0.18, 0.45, 0xffffff);
    const woodPiece = sidingBox(1, 1, 1, 0xffffff);
    const trunk = trunkFragment();
    const fragmentGeometry = {
      panel: woodPiece.geometry,
      beam: woodPiece.geometry,
      log: trunk.geometry,
      "drum-shell": barrelScrapGeometry("shell"),
      "drum-lid": barrelScrapGeometry("lid"),
      wood: woodFragment.geometry,
      armor: box(1.25, 0.16, 0.85, 0xffffff).geometry,
      wheel: new THREE.CylinderGeometry(0.48, 0.48, 0.28, 10),
      track: box(0.5, 0.2, 1.5, 0xffffff).geometry,
      shard: new THREE.TetrahedronGeometry(0.75),
    };
    for (const shape of Object.keys(fragmentGeometry) as NonNullable<Fragment["shape"]>[]) {
      const mesh = new THREE.InstancedMesh(
        fragmentGeometry[shape],
        shape === "wood"
          ? woodFragment.material
          : shape === "panel" || shape === "beam"
            ? woodPiece.material
            : shape === "log"
              ? trunk.material
              : material(0xffffff),
        MAX_FRAGMENTS,
      );
      addDebrisFade(mesh);
      mesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
      mesh.frustumCulled = false;
      mesh.castShadow = mesh.receiveShadow = true;
      mesh.count = 0;
      this.debrisMeshes.set(shape, mesh);
      this.scene.add(mesh);
    }
    this.scene.add(this.projectiles.group);
    this.scene.add(this.laserVisuals.group);
    this.scene.add(this.treeDebris.group);
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
    this.scene.add(this.particleEffects.mesh, this.particleEffects.explosions.group);
    const reticle = createReticle();
    this.crosshair = reticle.crosshair;
    this.reticleInk = reticle.ink;
    this.reticleCenter = reticle.center;
    this.scene.add(this.crosshair);
    this.resize();
  }
  reset(simulation: Simulation): void {
    const stress = simulation.customMap?.id === "stress-test";
    if (stress && !this.stressSpawnPads) {
      this.stressSpawnPads = createSpawnPads();
      this.scene.add(this.stressSpawnPads);
    }
    if (this.stressSpawnPads) {
      this.stressSpawnPads.visible = stress;
    }
    const harbor = simulation.mapTheme === "harbor";
    const quarry = simulation.mapTheme === "quarry";
    const village = simulation.mapTheme === "village";
    if (simulation.mapOuterFloor && !this.customOuterFloor) {
      this.customOuterFloor = createArenaFloor(
        this.renderer,
        simulation.mapOuterFloor,
        simulation.mapOuterFloorExtent,
      );
      this.customOuterFloor.position.y = -0.002;
      this.scene.add(this.customOuterFloor);
    }
    if (this.customOuterFloor) {
      this.customOuterFloor.visible = Boolean(simulation.mapOuterFloor);
    }
    if (simulation.mapFloor && !this.customFloor) {
      this.customFloor = createArenaFloor(this.renderer, simulation.mapFloor);
      this.customFloor.position.y = 0.008;
      this.scene.add(this.customFloor);
    }
    if (this.customFloor) {
      this.customFloor.visible = Boolean(simulation.mapFloor);
    }
    if (village && !this.villageScenery) {
      this.villageScenery = new VillageScenery(this.renderer);
      this.scene.add(this.villageScenery);
    }
    if (harbor && !this.harborScenery) {
      this.harborScenery = new HarborScenery();
      this.scene.add(this.harborScenery.group);
    }
    if (this.villageScenery) {
      this.villageScenery.visible = village;
    }
    if (village) {
      this.villageScenery?.setCovers(simulation.covers);
    }
    if (this.harborScenery) {
      this.harborScenery.group.visible = harbor;
    }
    if (quarry && !this.quarryScenery) {
      this.quarryScenery = new QuarryScenery(this.renderer);
      this.scene.add(this.quarryScenery);
    }
    if (this.quarryScenery) {
      this.quarryScenery.visible = quarry;
    }
    // Dusty Dig bakes low and warm: a raking sun, cool shade fill and a pale
    // dusty horizon. Every branch is reassigned on reset so switching maps
    // restores the other themes exactly.
    const sky = quarry ? 0xd3c6ae : harbor ? 0xa7bdc5 : 0xaacbc2;
    this.scene.background = new THREE.Color(sky);
    this.scene.fog = new THREE.Fog(
      sky,
      quarry ? 150 : harbor ? 150 : 210,
      quarry ? 345 : harbor ? 260 : 380,
    );
    this.lighting.sun.color.setHex(quarry ? 0xffcf9c : harbor ? 0xffbf85 : 0xffd59b);
    this.lighting.sun.position.set(
      quarry ? -50 : -45,
      quarry ? 43 : harbor ? 55 : 68,
      quarry ? 28 : 25,
    );
    this.lighting.sun.intensity = quarry ? 3.0 : 2.8;
    this.lighting.fill.intensity = quarry ? 1.1 : 1.65;
    this.lighting.fill.color.setHex(quarry ? 0xb9cff2 : harbor ? 0xafcfee : 0xbdd5f5);
    this.lighting.fill.groundColor.setHex(quarry ? 0x6f7d92 : harbor ? 0x63778e : 0x75859b);
    disposeOwned(this.worldGroup);
    this.worldGroup.clear();
    this.tankMeshes.clear();
    this.suspensions.clear();
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
    this.treeDebris.reset();
    for (const effect of this.pickupEffects) {
      this.scene.remove(effect.group);
      disposeOwned(effect.group);
    }
    this.pickupEffects = [];
    this.tracks.reset();
    this.trackDust.reset();
    this.quarryDust.reset();
    this.quarryDust.mesh.visible = quarry;
    for (const mesh of this.debrisMeshes.values()) {
      mesh.count = 0;
    }
    this.playerWasAlive = false;
    for (const cover of simulation.covers) {
      const group = physicalCoverModel(cover);
      this.coverMeshes.set(cover.id, group);
      this.worldGroup.add(group);
    }
    for (const tank of simulation.tanks) {
      const model = tankModel(tank.kind, tank.team);
      batchTank(model);
      this.tankMeshes.set(tank.id, model);
      this.worldGroup.add(model);
      this.makeBar(tank.id, tank.team);
    }
    for (const pickup of simulation.pickups) {
      const group = new THREE.Group() as PickupModel;
      const def = PICKUPS[pickup.kind];
      put(group, cylinder(1.05, 0.12, 0x25435f, 24), 0, 0.08, 0);
      const ring = new THREE.Mesh(new THREE.TorusGeometry(0.94, 0.045, 5, 24), material(def.color));
      ring.geometry.userData.owned = true;
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
    // Startup no longer renders a preview frame to establish the aiming camera.
    this.updateCamera(simulation, 1, false);
  }
  makeBar(id: number, team: number): void {
    const bar = createTankBar(team);
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
    // Contact telemetry is available for future material-specific sounds/effects.
    if (event.type === "debris-impact") {
      return;
    }
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
  private updateTanks(simulation: Simulation, alpha: number, dt: number): void {
    for (const tank of simulation.tanks) {
      let group = this.tankMeshes.get(tank.id);
      // Reinforcements arrive after reset, so create their visuals on first render.
      if (!group || group.userData.kind !== tank.kind) {
        this.suspensions.delete(tank.id);
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
        this.makeBar(tank.id, tank.team);
      }
      const bar = this.bars.get(tank.id)!;
      bar.visible = tank.alive;
      updateTankProtection(bar, tank);
      if (!tank.alive) {
        this.hitUntil.delete(tank.id);
        this.suspensions.delete(tank.id);
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
      const velocity = tank.body.linvel();
      let suspension = this.suspensions.get(tank.id);
      if (!suspension) {
        suspension = new TankSuspension();
        this.suspensions.set(tank.id, suspension);
      }
      suspension.update(
        velocity.x,
        velocity.z,
        tank.heading,
        simulation.elapsed,
        simulation.match.phase === "playing" ? dt : 0,
      );
      // The turret rides the hull's tilted ring, then rotates to its independent aim.
      // Keep suspension visual-only; the barrel inherits tilt and retains its recoil.
      group.userData.hull.rotation.set(
        suspension.pitch.angle,
        tank.heading,
        suspension.roll.angle,
        "YXZ",
      );
      group.userData.turret.quaternion.copy(group.userData.hull.quaternion);
      group.userData.turret.rotateY(tank.aim - tank.heading);
      group.userData.barrel.position.z = -tank.recoil * 0.2;
      group.userData.trackGroup.position.z =
        tank.kind === "humvee" ? 0 : (this.time * Math.hypot(velocity.x, velocity.z) * 0.4) % 0.25;
      group.scale.setScalar(VEHICLES[tank.kind].scale);
      bar.position.set(group.position.x, tank.human ? 2.85 : 2.15, group.position.z);
      bar.quaternion.copy(this.camera.quaternion);
      const health = healthBarState(tank.hp, simulation.maxHealth(tank), tank.team);
      bar.userData.fg.scale.x = health.ratio;
      bar.userData.fg.visible = health.ratio > 0;
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
      const damageStage = coverDamageStage(cover);
      if (
        !group ||
        (cover.alive &&
          ((group.userData.damageStage ?? 0) !== damageStage ||
            (cover.kind === "timber" &&
              (group.userData.timberHitCount ?? 0) !== (cover.timberHits?.length ?? 0))))
      ) {
        if (group) {
          disposeOwned(group);
          this.worldGroup.remove(group);
        }
        group = physicalCoverModel(cover);
        this.coverMeshes.set(cover.id, group);
        this.worldGroup.add(group);
      }
      // Destruction removes a movable cover's Rapier body immediately. Never read a transform
      // from that invalid handle; doing so traps inside WASM and stops the entire render loop.
      if (cover.motion && cover.alive) {
        const p = cover.body.translation();
        const q = cover.body.rotation();
        group.position.set(p.x, p.y, p.z);
        group.quaternion.set(q.x, q.y, q.z, q.w);
      }
      if (cover.kind === "tree") {
        if (cover.alive) {
          setTreeDamage(group, cover.hp / cover.maxHp, this.treeDebris.shed);
        }
        setTreeDestroyed(group, stump);
      }
      group.visible = cover.alive || stump;
    }
  }
  private updatePickups(simulation: Simulation, dt: number): void {
    for (const pickup of simulation.pickups) {
      const group = this.pickupMeshes.get(pickup.id)!;
      const { refill, ring } = group.userData;
      group.visible = true;
      group.userData.gem.visible = pickup.available;
      ring.material.opacity = pickup.available ? 1 : 0.2;
      refill.visible = !pickup.available;
      const fallbackDuration =
        pickup.kind === "laser" ? LASER_DEFENSE.initialDelay : AMMO_RESPAWN_SECONDS;
      const cooldownDuration = pickup.cooldownDuration || fallbackDuration;
      const progress = THREE.MathUtils.clamp(1 - pickup.cooldown / cooldownDuration, 0, 1);
      refill.geometry.setDrawRange(0, Math.floor(progress * 48) * 6);
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
      const cleanup = debrisCleanupProgress(f.life);
      if (!f.wreck && !f.timberPart && f.treeCoverId === undefined) {
        const mesh = this.debrisMeshes.get(f.shape ?? "shard")!;
        if (mesh.count >= MAX_FRAGMENTS) {
          continue;
        }
        this.dummy.position.set(pos.x, pos.y, pos.z);
        this.dummy.quaternion.set(q.x, q.y, q.z, q.w);
        const scale = f.size;
        if (f.dimensions) {
          this.dummy.scale.set(
            f.dimensions.x * scale,
            f.dimensions.y * scale,
            f.dimensions.z * scale,
          );
        } else {
          this.dummy.scale.setScalar(scale);
        }
        this.dummy.updateMatrix();
        // Project the piece's bounds onto world Y, including its resting rotation.
        // A flat panel should descend by its thickness, not by a whole metre.
        const bounds = mesh.geometry.boundingBox!;
        const e = this.dummy.matrix.elements;
        const height =
          Math.abs(e[1]) * (bounds.max.x - bounds.min.x) +
          Math.abs(e[5]) * (bounds.max.y - bounds.min.y) +
          Math.abs(e[9]) * (bounds.max.z - bounds.min.z);
        e[13] -= cleanup * (height + 0.03);
        mesh.setMatrixAt(mesh.count, this.dummy.matrix);
        mesh.geometry.getAttribute("debrisOpacity").setX(mesh.count, 1 - cleanup);
        mesh.setColorAt(mesh.count++, this.debrisColor.set(f.color));
        continue;
      }
      let g = this.fragmentMeshes.get(f.id);
      if (!g) {
        if (f.timberPart) {
          g = timberPartModel(f.timberPart);
        } else if (f.treeCoverId !== undefined) {
          const source = this.coverMeshes.get(f.treeCoverId)?.userData.crown as
            THREE.Group | undefined;
          if (!source) {
            continue;
          }
          const crown = source.clone(true);
          crown.visible = true;
          crown.position.set(0, -(f.treeCenterY ?? 0), 0);
          crown.traverse((object) => {
            object.matrixWorldAutoUpdate = true;
            object.matrixAutoUpdate = true;
            if (isMesh(object) && object.geometry.userData.owned) {
              object.geometry = object.geometry.clone();
            }
          });
          g = new THREE.Group();
          g.add(crown);
        } else {
          g = wreckModel(f.wreck!, f.team ?? 0, f.part ?? "hull");
        }
        g.traverse((o) => {
          if (!isMesh(o)) {
            return;
          }
          const clone = (material: THREE.Material) => {
            const copy = material.clone();
            // Keep wreck surfaces and ground decals correctly occluded, including
            // during cleanup. Alpha hashing fades without transparent mesh sorting.
            copy.transparent = false;
            copy.depthWrite = true;
            copy.alphaHash = true;
            copy.userData.owned = true;
            return copy;
          };
          o.material = Array.isArray(o.material) ? o.material.map(clone) : clone(o.material);
        });
        this.fragmentMeshes.set(f.id, g);
        this.worldGroup.add(g);
      }
      g.position.set(pos.x, pos.y, pos.z);
      g.quaternion.set(q.x, q.y, q.z, q.w);
      const remaining = 1 - cleanup;
      g.scale.setScalar(f.wreck ? VEHICLES[f.wreck].scale : 1);
      if (cleanup > 0) {
        if (g.userData.sinkDepth === undefined) {
          this.debrisBounds.setFromObject(g);
          g.userData.sinkDepth = this.debrisBounds.max.y - this.debrisBounds.min.y + 0.03;
        }
        g.position.y -= cleanup * g.userData.sinkDepth;
      }
      g.traverse((o) => {
        if (!isMesh(o)) {
          return;
        }
        const materials = Array.isArray(o.material) ? o.material : [o.material];
        for (const material of materials) {
          material.opacity = remaining;
          if (f.wreck) {
            ageWreckMaterial(material, simulation.elapsed - (f.createdAt ?? simulation.elapsed));
          }
        }
        o.castShadow = remaining === 1;
      });
    }
    for (const mesh of this.debrisMeshes.values()) {
      (mesh.geometry.getAttribute("debrisOpacity") as THREE.InstancedBufferAttribute).needsUpdate =
        true;
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
    this.flags.update(this.time);
    if (this.harborScenery?.group.visible) {
      this.harborScenery.update(this.time);
    }
    if (this.villageScenery?.visible) {
      this.villageScenery.update(this.time);
    }
    this.updatePickupEffects(simulation, alpha, dt);
    this.tracks.update(simulation, alpha);
    this.trackDust.update(simulation);
    if (this.quarryScenery?.visible) {
      this.quarryDust.update(simulation, dt);
    }
    this.updateCamera(simulation, alpha, overview);
    this.updatePlayerIndicators(simulation, alpha, dt, overview);
    this.updateTanks(simulation, alpha, dt);
    this.treeDebris.update(dt);
    this.updateCover(simulation);
    this.updatePickups(simulation, dt);
    this.updateFragments(simulation);
    this.updateMines(simulation);
    this.projectiles.update(simulation.shots, this.time);
    this.laserVisuals.update(simulation, alpha, dt);
    this.particleEffects.update(dt, this.time);
    this.crosshair.visible = simulation.match.phase === "playing";
    this.renderer.info.reset();
    this.renderer.render(this.scene, this.camera);
  }
}

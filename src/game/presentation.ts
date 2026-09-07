import { pickupCube } from "./pickup-visuals";
import { healthBarState } from "./health-bar";
import { TrackTrails } from "./tracks";
import { weaponInterval } from "./weapons";
import * as THREE from "three";
import { batch, freezeStatic } from "./batching";
import { groundMaterial, groundUVs, roadGeometry } from "./ground-surfaces";
import {
  box,
  put,
  cylinder,
  material,
  tankModel,
  wreckModel,
  coverModel,
  stumpModel,
  teamTexture,
  type TankModel,
} from "./models";
import { ARENA, TEAM_COLORS, PICKUPS, VEHICLES, MINE_RADIUS } from "./data";
import { spawnPositions } from "./arena";
import type { Simulation } from "./simulation";
import type { SimEvent, Fragment } from "./types";
interface Particle {
  shape?: "leaf" | "splinter";
  x: number;
  y: number;
  z: number;
  vx: number;
  vy: number;
  vz: number;
  life: number;
  max: number;
  size: number;
  color: THREE.Color;
}
// Life and size pairs are [minimum, random span]. Choose once per event.
const PARTICLE_STYLES = {
  tree: { count: 96, life: [0.85, 0.9], size: [0.18, 0.25],
    speed: 7, scatter: 1.5, height: 0.6, lift: 1 },
  pickup: { count: 24, life: [0.5, 0.3], size: [0.12, 0.1],
    speed: 5, scatter: 0, height: 1.3, lift: 4 },
  explosion: { count: 18, life: [0.35, 0.45], size: [0.22, 0.5],
    speed: 1.2, scatter: 0, height: 0.8, lift: 0 },
  hurt: { count: 12, life: [0.22, 0.16], size: [0.09, 0.09],
    speed: 6, scatter: 0.9, height: 2.1, lift: 1.5 },
  impact: { count: 8, life: [0.1, 0.2], size: [0.04, 0.09],
    speed: 4, scatter: 0, height: 1, lift: 0 },
};
function updateInstances(mesh: THREE.InstancedMesh) {
  if (!mesh.count) return;
  mesh.instanceMatrix.clearUpdateRanges();
  mesh.instanceMatrix.addUpdateRange(0, mesh.count * 16);
  mesh.instanceMatrix.needsUpdate = true;
  if (mesh.instanceColor) {
    mesh.instanceColor.clearUpdateRanges();
    mesh.instanceColor.addUpdateRange(0, mesh.count * 3);
    mesh.instanceColor.needsUpdate = true;
  }
}
function disposeOwned(g: THREE.Object3D) {
  g.traverse((o) => {
    if (o instanceof THREE.Mesh && o.geometry.userData.owned)
      o.geometry.dispose();
    if (
      (o instanceof THREE.Mesh || o instanceof THREE.Sprite) &&
      !Array.isArray(o.material) &&
      o.material.userData.owned
    )
      o.material.dispose();
  });
}
function batchTank(g: TankModel) {
  const d = g.userData;
  batch(d.trackGroup);
  batch(d.hull);
  batch(d.turret);
  batch(d.barrel);
}
export class Presentation {
  renderer: THREE.WebGLRenderer;
  scene = new THREE.Scene();
  camera = new THREE.PerspectiveCamera(43, 1, 0.1, 320);
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
  pickupMeshes = new Map<number, THREE.Group>();
  mineMeshes = new Map<number, THREE.Group>();
  bars = new Map<number, THREE.Group>();
  shotMesh: THREE.InstancedMesh;
  shotCore: THREE.InstancedMesh;
  shotOutline: THREE.InstancedMesh;
  particlesMesh: THREE.InstancedMesh;
  debrisMeshes = new Map<NonNullable<Fragment["shape"]>, THREE.InstancedMesh>();
  playerRing = new THREE.Group();
  spawnPulse: THREE.Mesh;
  spawnCue = 0;
  playerWasAlive = false;
  debrisColor = new THREE.Color();
  particles: Particle[] = [];
  hitUntil = new Map<number, number>();
  pickupEffects: { group: THREE.Group; age: number; tankId?: number }[] = [];
  pickupRingGeometry = new THREE.RingGeometry(0.88, 1, 48);
  pickupGlowGeometry = new THREE.SphereGeometry(1, 16, 10);
  tracks = new TrackTrails();
  dummy = new THREE.Object3D();
  follow = new THREE.Vector3();
  zoom = 34;
  time = 0;
  flash = new THREE.PointLight(0xffc178, 0, 20, 2);
  crosshair = new THREE.Group();
  constructor(public canvas: HTMLCanvasElement) {
    this.renderer = new THREE.WebGLRenderer({
      canvas,
      antialias: true,
      powerPreference: "high-performance",
    });
    this.renderer.setPixelRatio(Math.min(devicePixelRatio, 1.5));
    this.renderer.shadowMap.enabled = true;
    this.renderer.shadowMap.type = THREE.PCFShadowMap;
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.renderer.toneMappingExposure = 1.0;
    this.scene.background = new THREE.Color(0x59bbed);
    this.scene.fog = new THREE.Fog(0x59bbed, 150, 260);
    // Cooler fill and a neutral sun preserve paint colors and give cover more depth.
    this.scene.add(new THREE.HemisphereLight(0xe2efff, 0x918571, 1.65));
    const sun = new THREE.DirectionalLight(0xfff1df, 2.8);
    sun.position.set(-45, 85, 25);
    sun.castShadow = true;
    sun.shadow.mapSize.set(2048, 2048);
    sun.shadow.camera.left = -(ARENA + 10);
    sun.shadow.camera.right = ARENA + 10;
    sun.shadow.camera.top = ARENA + 10;
    sun.shadow.camera.bottom = -(ARENA + 10);
    sun.shadow.camera.far = 220;
    sun.shadow.normalBias = 0.05;
    sun.shadow.bias = -0.0002;
    this.scene.add(sun);
    this.scene.add(this.flash);
    this.scene.add(this.worldGroup);
    this.scene.add(this.tracks.mesh);
    const board = box(ARENA * 2 + 6, 1.2, ARENA * 2 + 6, 0x947c4d, 0.4);
    put(this.scene, board, 0, -0.8, 0);
    const floor = box(ARENA * 2, 0.15, ARENA * 2, 0xffdb92, 0.03);
    floor.material = groundMaterial(this.renderer, "dry-grass");
    groundUVs(floor.geometry);
    put(this.scene, floor, 0, -0.07, 0);
    const outskirts = new THREE.Mesh(
      new THREE.BoxGeometry(180, 0.15, 180),
      floor.material,
    );
    outskirts.receiveShadow = true;
    groundUVs(outskirts.geometry);
    put(this.scene, outskirts, 0, -0.9, 0);
    this.createYardDetails();
    const fragmentGeometry = {
      armor: box(1.25, 0.16, 0.85, 0xffffff).geometry,
      wheel: new THREE.CylinderGeometry(0.48, 0.48, 0.28, 10),
      track: box(0.5, 0.2, 1.5, 0xffffff).geometry,
      shard: new THREE.TetrahedronGeometry(0.75),
    };
    for (const shape of Object.keys(fragmentGeometry) as NonNullable<
      Fragment["shape"]
    >[]) {
      const mesh = new THREE.InstancedMesh(
        fragmentGeometry[shape],
        material(0xffffff),
        80,
      );
      mesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
      mesh.frustumCulled = false;
      mesh.castShadow = mesh.receiveShadow = true;
      mesh.count = 0;
      this.debrisMeshes.set(shape, mesh);
      this.scene.add(mesh);
    }
    const shellGeometry = new THREE.SphereGeometry(0.1575, 8, 6);
    const shellLayer = (color: number) => {
      const mesh = new THREE.InstancedMesh(
        shellGeometry,
        new THREE.MeshBasicMaterial({ color, toneMapped: false }),
        600,
      );
      mesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
      mesh.frustumCulled = false;
      mesh.count = 0;
      this.scene.add(mesh);
      return mesh;
    };
    this.shotMesh = shellLayer(0xffffff);
    this.shotCore = shellLayer(0xfffbed);
    this.shotOutline = shellLayer(0x283245);
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
    this.particlesMesh = new THREE.InstancedMesh(
      new THREE.IcosahedronGeometry(1, 0),
      new THREE.MeshBasicMaterial({ color: 0xffffff }),
      1200,
    );
    this.particlesMesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    this.particlesMesh.frustumCulled = false;
    this.scene.add(this.particlesMesh);
    // Two-tone reticle stays legible over bright ground, tank paint and cover.
    const reticleMaterial = (color: number) =>
      new THREE.MeshBasicMaterial({
        color,
        side: THREE.DoubleSide,
        depthTest: false,
        depthWrite: false,
        transparent: true,
        toneMapped: false,
      });
    const outline = reticleMaterial(0x12263c),
      ink = reticleMaterial(0xfff9da);
    const ring = (
      inner: number,
      outer: number,
      mat: THREE.Material,
      order: number,
    ) => {
      const mesh = new THREE.Mesh(
        new THREE.RingGeometry(inner, outer, 40),
        mat,
      );
      mesh.rotation.x = -Math.PI / 2;
      mesh.renderOrder = order;
      this.crosshair.add(mesh);
    };
    ring(0.4, 0.64, outline, 50);
    ring(0.46, 0.57, ink, 51);
    for (const [x, z] of [
      [-0.83, 0],
      [0.83, 0],
      [0, -0.83],
      [0, 0.83],
    ]) {
      for (const back of [true, false]) {
        const length = back ? 0.43 : 0.31,
          width = back ? 0.18 : 0.075;
        const mesh = new THREE.Mesh(
          new THREE.PlaneGeometry(
            x === 0 ? width : length,
            x === 0 ? length : width,
          ),
          back ? outline : ink,
        );
        mesh.rotation.x = -Math.PI / 2;
        mesh.renderOrder = back ? 50 : 51;
        put(this.crosshair, mesh, x, 0, z);
      }
    }
    const center = new THREE.Mesh(
      new THREE.CircleGeometry(0.075, 16),
      reticleMaterial(0xffdf38),
    );
    center.rotation.x = -Math.PI / 2;
    center.renderOrder = 52;
    this.crosshair.add(center);
    this.crosshair.position.y = 1.05;
    this.scene.add(this.crosshair);
    this.resize();
  }
  createYardDetails() {
    const details = new THREE.Group();
    const roads = new THREE.Group();
    const roadMaterial = groundMaterial(this.renderer, "packed-dirt");
    roadMaterial.vertexColors = true;
    roadMaterial.transparent = true;
    roadMaterial.depthWrite = false;
    const road = (w: number, d: number, x: number, z: number, y: number) => {
      const geometry = roadGeometry(w, d, x, z);
      put(roads, new THREE.Mesh(geometry, roadMaterial), x, y, z);
    };
    // Broad village roads retain the roomy midfield and outer flanking circuits.
    for (const x of [-52, 0, 52])
      road(x === 0 ? 18 : 10, ARENA * 2 - 2, x, 0, 0.0425);
    for (const z of [-38, 0, 38])
      road(ARENA * 2 - 2, z === 0 ? 12 : 8, 0, z, 0.0625);
    batch(roads);
    for (const mesh of roads.children) {
      mesh.castShadow = false;
      // Ground blending must precede shields, pickup glows and track decals.
      mesh.renderOrder = -1;
    }
    this.scene.add(roads);
    freezeStatic(roads);
    const spawnRimGeometry = new THREE.RingGeometry(
      2.05,
      2.3,
      12,
      1,
      0.06,
      Math.PI / 4 - 0.12,
    ).rotateX(-Math.PI / 2);
    const arrowShape = new THREE.Shape();
    arrowShape.moveTo(-0.28, -0.55);
    arrowShape.lineTo(0.28, 0);
    arrowShape.lineTo(-0.28, 0.55);
    arrowShape.lineTo(-0.48, 0.37);
    arrowShape.lineTo(-0.1, 0);
    arrowShape.lineTo(-0.48, -0.37);
    arrowShape.closePath();
    const spawnArrowGeometry = new THREE.ShapeGeometry(arrowShape).rotateX(
      -Math.PI / 2,
    );
    for (const team of [0, 1] as const) {
      const side = team === 0 ? -1 : 1,
        color = TEAM_COLORS[team];
      for (const p of spawnPositions(team)) {
        // Low octagonal deployment plinth with a recessed deck and segmented team lights.
        put(details, cylinder(2.75, 0.1, 0x283c4e, 8), p.x, 0.08, p.z);
        put(details, cylinder(2.52, 0.045, 0x718898, 8), p.x, 0.135, p.z);
        put(details, cylinder(2.37, 0.035, 0x223d51, 32), p.x, 0.17, p.z);
        put(details, cylinder(1.98, 0.025, 0x455e70, 8), p.x, 0.193, p.z);
        for (let i = 0; i < 8; i++) {
          const angle = (i * Math.PI) / 4;
          const segment = new THREE.Mesh(spawnRimGeometry, material(color));
          segment.rotation.y = angle;
          put(details, segment, p.x, 0.198, p.z);
          put(
            details,
            cylinder(0.075, 0.025, 0xc9d6dd, 8),
            p.x + Math.cos(angle) * 2.58,
            0.175,
            p.z + Math.sin(angle) * 2.58,
          );
        }
        for (const z of [-1.25, 1.25])
          for (let i = 0; i < 5; i++)
            put(
              details,
              box(0.18, 0.02, 0.4, 0x1a2b3c, 0.005),
              p.x - 0.52 + i * 0.26,
              0.218,
              p.z + z,
            );
        // Concentric paint and inward chevrons make the pad legible when unoccupied.
        const badge = box(0.7, 0.025, 0.7, color, 0.035);
        badge.rotation.y = Math.PI / 4;
        put(details, badge, p.x, 0.22, p.z);
        for (const offset of [3.1, 3.8]) {
          const arrow = new THREE.Mesh(spawnArrowGeometry, material(color));
          arrow.rotation.y = team === 0 ? 0 : Math.PI;
          put(details, arrow, p.x - side * offset, 0.09, p.z);
        }
        // Team pennants sit behind the spawn line, outside the playable boundary.
        put(details, cylinder(0.055, 4.8, 0x68523b, 8), side * 62, 2.4, p.z);
        put(
          details,
          box(0.04, 0.9, 1.4, color, 0.01),
          side * 62,
          4.15,
          p.z + 0.6,
        );
      }
      for (let z = -57; z <= 57; z += 2) {
        put(details, box(0.16, 1.7, 0.16, color, 0.015), side * ARENA, 2.7, z);
        put(details, box(0.13, 0.16, 2.2, color, 0.01), side * ARENA, 3.0, z);
      }
    }
    // Low tufts add terrain detail without obscuring shots or pretending to be solid cover.
    for (let i = 0; i < 160; i++) {
      const x = Math.sin(i * 71.3) * 57,
        z = Math.sin(i * 39.7 + 2) * 57;
      if (
        Math.abs(x) < 11 ||
        Math.abs(x) > 46 ||
        Math.abs(z) < 8 ||
        Math.abs(Math.abs(z) - 38) < 6
      )
        continue;
      const tuft = new THREE.Mesh(
        new THREE.ConeGeometry(0.13, 0.35, 3),
        material(i % 2 ? 0x8caa54 : 0xd6c880),
      );
      put(details, tuft, x, 0.18, z);
    }
    batch(details);
    this.scene.add(details);
    freezeStatic(details);
    // The tree line frames the board; in-arena trees have matching cover colliders.
    for (const side of [-1, 1]) {
      const row = new THREE.Group();
      for (let i = 0; i < 12; i++) {
        const tree = coverModel({
          kind: "tree",
          x: -57 + i * 10.2,
          z: side * 65,
          w: 3.8,
          d: 3.8,
          h: 6 + (i % 3),
          color: 0x19935c,
        } as Parameters<typeof coverModel>[0], "background");
        tree.position.y = -0.8;
        tree.updateMatrix();
        for (const mesh of [...tree.children]) {
          mesh.applyMatrix4(tree.matrix);
          row.add(mesh);
        }
      }
      batch(row);
      this.scene.add(row);
      freezeStatic(row);
    }
  }
  reset(s: Simulation) {
    disposeOwned(this.worldGroup);
    this.worldGroup.clear();
    this.tankMeshes.clear();
    this.coverMeshes.clear();
    this.fragmentMeshes.clear();
    this.pickupMeshes.clear();
    this.mineMeshes.clear();
    this.bars.clear();
    this.particles = [];
    this.hitUntil.clear();
    for (const effect of this.pickupEffects) {
      this.scene.remove(effect.group);
      disposeOwned(effect.group);
    }
    this.pickupEffects = [];
    this.tracks.reset();
    for (const mesh of this.debrisMeshes.values()) mesh.count = 0;
    this.playerWasAlive = false;
    for (const c of s.covers) {
      const g = coverModel(c);
      batch(g);
      this.coverMeshes.set(c.id, g);
      this.worldGroup.add(g);
      freezeStatic(g);
    }
    for (const t of s.tanks) {
      const g = tankModel(t.kind, t.team);
      batchTank(g);
      this.tankMeshes.set(t.id, g);
      this.worldGroup.add(g);
      this.makeBar(t.id, t.team, t.human);
    }
    for (const p of s.pickups) {
      const g = new THREE.Group(),
        def = PICKUPS[p.kind];
      put(g, cylinder(1.05, 0.12, 0x25435f, 24), 0, 0.08, 0);
      const ring = new THREE.Mesh(
        new THREE.TorusGeometry(0.94, 0.045, 5, 24),
        material(def.color),
      );
      ring.geometry.userData.owned = true;
      ring.rotation.x = Math.PI / 2;
      put(g, ring, 0, 0.2, 0);
      const gem = pickupCube(p.kind);
      gem.rotation.y = Math.PI / 4;
      put(g, gem, 0, 1.25, 0);
      g.userData.gem = gem;
      g.position.set(p.x, 0, p.z);
      this.worldGroup.add(g);
      this.pickupMeshes.set(p.id, g);
    }
    const p = s.human.body.translation();
    this.follow.set(p.x, 0, p.z);
  }
  makeBar(id: number, team: number, human: boolean) {
    const g = new THREE.Group();
    const border = new THREE.Mesh(
      new THREE.PlaneGeometry(1.87, 0.28),
      new THREE.MeshBasicMaterial({ color: 0x9eb8ab, depthTest: false, toneMapped: false }),
    );
    const bg = new THREE.Mesh(
      new THREE.PlaneGeometry(1.81, 0.22),
      new THREE.MeshBasicMaterial({ color: 0x010504, depthTest: false, toneMapped: false }),
    );
    const fg = new THREE.Mesh(
      new THREE.PlaneGeometry(1.75, 0.16).translate(1.75 / 2, 0, 0),
      new THREE.MeshBasicMaterial({
        color: TEAM_COLORS[team],
        depthTest: false,
        toneMapped: false,
      }),
    );
    bg.position.z = 0.005;
    fg.position.set(-1.75 / 2, 0, 0.01);
    border.renderOrder = 10; bg.renderOrder = 11; fg.renderOrder = 12;
    g.add(border, bg, fg);
    if (!human) {
      const icon = new THREE.Sprite(
        new THREE.SpriteMaterial({
          toneMapped: false,
          map: teamTexture(team),
          depthTest: false,
        }),
      );
      icon.scale.set(0.8, 0.4, 1);
      icon.position.y = 0.35;
      g.add(icon);
    }
    g.userData.fg = fg;
    const ammo = new THREE.Mesh(
      new THREE.PlaneGeometry(0.5, 0.045),
      new THREE.MeshBasicMaterial({ color: 0xf1d286, depthTest: false }),
    );
    ammo.position.y = -0.21;
    ammo.renderOrder = 13;
    g.add(ammo);
    g.userData.ammo = ammo;
    g.traverse((o) => {
      if (o instanceof THREE.Mesh || o instanceof THREE.Sprite) {
        // Transparent terrain is drawn after opaque meshes regardless of their
        // renderOrder. Keep all world-space HUD layers in the later pass too.
        const mat = o.material as THREE.Material;
        mat.transparent = true;
        mat.depthWrite = false;
        if (o instanceof THREE.Sprite) o.renderOrder = 13;
      }
      if (o instanceof THREE.Mesh) {
        o.geometry.userData.owned = true;
        (o.material as THREE.Material).userData.owned = true;
      }
      if (o instanceof THREE.Sprite) o.material.userData.owned = true;
    });
    this.bars.set(id, g);
    this.worldGroup.add(g);
  }
  resize(width = innerWidth, height = innerHeight, exact = false) {
    this.renderer.setPixelRatio(exact ? 1 : Math.min(devicePixelRatio, 1.5));
    this.renderer.setSize(width, height, !exact);
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
  }
  aim(nx: number, ny: number) {
    this.raycaster.setFromCamera(this.pointer.set(nx, ny), this.camera);
    const p = this.aimPoint;
    this.raycaster.ray.intersectPlane(this.groundPlane, p);
    this.crosshair.position.x = p.x;
    this.crosshair.position.z = p.z;
    return p;
  }
  event(e: SimEvent) {
    if ((e.type === "death" || e.type === "respawn") && e.id !== undefined)
      this.hitUntil.delete(e.id);
    const hurt = e.type === "hurt";
    if (hurt) {
      if (e.id === undefined || (e.size ?? 0) <= 0) return;
      this.hitUntil.set(e.id, this.time + 0.28);
    }
    if (e.type === "respawn") return;
    const pickup = e.type === "pickup";
    if (pickup) {
      if (this.pickupEffects.length >= 24) {
        const oldest = this.pickupEffects.shift()!;
        this.scene.remove(oldest.group); disposeOwned(oldest.group);
      }
      const group = new THREE.Group();
      const ringMaterial = new THREE.MeshBasicMaterial({ color: e.color ?? 0xffffff,
        transparent: true, opacity: 0.9, depthWrite: false, side: THREE.DoubleSide,
        blending: THREE.AdditiveBlending });
      ringMaterial.userData.owned = true;
      const glowMaterial = ringMaterial.clone();
      glowMaterial.opacity = 0.2; glowMaterial.side = THREE.BackSide;
      glowMaterial.userData.owned = true;
      const ring = new THREE.Mesh(this.pickupRingGeometry, ringMaterial);
      ring.rotation.x = -Math.PI / 2;
      ring.position.y = 0.08;
      group.add(ring, new THREE.Mesh(this.pickupGlowGeometry, glowMaterial));
      group.position.set(e.x, 0, e.z);
      this.scene.add(group);
      this.pickupEffects.push({ group, age: 0, tankId: e.id });
    }
    const explosion =
      e.type === "explosion" || e.type === "death" || e.type === "destroy";
    const tree = e.type === "destroy" && e.coverKind === "tree";
    const style = PARTICLE_STYLES[tree ? "tree" : pickup ? "pickup" : explosion ? "explosion" : hurt ? "hurt" : "impact"];
    const count = e.type === "shot" ? 5 : style.count;
    const baseSpeed = style.speed * (explosion && !tree ? e.size ?? 3 : 1);
    const colors = tree
      ? Array.from({ length: 12 }, (_, i) => i % 4 === 0 ? 0x98633e : [0x175e3b, 0x2c9452, e.color ?? 0x389b58][i % 3])
      : pickup ? [0xffffff, e.color ?? 0xffffff, e.color ?? 0xffffff, e.color ?? 0xffffff]
      : explosion ? [0x536779, 0xff9250, 0xffc569, 0x536779, 0xffc569, 0xff9250]
      : hurt ? [0xffffff, 0xffcb58, 0xffcb58] : [e.color ?? 0xffdf91];
    for (let i = 0; i < count && this.particles.length < 1200; i++) {
      const life = style.life[0] + Math.random() * style.life[1];
      const speed = baseSpeed + (tree ? Math.random() * 4 : 0);
      this.particles.push({
        shape: tree ? (i % 4 === 0 ? "splinter" : "leaf") : undefined,
        x: e.x + (Math.random() - 0.5) * style.scatter,
        y: style.height + (tree ? Math.random() * (e.height ?? 5) * 0.85 : 0),
        z: e.z + (Math.random() - 0.5) * style.scatter,
        vx: (Math.random() - 0.5) * speed,
        vy: style.lift + Math.random() * (tree ? 5 : speed),
        vz: (Math.random() - 0.5) * speed,
        life,
        max: life,
        size: style.size[0] + Math.random() * style.size[1],
        color: new THREE.Color(colors[i % colors.length]),
      });
    }
    if (explosion && !tree) {
      this.flash.position.set(e.x, 3, e.z);
      this.flash.intensity = 45;
    }
  }
  render(s: Simulation, alpha: number, dt: number, overview = false) {
    this.time += dt;
    for (let i = this.pickupEffects.length - 1; i >= 0; i--) {
      const effect = this.pickupEffects[i];
      effect.age += dt;
      const progress = effect.age / 0.8;
      if (progress >= 1) {
        this.scene.remove(effect.group); disposeOwned(effect.group);
        this.pickupEffects.splice(i, 1); continue;
      }
      const [ring, glow] = effect.group.children as THREE.Mesh<THREE.BufferGeometry, THREE.MeshBasicMaterial>[];
      ring.scale.setScalar(1 + progress * 3);
      ring.material.opacity = 0.85 * (1 - progress) ** 2;
      const tank = s.tanks.find((t) => t.id === effect.tankId && t.alive);
      glow.visible = !!tank;
      if (tank) {
        const pos = tank.body.translation();
        glow.position.set(
          THREE.MathUtils.lerp(tank.previous.x, pos.x, alpha) - effect.group.position.x,
          1.1,
          THREE.MathUtils.lerp(tank.previous.z, pos.z, alpha) - effect.group.position.z,
        );
        glow.scale.set(1.65, 1.25, 1.9).multiplyScalar(VEHICLES[tank.kind].scale * (1 + progress * 0.15));
        glow.material.opacity = 0.2 * (1 - progress) ** 2;
      }
    }
    this.tracks.update(s, alpha);
    const p = s.human.alive ? s.human.body.translation() : s.human.previous;
    // Follow the same interpolated pose as the tank, with no edge clamp or trailing lag.
    this.follow.set(
      overview ? 0 : THREE.MathUtils.lerp(s.human.previous.x, p.x, alpha),
      overview ? 0 : 0.7,
      overview ? 0 : THREE.MathUtils.lerp(s.human.previous.z, p.z, alpha),
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
      this.raycaster.setFromCamera(this.pointer.set(i % 2 ? 0.8 : -0.8, i < 2 ? -0.7 : 0.65), this.camera);
      this.raycaster.ray.intersectPlane(this.floorPlane, corners[i]);
    }
    const bounds = s.wreckView ??= { minX: 0, maxX: 0, minZ: 0, maxZ: 0 };
    bounds.minX = Math.max(corners[0].x, corners[2].x);
    bounds.maxX = Math.min(corners[1].x, corners[3].x);
    bounds.minZ = corners[2].z;
    bounds.maxZ = corners[0].z;
    this.flash.intensity *= Math.exp(-dt * 12);
    if (s.human.alive && !this.playerWasAlive) this.spawnCue = 2.5;
    this.playerWasAlive = s.human.alive;
    this.spawnCue = Math.max(0, this.spawnCue - dt);
    this.playerRing.visible = s.human.alive && !overview;
    this.playerRing.position.set(
      THREE.MathUtils.lerp(s.human.previous.x, p.x, alpha),
      0,
      THREE.MathUtils.lerp(s.human.previous.z, p.z, alpha),
    );
    this.playerRing.scale.setScalar(VEHICLES[s.human.kind].scale);
    this.spawnPulse.visible = this.playerRing.visible && this.spawnCue > 0;
    this.spawnPulse.position.copy(this.playerRing.position);
    this.spawnPulse.position.y = 0.14;
    this.spawnPulse.scale.setScalar(1 + ((2.5 - this.spawnCue) % 1.25) * 2);
    (this.spawnPulse.material as THREE.MeshBasicMaterial).opacity =
      Math.min(1, this.spawnCue) * (1 - ((2.5 - this.spawnCue) % 1.25) / 1.25);

    for (const t of s.tanks) {
      let g = this.tankMeshes.get(t.id);
      // Reinforcements arrive after reset, so create their visuals on first render.
      if (!g || g.userData.kind !== t.kind) {
        if (g) {
          disposeOwned(g);
          this.worldGroup.remove(g);
        }
        g = tankModel(t.kind, t.team);
        batchTank(g);
        this.tankMeshes.set(t.id, g);
        this.worldGroup.add(g);
      }
      g.visible = t.alive;
      if (!this.bars.has(t.id)) this.makeBar(t.id, t.team, t.human);
      const bar = this.bars.get(t.id)!;
      bar.visible = t.alive;
      if (!t.alive) {
        this.hitUntil.delete(t.id);
        continue;
      }
      const pos = t.body.translation();
      g.position.set(
        THREE.MathUtils.lerp(t.previous.x, pos.x, alpha),
        pos.y - 0.4,
        THREE.MathUtils.lerp(t.previous.z, pos.z, alpha),
      );
      const hitRemaining = Math.max(0, (this.hitUntil.get(t.id) ?? 0) - this.time);
      const hitFade = hitRemaining / 0.28;
      const hitAge = 0.28 - hitRemaining;
      // Render-only recoil: physics, steering and the camera keep their true pose.
      g.position.x += Math.cos(hitAge * 70) * 0.12 * hitFade;
      g.position.z += Math.sin(hitAge * 55) * 0.09 * hitFade;
      g.rotation.x = Math.sin(hitAge * 60) * 0.035 * hitFade;
      g.rotation.z = Math.cos(hitAge * 65) * 0.045 * hitFade;
      if (hitRemaining === 0) this.hitUntil.delete(t.id);
      g.userData.hull.rotation.y = t.heading;
      g.userData.turret.rotation.y = t.aim;
      g.userData.barrel.position.z = -t.recoil * 0.2;
      const velocity = t.body.linvel();
      g.userData.trackGroup.position.z = (this.time * Math.hypot(velocity.x, velocity.z) * 0.4) % 0.25;
      g.scale.setScalar(VEHICLES[t.kind].scale);
      bar.position.set(g.position.x, t.human ? 3.2 : 2.5, g.position.z);
      bar.quaternion.copy(this.camera.quaternion);
      const health = healthBarState(t.hp, s.maxHealth(t), t.team);
      bar.userData.fg.scale.x = health.ratio;
      bar.userData.fg.visible = health.ratio > 0;
      bar.userData.ammo.scale.x = Math.max(0, 1 - t.cooldown / weaponInterval(t));
      bar.userData.fg.material.color.setHex(health.color);
    }
    for (const c of s.covers) {
      let g = this.coverMeshes.get(c.id);
      const stump = c.kind === "tree" && !c.alive;
      if (!g || !!g.userData.stump !== stump) {
        if (g) {
          disposeOwned(g);
          this.worldGroup.remove(g);
        }
        g = stump ? stumpModel(c) : coverModel(c);
        batch(g);
        this.coverMeshes.set(c.id, g);
        this.worldGroup.add(g);
        freezeStatic(g);
      }
      g.visible = c.alive || stump;
    }
    for (const pickup of s.pickups) {
      const g = this.pickupMeshes.get(pickup.id)!;
      g.visible = pickup.available;
      g.userData.gem.rotation.y += dt;
      g.userData.gem.position.y =
        1.2 + Math.sin(this.time * 2 + pickup.id) * 0.18;
    }
    const fragIds = new Set(s.fragments.map((f) => f.id));
    for (const [id, g] of this.fragmentMeshes)
      if (!fragIds.has(id)) {
        disposeOwned(g);
        this.worldGroup.remove(g);
        this.fragmentMeshes.delete(id);
      }
    for (const mesh of this.debrisMeshes.values()) mesh.count = 0;
    for (const f of s.fragments) {
      const pos = f.body.translation(),
        q = f.body.rotation();
      if (!f.wreck) {
        const mesh = this.debrisMeshes.get(f.shape ?? "shard")!;
        if (mesh.count >= 80) continue;
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
            if (!(o instanceof THREE.Mesh)) return;
            const clone = (material: THREE.Material) => {
              const copy = material.clone();
              copy.transparent = true;
              copy.userData.owned = true;
              return copy;
            };
            o.material = Array.isArray(o.material)
              ? o.material.map(clone)
              : clone(o.material);
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
          if (!(o instanceof THREE.Mesh)) return;
          const materials = Array.isArray(o.material) ? o.material : [o.material];
          for (const material of materials) material.opacity = remaining;
          o.castShadow = remaining === 1;
        });
      }
    }
    for (const mesh of this.debrisMeshes.values()) {
      updateInstances(mesh);
    }
    const mineIds = new Set(s.mines.map((m) => m.id));
    for (const [id, g] of this.mineMeshes)
      if (!mineIds.has(id)) {
        this.worldGroup.remove(g);
        this.mineMeshes.delete(id);
      }
    for (const m of s.mines) {
      let g = this.mineMeshes.get(m.id);
      if (!g) {
        g = new THREE.Group();
        put(g, cylinder(MINE_RADIUS, 0.17, 0x384f47), 0, 0.12, 0);
        put(g, cylinder(0.17, 0.07, TEAM_COLORS[m.team]), 0, 0.24, 0);
        g.position.set(m.x, 0, m.z);
        this.mineMeshes.set(m.id, g);
        this.worldGroup.add(g);
      }
      g.children[1].visible = m.arm > 0 || Math.sin(this.time * 10) > 0;
    }
    this.shotMesh.count =
      this.shotCore.count =
      this.shotOutline.count =
        Math.min(s.shots.length, 600);
    for (let i = 0; i < this.shotMesh.count; i++) {
      const shot = s.shots[i],
        length = shot.weapon === "rocket" ? 3 : 2.25;
      this.dummy.position.set(shot.x, shot.y ?? 1, shot.z);
      this.dummy.rotation.set(0, Math.atan2(shot.vx, shot.vz), 0);
      this.dummy.scale.set(1.15, 1.15, length + 0.2);
      this.dummy.updateMatrix();
      this.shotOutline.setMatrixAt(i, this.dummy.matrix);
      this.dummy.position.y = (shot.y ?? 1) + 0.105;
      this.dummy.scale.set(1, 1, length);
      this.dummy.updateMatrix();
      this.shotMesh.setMatrixAt(i, this.dummy.matrix);
      this.shotMesh.setColorAt(i, this.debrisColor.set(TEAM_COLORS[shot.team]));
      this.dummy.position.y = (shot.y ?? 1) + 0.21;
      this.dummy.scale.set(0.43, 0.43, length * 0.72);
      this.dummy.updateMatrix();
      this.shotCore.setMatrixAt(i, this.dummy.matrix);
    }
    for (const mesh of [this.shotMesh, this.shotCore, this.shotOutline]) updateInstances(mesh);
    let live = 0;
    for (const q of this.particles) {
      q.life -= dt;
      if (q.life <= 0) continue;
      q.x += q.vx * dt;
      q.y += q.vy * dt;
      q.z += q.vz * dt;
      q.vy -= 8 * dt;
      this.particles[live++] = q;
    }
    this.particles.length = live;
    this.particlesMesh.count = this.particles.length;
    for (let i = 0; i < this.particles.length; i++) {
      const q = this.particles[i];
      this.dummy.position.set(q.x, Math.max(0.1, q.y), q.z);
      this.dummy.rotation.set(0, this.time, 0);
      this.dummy.scale.setScalar((q.size * q.life) / q.max);
      if (q.shape) {
        this.dummy.rotation.set(this.time * 5 + i, this.time * 3 + i, this.time * 4);
        this.dummy.scale.x *= q.shape === "leaf" ? 1.5 : 0.4;
        this.dummy.scale.y *= q.shape === "leaf" ? 0.25 : 2.4;
        this.dummy.scale.z *= q.shape === "leaf" ? 0.8 : 0.4;
      }
      this.dummy.updateMatrix();
      this.particlesMesh.setMatrixAt(i, this.dummy.matrix);
      this.particlesMesh.setColorAt(i, q.color);
    }
    updateInstances(this.particlesMesh);
    this.crosshair.visible = s.match.phase === "playing";
    this.renderer.render(this.scene, this.camera);
  }
}

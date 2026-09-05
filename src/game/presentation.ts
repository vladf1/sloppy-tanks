import * as THREE from "three";
import { mergeGeometries } from "three/addons/utils/BufferGeometryUtils.js";
import {
  box,
  put,
  cylinder,
  material,
  tankModel,
  wreckModel,
  coverModel,
  labelTexture,
} from "./models";
import { ARENA, TEAM_COLORS, PICKUPS, WEAPONS, VEHICLES } from "./data";
import { spawnPositions } from "./arena";
import type { Simulation } from "./simulation";
import type { SimEvent, Fragment } from "./types";
interface Particle {
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
/** Batch static pieces while retaining authored movable turret/barrel/track groups. */
function batch(group: THREE.Group) {
  group.updateMatrixWorld(true);
  const byMat = new Map<THREE.Material, THREE.BufferGeometry[]>();
  for (const child of [...group.children])
    if (child instanceof THREE.Mesh) {
      const mat = child.material as THREE.Material;
      const geo = (
        child.geometry.index
          ? child.geometry.toNonIndexed()
          : child.geometry.clone()
      ).applyMatrix4(child.matrix);
      const list = byMat.get(mat) ?? [];
      list.push(geo);
      byMat.set(mat, list);
      group.remove(child);
    }
  for (const [mat, geos] of byMat) {
    const geo = mergeGeometries(geos);
    if (geo) {
      const m = new THREE.Mesh(geo, mat);
      m.castShadow = true;
      m.receiveShadow = true;
      geo.userData.owned = true;
      group.add(m);
    }
    for (const g of geos) g.dispose();
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
function batchTank(g: THREE.Group) {
  const d = g.userData;
  const tracks = new THREE.Group();
  for (const t of d.tracks) tracks.add(t);
  d.hull.add(tracks);
  batch(tracks);
  d.trackGroup = tracks;
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
  worldGroup = new THREE.Group();
  tankMeshes = new Map<number, THREE.Group>();
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
  dummy = new THREE.Object3D();
  follow = new THREE.Vector3();
  zoom = 34;
  time = 0;
  flash = new THREE.PointLight(0xffc178, 0, 20, 2);
  crosshair = new THREE.Group();
  resolution = 1;
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
    this.renderer.toneMappingExposure = 1.05;
    this.scene.background = new THREE.Color(0x59bbed);
    this.scene.fog = new THREE.Fog(0x59bbed, 150, 260);
    this.scene.add(new THREE.HemisphereLight(0xeaf7ff, 0xbda07c, 2.0));
    const sun = new THREE.DirectionalLight(0xffead2, 2.8);
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
    const board = box(ARENA * 2 + 6, 1.2, ARENA * 2 + 6, 0x947c4d, 0.4);
    put(this.scene, board, 0, -0.8, 0);
    const floor = box(ARENA * 2, 0.15, ARENA * 2, 0xffdb92, 0.03);
    const groundTexture = this.groundTexture();
    floor.material = new THREE.MeshStandardMaterial({
      map: groundTexture,
      roughness: 1,
    });
    put(this.scene, floor, 0, -0.07, 0);
    const outskirts = new THREE.Mesh(
      new THREE.BoxGeometry(180, 0.15, 180),
      floor.material,
    );
    outskirts.receiveShadow = true;
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
  groundTexture() {
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 256;
    const ctx = canvas.getContext("2d")!;
    ctx.fillStyle = "#bbbf73";
    ctx.fillRect(0, 0, 256, 256);
    let seed = 719;
    const random = () => {
      seed = (seed * 1664525 + 1013904223) >>> 0;
      return seed / 4294967296;
    };
    for (let i = 0; i < 18000; i++) {
      ctx.fillStyle = ["#939f58", "#d2cd8d", "#adb269", "#c6c87a"][i % 4];
      ctx.globalAlpha = 0.2 + random() * 0.35;
      ctx.fillRect(
        random() * 256,
        random() * 256,
        0.6 + random() * 1.3,
        1 + random() * 2.5,
      );
    }
    const texture = new THREE.CanvasTexture(canvas);
    texture.colorSpace = THREE.SRGBColorSpace;
    texture.wrapS = texture.wrapT = THREE.RepeatWrapping;
    texture.repeat.set(24, 24);
    texture.anisotropy = Math.min(
      8,
      this.renderer.capabilities.getMaxAnisotropy(),
    );
    return texture;
  }
  createYardDetails() {
    const details = new THREE.Group();
    // Broad village roads retain the roomy midfield and outer flanking circuits.
    for (const x of [-52, 0, 52])
      put(
        details,
        box(x === 0 ? 18 : 10, 0.025, ARENA * 2 - 2, 0xddbd80, 0),
        x,
        0.03,
        0,
      );
    for (const z of [-38, 0, 38])
      put(
        details,
        box(ARENA * 2 - 2, 0.025, z === 0 ? 12 : 8, 0xddbd80, 0),
        0,
        0.05,
        z,
      );
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
    // The tree line frames the board; in-arena trees have matching cover colliders.
    for (const side of [-1, 1])
      for (let i = 0; i < 12; i++) {
        const tree = coverModel({
          kind: "tree",
          x: -57 + i * 10.2,
          z: side * 65,
          w: 3.8,
          d: 3.8,
          h: 6 + (i % 3),
          color: 0x19935c,
        } as Parameters<typeof coverModel>[0]);
        tree.position.y = -0.8;
        batch(tree);
        this.scene.add(tree);
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
    for (const mesh of this.debrisMeshes.values()) mesh.count = 0;
    this.playerWasAlive = false;
    for (const c of s.covers) {
      const g = coverModel(c);
      batch(g);
      this.coverMeshes.set(c.id, g);
      this.worldGroup.add(g);
    }
    for (const t of s.tanks) {
      const g = tankModel(t.kind, t.team);
      batchTank(g);
      g.userData.kind = t.kind;
      this.tankMeshes.set(t.id, g);
      this.worldGroup.add(g);
      this.makeBar(t.id, t.team, t.human);
    }
    for (const p of s.pickups) {
      const g = new THREE.Group(),
        def = PICKUPS[p.kind];
      put(g, cylinder(0.95, 0.12, 0x25435f, 24), 0, 0.08, 0);
      const ring = new THREE.Mesh(
        new THREE.TorusGeometry(0.8, 0.04, 5, 24),
        material(def.color),
      );
      ring.geometry.userData.owned = true;
      ring.rotation.x = Math.PI / 2;
      put(g, ring, 0, 0.2, 0);
      const gem = box(0.8, 0.8, 0.8, def.color, 0.14);
      gem.rotation.y = Math.PI / 4;
      put(g, gem, 0, 1.25, 0);
      const sprite = new THREE.Sprite(
        new THREE.SpriteMaterial({
          map: labelTexture(def.icon),
          depthTest: false,
        }),
      );
      sprite.material.userData.owned = true;
      sprite.scale.set(1.15, 0.65, 1);
      put(g, sprite, 0, 2.05, 0);
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
    const bg = new THREE.Mesh(
      new THREE.PlaneGeometry(1.75, 0.16),
      new THREE.MeshBasicMaterial({ color: 0x293a34, depthTest: false }),
    );
    const fg = new THREE.Mesh(
      new THREE.PlaneGeometry(1.65, 0.09),
      new THREE.MeshBasicMaterial({
        color: TEAM_COLORS[team],
        depthTest: false,
      }),
    );
    fg.position.z = 0.01;
    g.add(bg, fg);
    if (!human) {
      const icon = new THREE.Sprite(
        new THREE.SpriteMaterial({
          toneMapped: false,
          map: labelTexture(team === 0 ? "◆" : "Ⅱ", "#ffffff"),
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
    g.add(ammo);
    g.userData.ammo = ammo;
    g.traverse((o) => {
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
    this.raycaster.setFromCamera(new THREE.Vector2(nx, ny), this.camera);
    const p = new THREE.Vector3();
    this.raycaster.ray.intersectPlane(this.groundPlane, p);
    this.crosshair.position.x = p.x;
    this.crosshair.position.z = p.z;
    return p;
  }
  event(e: SimEvent) {
    if (e.type === "hurt") return;
    if (e.type === "respawn" || e.type === "pickup") return;
    const explosion =
      e.type === "explosion" || e.type === "death" || e.type === "destroy";
    const count = explosion ? 18 : e.type === "shot" ? 5 : 8;
    for (let i = 0; i < count && this.particles.length < 1200; i++) {
      const life = explosion
        ? 0.35 + Math.random() * 0.45
        : 0.1 + Math.random() * 0.2;
      const speed = explosion ? (e.size ?? 3) * 1.2 : 4;
      this.particles.push({
        x: e.x,
        y: explosion ? 0.8 : 1,
        z: e.z,
        vx: (Math.random() - 0.5) * speed,
        vy: Math.random() * speed,
        vz: (Math.random() - 0.5) * speed,
        life,
        max: life,
        size: explosion
          ? 0.22 + Math.random() * 0.5
          : 0.04 + Math.random() * 0.09,
        color: new THREE.Color(
          explosion
            ? i % 3 === 0
              ? 0x536779
              : i % 2 === 0
                ? 0xffc569
                : 0xff9250
            : (e.color ?? 0xffdf91),
        ),
      });
    }
    if (explosion) {
      this.flash.position.set(e.x, 3, e.z);
      this.flash.intensity = 45;
    }
  }
  render(s: Simulation, alpha: number, dt: number, overview = false) {
    this.time += dt;
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
    const corners = [
      [-0.8, -0.7],
      [0.8, -0.7],
      [-0.8, 0.65],
      [0.8, 0.65],
    ].map(([x, y]) => {
      this.raycaster.setFromCamera(new THREE.Vector2(x, y), this.camera);
      return this.raycaster.ray.intersectPlane(
        new THREE.Plane(new THREE.Vector3(0, 1, 0), 0),
        new THREE.Vector3(),
      )!;
    });
    s.wreckView = {
      minX: Math.max(corners[0].x, corners[2].x),
      maxX: Math.min(corners[1].x, corners[3].x),
      minZ: corners[2].z,
      maxZ: corners[0].z,
    };
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
      let g = this.tankMeshes.get(t.id)!;
      if (g.userData.kind !== t.kind) {
        disposeOwned(g);
        this.worldGroup.remove(g);
        g = tankModel(t.kind, t.team);
        batchTank(g);
        g.userData.kind = t.kind;
        this.tankMeshes.set(t.id, g);
        this.worldGroup.add(g);
      }
      g.visible = t.alive;
      const bar = this.bars.get(t.id)!;
      bar.visible = t.alive;
      if (!t.alive) continue;
      const pos = t.body.translation();
      g.position.set(
        THREE.MathUtils.lerp(t.previous.x, pos.x, alpha),
        pos.y - 0.4,
        THREE.MathUtils.lerp(t.previous.z, pos.z, alpha),
      );
      g.userData.hull.rotation.y = t.heading;
      g.userData.turret.rotation.y = t.aim;
      g.userData.barrel.position.z = -t.recoil * 0.2;
      g.userData.trackGroup.position.z =
        (this.time * Math.hypot(t.body.linvel().x, t.body.linvel().z) * 0.4) %
        0.25;
      g.scale.setScalar(VEHICLES[t.kind].scale);
      bar.position.set(g.position.x, t.human ? 3.2 : 2.5, g.position.z);
      bar.quaternion.copy(this.camera.quaternion);
      bar.userData.fg.scale.x = t.hp / VEHICLES[t.kind].health;
      bar.userData.ammo.scale.x = 1 - t.cooldown / WEAPONS[t.weapon].interval;
      bar.userData.fg.material.color.set(
        t.protection > 0
          ? 0xffffff
          : t.shield > 0
            ? 0x78d9ff
            : TEAM_COLORS[t.team],
      );
    }
    for (const c of s.covers) {
      let g = this.coverMeshes.get(c.id);
      if (!g) {
        g = coverModel(c);
        batch(g);
        this.coverMeshes.set(c.id, g);
        this.worldGroup.add(g);
      }
      g.visible = c.alive;
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
        // Flatten the selected assembly before batching its material groups.
        g.updateMatrixWorld(true);
        const meshes: THREE.Mesh[] = [];
        g.traverse((o) => {
          if (o instanceof THREE.Mesh) meshes.push(o);
        });
        const flat = new THREE.Group();
        for (const mesh of meshes) {
          mesh.applyMatrix4(mesh.parent!.matrixWorld);
          flat.add(mesh);
        }
        batch(flat);
        if (f.cleanup === "fade") {
          flat.traverse((o) => {
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
        g = flat;
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
      mesh.instanceMatrix.needsUpdate = true;
      if (mesh.instanceColor) mesh.instanceColor.needsUpdate = true;
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
        put(g, cylinder(0.5, 0.17, 0x384f47), 0, 0.12, 0);
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
      this.dummy.position.set(shot.x, 1, shot.z);
      this.dummy.rotation.set(0, Math.atan2(shot.vx, shot.vz), 0);
      this.dummy.scale.set(1.15, 1.15, length + 0.2);
      this.dummy.updateMatrix();
      this.shotOutline.setMatrixAt(i, this.dummy.matrix);
      this.dummy.position.y = 1.105;
      this.dummy.scale.set(1, 1, length);
      this.dummy.updateMatrix();
      this.shotMesh.setMatrixAt(i, this.dummy.matrix);
      this.shotMesh.setColorAt(i, this.debrisColor.set(TEAM_COLORS[shot.team]));
      this.dummy.position.y = 1.21;
      this.dummy.scale.set(0.43, 0.43, length * 0.72);
      this.dummy.updateMatrix();
      this.shotCore.setMatrixAt(i, this.dummy.matrix);
    }
    for (const mesh of [this.shotMesh, this.shotCore, this.shotOutline])
      mesh.instanceMatrix.needsUpdate = true;
    if (this.shotMesh.instanceColor)
      this.shotMesh.instanceColor.needsUpdate = true;
    for (let i = this.particles.length - 1; i >= 0; i--) {
      const q = this.particles[i];
      q.life -= dt;
      if (q.life <= 0) {
        this.particles.splice(i, 1);
        continue;
      }
      q.x += q.vx * dt;
      q.y += q.vy * dt;
      q.z += q.vz * dt;
      q.vy -= 8 * dt;
    }
    this.particlesMesh.count = this.particles.length;
    for (let i = 0; i < this.particles.length; i++) {
      const q = this.particles[i];
      this.dummy.position.set(q.x, Math.max(0.1, q.y), q.z);
      this.dummy.rotation.set(0, this.time, 0);
      this.dummy.scale.setScalar((q.size * q.life) / q.max);
      this.dummy.updateMatrix();
      this.particlesMesh.setMatrixAt(i, this.dummy.matrix);
      this.particlesMesh.setColorAt(i, q.color);
    }
    this.particlesMesh.instanceMatrix.needsUpdate = true;
    if (this.particlesMesh.instanceColor)
      this.particlesMesh.instanceColor.needsUpdate = true;
    this.crosshair.visible = s.match.phase === "playing";
    this.renderer.render(this.scene, this.camera);
  }
}

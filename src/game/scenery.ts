import * as THREE from "three/webgpu";
import { spawnPositions } from "./arena";
import { batch, freezeStatic } from "./batching";
import { ARENA, TEAM_COLORS } from "./data";
import { groundMaterial, groundUVs, roadGeometry } from "./ground-surfaces";
import type { GroundKind } from "./ground-surfaces";
import { box, cylinder, material, put } from "./model-primitives";
import { VILLAGE_ROADS } from "./village-roads";
/** Static village dressing; collidable objects are created separately from the arena layout. */
function createYardDetails(scene: THREE.Scene, renderer: THREE.WebGPURenderer): void {
  const details = new THREE.Group();
  const roads = new THREE.Group();
  const roadMaterial = groundMaterial(renderer, "packed-dirt");
  roadMaterial.vertexColors = true;
  roadMaterial.transparent = true;
  roadMaterial.depthWrite = false;
  for (const { w, d, x, z, y } of VILLAGE_ROADS) {
    const geometry = roadGeometry(w, d, x, z);
    put(roads, new THREE.Mesh(geometry, roadMaterial), x, y, z);
  }
  batch(roads);
  for (const mesh of roads.children) {
    mesh.castShadow = false;
    // Ground blending must precede shields, pickup glows and track decals.
    mesh.renderOrder = -1;
  }
  scene.add(roads);
  freezeStatic(roads);
  details.add(createSpawnPads());
  for (const team of [0, 1] as const) {
    const side = team === 0 ? -1 : 1;
    const color = TEAM_COLORS[team];
    for (let z = -57; z <= 57; z += 2) {
      put(details, box(0.16, 1.7, 0.16, color, 0.015), side * ARENA, 2.7, z);
      put(details, box(0.13, 0.16, 2.2, color, 0.01), side * ARENA, 3.0, z);
    }
  }
  batch(details);
  scene.add(details);
  freezeStatic(details);
}

export function createSpawnPads(): THREE.Group {
  const details = new THREE.Group();
  details.name = "spawn-pads";
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
  const spawnArrowGeometry = new THREE.ShapeGeometry(arrowShape).rotateX(-Math.PI / 2);
  for (const team of [0, 1] as const) {
    const side = team === 0 ? -1 : 1;
    const color = TEAM_COLORS[team];
    for (const position of spawnPositions(team)) {
      // Low octagonal deployment plinth with a recessed deck and segmented team lights.
      put(details, cylinder(2.75, 0.1, 0x283c4e, 8), position.x, 0.08, position.z);
      put(details, cylinder(2.52, 0.045, 0x718898, 8), position.x, 0.135, position.z);
      put(details, cylinder(2.37, 0.035, 0x223d51, 32), position.x, 0.17, position.z);
      put(details, cylinder(1.98, 0.025, 0x455e70, 8), position.x, 0.193, position.z);
      for (let i = 0; i < 8; i++) {
        const angle = (i * Math.PI) / 4;
        const segment = new THREE.Mesh(spawnRimGeometry, material(color));
        segment.rotation.y = angle;
        put(details, segment, position.x, 0.198, position.z);
        put(
          details,
          cylinder(0.075, 0.025, 0xc9d6dd, 8),
          position.x + Math.cos(angle) * 2.58,
          0.175,
          position.z + Math.sin(angle) * 2.58,
        );
      }
      for (const z of [-1.25, 1.25]) {
        for (let i = 0; i < 5; i++) {
          put(
            details,
            box(0.18, 0.02, 0.4, 0x1a2b3c, 0.005),
            position.x - 0.52 + i * 0.26,
            0.218,
            position.z + z,
          );
        }
      }
      // Concentric paint and inward chevrons make the pad legible when unoccupied.
      const badge = box(0.7, 0.025, 0.7, color, 0.035);
      badge.rotation.y = Math.PI / 4;
      put(details, badge, position.x, 0.22, position.z);
      for (const offset of [3.1, 3.8]) {
        const arrow = new THREE.Mesh(spawnArrowGeometry, material(color));
        arrow.rotation.y = team === 0 ? 0 : Math.PI;
        put(details, arrow, position.x - side * offset, 0.09, position.z);
      }
    }
  }
  batch(details);
  freezeStatic(details);
  return details;
}

const SHADOW_DEPTH = 219.5;

/** The original square sun shadow box shared by the village and harbor. */
export function defaultSunShadow(sun: THREE.DirectionalLight): void {
  const camera = sun.shadow.camera;
  camera.left = camera.bottom = -(ARENA + 10);
  camera.right = camera.top = ARENA + 10;
  camera.near = 0.5;
  camera.far = 0.5 + SHADOW_DEPTH;
  camera.updateProjectionMatrix();
}

/** Fit the sun's orthographic shadow box around a ground square for the sun's
 * current direction. A square box aimed at a low, diagonal sun lands on the
 * ground as a tilted strip that misses two arena corners; this one covers the
 * whole square. The depth range keeps its default span so the tuned bias holds. */
export function fitSunShadow(
  sun: THREE.DirectionalLight,
  half: number,
  low: number,
  high: number,
): void {
  const probe = new THREE.OrthographicCamera();
  probe.position.copy(sun.position);
  probe.lookAt(sun.target.position);
  probe.updateMatrixWorld();
  const min = new THREE.Vector3(Infinity, Infinity, Infinity);
  const max = new THREE.Vector3(-Infinity, -Infinity, -Infinity);
  const corner = new THREE.Vector3();
  for (const x of [-half, half]) {
    for (const y of [low, high]) {
      for (const z of [-half, half]) {
        corner.set(x, y, z).applyMatrix4(probe.matrixWorldInverse);
        min.min(corner);
        max.max(corner);
      }
    }
  }
  const camera = sun.shadow.camera;
  camera.left = min.x;
  camera.right = max.x;
  camera.bottom = min.y;
  camera.top = max.y;
  // The camera looks down -z; orthographic near may sit behind the light.
  camera.near = -max.z - 2;
  camera.far = camera.near + SHADOW_DEPTH;
  camera.updateProjectionMatrix();
}

export function createLighting(scene: THREE.Scene) {
  scene.background = new THREE.Color(0x59bbed);
  scene.fog = new THREE.Fog(0x59bbed, 150, 260);
  // Golden direct light and blue-gray ambient fill separate sunlit faces from shade.
  const fill = new THREE.HemisphereLight(0xbdd5f5, 0x75859b, 1.65);
  scene.add(fill);
  const sun = new THREE.DirectionalLight(0xffd59b, 2.8);
  sun.position.set(-45, 85, 25);
  sun.castShadow = true;
  sun.shadow.mapSize.set(2048, 2048);
  defaultSunShadow(sun);
  sun.shadow.normalBias = 0.05;
  sun.shadow.bias = -0.0002;
  scene.add(sun);
  return { sun, fill };
}

export function createArenaFloor(
  renderer: THREE.WebGPURenderer,
  kind: GroundKind,
  extent = ARENA * 2,
): THREE.Mesh {
  const surface = groundMaterial(renderer, kind);
  const segments = kind === "dry-grass" ? Math.max(1, Math.round(extent / 2.5)) : 1;
  const geometry = new THREE.PlaneGeometry(extent, extent, segments, segments).rotateX(
    -Math.PI / 2,
  );
  groundUVs(geometry);
  if (kind === "dry-grass") {
    surface.color.setHex(0xaee6a6);
    surface.vertexColors = true;
    const positions = geometry.getAttribute("position");
    const colors: number[] = [];
    for (let i = 0; i < positions.count; i++) {
      const x = positions.getX(i);
      const z = positions.getZ(i);
      const patch =
        0.5 + 0.25 * Math.sin(x * 0.18 + z * 0.09) + 0.25 * Math.sin(z * 0.22 - x * 0.1);
      colors.push(0.68 + patch * 0.28, 0.83 + patch * 0.14, 0.42 + patch * 0.36);
    }
    geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  }
  const floor = new THREE.Mesh(geometry, surface);
  floor.receiveShadow = true;
  return floor;
}

export function createTerrain(
  scene: THREE.Scene,
  renderer: THREE.WebGPURenderer,
): THREE.MeshStandardMaterial {
  const board = box(ARENA * 2 + 6, 1.2, ARENA * 2 + 6, 0x947c4d, 0.4);
  put(scene, board, 0, -0.8, 0);
  const floor = createArenaFloor(renderer, "dry-grass");
  put(scene, floor, 0, 0.008, 0);
  createYardDetails(scene, renderer);
  return floor.material as THREE.MeshStandardMaterial;
}

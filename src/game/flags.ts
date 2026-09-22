import * as THREE from "three/webgpu";
import {
  Fn,
  If,
  attribute,
  cross,
  float,
  frameGroup,
  normalLocal,
  positionGeometry,
  positionLocal,
  sin,
  uniform,
  uv,
  vec2,
  vec3,
} from "three/tsl";
import { spawnPositions } from "./arena";
import { freezeStatic } from "./batching";
import { TEAM_COLORS } from "./data";
import { cylinder } from "./model-primitives";

function randomWind() {
  return { strength: 0.15 + Math.random() * 0.85, direction: (Math.random() - 0.5) * 1.3 };
}

/** Cloth stays attached to the pole while gusts carry ripples toward its free edge. */
export class Flags {
  group = new THREE.Group();
  private windFrom = randomWind();
  private windTo = randomWind();
  private windStart = 0;
  private windDuration = 3 + Math.random() * 4;
  private clock = uniform(0).setGroup(frameGroup);
  private breeze = uniform(new THREE.Vector3()).setGroup(frameGroup);

  constructor() {
    const pole = cylinder(0.055, 4.8, 0x59656a, 8);
    const spawns = [spawnPositions(0), spawnPositions(1)];
    const poles = new THREE.InstancedMesh(
      pole.geometry,
      pole.material,
      spawns[0].length + spawns[1].length,
    );
    poles.name = "flag-pole";
    poles.castShadow = poles.receiveShadow = true;
    this.group.add(poles);
    const matrix = new THREE.Matrix4();
    let poleIndex = 0;
    for (const team of [0, 1] as const) {
      const material = new THREE.MeshStandardNodeMaterial({
        color: TEAM_COLORS[team],
        roughness: 1,
        side: THREE.DoubleSide,
      });
      material.positionNode = this.clothPosition();
      const geometry = new THREE.PlaneGeometry(1.4, 0.9, 16, 6);
      // Wind moves vertices in the shader. This sphere contains every allowed
      // gust/direction, so CPU culling never relies on the undeformed plane.
      geometry.boundingSphere = new THREE.Sphere(new THREE.Vector3(), 2);
      const phases = new Float32Array(spawns[team].length);
      geometry.setAttribute("flagPhase", new THREE.InstancedBufferAttribute(phases, 1));
      const cloth = new THREE.InstancedMesh(geometry, material, phases.length);
      cloth.name = "flag-cloth";
      cloth.castShadow = cloth.receiveShadow = true;
      for (const [i, position] of spawns[team].entries()) {
        const x = team === 0 ? -62 : 62;
        poles.setMatrixAt(poleIndex++, matrix.makeTranslation(x, 2.4, position.z));
        cloth.setMatrixAt(i, matrix.makeTranslation(x, 4.6, position.z));
        phases[i] = position.z * 0.12 + x * 0.04;
      }
      cloth.computeBoundingSphere();
      this.group.add(cloth);
    }
    poles.computeBoundingSphere();
    freezeStatic(this.group);
    this.update(0);
  }

  private clothPosition() {
    const phase = attribute("flagPhase", "float" as const);
    const t = this.clock;
    const gust = this.breeze.x;
    const alongX = this.breeze.y;
    const alongZ = this.breeze.z;
    const point = (u: THREE.Node<"float">, v: THREE.Node<"float">) => {
      const ripple = sin(u.mul(9).sub(t.mul(5)).add(phase).add(v.mul(1.8)));
      const flutter = sin(u.mul(19).sub(t.mul(8)).add(phase))
        .mul(0.035)
        .mul(u.mul(u));
      const reach = u.mul(gust.mul(0.3).add(1.05));
      const sideways = u.mul(gust.mul(0.09).add(0.1)).mul(ripple).add(flutter);
      return vec3(
        reach.mul(alongX).add(sideways.mul(alongZ)),
        v
          .mul(-0.9)
          .sub(u.mul(u).mul(float(0.45).sub(gust.mul(0.22))))
          .add(u.mul(0.045).mul(ripple)),
        reach.mul(alongZ).sub(sideways.mul(alongX)),
      ).toVar();
    };
    return Fn(() => {
      const coord = vec2(uv().x, uv().y.oneMinus());
      const u = coord.x;
      const v = coord.y;
      const p = point(u, v);
      const up = point(u, v.sub(1 / 6));
      const down = point(u, v.add(1 / 6));
      const left = point(u.sub(1 / 16), v);
      const right = point(u.add(1 / 16), v);
      const northEast = point(u.add(1 / 16), v.sub(1 / 6));
      const southWest = point(u.sub(1 / 16), v.add(1 / 6));
      const normal = vec3(0).toVar();
      // Sum the same adjacent triangle area normals as computeVertexNormals().
      // Finite grid neighbors preserve the original cloth lighting, including edges.
      If(u.lessThan(1).and(v.lessThan(1)), () => {
        normal.addAssign(cross(down.sub(p), right.sub(p)));
      });
      If(u.lessThan(1).and(v.greaterThan(0)), () => {
        normal.addAssign(cross(p.sub(up), northEast.sub(up)));
        normal.addAssign(cross(right.sub(p), northEast.sub(p)));
      });
      If(u.greaterThan(0).and(v.greaterThan(0)), () => {
        normal.addAssign(cross(p.sub(left), up.sub(left)));
      });
      If(u.greaterThan(0).and(v.lessThan(1)), () => {
        normal.addAssign(cross(southWest.sub(left), p.sub(left)));
        normal.addAssign(cross(down.sub(southWest), p.sub(southWest)));
      });
      normalLocal.assign(normal.normalize());
      // positionNode runs after instancing. These instances are translations;
      // replace the undeformed local plane while retaining its pole attachment.
      return positionLocal.add(p.sub(positionGeometry));
    })();
  }

  update(time: number): void {
    // One breeze for the entire arena, easing toward a new random target every 3–7 seconds.
    while (time >= this.windStart + this.windDuration) {
      this.windStart += this.windDuration;
      this.windFrom = this.windTo;
      this.windTo = randomWind();
      this.windDuration = 3 + Math.random() * 4;
    }
    const progress = Math.max(0, (time - this.windStart) / this.windDuration);
    const blend = progress * progress * (3 - 2 * progress);
    const gust = THREE.MathUtils.lerp(this.windFrom.strength, this.windTo.strength, blend);
    const direction = THREE.MathUtils.lerp(this.windFrom.direction, this.windTo.direction, blend);
    this.clock.value = time;
    this.breeze.value.set(gust, Math.sin(direction), Math.cos(direction));
  }
}

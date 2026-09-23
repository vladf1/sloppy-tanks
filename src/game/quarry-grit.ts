import * as THREE from "three/webgpu";
import {
  dot,
  faceDirection,
  float,
  mix,
  normalView,
  positionView,
  positionWorld,
  texture as sampleTexture,
  vec2,
  vec3,
} from "three/tsl";

let grit: THREE.Texture | undefined;
/** Shared packed-dirt grit, sampled in world space by every quarry soil surface. */
function gritTexture(): THREE.Texture {
  if (!grit) {
    grit = new THREE.TextureLoader().load(
      `${import.meta.env?.BASE_URL ?? "/"}textures/ground/packed-dirt.webp`,
    );
    grit.colorSpace = THREE.SRGBColorSpace;
    // Mirroring joins the generated edges without relying on perfect AI tiling.
    grit.wrapS = grit.wrapT = THREE.MirroredRepeatWrapping;
    grit.anisotropy = 8;
  }
  return grit;
}

const LUMA = vec3(0.2126, 0.7152, 0.0722);
/** Linear mean luminance of packed-dirt.webp; dividing by it centres grit on 1. */
const GRIT_MEAN = 0.327;
const GRIT_TILE = 3.3;

/** Screen-space derivative bump (Mikkelsen's surface gradient) from a height
 * already sampled for colour, so relief costs no extra texture reads. */
export function derivativeBump(height: THREE.Node<"float">, scale: THREE.Node<"float"> | number) {
  const slope = vec2(height.dFdx(), height.dFdy()).mul(scale);
  const sigmaX = positionView.dFdx().normalize();
  const sigmaY = positionView.dFdy().normalize();
  const r1 = sigmaY.cross(normalView);
  const r2 = normalView.cross(sigmaX);
  const det = sigmaX.dot(r1).mul(faceDirection);
  const gradient = det.sign().mul(slope.x.mul(r1).add(slope.y.mul(r2)));
  return det.abs().mul(normalView).sub(gradient).normalize();
}

/** Centimetre grit and pebbles the 10 cm soil bake cannot resolve. Two rotated,
 * differently scaled samples hide the tile. `color` is a luminance multiplier
 * averaging 1, so the baked hue survives; `normal` lights the same pebbles. */
export function quarryGrit(
  amount: THREE.Node<"float"> | number = 1,
  relief: THREE.Node<"float"> | number = 0,
) {
  const p = positionWorld.xz;
  const near = dot(sampleTexture(gritTexture(), p.div(GRIT_TILE)).rgb, LUMA);
  const far = sampleTexture(
    gritTexture(),
    vec2(p.x.mul(0.6).add(p.y.mul(0.8)), p.y.mul(0.6).sub(p.x.mul(0.8))).div(11.7),
  );
  const detail = near.mul(dot(far.rgb, LUMA).add(GRIT_MEAN)).div(GRIT_MEAN * GRIT_MEAN * 2);
  return {
    color: mix(float(1), detail, amount),
    normal: derivativeBump(near, relief),
  };
}

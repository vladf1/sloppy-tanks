import * as THREE from "three";
import { PICKUPS } from "./data";
import type { PickupKind } from "./types";

const cubeGeometry = new THREE.BoxGeometry(1.25, 1.25, 1.25);
const faceMaterials = new Map<PickupKind, THREE.MeshStandardMaterial>();
const names: Record<PickupKind, string> = {
  rapid: "RAPID", spread: "SPREAD", rocket: "ROCKET", ricochet: "BOUNCE",
  shield: "SHIELD", speed: "SPEED", repair: "REPAIR",
};

/** Original high-contrast pictograms, shared across every face and pickup of a type. */
function faceMaterial(kind: PickupKind) {
  const cached = faceMaterials.get(kind);
  if (cached) return cached;
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 256;
  const c = canvas.getContext("2d")!;
  const color = `#${PICKUPS[kind].color.toString(16).padStart(6, "0")}`;
  c.fillStyle = "#122638"; c.fillRect(0, 0, 256, 256);
  c.strokeStyle = color; c.lineWidth = 14; c.strokeRect(12, 12, 232, 232);
  c.fillStyle = color;
  for (const x of [28, 214]) for (const y of [28, 214]) c.fillRect(x, y, 14, 14);
  c.strokeStyle = c.fillStyle = "#ffffff";
  c.lineWidth = 16; c.lineCap = "round"; c.lineJoin = "round";
  const line = (points: number[][], fill = false) => {
    c.beginPath(); points.forEach(([x, y], i) => i ? c.lineTo(x, y) : c.moveTo(x, y));
    if (fill) { c.closePath(); c.fill(); } else c.stroke();
  };
  if (kind === "speed") line([[143, 46], [77, 126], [118, 126], [105, 181], [178, 96], [136, 96]], true);
  else if (kind === "repair") {
    c.fillRect(109, 57, 38, 124); c.fillRect(66, 100, 124, 38);
  } else if (kind === "shield") {
    line([[128, 52], [181, 73], [177, 126], [160, 157], [128, 183], [96, 157], [79, 126], [75, 73], [128, 52]]);
    line([[128, 78], [128, 152]]);
  } else if (kind === "rapid") {
    for (const x of [64, 105, 146]) line([[x, 73], [x + 35, 118], [x, 163]]);
  } else if (kind === "spread") {
    for (const x of [66, 128, 190]) {
      line([[128, 172], [x, 70]]);
      line([[x - 17, 82], [x, 58], [x + 17, 82]]);
    }
  } else if (kind === "ricochet") {
    c.strokeStyle = color; line([[190, 61], [190, 173]]);
    c.strokeStyle = "#fff";
    line([[67, 165], [166, 115], [77, 65]]);
    line([[82, 96], [67, 60], [107, 58]]);
  } else {
    line([[128, 47], [153, 79], [153, 140], [103, 140], [103, 79]], true);
    line([[103, 116], [81, 155], [105, 149]], true);
    line([[153, 116], [175, 155], [151, 149]], true);
    c.strokeStyle = color; line([[128, 155], [128, 183]]);
    c.fillStyle = "#122638"; c.beginPath(); c.arc(128, 94, 10, 0, Math.PI * 2); c.fill();
  }
  c.fillStyle = color; c.font = "900 25px sans-serif";
  c.textAlign = "center"; c.fillText(names[kind], 128, 218);
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.anisotropy = 4;
  const material = new THREE.MeshStandardMaterial({
    map: texture, roughness: 0.55, metalness: 0.15,
    emissive: 0xffffff, emissiveMap: texture, emissiveIntensity: 0.3,
    toneMapped: false,
  });
  faceMaterials.set(kind, material);
  return material;
}

export function pickupCube(kind: PickupKind) {
  const cube = new THREE.Mesh(cubeGeometry, faceMaterial(kind));
  cube.castShadow = true;
  return cube;
}

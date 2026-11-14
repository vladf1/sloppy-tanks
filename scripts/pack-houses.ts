// Packs the Neighborhood's Blender-made houses (`assets/models/houses/<design>.glb`, written
// by `bun run house:<design>`) into the one file the page downloads, `public/models/houses.bin`,
// which `crates/core/src/models/house_pack.rs` reads. One file, because the designs share most
// of their kit parts and Brotli finds those repeats only inside one stream; integer columns
// rather than glTF's float arrays, because that is what Brotli shrinks best:
//
// - positions as 16-bit integers on a grid of 2^-POSITION_STEP_BITS m (within 0.12 mm of
//   Blender's), one column per axis (Brotli shrank these smaller than differences);
// - no UVs: each vertex names which of Blender's planar projections (`house_kit.export`:
//   metres along two axes of the face's dominant one, v flipped as glTF stores it) gives its
//   UV, in runs, and the importer projects its position again;
// - no normals where the faces say them: the importer sums the faces round each vertex
//   (flat shading splits vertices at every crease), and the file keeps the few vertices
//   whose normal differs (smooth corners) as 8-bit exceptions.
//
// The bytes depend only on the source files, so an unchanged model keeps its hashed name.
// Layout (little-endian; "uint" is an unsigned LEB128 varint, "int" a zigzag one):
//   "SLHP", u8 format version, u8 position step bits,
//   uint designs; per design: string name, uint objects;
//   per object (designs in order): string name (`<HousePart>__<Surface>`), uint vertices,
//     uint indices, uint UV runs, uint normal exceptions;
//   then the columns, each over every object in order:
//     x, then y, then z: i16 per vertex;
//     indices: int, one more than the highest index before it in its object, less the index
//       (0 for each new vertex in order);
//     UV runs: uint vertices, then (a second column) u8 projection
//       (u axis | v axis << 2 | u negated << 4 | v negated << 5);
//     normal exceptions: uint vertex (less the previous exception's in its object), then
//       (a second column) three i8 components of the normal times 127.
// A string is a uint byte length and UTF-8.
import { readdirSync, readFileSync, writeFileSync } from "node:fs";

const SOURCES = "assets/models/houses/";
const OUTPUT = "public/models/houses.bin";
const FORMAT_VERSION = 1;
/** Positions on a grid of 2^-12 m (0.24 mm): within 0.12 mm of Blender's, inside ±8 m. */
const POSITION_STEP_BITS = 12;
/** How far (m) a projected UV may sit from the source's before the packer refuses it. */
const UV_TOLERANCE = 0.0005;
/** A recomputed normal further than this (degrees) from the source's is stored. */
const NORMAL_TOLERANCE = 0.5;

type Mesh = {
  name: string;
  positions: Float32Array;
  normals: Float32Array;
  uvs: Float32Array;
  indices: Uint32Array;
};

/** The meshes of one of Blender's binary glTF exports, by node, in node order. */
function readGlb(path: string): Mesh[] {
  const bytes = readFileSync(path);
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const jsonLength = view.getUint32(12, true);
  const json = JSON.parse(new TextDecoder().decode(bytes.subarray(20, 20 + jsonLength)));
  const binary = bytes.subarray(20 + jsonLength + 8);
  const accessor = (index: number, components: number) => {
    const { bufferView, byteOffset = 0, componentType, count } = json.accessors[index];
    const target = json.bufferViews[bufferView];
    const start = binary.byteOffset + (target.byteOffset ?? 0) + byteOffset;
    const length = count * components;
    const copy = (array: ArrayLike<number>) => Float64Array.from(array);
    if (target.byteStride) throw new Error(`${path}: interleaved accessor ${index}`);
    switch (componentType) {
      case 5126:
        return copy(new Float32Array(binary.buffer.slice(start, start + length * 4)));
      case 5123:
        return copy(new Uint16Array(binary.buffer.slice(start, start + length * 2)));
      case 5125:
        return copy(new Uint32Array(binary.buffer.slice(start, start + length * 4)));
      default:
        throw new Error(`${path}: accessor ${index} has component type ${componentType}`);
    }
  };
  const meshes: Mesh[] = [];
  for (const node of json.nodes) {
    if (node.mesh === undefined) continue;
    if (node.rotation || node.scale || node.matrix)
      throw new Error(`${path}: ${node.name} is turned or scaled`);
    const [tx, ty, tz] = node.translation ?? [0, 0, 0];
    for (const primitive of json.meshes[node.mesh].primitives) {
      const { POSITION, NORMAL, TEXCOORD_0 } = primitive.attributes;
      const positions = Float32Array.from(accessor(POSITION, 3), (p, i) => p + [tx, ty, tz][i % 3]);
      meshes.push({
        name: node.name,
        positions,
        normals: Float32Array.from(accessor(NORMAL, 3)),
        uvs: Float32Array.from(accessor(TEXCOORD_0, 2)),
        indices: Uint32Array.from(accessor(primitive.indices, 1)),
      });
    }
  }
  return meshes;
}

/** Blender's projections (u axis, v axis, u sign, v sign), the common ones first. */
const PROJECTIONS: [number, number, number, number][] = [
  [0, 2, 1, 1],
  [0, 1, 1, -1],
  [2, 1, -1, -1],
];
for (const [u, v] of [0, 1, 2].flatMap((u) => [0, 1, 2].filter((v) => v !== u).map((v) => [u, v])))
  for (const su of [1, -1])
    for (const sv of [1, -1])
      if (!PROJECTIONS.some((p) => p.join() === [u, v, su, sv].join()))
        PROJECTIONS.push([u, v, su, sv]);

/** The UV a projection gives a point: metres along two axes, v flipped as glTF stores it. */
function projected([u, v, su, sv]: number[], p: ArrayLike<number>): [number, number] {
  return [su * p[u], 1 + sv * p[v]];
}

const projectionCode = ([u, v, su, sv]: number[]) =>
  u | (v << 2) | (su < 0 ? 16 : 0) | (sv < 0 ? 32 : 0);

/** Each vertex's normal as the importer recomputes it: its triangles' area-weighted sum. */
function faceNormals(positions: Float64Array, indices: Uint32Array): Float64Array {
  const sums = new Float64Array(positions.length);
  for (let t = 0; t < indices.length; t += 3) {
    const [a, b, c] = [indices[t], indices[t + 1], indices[t + 2]];
    const e = [0, 1, 2].map((k) => positions[b * 3 + k] - positions[a * 3 + k]);
    const f = [0, 1, 2].map((k) => positions[c * 3 + k] - positions[a * 3 + k]);
    const cross = [e[1] * f[2] - e[2] * f[1], e[2] * f[0] - e[0] * f[2], e[0] * f[1] - e[1] * f[0]];
    for (const vertex of [a, b, c]) for (let k = 0; k < 3; k++) sums[vertex * 3 + k] += cross[k];
  }
  return sums;
}

function degreesBetween(a: ArrayLike<number>, b: ArrayLike<number>): number {
  const length = Math.hypot(a[0], a[1], a[2]) * Math.hypot(b[0], b[1], b[2]);
  if (length === 0) return 180;
  const cos = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]) / length;
  return (Math.acos(Math.min(1, Math.max(-1, cos))) * 180) / Math.PI;
}

class Writer {
  bytes: number[] = [];
  uint(value: number) {
    if (!Number.isInteger(value) || value < 0) throw new Error(`not a uint: ${value}`);
    while (value >= 0x80) {
      this.bytes.push((value & 0x7f) | 0x80);
      value = Math.floor(value / 128);
    }
    this.bytes.push(value);
  }
  int(value: number) {
    this.uint(value < 0 ? -2 * value - 1 : 2 * value);
  }
  string(text: string) {
    const encoded = new TextEncoder().encode(text);
    this.uint(encoded.length);
    this.bytes.push(...encoded);
  }
}

type Packed = {
  name: string;
  x: number[];
  y: number[];
  z: number[];
  indices: Uint32Array;
  runs: [number, number][];
  exceptions: [number, number[]][];
};

/** How far the packed geometry sits from Blender's at worst: m, degrees and m of UV. */
const worst = { position: 0, normal: 0, uv: 0 };

/** One object's columns: quantized positions, projection runs and normal exceptions. */
function packMesh(design: string, mesh: Mesh): Packed {
  const step = 2 ** -POSITION_STEP_BITS;
  const count = mesh.positions.length / 3;
  const quantized = Array.from(mesh.positions, (p) => {
    const q = Math.round(p / step);
    if (Math.abs(q) > 32767) throw new Error(`${design} ${mesh.name}: ${p} m is off the grid`);
    worst.position = Math.max(worst.position, Math.abs(q * step - p));
    return q;
  });
  const decoded = Float64Array.from(quantized, (q) => q * step);
  const runs: [number, number][] = [];
  let current = -1;
  for (let vertex = 0; vertex < count; vertex++) {
    const point = decoded.subarray(vertex * 3, vertex * 3 + 3);
    const uv = mesh.uvs.subarray(vertex * 2, vertex * 2 + 2);
    const distance = (index: number) => {
      const [u, v] = projected(PROJECTIONS[index], point);
      return Math.max(Math.abs(u - uv[0]), Math.abs(v - uv[1]));
    };
    const fits = (index: number) => distance(index) <= UV_TOLERANCE;
    // Keep the run's projection while it fits: runs follow Blender's faces.
    const found =
      current >= 0 && fits(current) ? current : PROJECTIONS.findIndex((_, i) => fits(i));
    if (found < 0)
      throw new Error(`${design} ${mesh.name}: vertex ${vertex}'s UV is no planar projection`);
    worst.uv = Math.max(worst.uv, distance(found));
    if (found === current) runs[runs.length - 1][0]++;
    else runs.push([1, projectionCode(PROJECTIONS[found])]);
    current = found;
  }
  const recomputed = faceNormals(decoded, mesh.indices);
  const exceptions: [number, number[]][] = [];
  for (let vertex = 0; vertex < count; vertex++) {
    const source = mesh.normals.subarray(vertex * 3, vertex * 3 + 3);
    let error = degreesBetween(recomputed.subarray(vertex * 3, vertex * 3 + 3), source);
    if (error > NORMAL_TOLERANCE) {
      const stored = Array.from(source, (n) => Math.round(n * 127));
      exceptions.push([vertex, stored]);
      error = degreesBetween(stored, source);
    }
    worst.normal = Math.max(worst.normal, error);
  }
  return {
    name: mesh.name,
    x: quantized.filter((_, i) => i % 3 === 0),
    y: quantized.filter((_, i) => i % 3 === 1),
    z: quantized.filter((_, i) => i % 3 === 2),
    indices: mesh.indices,
    runs,
    exceptions,
  };
}

export function pack(sources = SOURCES): Uint8Array {
  const designs = readdirSync(sources)
    .filter((name) => name.endsWith(".glb"))
    .sort()
    .map((file) => {
      const design = file.slice(0, -".glb".length);
      return { design, objects: readGlb(sources + file).map((mesh) => packMesh(design, mesh)) };
    });
  const objects = designs.flatMap(({ objects }) => objects);
  const out = new Writer();
  out.bytes.push(...new TextEncoder().encode("SLHP"), FORMAT_VERSION, POSITION_STEP_BITS);
  out.uint(designs.length);
  for (const { design, objects } of designs) {
    out.string(design);
    out.uint(objects.length);
  }
  for (const object of objects) {
    out.string(object.name);
    out.uint(object.x.length);
    out.uint(object.indices.length);
    out.uint(object.runs.length);
    out.uint(object.exceptions.length);
  }
  for (const axis of ["x", "y", "z"] as const)
    for (const object of objects)
      for (const value of object[axis]) out.bytes.push(value & 0xff, (value >> 8) & 0xff);
  // Blender writes a face's new vertices in order, so most indices code as 0 or a small
  // step back to a vertex an earlier triangle of the face used.
  for (const object of objects) {
    let highest = -1;
    for (const index of object.indices) {
      out.int(highest + 1 - index);
      highest = Math.max(highest, index);
    }
  }
  for (const object of objects) for (const [length] of object.runs) out.uint(length);
  for (const object of objects) for (const [, code] of object.runs) out.bytes.push(code);
  for (const object of objects) {
    let previous = 0;
    for (const [vertex] of object.exceptions) {
      out.uint(vertex - previous);
      previous = vertex;
    }
  }
  for (const object of objects)
    for (const [, normal] of object.exceptions) out.bytes.push(...normal.map((n) => n & 0xff));
  return Uint8Array.from(out.bytes);
}

if (import.meta.main) {
  const bytes = pack();
  writeFileSync(OUTPUT, bytes);
  console.log(
    `${OUTPUT}: ${bytes.length} bytes; from Blender's at worst: positions ` +
      `${(worst.position * 1000).toFixed(3)} mm, normals ${worst.normal.toFixed(2)}°, ` +
      `UVs ${(worst.uv * 1000).toFixed(3)} mm`,
  );
}

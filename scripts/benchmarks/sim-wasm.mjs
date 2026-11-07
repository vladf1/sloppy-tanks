// Times the `sim_wasm` example (crates/core/examples/sim_wasm.rs) built several ways,
// alternating the builds in one process so they share the machine's state:
// bun scripts/benchmarks/sim-wasm.mjs <map index> <seed> <ticks> <wasm>...
// Map indexes: 0 village, 1 stress-test, 2 harbor, 3 superstress. ROUNDS sets the rounds
// (default 5). Prints every sample and each build's body-pose fingerprint.
import { readFileSync } from "node:fs";

const [map, seed, ticks, ...paths] = process.argv.slice(2);
const modules = paths.map((path) => new WebAssembly.Module(readFileSync(path)));
const samples = paths.map(() => []);
const fingerprints = paths.map(() => "");
const rounds = Number(process.env.ROUNDS ?? 5);
for (let round = 0; round < rounds; round++) {
  const order = round % 2 ? [...modules.keys()].reverse() : [...modules.keys()];
  for (const index of order) {
    const { exports } = new WebAssembly.Instance(modules[index], {});
    exports.prepare(Number(map), Number(seed));
    const start = performance.now();
    const played = exports.steps(Number(ticks));
    samples[index].push((performance.now() - start) / played);
    fingerprints[index] = `${BigInt.asUintN(64, exports.fingerprint()).toString(16)}, ${played} ticks`;
  }
}
paths.forEach((path, index) => {
  const values = samples[index];
  const mean = values.reduce((sum, value) => sum + value, 0) / values.length;
  const list = values.map((value) => value.toFixed(4)).join(" ");
  console.log(`${path}: mean ${mean.toFixed(4)} ms/tick (${list}), fingerprint ${fingerprints[index]}`);
});

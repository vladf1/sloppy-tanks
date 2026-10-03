// Record `stress-mesh-uploads.txt`, the mesh uploads and frees of one Stress Grid
// session, for the mesh page planner's replay test (`../mesh_pages.rs`).
//
// It reads them off the engine's WebGPU buffers, so it needs an engine that gave each
// mesh buffers of its own: one from before the commit "Store meshes in shared mesh
// pages". Serve a checkout of its parent and pass the URL:
//
//   git worktree add ../before-pages \
//     "$(git log -1 --format=%H --grep='^Store meshes in shared mesh pages$')^"
//   cd ../before-pages && pnpm install && pnpm run wasm && pnpm run dev
//   SLOPPY_URL=<its URL> node crates/render/tests/fixtures/record-mesh-uploads.mjs \
//     > crates/render/tests/fixtures/stress-mesh-uploads.txt
//
// The session: Battle Setup on the village, a round with seed 12345 and the Stress
// Grid's extra tanks (`stress()`) under autoplay, a tower `collapse()`, then a restart,
// which ends the recording. The old `MeshStore` made a mesh's buffers one after another
// with labels that name them: a surface mesh's "mesh vertices" (48-byte `Vertex`), its
// "mesh indices" and, with effect vec4s, its "mesh effect attributes" before or after
// the indices; a merged shadow group's "shadow merged vertices" (24-byte
// `ShadowVertex`) and "shadow merged indices". A buffer's size is padded, so the counts
// come from the bytes written to it. Wall-clock play makes every recording a little
// different.
import {
  chooseMap,
  chosenMap,
  gameUrl,
  launchGame,
  startRound,
} from "../../../../scripts/browser-helpers.mjs";

const SEED = 12345;
const PLAY_MS = 10_000;
const COLLAPSE_MS = 5_000;
const RESTART_MS = 3_000;
/** Each mesh's index buffer, the vertex buffer label it pairs with, and that stride. */
const MESH_KINDS = {
  "mesh indices": { family: "S", vertices: "mesh vertices", stride: 48 },
  "shadow merged indices": { family: "H", vertices: "shadow merged vertices", stride: 24 },
};
const EFFECT_LABEL = "mesh effect attributes";

const HEADER = `\
# The mesh uploads and frees of one Stress Grid session on the WebGPU engine, as
# record-mesh-uploads.mjs recorded them: Battle Setup and the village load, the round
# start, ${PLAY_MS / 1000} seconds of play and a \`collapse()\`.
#
# + <S surface | H shadow> <vertices> <indices> [keep] [*<repeats>]
#     Upload a mesh; uploads are numbered from 0 in file order. \`keep\`: made during
#     the round but kept for the session (a cached library model), so a replayed
#     round does not upload it again.
# - <id>[-<last id>]   Free uploads.
# round                The round starts; a replay repeats everything after it.
# end                  The round ends: its uploads still live are freed, \`keep\` aside.
`;

/** Every WebGPU buffer in creation order: its label, when and in which phase it was
 * made and destroyed, and the end of the furthest write into it. */
function recordBuffers() {
  const record = (window.meshRecord = { buffers: [], phase: "load", frame: 0 });
  const entries = new WeakMap();
  const createBuffer = GPUDevice.prototype.createBuffer;
  GPUDevice.prototype.createBuffer = function (descriptor) {
    const buffer = createBuffer.call(this, descriptor);
    const entry = { label: descriptor.label ?? "", created: record.frame, phase: record.phase };
    Object.assign(entry, { deleted: null, deletedPhase: null, written: 0 });
    entries.set(buffer, entry);
    record.buffers.push(entry);
    return buffer;
  };
  const destroy = GPUBuffer.prototype.destroy;
  GPUBuffer.prototype.destroy = function () {
    const entry = entries.get(this);
    if (entry && entry.deleted === null) {
      Object.assign(entry, { deleted: record.frame, deletedPhase: record.phase });
    }
    return destroy.call(this);
  };
  const writeBuffer = GPUQueue.prototype.writeBuffer;
  GPUQueue.prototype.writeBuffer = function (buffer, offset, data, ...rest) {
    const entry = entries.get(buffer);
    if (entry) {
      // A typed array's data offset and size count its elements.
      const element = data.BYTES_PER_ELEMENT ?? 1;
      const [dataOffset = 0, size] = rest;
      const bytes = size === undefined ? data.byteLength - dataOffset * element : size * element;
      entry.written = Math.max(entry.written, Number(offset) + bytes);
    }
    return writeBuffer.call(this, buffer, offset, data, ...rest);
  };
  const raf = window.requestAnimationFrame.bind(window);
  window.requestAnimationFrame = (callback) =>
    raf((time) => {
      record.frame++;
      callback(time);
    });
}

/** The meshes made before the restart, from their index buffers and the vertex buffers
 * made just before them. A mesh without vertices or indices gets no page and is left
 * out. */
function meshes(buffers) {
  const found = [];
  buffers.forEach((indices, at) => {
    const kind = MESH_KINDS[indices.label];
    if (!kind || indices.phase === "restart") return;
    let vertices = buffers[at - 1];
    if (vertices?.label === EFFECT_LABEL) vertices = buffers[at - 2];
    if (vertices?.label !== kind.vertices) {
      throw new Error(`no ${kind.vertices} before buffer ${at} (${indices.label})`);
    }
    const counts = { vertices: vertices.written / kind.stride, indices: indices.written / 4 };
    if (!counts.vertices || !counts.indices) return;
    found.push({
      family: kind.family,
      ...counts,
      created: indices.created,
      inRound: indices.phase === "round",
      // Frees from the restart on are the round's end.
      freed: indices.deletedPhase === null || indices.deletedPhase === "restart" ? null : indices,
      kept: indices.phase === "round" && indices.deleted === null,
    });
  });
  return found;
}

/** The fixture text: uploads in order, frees before uploads within a frame (a reset
 * releases before the new round uploads), repeated uploads and runs of frees merged. */
function fixture(found) {
  const events = [];
  found.forEach((mesh, id) => {
    events.push({ frame: mesh.created, free: false, id, inRound: mesh.inRound, mesh });
    if (mesh.freed) {
      const inRound = mesh.freed.deletedPhase === "round";
      events.push({ frame: mesh.freed.deleted, free: true, id, inRound, mesh });
    }
  });
  events.sort((a, b) => a.frame - b.frame || b.free - a.free || a.id - b.id);
  const lines = [];
  let round = false;
  for (const { free, id, inRound, mesh } of events) {
    if (!round && inRound) {
      lines.push("round");
      round = true;
    }
    const last = lines.at(-1);
    if (free) {
      if (last?.free && last.last + 1 === id) last.last = id;
      else lines.push({ free: true, first: id, last: id });
    } else {
      const upload = `+ ${mesh.family} ${mesh.vertices} ${mesh.indices}${mesh.kept ? " keep" : ""}`;
      if (last?.upload === upload) last.repeats++;
      else lines.push({ upload, repeats: 1 });
    }
  }
  lines.push("end");
  const text = lines.map((line) => {
    if (typeof line === "string") return line;
    if (line.free) return `- ${line.first}${line.last === line.first ? "" : `-${line.last}`}`;
    return line.repeats > 1 ? `${line.upload} *${line.repeats}` : line.upload;
  });
  return `${HEADER}${text.join("\n")}\n`;
}

const { browser, context, page } = await launchGame();
try {
  await context.addInitScript(recordBuffers);
  page.setDefaultTimeout(300_000);
  await page.goto(gameUrl);
  await page.waitForFunction(
    () => document.querySelector("#startup-overlay")?.dataset.state === "ready",
  );
  if ((await chosenMap(page)) !== "village") await chooseMap(page, "village");
  const phase = (name) => page.evaluate((name) => (window.meshRecord.phase = name), name);
  await phase("round");
  await startRound(page);
  await page.evaluate((seed) => {
    const debug = window.sloppy;
    debug.game.debug_configure(seed, 12, 0);
    debug.start();
    debug.stress();
    debug.exactResolution();
    debug.overview(true);
    debug.autoplay();
  }, SEED);
  await page.waitForTimeout(PLAY_MS);
  await page.evaluate(() => window.sloppy.collapse());
  await page.waitForTimeout(COLLAPSE_MS);
  await phase("restart");
  await page.evaluate(() => window.sloppy.restart());
  await page.waitForTimeout(RESTART_MS);
  const buffers = await page.evaluate(() => window.meshRecord.buffers);
  process.stdout.write(fixture(meshes(buffers)));
} finally {
  await browser.close();
}

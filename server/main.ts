import RAPIER from "@dimforge/rapier3d-compat";
import { CONTENT_VERSION } from "../src/net/protocol";
import { createServer } from "./server";

/** Local Vite and preview origins; deployments list their public sites in ALLOWED_ORIGINS. */
const LOCAL_ORIGINS = [5173, 5174, 5175, 4179, 4180].flatMap((port) => [
  `http://127.0.0.1:${port}`,
  `http://localhost:${port}`,
]);

const host = process.env.HOST ?? "127.0.0.1",
  port = Number(process.env.PORT ?? 8787),
  loopback = ["127.0.0.1", "::1", "localhost"].includes(host);
const origins = process.env.ALLOWED_ORIGINS?.split(",")
  .map((origin) => origin.trim())
  .filter(Boolean);

// Simulation needs the WASM module before the first room creates a Rapier world.
await RAPIER.init();
const server = createServer({
  allowedOrigins: origins?.length ? origins : LOCAL_ORIGINS,
  multiplayerEnabled: process.env.MULTIPLAYER_ENABLED !== "false",
  // Only a loopback listener can be sure its X-Forwarded-For came from the local proxy.
  trustProxy: (process.env.TRUST_PROXY ?? String(loopback)) === "true",
});
const address = await server.listen(port, host);
console.log(
  `Sloppy Tanks multiplayer listening on ${address.address}:${address.port} (content ${CONTENT_VERSION})`,
);

let stopping = false;
for (const signal of ["SIGTERM", "SIGINT"] as const) {
  process.on(signal, () => {
    if (stopping) return;
    stopping = true;
    console.log(`${signal}: resetting ${server.rooms.size} room(s)`);
    void server.close().then(() => process.exit(0));
  });
}

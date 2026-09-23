import { randomBytes } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
const evidence = new URL("../artifacts/performance/multiplayer/", import.meta.url);
await mkdir(evidence, { recursive: true });
const keyFile = new URL("experiment-key", evidence);
let key;
try {
  key = (await readFile(keyFile, "utf8")).trim();
} catch (error) {
  if (error.code !== "ENOENT") throw error;
  key = randomBytes(32).toString("hex");
  await writeFile(keyFile, key, { mode: 0o600, flag: "wx" });
}
if (!/^[a-f0-9]{64}$/.test(key)) throw new Error("Invalid local experiment key");
await writeFile(
  new URL(".dev.vars", import.meta.url),
  `EXPERIMENT_ENABLED=true\nMULTIPLAYER_ENABLED=true\nEXPERIMENT_KEY=${key}\n`,
  { mode: 0o600 },
);
await writeFile(new URL("worker-secrets.json", evidence), JSON.stringify({ EXPERIMENT_KEY: key }), {
  mode: 0o600,
});
console.log(
  "Prepared ignored local experiment credentials. No credentials were printed or deployed.",
);

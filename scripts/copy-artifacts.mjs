import { mkdirSync, copyFileSync, readdirSync } from "node:fs";
const output = process.argv[2] ?? "dist";
mkdirSync(`${output}/artifacts`, { recursive: true });
copyFileSync("benchmark.html", `${output}/benchmark.html`);
for (const name of readdirSync("artifacts"))
  if (name.endsWith(".json")) copyFileSync(`artifacts/${name}`, `${output}/artifacts/${name}`);

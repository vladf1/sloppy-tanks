import { mkdirSync, copyFileSync, readdirSync } from "node:fs";
mkdirSync("dist/artifacts", { recursive: true });
copyFileSync("benchmark.html", "dist/benchmark.html");
for (const name of readdirSync("artifacts"))
  if (name.endsWith(".json"))
    copyFileSync(`artifacts/${name}`, `dist/artifacts/${name}`);

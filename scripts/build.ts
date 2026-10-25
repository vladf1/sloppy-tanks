import { parseArgs } from "node:util";
import { buildSite } from "./site-build.ts";
const { values } = parseArgs({
  options: {
    "dev-site": { type: "boolean", default: false },
    outdir: { type: "string" },
    base: { type: "string" },
    minify: { type: "string" },
  },
});
const outdir = values.outdir ?? (values["dev-site"] ? "dist-dev" : "dist");
const start = performance.now();
const result = await buildSite({
  devSite: values["dev-site"],
  outdir,
  base: values.base,
  minify: values.minify === "false" ? false : undefined,
});
console.log(
  `Built ${result.files.size} files plus public/ in ${outdir} (${(performance.now() - start).toFixed(0)} ms, base ${result.base})`,
);

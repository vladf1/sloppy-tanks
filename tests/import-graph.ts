// The module graph behind the import boundary tests (`*-imports.test.ts`).
import { fileURLToPath } from "node:url";
import { build, type Metafile } from "esbuild";

/** The modules bundling `entry` reaches. Imports matching `external` stay out, such as
 * the generated engine glue (`pnpm run wasm`), whose import is all a test needs. */
export async function moduleGraph(
  entry: string,
  external: RegExp,
  splitting = false,
): Promise<Metafile["inputs"]> {
  const { metafile } = await build({
    absWorkingDir: fileURLToPath(new URL("..", import.meta.url)),
    entryPoints: [entry],
    bundle: true,
    splitting,
    write: false,
    metafile: true,
    format: "esm",
    outdir: "unused",
    loader: { ".css": "empty" },
    logLevel: "silent",
    plugins: [
      {
        name: "external",
        setup(context) {
          context.onResolve({ filter: external }, ({ path }) => ({ path, external: true }));
        },
      },
    ],
  });
  return metafile.inputs;
}

/** The import path from `entry` to `target`, for a readable failure. */
export function importChain(inputs: Metafile["inputs"], entry: string, target: string): string[] {
  const parents = new Map<string, string>([[entry, ""]]);
  const queue = [entry];
  for (const file of queue) {
    for (const { path } of inputs[file]?.imports ?? []) {
      if (!parents.has(path)) {
        parents.set(path, file);
        queue.push(path);
      }
    }
  }
  const chain = [target];
  while (parents.get(chain[0])) {
    chain.unshift(parents.get(chain[0])!);
  }
  return chain;
}

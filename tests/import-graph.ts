// The module graph behind the import boundary tests (`*-imports.test.ts`).
import path from "node:path";
import { fileURLToPath } from "node:url";
type Metafile = Bun.BuildMetafile;

const root = fileURLToPath(new URL("..", import.meta.url));
/** A module's path from the checkout; Bun writes them from the working directory. */
const fromRoot = (file: string) => path.relative(root, path.resolve(file));

/** The modules bundling `entry` reaches. Imports matching `external` stay out, such as
 * the generated engine glue (`bun run wasm`), whose import is all a test needs. */
export async function moduleGraph(
  entry: string,
  external: RegExp,
  splitting = false,
): Promise<Metafile["inputs"]> {
  const { metafile } = await Bun.build({
    root,
    entrypoints: [entry],
    splitting,
    metafile: true,
    format: "esm",
    target: "browser",
    plugins: [
      {
        name: "external",
        setup(context) {
          context.onResolve({ filter: external }, ({ path }) => ({ path, external: true }));
        },
      },
    ],
  });
  if (!metafile) throw new Error("Missing bundle import graph");
  const inputs: Metafile["inputs"] = {};
  for (const [file, input] of Object.entries(metafile.inputs)) {
    inputs[fromRoot(file)] = {
      ...input,
      imports: input.imports.map((entry) =>
        entry.external ? entry : { ...entry, path: fromRoot(entry.path) },
      ),
    };
  }
  return inputs;
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

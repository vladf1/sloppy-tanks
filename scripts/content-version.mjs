import { createHash } from "node:crypto";
import { readFile, readdir } from "node:fs/promises";
/** Both builds hash the same gameplay, wire and presentation sources and pinned engine versions. */
export async function contentVersion() {
  const root = new URL("../", import.meta.url),
    hash = createHash("sha256");
  for (const directory of ["src/game/", "src/net/"]) {
    for (const file of (await readdir(new URL(directory, root)))
      .filter((file) => file.endsWith(".ts"))
      .sort()) {
      hash.update(directory + file + "\0").update(await readFile(new URL(directory + file, root)));
    }
  }
  const pkg = JSON.parse(await readFile(new URL("package.json", root), "utf8"));
  for (const name of ["@dimforge/rapier3d", "three"])
    hash.update(name + "=" + pkg.dependencies[name]);
  return hash.digest("hex").slice(0, 24);
}

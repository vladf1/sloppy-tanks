import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { lstat, readFile, readlink } from "node:fs/promises";
import { resolve } from "node:path";

export async function desktopUiSourceHash(root) {
  const hash = createHash("sha256");
  const files = execFileSync(
    "git",
    ["ls-files", "--cached", "--others", "--exclude-standard", "-z"],
    { cwd: root, encoding: "utf8" },
  );
  for (const file of files.split("\0").filter(Boolean).sort()) {
    const path = resolve(root, file);
    hash.update(file);
    // Hash the repository's link, without following it into a skill directory.
    if ((await lstat(path)).isSymbolicLink()) {
      hash.update("symlink\0");
      hash.update(await readlink(path));
    } else {
      hash.update(await readFile(path));
    }
  }
  return hash.digest("hex");
}

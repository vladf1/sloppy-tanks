import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdir, mkdtemp, rm, symlink, unlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { desktopUiSourceHash } from "../scripts/desktop-ui-source.mjs";

test("UI evidence hashes linked skills and source changes without following directories", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "sloppy-ui-source-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  execFileSync("git", ["init", "--quiet", root]);
  for (const name of ["shared", "skills", "artifacts"]) {
    await mkdir(join(root, name));
  }
  await writeFile(join(root, ".gitignore"), "artifacts/\n");
  const source = join(root, "shared", "SKILL.md");
  const link = join(root, "skills", "parity");
  const linkType = process.platform === "win32" ? "junction" : "dir";
  await writeFile(source, "UI parity instructions\n");
  await symlink("../shared", link, linkType);
  execFileSync("git", ["add", ".gitignore", "shared", "skills"], { cwd: root });
  const baseline = await desktopUiSourceHash(root);
  assert.equal(await desktopUiSourceHash(root), baseline);

  await writeFile(join(root, "artifacts", "capture.png"), "ignored capture");
  assert.equal(await desktopUiSourceHash(root), baseline);

  await writeFile(source, "Updated instructions\n");
  assert.notEqual(await desktopUiSourceHash(root), baseline);
  await writeFile(source, "UI parity instructions\n");
  assert.equal(await desktopUiSourceHash(root), baseline);

  await writeFile(join(root, "new-source.rs"), "// untracked source\n");
  assert.notEqual(await desktopUiSourceHash(root), baseline);
  await unlink(join(root, "new-source.rs"));

  await unlink(link);
  await symlink("../skills", link, linkType);
  assert.notEqual(await desktopUiSourceHash(root), baseline);
});

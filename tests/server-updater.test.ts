import { afterEach, beforeEach, test } from "bun:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  appendFileSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { SERVER_IMAGE, VERSIONS_LABEL } from "../scripts/servers.mjs";

/** deploy/server/sloppy-tanks-update under a scratch root, with fake podman, systemctl
 * and curl: a registry and local image store in files, units that "run" by recording
 * their image and port, and servers whose /health and /stats answer from those files. */
const updater = fileURLToPath(new URL("../deploy/server/sloppy-tanks-update", import.meta.url));
const template = fileURLToPath(new URL("../deploy/server/sloppy-tanks.container", import.meta.url));

const FAKE_PODMAN = `#!/bin/bash
F=$FAKE
resolve() {
  if [[ $1 == sha256:* && -f $F/images/$1 ]]; then echo "$1"; return; fi
  awk -v r="$1" '$1 == r { id = $2 } END { if (id) print id; else exit 1 }' "$F/local"
}
untag() { grep -v "^$1 " "$F/local" >"$F/local.new" || true; mv "$F/local.new" "$F/local"; }
case "$1 $2" in
  "pull --quiet")
    id=$(awk -v r="$3" '$1 == r { id = $2 } END { if (id) print id; else exit 1 }' "$F/remote") || exit 1
    untag "$3"; echo "$3 $id" >>"$F/local"; echo "$id" ;;
  "image inspect")
    id=$(resolve "$5") || exit 125
    if [[ $4 == "{{.Id}}" ]]; then echo "\${id#sha256:}"; exit 0; fi
    key=$(sed 's/.*Labels "\\(.*\\)"}}$/\\1/' <<<"$4")
    grep "^$key=" "$F/images/$id" | cut -d= -f2- || true ;;
  "image ls") grep "^$6" "$F/local" || true ;;
  "image rm") shift 2; for ref in "$@"; do untag "$ref"; done ;;
  "image prune") ;;
  tag*) id=$(resolve "$2") || exit 125; untag "$3"; echo "$3 $id" >>"$F/local" ;;
  *) echo "fake podman: $*" >&2; exit 1 ;;
esac
`;

const FAKE_SYSTEMCTL = `#!/bin/bash
F=$FAKE
quadlet=$SLOPPY_TANKS_ROOT/etc/containers/systemd
case $1 in
  daemon-reload | is-enabled) ;;
  restart)
    image=$(sed -n 's/^Image=//p' "$quadlet/$2.container")
    port=$(sed -n 's/.* PORT=\\([0-9]*\\).*/\\1/p' "$quadlet/$2.container")
    echo "$image $port" >"$F/services/$2" ;;
  stop) rm -f "$F/services/$2" ;;
  is-active) [[ $2 == --quiet ]] && shift; [[ $2 == caddy ]] || [[ -f $F/services/$2 ]] ;;
  reload) [[ -f $F/reload-fails ]] && exit 1; echo reload >>"$F/caddy-reloads" ;;
  *) echo "fake systemctl: $*" >&2; exit 1 ;;
esac
`;

const FAKE_CURL = `#!/bin/bash
F=$FAKE
url=\${@: -1}
label() { grep "^me.fridman.sloppy-tanks.$2=" "$F/images/$1" | cut -d= -f2-; }
service_on() { grep -l " $1$" "$F"/services/* 2>/dev/null | head -n 1; }
case $url in
  *"/token?"*) echo '{"token":"t"}' ;;
  */manifests/*) [[ -f $F/digest/\${url##*/} ]] || exit 22; echo "Docker-Content-Digest: $(cat "$F/digest/\${url##*/}")" ;;
  http://127.0.0.1:*/health | http://127.0.0.1:*/stats)
    port=\${url#http://127.0.0.1:}; port=\${port%%/*}
    service=$(service_on "$port"); [[ -n $service ]] || exit 7
    image=$(cut -d' ' -f1 "$service")
    grep -qxF "$image" "$F/broken" 2>/dev/null && exit 7
    if [[ $url == */health ]]; then
      echo "{\\"contentVersion\\": \\"$(label "$image" content-version)\\", \\"serverBuild\\": \\"$(label "$image" server-build)\\"}"
    else
      echo "{\\"players\\": $(cat "$F/players/$port" 2>/dev/null || echo 0), \\"rooms\\": []}"
    fi ;;
  *) echo "fake curl: $url" >&2; exit 1 ;;
esac
`;

let scratch: string;
let fake: string;
let root: string;
let bin: string;

beforeEach(() => {
  scratch = mkdtempSync(join(tmpdir(), "sloppy-updater-"));
  fake = join(scratch, "fake");
  root = join(scratch, "root");
  bin = join(scratch, "bin");
  for (const dir of ["images", "digest", "services", "players"])
    mkdirSync(join(fake, dir), { recursive: true });
  for (const file of ["remote", "local"]) writeFileSync(join(fake, file), "");
  mkdirSync(join(root, "etc/sloppy-tanks"), { recursive: true });
  mkdirSync(join(root, "etc/containers/systemd"), { recursive: true });
  copyFileSync(template, join(root, "etc/sloppy-tanks/sloppy-tanks.container"));
  writeFileSync(join(root, "etc/sloppy-tanks/site"), "45-63-56-58.nip.io\n");
  mkdirSync(bin);
  const tools = {
    podman: FAKE_PODMAN,
    systemctl: FAKE_SYSTEMCTL,
    curl: FAKE_CURL,
    flock: "#!/bin/sh\n",
    sleep: "#!/bin/sh\n",
  };
  for (const [name, script] of Object.entries(tools))
    writeFileSync(join(bin, name), script, { mode: 0o755 });
  environment("production");
});

afterEach(() => rmSync(scratch, { recursive: true, force: true }));

function environment(name: string) {
  writeFileSync(
    join(root, "etc/sloppy-tanks/server.env"),
    `SERVER_ENVIRONMENT=${name}\nDIRECTORY_URL=https://directory.example/\n`,
  );
}

const hex = (seed: string, length: number) => seed.repeat(length).slice(0, length);
/** A server image of `content` (eight hex digits) and server build `build`, in the fake
 * registry under its digest; returns `content@digest` as the versions label lists it. */
function image(content: string, build: string): string {
  const id = `sha256:${hex(content + build, 64)}`;
  const digest = `sha256:${hex(build + content, 64)}`;
  writeFileSync(
    join(fake, "images", id),
    `me.fridman.sloppy-tanks.content-version=${content}\nme.fridman.sloppy-tanks.server-build=${build}\n`,
  );
  appendFileSync(join(fake, "remote"), `${SERVER_IMAGE}@${digest} ${id}\n`);
  return `${content}@${digest}`;
}
const idOf = (content: string, build: string) => `sha256:${hex(content + build, 64)}`;

/** Points the environment's tag at an image listing `versions`, as promote-server.mjs does. */
function publish(tag: string, versions: string[], move: string) {
  const id = `sha256:${hex(move, 64)}`;
  const [content] = versions[0].split("@");
  writeFileSync(
    join(fake, "images", id),
    `me.fridman.sloppy-tanks.content-version=${content}\n${VERSIONS_LABEL}=${versions.join(" ")}\n`,
  );
  appendFileSync(join(fake, "remote"), `${SERVER_IMAGE}:${tag} ${id}\n`);
  writeFileSync(join(fake, "digest", tag), `sha256:${hex(move, 64)}\n`);
}

function run(...args: string[]) {
  const result = spawnSync("bash", [updater, ...args], {
    encoding: "utf8",
    env: {
      ...process.env,
      PATH: `${bin}:${process.env.PATH}`,
      FAKE: fake,
      SLOPPY_TANKS_ROOT: root,
      SLOPPY_TANKS_HEALTH_TIMEOUT: "0",
    },
  });
  return { status: result.status, output: result.stdout + result.stderr };
}

/** What runs: unit name → `<image> <port>`. */
function services(): Record<string, string> {
  const dir = join(fake, "services");
  return Object.fromEntries(
    spawnSync("ls", [dir], { encoding: "utf8" })
      .stdout.split("\n")
      .filter(Boolean)
      .map((unit) => [unit, readFileSync(join(dir, unit), "utf8").trim()]),
  );
}
const caddyfile = () => readFileSync(join(root, "etc/sloppy-tanks/caddy/Caddyfile"), "utf8");
const unitFile = (content: string) =>
  readFileSync(join(root, `etc/containers/systemd/sloppy-tanks-${content}.container`), "utf8");
const players = (port: number, count: number) =>
  writeFileSync(join(fake, "players", String(port)), String(count));

test("a machine runs every listed version on its own port and path, the newest at the root", () => {
  publish("production", [image("aaaaaaaa", "b1"), image("bbbbbbbb", "b1")], "1");
  const result = run("pull");
  assert.equal(result.status, 0, result.output);
  assert.deepEqual(services(), {
    "sloppy-tanks-aaaaaaaa": `${idOf("aaaaaaaa", "b1")} 8787`,
    "sloppy-tanks-bbbbbbbb": `${idOf("bbbbbbbb", "b1")} 8788`,
  });
  assert.match(unitFile("aaaaaaaa"), /^Environment=HOST=127\.0\.0\.1 PORT=8787$/m);
  assert.match(unitFile("bbbbbbbb"), /^ContainerName=sloppy-tanks-bbbbbbbb$/m);
  // Each server announces where the directory lists it at its version's path, whatever
  // build it is.
  const settings = join(root, "var/lib/sloppy-tanks/versions/aaaaaaaa/server.env");
  assert.ok(unitFile("aaaaaaaa").includes(`\nEnvironmentFile=${settings}\n`));
  assert.equal(
    readFileSync(settings, "utf8"),
    "SERVER_ENVIRONMENT=production\nDIRECTORY_URL=https://directory.example/v/aaaaaaaa\n",
  );
  const config = caddyfile();
  assert.match(config, /^45-63-56-58\.nip\.io \{$/m);
  assert.match(config, /handle_path \/v\/aaaaaaaa\* \{\n\t\treverse_proxy 127\.0\.0\.1:8787 /);
  assert.match(config, /handle_path \/v\/bbbbbbbb\* \{\n\t\treverse_proxy 127\.0\.0\.1:8788 /);
  assert.match(config, /\thandle \{\n\t\treverse_proxy 127\.0\.0\.1:8787 /);
  // A reload for another version leaves every match's WebSocket open.
  assert.equal(config.match(/stream_close_delay 14400s/g)?.length, 3);
});

test("a version pushed out of the list stops once nobody plays on it, and a new one starts at once", () => {
  const a = image("aaaaaaaa", "b1");
  publish("production", [a, image("bbbbbbbb", "b1")], "1");
  assert.equal(run("pull").status, 0);
  publish("production", [image("cccccccc", "b1"), a], "2");
  players(8788, 2);
  let result = run("auto");
  assert.equal(result.status, 0, result.output);
  assert.equal(services()["sloppy-tanks-cccccccc"], `${idOf("cccccccc", "b1")} 8789`);
  assert.ok(services()["sloppy-tanks-bbbbbbbb"], "players keep their server");
  assert.match(result.output, /Waiting: 2 playing on sloppy-tanks-bbbbbbbb/);
  assert.match(caddyfile(), /\thandle \{\n\t\treverse_proxy 127\.0\.0\.1:8789 /);
  players(8788, 0);
  result = run("auto");
  assert.equal(result.status, 0, result.output);
  assert.equal(services()["sloppy-tanks-bbbbbbbb"], undefined);
  assert.ok(!existsSync(join(root, "etc/containers/systemd/sloppy-tanks-bbbbbbbb.container")));
  assert.doesNotMatch(caddyfile(), /bbbbbbbb/);
  assert.deepEqual(Object.keys(services()).sort(), [
    "sloppy-tanks-aaaaaaaa",
    "sloppy-tanks-cccccccc",
  ]);
});

test("a server-only change replaces its version's server once its rooms are empty", () => {
  publish("production", [image("aaaaaaaa", "b1")], "1");
  assert.equal(run("pull").status, 0);
  publish("production", [image("aaaaaaaa", "b2")], "2");
  players(8787, 1);
  let result = run("auto");
  assert.match(result.output, /Waiting: 1 playing/);
  assert.equal(services()["sloppy-tanks-aaaaaaaa"], `${idOf("aaaaaaaa", "b1")} 8787`);
  players(8787, 0);
  result = run("auto");
  assert.equal(result.status, 0, result.output);
  assert.equal(services()["sloppy-tanks-aaaaaaaa"], `${idOf("aaaaaaaa", "b2")} 8787`);
  // The image it replaced is the one to roll back to.
  result = run("rollback");
  assert.equal(result.status, 0, result.output);
  assert.equal(services()["sloppy-tanks-aaaaaaaa"], `${idOf("aaaaaaaa", "b1")} 8787`);
  assert.match(run("auto").output, /Holding: Rollback/);
});

test("dev starts, switches and stops versions without waiting for players", () => {
  environment("dev");
  publish("dev", [image("aaaaaaaa", "b1")], "1");
  assert.equal(run("pull").status, 0);
  players(8787, 3);
  publish("dev", [image("bbbbbbbb", "b1")], "2");
  const result = run("auto");
  assert.equal(result.status, 0, result.output);
  assert.deepEqual(Object.keys(services()), ["sloppy-tanks-bbbbbbbb"]);
});

test("an image that fails its health check is stopped and not tried again until the list changes", () => {
  const a = image("aaaaaaaa", "b1");
  writeFileSync(join(fake, "broken"), `${idOf("aaaaaaaa", "b1")}\n`);
  publish("production", [a], "1");
  assert.equal(run("pull").status, 1, "a pull reports the version it could not start");
  assert.deepEqual(services(), {});
  assert.ok(!existsSync(join(root, "etc/containers/systemd/sloppy-tanks-aaaaaaaa.container")));
  assert.match(caddyfile(), /respond "No game server runs here yet" 503/);
  const again = run("auto");
  assert.equal(again.status, 0);
  assert.doesNotMatch(again.output, /Starting/);
  assert.equal(run("pull").status, 1);
  publish("production", [image("aaaaaaaa", "b2")], "2");
  assert.equal(run("auto").status, 0);
  assert.equal(services()["sloppy-tanks-aaaaaaaa"], `${idOf("aaaaaaaa", "b2")} 8787`);
});

test("a tag pushed before the list existed stands for its own image", () => {
  const id = `sha256:${hex("1", 64)}`;
  writeFileSync(
    join(fake, "images", id),
    "me.fridman.sloppy-tanks.content-version=aaaaaaaa\nme.fridman.sloppy-tanks.server-build=b1\n",
  );
  appendFileSync(join(fake, "remote"), `${SERVER_IMAGE}:production ${id}\n`);
  writeFileSync(join(fake, "digest", "production"), `sha256:${hex("1", 64)}\n`);
  const result = run("pull");
  assert.equal(result.status, 0, result.output);
  assert.deepEqual(services(), { "sloppy-tanks-aaaaaaaa": `${id} 8787` });
});

test("an image another environment's checkout loads runs as its version and holds", () => {
  publish("production", [image("aaaaaaaa", "b1")], "1");
  assert.equal(run("pull").status, 0);
  image("bbbbbbbb", "b9");
  appendFileSync(join(fake, "local"), `sloppy-tanks-server:b9 ${idOf("bbbbbbbb", "b9")}\n`);
  const result = run("local", "sloppy-tanks-server:b9");
  assert.equal(result.status, 0, result.output);
  assert.equal(services()["sloppy-tanks-bbbbbbbb"], `${idOf("bbbbbbbb", "b9")} 8788`);
  // The site's root serves what was deployed, which the deploy then checks.
  assert.match(caddyfile(), /\thandle \{\n\t\treverse_proxy 127\.0\.0\.1:8788 /);
  assert.match(run("auto").output, /Holding: SSH deploy/);
});

test("a failed switch keeps the image to roll back to", () => {
  publish("production", [image("aaaaaaaa", "b1")], "1");
  assert.equal(run("pull").status, 0);
  publish("production", [image("aaaaaaaa", "b2")], "2");
  assert.equal(run("auto").status, 0);
  writeFileSync(join(fake, "broken"), `${idOf("aaaaaaaa", "b3")}\n`);
  publish("production", [image("aaaaaaaa", "b3")], "3");
  assert.equal(run("auto").status, 0);
  assert.equal(services()["sloppy-tanks-aaaaaaaa"], `${idOf("aaaaaaaa", "b2")} 8787`);
  assert.equal(run("rollback").status, 0);
  assert.equal(services()["sloppy-tanks-aaaaaaaa"], `${idOf("aaaaaaaa", "b1")} 8787`);
});

test("an SSH deploy of another unlisted version replaces the last one", () => {
  publish("production", [image("aaaaaaaa", "b1")], "1");
  assert.equal(run("pull").status, 0);
  for (const content of ["bbbbbbbb", "cccccccc"]) {
    image(content, "b9");
    appendFileSync(join(fake, "local"), `sloppy-tanks-server:${content} ${idOf(content, "b9")}\n`);
    assert.equal(run("local", `sloppy-tanks-server:${content}`).status, 0);
  }
  assert.deepEqual(Object.keys(services()).sort(), [
    "sloppy-tanks-aaaaaaaa",
    "sloppy-tanks-cccccccc",
  ]);
});

test("a Caddy reload that fails is tried again at the next check", () => {
  publish("production", [image("aaaaaaaa", "b1")], "1");
  assert.equal(run("pull").status, 0);
  const before = caddyfile();
  writeFileSync(join(fake, "reload-fails"), "");
  publish("production", [image("bbbbbbbb", "b1"), image("aaaaaaaa", "b1")], "2");
  assert.match(run("auto").output, /Caddy did not reload/);
  assert.equal(caddyfile(), before);
  rmSync(join(fake, "reload-fails"));
  assert.equal(run("auto").status, 0);
  assert.match(caddyfile(), /handle_path \/v\/bbbbbbbb\*/);
});

import { afterAll, beforeAll, beforeEach, test } from "bun:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { formatDeployment, parseDeployment, registry } from "../scripts/deployment.mjs";

/** A registry on this machine that speaks the parts of the OCI distribution API the
 * deployment file uses, as GHCR does: a bearer token from a challenge (push only with
 * credentials), blob uploads, manifests by tag, and blob reads redirected to storage that
 * refuses a registry token. */
const blobs = new Map<string, Buffer>();
const manifests = new Map<string, { type: string; body: string }>();
let tokenRequests: string[] = [];
let server: ReturnType<typeof Bun.serve>;
const digestOf = (bytes: Buffer | string) =>
  `sha256:${createHash("sha256").update(bytes).digest("hex")}`;

beforeAll(() => {
  server = Bun.serve({
    port: 0,
    async fetch(request) {
      const url = new URL(request.url);
      const origin = url.origin;
      if (url.pathname === "/token") {
        const scope = url.searchParams.get("scope") ?? "";
        tokenRequests.push(scope);
        const basic = request.headers.get("Authorization")?.startsWith("Basic ");
        if (scope.endsWith("push") && !basic) return new Response(null, { status: 401 });
        return Response.json({ token: scope.endsWith("push") ? "push" : "pull" });
      }
      if (url.pathname.startsWith("/storage/")) {
        if (request.headers.get("Authorization")) return new Response(null, { status: 400 });
        const bytes = blobs.get(url.pathname.slice("/storage/".length));
        return bytes ? new Response(bytes) : new Response(null, { status: 404 });
      }
      const auth = request.headers.get("Authorization");
      const write = ["PUT", "POST"].includes(request.method);
      if (auth !== "Bearer push" && (write || auth !== "Bearer pull")) {
        const scope = `repository:team/server:${write ? "pull,push" : "pull"}`;
        return new Response(null, {
          status: 401,
          headers: {
            "WWW-Authenticate": `Bearer realm="${origin}/token",service="test",scope="${scope}"`,
          },
        });
      }
      const path = url.pathname.replace("/v2/team/server", "");
      if (path === "/blobs/uploads/" && request.method === "POST") {
        return new Response(null, { status: 202, headers: { Location: "/v2/team/server/up/1" } });
      }
      if (path.startsWith("/up/") && request.method === "PUT") {
        const bytes = Buffer.from(await request.arrayBuffer());
        if (digestOf(bytes) !== url.searchParams.get("digest")) {
          return new Response(null, { status: 400 });
        }
        blobs.set(digestOf(bytes), bytes);
        return new Response(null, { status: 201 });
      }
      if (path.startsWith("/blobs/")) {
        const digest = path.slice("/blobs/".length);
        if (!blobs.has(digest)) return new Response(null, { status: 404 });
        if (request.method === "HEAD") return new Response(null, { status: 200 });
        return new Response(null, { status: 307, headers: { Location: `/storage/${digest}` } });
      }
      if (path.startsWith("/manifests/")) {
        const tag = path.slice("/manifests/".length);
        if (request.method === "PUT") {
          const body = await request.text();
          for (const { digest } of [JSON.parse(body).config, ...JSON.parse(body).layers]) {
            if (!blobs.has(digest)) return new Response(null, { status: 400 });
          }
          manifests.set(tag, { type: request.headers.get("Content-Type") ?? "", body });
          return new Response(null, {
            status: 201,
            headers: { "Docker-Content-Digest": digestOf(body) },
          });
        }
        const manifest = manifests.get(tag);
        return manifest
          ? new Response(manifest.body, { headers: { "Content-Type": manifest.type } })
          : new Response(null, { status: 404 });
      }
      return new Response(null, { status: 404 });
    },
  });
  process.env.GITHUB_TOKEN = "secret";
  process.env.GITHUB_ACTOR = "ci";
});
const { GITHUB_TOKEN, GITHUB_ACTOR } = process.env;
afterAll(() => {
  server.stop(true);
  Object.assign(process.env, { GITHUB_TOKEN, GITHUB_ACTOR });
  if (GITHUB_TOKEN === undefined) delete process.env.GITHUB_TOKEN;
  if (GITHUB_ACTOR === undefined) delete process.env.GITHUB_ACTOR;
});
beforeEach(() => {
  blobs.clear();
  manifests.clear();
  tokenRequests = [];
});

const digest = (n: number) => `sha256:${String(n).repeat(64)}`;
const deployment = {
  versions: [
    { contentVersion: "aaaaaaaa", digest: digest(1), release: "1.6.0.2", serverBuild: "b2" },
    { contentVersion: "bbbbbbbb", digest: digest(2), release: "1.6.0.1", serverBuild: "b1" },
  ],
};

test("a deployment file written to the registry reads back as written", async () => {
  const servers = registry(`localhost:${server.port}/team/server`);
  assert.equal(await servers.read("deployment-dev"), undefined);
  const written = await servers.write("deployment-dev", deployment);
  assert.match(written, /^sha256:[0-9a-f]{64}$/);
  assert.deepEqual(await servers.read("deployment-dev"), deployment);
  // Pushing asks for a push token with credentials; reading asks for none.
  assert.ok(tokenRequests.includes("repository:team/server:pull,push"));
  const manifest = JSON.parse(manifests.get("deployment-dev")!.body);
  assert.equal(manifest.config.mediaType, "application/vnd.sloppy-tanks.deployment.config.v1+json");
  assert.equal(manifest.layers.length, 1);
  assert.equal(blobs.get(manifest.layers[0].digest)!.toString(), formatDeployment(deployment));
  // A fresh reader starts anonymous, as the machines do.
  tokenRequests = [];
  assert.deepEqual(
    await registry(`localhost:${server.port}/team/server`).read("deployment-dev"),
    deployment,
  );
  assert.deepEqual(tokenRequests, ["repository:team/server:pull"]);
});

test("a file that is not what its digest says, or not a deployment file, is refused", async () => {
  const servers = registry(`localhost:${server.port}/team/server`);
  await servers.write("deployment-dev", deployment);
  const { layers } = JSON.parse(manifests.get("deployment-dev")!.body);
  blobs.set(layers[0].digest, Buffer.from(formatDeployment({ versions: [] })));
  await assert.rejects(servers.read("deployment-dev"), /is not what it says/);
  manifests.set("deployment-dev", {
    type: "application/vnd.oci.image.manifest.v1+json",
    body: JSON.stringify({ config: { mediaType: "application/vnd.oci.image.config.v1+json" } }),
  });
  await assert.rejects(servers.read("deployment-dev"), /no deployment file/);
});

test("a deployment lists each content version once, with hex names and sha256 digests", () => {
  assert.deepEqual(parseDeployment(formatDeployment(deployment)), deployment);
  assert.deepEqual(parseDeployment('{"versions":[]}'), { versions: [] });
  for (const bad of [
    "{}",
    '{"versions":[{"contentVersion":"../x","digest":"' + digest(1) + '"}]}',
    '{"versions":[{"contentVersion":"aaaaaaaa","digest":"sha256:12"}]}',
    JSON.stringify({ versions: [deployment.versions[0], deployment.versions[0]] }),
  ]) {
    assert.throws(() => parseDeployment(bad), undefined, bad);
  }
});

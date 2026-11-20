import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";

/**
 * The deployment file: what an environment's machines run, a small JSON document kept in
 * the server image's repository under the tag `deployment-<environment>` beside the images
 * it names. It is an OCI artifact, not an image: a manifest whose one layer is the JSON.
 *
 *   { "versions": [{ "contentVersion": "6e7c…", "digest": "sha256:8191…",
 *                    "release": "1.6.0.353", "serverBuild": "e459…" }, …] }
 *
 * Newest first; each machine runs a server for every version listed, from the image of
 * that digest, and serves the first at its root (deploy/server/sloppy-tanks-update reads
 * it). scripts/promote-server.mjs writes it. Like an image, each file pushed is fixed;
 * the tag moves to the newest, and the registry keeps the ones before as untagged versions.
 * Reading needs no credentials; writing needs a token that can write packages
 * (GITHUB_TOKEN, else `gh auth token`).
 */
export const deploymentTag = (environment) => `deployment-${environment}`;

const MANIFEST_TYPE = "application/vnd.oci.image.manifest.v1+json";
/** The config's media type names what the artifact is, which registries that predate
 * OCI 1.1's artifactType read too. */
const CONFIG_TYPE = "application/vnd.sloppy-tanks.deployment.config.v1+json";
const FILE_TYPE = "application/vnd.sloppy-tanks.deployment.v1+json";
const CONFIG = Buffer.from("{}");
const CONTENT_VERSION = /^[0-9a-f]{8,64}$/;
const DIGEST = /^sha256:[0-9a-f]{64}$/;

const sha256 = (bytes) => `sha256:${createHash("sha256").update(bytes).digest("hex")}`;

/** The deployment a file's text holds, checked as the machines check it: a content
 * version names a unit, a path and a directory there. */
export function parseDeployment(text) {
  const value = JSON.parse(text);
  if (!Array.isArray(value?.versions)) throw new Error("A deployment file lists versions");
  const seen = new Set();
  const versions = value.versions.map((version) => {
    const { contentVersion, digest, release, serverBuild } = version ?? {};
    if (!CONTENT_VERSION.test(contentVersion) || !DIGEST.test(digest)) {
      throw new Error(`Not a deployed version: ${JSON.stringify(version)}`);
    }
    if (seen.has(contentVersion)) throw new Error(`${contentVersion} is listed twice`);
    seen.add(contentVersion);
    return { contentVersion, digest, release, serverBuild };
  });
  return { versions };
}

/** A deployment as its file's text, one version a line so a diff reads well. */
export function formatDeployment({ versions }) {
  const lines = versions.map((version) => `    ${JSON.stringify(version)}`);
  return `{\n  "versions": [\n${lines.join(",\n")}\n  ]\n}\n`;
}

/** A client of the OCI distribution API for one repository, `host/name` as in an image
 * reference; a registry on this machine is spoken to over plain HTTP (the tests'). */
export function registry(repository) {
  const host = repository.slice(0, repository.indexOf("/"));
  const name = repository.slice(repository.indexOf("/") + 1);
  const local = /^(localhost|127\.0\.0\.1)(:\d+)?$/.test(host);
  const base = `${local ? "http" : "https"}://${host}/v2/${name}`;
  const tokens = new Map();

  /** A bearer token for `actions` on the repository, as the registry's challenge asks;
   * anonymous for reads of a public repository. */
  async function token(challenge, actions) {
    const key = actions;
    if (tokens.has(key)) return tokens.get(key);
    const fields = Object.fromEntries(
      [...challenge.matchAll(/(\w+)="([^"]*)"/g)].map(([, field, value]) => [field, value]),
    );
    if (!/^Bearer /i.test(challenge) || !fields.realm) {
      throw new Error(`${host} asks for an authentication it does not name: ${challenge}`);
    }
    const url = new URL(fields.realm);
    if (fields.service) url.searchParams.set("service", fields.service);
    url.searchParams.set("scope", `repository:${name}:${actions}`);
    const headers = {};
    if (actions.includes("push")) {
      const { user, secret } = credentials();
      headers.Authorization = `Basic ${Buffer.from(`${user}:${secret}`).toString("base64")}`;
    }
    const response = await fetch(url, { headers, signal: AbortSignal.timeout(15_000) });
    if (!response.ok)
      throw new Error(`${url.origin} refused a ${actions} token: ${response.status}`);
    const body = await response.json();
    const value = body.token ?? body.access_token;
    tokens.set(key, value);
    return value;
  }

  /** A request to the repository, authenticating once the registry asks. */
  async function request(path, init = {}, actions = "pull") {
    const url = path.startsWith("http") ? path : `${base}${path}`;
    const send = (bearer) =>
      fetch(url, {
        redirect: "manual",
        signal: AbortSignal.timeout(30_000),
        ...init,
        headers: { ...init.headers, ...(bearer ? { Authorization: `Bearer ${bearer}` } : {}) },
      });
    let response = await send(tokens.get(actions));
    if (response.status === 401) {
      tokens.delete(actions);
      response = await send(await token(response.headers.get("WWW-Authenticate") ?? "", actions));
    }
    return response;
  }

  /** A blob's bytes. Registries redirect blob reads to storage that takes no token. */
  async function blob(digest) {
    let response = await request(`/blobs/${digest}`);
    if ([301, 302, 303, 307, 308].includes(response.status)) {
      const location = new URL(response.headers.get("Location"), base).href;
      response = await fetch(location, { signal: AbortSignal.timeout(30_000) });
    }
    if (!response.ok) throw new Error(`${repository}@${digest} answered ${response.status}`);
    const bytes = Buffer.from(await response.arrayBuffer());
    if (sha256(bytes) !== digest) throw new Error(`${repository}@${digest} is not what it says`);
    return bytes;
  }

  async function upload(bytes) {
    const digest = sha256(bytes);
    const known = await request(`/blobs/${digest}`, { method: "HEAD" }, "pull,push");
    if (known.ok) return digest;
    const started = await request("/blobs/uploads/", { method: "POST" }, "pull,push");
    if (started.status !== 202)
      throw new Error(`${repository} refused an upload: ${started.status}`);
    const location = new URL(started.headers.get("Location"), base);
    location.searchParams.set("digest", digest);
    const done = await request(
      location.href,
      {
        method: "PUT",
        headers: { "Content-Type": "application/octet-stream" },
        body: bytes,
      },
      "pull,push",
    );
    if (done.status !== 201) throw new Error(`${repository} refused ${digest}: ${done.status}`);
    return digest;
  }

  return {
    repository,

    /** The deployment the tag names, or nothing when the registry holds none. */
    async read(tag) {
      const response = await request(`/manifests/${tag}`, { headers: { Accept: MANIFEST_TYPE } });
      if (response.status === 404) return undefined;
      if (!response.ok) throw new Error(`${repository}:${tag} answered ${response.status}`);
      const manifest = await response.json();
      const layer = manifest.layers?.[0];
      if (manifest.config?.mediaType !== CONFIG_TYPE || layer?.mediaType !== FILE_TYPE) {
        throw new Error(`${repository}:${tag} is no deployment file`);
      }
      return parseDeployment((await blob(layer.digest)).toString("utf8"));
    },

    /** Pushes `deployment` as a new file and points the tag at it; returns its digest. */
    async write(tag, deployment) {
      const file = Buffer.from(formatDeployment(deployment));
      const layer = { mediaType: FILE_TYPE, digest: await upload(file), size: file.length };
      const config = { mediaType: CONFIG_TYPE, digest: await upload(CONFIG), size: CONFIG.length };
      const manifest = JSON.stringify({
        schemaVersion: 2,
        mediaType: MANIFEST_TYPE,
        config,
        layers: [layer],
        annotations: { "org.opencontainers.image.created": new Date().toISOString() },
      });
      const response = await request(
        `/manifests/${tag}`,
        { method: "PUT", headers: { "Content-Type": MANIFEST_TYPE }, body: manifest },
        "pull,push",
      );
      if (response.status !== 201) {
        throw new Error(
          `${repository} refused ${tag}: ${response.status} ${await response.text()}`,
        );
      }
      return response.headers.get("Docker-Content-Digest") ?? sha256(Buffer.from(manifest));
    },
  };
}

/** Who writes: CI's GITHUB_TOKEN as its actor, else the GitHub CLI's login. */
function credentials() {
  const gh = (...args) => spawnSync("gh", args, { encoding: "utf8" });
  const secret = process.env.GITHUB_TOKEN || gh("auth", "token").stdout?.trim();
  if (!secret) {
    throw new Error(
      "Writing a deployment file needs GITHUB_TOKEN or the GitHub CLI logged in with a token " +
        "that can write packages: gh auth refresh -h github.com -s write:packages",
    );
  }
  const user = process.env.GITHUB_ACTOR || gh("api", "user", "-q", ".login").stdout?.trim();
  return { user: user || "token", secret };
}

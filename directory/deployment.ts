// Reads an environment's deployment file for the overview: the server versions its machines
// run (scripts/deployment.mjs writes it, deploy/server/sloppy-tanks-update follows it).
// Free of Cloudflare imports so tests run it.

/** The server images' repository, where the deployment files sit beside them under
 * `deployment-<environment>` (SERVER_IMAGE in scripts/servers.mjs). It is public, so reading
 * takes an anonymous token. */
const REGISTRY = "https://ghcr.io";
const REPOSITORY = "vladf1/sloppy-tanks-server";
const MANIFEST_TYPE = "application/vnd.oci.image.manifest.v1+json";
const CONFIG_TYPE = "application/vnd.sloppy-tanks.deployment.config.v1+json";
const FILE_TYPE = "application/vnd.sloppy-tanks.deployment.v1+json";
const CONTENT_VERSION = /^[0-9a-f]{8,64}$/;
const DIGEST = /^sha256:[0-9a-f]{64}$/;
/** Each registry request; the overview shows the file as unavailable after that. */
const TIMEOUT_MS = 10_000;
/** The directory answers from its cache for this long, so the overview's 5 s refreshes
 * reach the registry about once a minute. */
export const DEPLOYMENT_CACHE_SECONDS = 60;

export interface DeployedVersion {
  contentVersion: string;
  /** The server image's digest. */
  digest: string;
  release?: string;
  serverBuild?: string;
}

export interface Deployment {
  /** Newest first: the machines serve the first at their root. */
  versions: DeployedVersion[];
  /** When the file was written, ISO 8601, from its manifest. */
  writtenAt?: string;
}

const stamp = (value: unknown) =>
  typeof value === "string" && value.length >= 1 && value.length <= 128 ? value : undefined;

/** The versions a file's text lists, checked as the machines check them. */
export function parseDeployment(text: string): DeployedVersion[] {
  const value = JSON.parse(text) as { versions?: unknown };
  if (!Array.isArray(value?.versions)) throw new Error("A deployment file lists versions");
  return value.versions.map((version: Record<string, unknown> | null) => {
    const { contentVersion, digest, release, serverBuild } = version ?? {};
    if (
      typeof contentVersion !== "string" ||
      !CONTENT_VERSION.test(contentVersion) ||
      typeof digest !== "string" ||
      !DIGEST.test(digest)
    ) {
      throw new Error(`Not a deployed version: ${JSON.stringify(version)}`);
    }
    return { contentVersion, digest, release: stamp(release), serverBuild: stamp(serverBuild) };
  });
}

async function sha256(bytes: ArrayBuffer): Promise<string> {
  const hash = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  return `sha256:${[...hash].map((byte) => byte.toString(16).padStart(2, "0")).join("")}`;
}

/** The environment's deployment file, or undefined when the registry holds none (an
 * environment nothing was published to yet). Throws when the registry fails or the file
 * is not one. */
export async function readDeployment(
  environment: string,
  fetcher: typeof fetch = fetch,
): Promise<Deployment | undefined> {
  const get = (url: string, headers: Record<string, string> = {}) =>
    fetcher(url, { headers, redirect: "manual", signal: AbortSignal.timeout(TIMEOUT_MS) });
  const scope = `repository:${REPOSITORY}:pull`;
  const tokenResponse = await get(`${REGISTRY}/token?service=ghcr.io&scope=${scope}`);
  if (!tokenResponse.ok) throw new Error(`the registry refused a token: ${tokenResponse.status}`);
  const { token } = (await tokenResponse.json()) as { token?: string };
  const authorization = { Authorization: `Bearer ${token}` };
  const tag = `deployment-${environment}`;
  const manifestResponse = await get(`${REGISTRY}/v2/${REPOSITORY}/manifests/${tag}`, {
    ...authorization,
    Accept: MANIFEST_TYPE,
  });
  if (manifestResponse.status === 404) return undefined;
  if (!manifestResponse.ok) throw new Error(`${tag} answered ${manifestResponse.status}`);
  const manifest = (await manifestResponse.json()) as {
    config?: { mediaType?: string };
    layers?: { mediaType?: string; digest?: string }[];
    annotations?: Record<string, string>;
  };
  const layer = manifest.layers?.[0];
  if (
    manifest.config?.mediaType !== CONFIG_TYPE ||
    layer?.mediaType !== FILE_TYPE ||
    !DIGEST.test(layer.digest ?? "")
  ) {
    throw new Error(`${tag} is no deployment file`);
  }
  // The registry redirects blob reads to storage that takes no token.
  let blob = await get(`${REGISTRY}/v2/${REPOSITORY}/blobs/${layer.digest}`, authorization);
  const location = blob.headers.get("Location");
  if (blob.status >= 300 && blob.status < 400 && location) {
    blob = await get(new URL(location, REGISTRY).href);
  }
  if (!blob.ok) throw new Error(`${tag}'s file answered ${blob.status}`);
  const bytes = await blob.arrayBuffer();
  if ((await sha256(bytes)) !== layer.digest) throw new Error(`${tag}'s file is not what it says`);
  return {
    versions: parseDeployment(new TextDecoder().decode(bytes)),
    writtenAt: stamp(manifest.annotations?.["org.opencontainers.image.created"]),
  };
}

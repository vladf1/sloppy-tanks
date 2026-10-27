/** Where this site's game servers are listed in the server directory, or undefined
 * when this site has no multiplayer. */
export function directoryAddress(): URL | undefined {
  const directory = import.meta.env.DEV
    ? new URLSearchParams(location.search).get("directory")
    : null;
  return resolveDirectoryAddress(
    directory,
    import.meta.env.VITE_DIRECTORY_URL,
    import.meta.env.VITE_SERVER_ENVIRONMENT,
    location.hostname,
  );
}

const isLocal = (hostname: string) => ["localhost", "127.0.0.1"].includes(hostname);

/** The directory is a development `?directory=` override, then the build's configured
 * one, then on a local page the local server, which lists itself. It is a base address,
 * `http:` or `https:` without credentials, query or fragment, and anything but a local
 * page needs `https:`. The page reads its environment's servers at
 * `/servers?environment=`, production unless the build names another. */
export function resolveDirectoryAddress(
  directory: string | null,
  configured: unknown,
  environment: unknown,
  hostname: string,
): URL | undefined {
  const local = isLocal(hostname);
  const base =
    directory ||
    (typeof configured === "string" ? configured : "") ||
    (local ? "http://127.0.0.1:8787" : "");
  if (!base) {
    return undefined;
  }
  const address = new URL(base);
  if (
    !["http:", "https:"].includes(address.protocol) ||
    address.username ||
    address.password ||
    address.search ||
    address.hash ||
    (!local && address.protocol !== "https:")
  ) {
    throw new Error("Invalid server directory configuration");
  }
  const servers = new URL("servers", address.href.replace(/\/?$/, "/"));
  servers.searchParams.set(
    "environment",
    typeof environment === "string" && environment ? environment : "production",
  );
  return servers;
}

/** A game server's WebSocket address from the directory, without a trailing slash. It
 * must be `ws:` or `wss:` without credentials, query or fragment, and anything but a local
 * page needs `wss:`. */
export function gameServerAddress(url: unknown, hostname: string): string {
  let address: URL | undefined;
  try {
    address = typeof url === "string" ? new URL(url) : undefined;
  } catch {
    address = undefined;
  }
  if (
    !address ||
    !["ws:", "wss:"].includes(address.protocol) ||
    address.username ||
    address.password ||
    address.search ||
    address.hash ||
    (!isLocal(hostname) && address.protocol !== "wss:")
  ) {
    throw new Error("Invalid game server address");
  }
  return address.href.replace(/\/$/, "");
}

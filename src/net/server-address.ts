/** The server directory's URL, or undefined when this site has no multiplayer. */
export function directoryAddress(): URL | undefined {
  const directory = import.meta.env.DEV
    ? new URLSearchParams(location.search).get("directory")
    : null;
  return resolveDirectoryAddress(directory, import.meta.env.VITE_DIRECTORY_URL, location.hostname);
}

const isLocal = (hostname: string) => ["localhost", "127.0.0.1"].includes(hostname);

/** The directory rules: a development `?directory=` override, then the build's configured
 * URL, then on a local page the local server, which lists itself at `/servers`. It must be
 * `http:` or `https:` without credentials or fragment (its query names the environment),
 * and anything but a local page needs `https:`. */
export function resolveDirectoryAddress(
  directory: string | null,
  configured: unknown,
  hostname: string,
): URL | undefined {
  const local = isLocal(hostname);
  const endpoint =
    directory ||
    (typeof configured === "string" ? configured : "") ||
    (local ? "http://127.0.0.1:8787/servers" : "");
  if (!endpoint) {
    return undefined;
  }
  const address = new URL(endpoint);
  if (
    !["http:", "https:"].includes(address.protocol) ||
    address.username ||
    address.password ||
    address.hash ||
    (!local && address.protocol !== "https:")
  ) {
    throw new Error("Invalid server directory configuration");
  }
  return address;
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

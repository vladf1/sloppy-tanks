/** The game server's WebSocket address, or undefined when this site has no multiplayer. */
export function serverAddress(): URL | undefined {
  const server = import.meta.env.DEV ? new URLSearchParams(location.search).get("server") : null;
  return resolveServerAddress(server, import.meta.env.VITE_MULTIPLAYER_URL, new URL(location.href));
}

/** The address rules: a development `?server=` override, then the build's configured
 * URL, then the local server on a local page. A configured path such as `/` names the
 * server that serves the page itself, as the multiplayer server's own copy of the site
 * does. It must be `ws:` or `wss:` without credentials, query or fragment, and anything
 * but a local page needs `wss:`. */
export function resolveServerAddress(
  server: string | null,
  configured: unknown,
  page: URL,
): URL | undefined {
  const local = ["localhost", "127.0.0.1"].includes(page.hostname);
  const endpoint =
    server ||
    (typeof configured === "string" ? configured : "") ||
    (local ? "ws://127.0.0.1:8787" : "");
  if (!endpoint) {
    return undefined;
  }
  const address = endpoint.startsWith("/")
    ? new URL(endpoint, page.origin.replace(/^http/, "ws"))
    : new URL(endpoint);
  if (
    !["ws:", "wss:"].includes(address.protocol) ||
    address.username ||
    address.password ||
    address.search ||
    address.hash ||
    (!local && address.protocol !== "wss:")
  ) {
    throw new Error("Invalid multiplayer server configuration");
  }
  return address;
}

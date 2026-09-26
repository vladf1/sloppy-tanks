/** The game server's WebSocket address, or undefined when this site has no multiplayer. */
export function serverAddress(): URL | undefined {
  const params = new URLSearchParams(location.search);
  const local = ["localhost", "127.0.0.1"].includes(location.hostname);
  const configured: unknown = import.meta.env.VITE_MULTIPLAYER_URL;
  const endpoint =
    (import.meta.env.DEV ? params.get("server") : null) ||
    (typeof configured === "string" ? configured : "") ||
    (local ? "ws://127.0.0.1:8787" : "");
  if (!endpoint) {
    return undefined;
  }
  const address = new URL(endpoint);
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

// The directory's overview page: every environment, its servers and their rooms, as plain
// HTML the Worker renders on each request. Free of Cloudflare imports so node tests run it.
import type { RoomListing } from "../src/net/room-listing";
import { ROOM_SEATS } from "../src/net/room-protocol";
import type { ListedServer } from "./listing";

/** The page reloads itself this often; servers announce changes within about 2 s. */
const REFRESH_SECONDS = 5;

export interface EnvironmentListing {
  name: string;
  servers: ListedServer[];
}

/** Text from announcements and its own markup alike goes through here before the page. */
function escape(value: string | number): string {
  return String(value).replace(
    /[&<>"']/g,
    (char) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[char]!,
  );
}

const count = (n: number, noun: string) => `${n} ${noun}${n === 1 ? "" : "s"}`;
const players = (server: ListedServer) => server.rooms.reduce((sum, room) => sum + room.players, 0);

function clock(seconds: number): string {
  const whole = Math.max(0, Math.ceil(seconds));
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, "0")}`;
}

function roomRow(room: RoomListing): string {
  const reconnecting = room.reserved - room.players;
  const cells = [
    room.room,
    room.mapMode,
    `${room.players}/${ROOM_SEATS}${reconnecting > 0 ? ` (+${reconnecting} reconnecting)` : ""}`,
    room.phase === "playing" ? `playing · ${clock(room.time)} left` : room.phase,
    room.scores.join("–"),
    room.humansOnly ? "humans only" : room.difficulty,
    `${room.roundMinutes} min`,
  ];
  return `<tr>${cells.map((cell) => `<td>${escape(cell)}</td>`).join("")}</tr>`;
}

function serverSection(server: ListedServer, now: number): string {
  const dashboard = server.url.replace(/^wss?:/, "https:") + "/dashboard";
  const seen = Math.max(0, Math.round((now - server.seen) / 1000));
  const details = [
    server.version && `v${server.version}`,
    `content ${server.contentVersion.slice(0, 8)}`,
    `build ${server.serverBuild.slice(0, 8)}`,
    count(players(server), "player"),
    `seen ${seen} s ago`,
  ].filter((detail) => detail !== undefined);
  const rooms = server.rooms.length
    ? `<table><thead><tr><th>Room</th><th>Map</th><th>Players</th><th>Phase</th><th>Score</th><th>Bots</th><th>Length</th></tr></thead><tbody>${server.rooms.map(roomRow).join("")}</tbody></table>`
    : `<p class="empty">No rooms</p>`;
  return `<section class="server"><h3><a href="${escape(dashboard)}">${escape(server.url)}</a></h3><p class="details">${details.map(escape).join(" · ")}</p>${rooms}</section>`;
}

function environmentSection({ name, servers }: EnvironmentListing, now: number): string {
  const rooms = servers.reduce((sum, server) => sum + server.rooms.length, 0);
  const summary = [
    count(servers.length, "server"),
    count(rooms, "room"),
    count(
      servers.reduce((sum, server) => sum + players(server), 0),
      "player",
    ),
  ].join(" · ");
  const body = servers.length
    ? servers.map((server) => serverSection(server, now)).join("")
    : `<p class="empty">No servers</p>`;
  return `<section class="environment"><h2>${escape(name)} <small>${summary}</small></h2>${body}</section>`;
}

export function renderPage(environments: EnvironmentListing[], now: number): string {
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="refresh" content="${REFRESH_SECONDS}">
<title>Sloppy Tanks servers</title>
<style>
:root { color-scheme: light dark; --muted: #6b7280; --line: #d1d5db; --card: #f9fafb; }
@media (prefers-color-scheme: dark) { :root { --muted: #9ca3af; --line: #374151; --card: #111827; } }
body { margin: 0 auto; max-width: 960px; padding: 16px; font: 14px/1.4 system-ui, sans-serif; }
h1 { font-size: 20px; margin: 0 0 4px; }
h2 { font-size: 16px; margin: 24px 0 8px; text-transform: capitalize; }
h2 small, .details, .empty, .note { color: var(--muted); font-weight: normal; text-transform: none; }
h3 { font-size: 14px; margin: 0; }
.server { background: var(--card); border: 1px solid var(--line); border-radius: 8px; padding: 12px; margin: 8px 0; overflow-x: auto; }
.details { margin: 2px 0 8px; }
table { border-collapse: collapse; width: 100%; }
th, td { text-align: left; padding: 4px 8px 4px 0; border-top: 1px solid var(--line); white-space: nowrap; }
th { font-weight: 600; border-top: 0; }
.empty { margin: 0; }
</style>
</head>
<body>
<h1>Sloppy Tanks servers</h1>
<p class="note">Game servers announce their rooms here; this page refreshes every ${REFRESH_SECONDS} s.</p>
${environments.map((environment) => environmentSection(environment, now)).join("\n")}
</body>
</html>
`;
}

// Fills the overview (index.html) from each environment's /servers list. Values only ever
// reach the page as text, through the markup's templates.
const REFRESH_MS = 5000;
const ROOM_SEATS = 8;

const count = (n, noun) => `${n} ${noun}${n === 1 ? "" : "s"}`;
const players = (server) => server.rooms.reduce((sum, room) => sum + room.players, 0);

function clock(seconds) {
  const whole = Math.max(0, Math.ceil(seconds));
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, "0")}`;
}

/** A copy of template `id` with each `data-field` element's text set from `fields`. */
function fromTemplate(id, fields) {
  const copy = document.getElementById(id).content.firstElementChild.cloneNode(true);
  for (const element of copy.querySelectorAll("[data-field]")) {
    element.textContent = fields[element.dataset.field] ?? "";
  }
  return copy;
}

function roomRow(room) {
  const reconnecting = room.reserved - room.players;
  return fromTemplate("room", {
    room: room.room,
    map: room.mapMode,
    players: `${room.players}/${ROOM_SEATS}${reconnecting > 0 ? ` (+${reconnecting} reconnecting)` : ""}`,
    phase: room.phase === "playing" ? `playing · ${clock(room.time)} left` : room.phase,
    score: room.scores.join("–"),
    bots: room.humansOnly ? "humans only" : room.difficulty,
    length: `${room.roundMinutes} min`,
  });
}

function serverCard(server) {
  const details = [
    server.version && `v${server.version}`,
    `content ${server.contentVersion.slice(0, 8)}`,
    `build ${server.serverBuild.slice(0, 8)}`,
    count(players(server), "player"),
    `seen ${Math.max(0, Math.round((Date.now() - server.seen) / 1000))} s ago`,
  ];
  const card = fromTemplate("server", {
    url: server.url,
    details: details.filter(Boolean).join(" · "),
  });
  card.querySelector("a").href = `${server.url.replace(/^wss?:/, "https:")}/dashboard`;
  card.querySelector("tbody").replaceChildren(...server.rooms.map(roomRow));
  card.querySelector("table").hidden = !server.rooms.length;
  card.querySelector(".empty").hidden = !!server.rooms.length;
  return card;
}

async function showEnvironment(section) {
  const summary = section.querySelector("h2 small");
  try {
    const response = await fetch(`servers?environment=${section.dataset.environment}`, {
      cache: "no-store",
    });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const { servers } = await response.json();
    const rooms = servers.reduce((sum, server) => sum + server.rooms.length, 0);
    summary.textContent = [
      count(servers.length, "server"),
      count(rooms, "room"),
      count(servers.reduce((sum, server) => sum + players(server), 0), "player"),
    ].join(" · ");
    section.querySelector(".servers").replaceChildren(...servers.map(serverCard));
    section.querySelector(":scope > .empty").hidden = !!servers.length;
  } catch (error) {
    summary.textContent = `unavailable: ${error.message}`;
  }
}

async function refresh() {
  await Promise.all([...document.querySelectorAll("[data-environment]")].map(showEnvironment));
  setTimeout(refresh, REFRESH_MS);
}

refresh();

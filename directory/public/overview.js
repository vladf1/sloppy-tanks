// Fills the overview (index.html) from each environment's /servers list. Values only ever
// reach the page as text, through the markup's templates.
const REFRESH_MS = 5000;
/** A probe gives up after this, and the server shows as unreachable. */
const PROBE_TIMEOUT_MS = 3000;
const ROOM_SEATS = 8;

/** The latest round trip from this browser to each server, in ms (null when unreachable). */
const latencies = new Map();
/** Servers with a probe under way, so a slow one never piles up probes. */
const probing = new Set();

const number = new Intl.NumberFormat();
const count = (n, noun) => `${number.format(n)} ${noun}${n === 1 ? "" : "s"}`;
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

const writtenTime = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
  timeStyle: "short",
});

/** The environment's deployment file, with how many listed servers announce each version:
 * none means its machines have not started it (or are down). A server-only promotion keeps
 * the content version, so a server counts only when its server build matches too. */
function deploymentCard(deployment, servers) {
  const card = fromTemplate("deployment", {
    written: deployment.writtenAt
      ? `written ${writtenTime.format(new Date(deployment.writtenAt))}`
      : "",
  });
  card.querySelector("tbody").replaceChildren(
    ...deployment.versions.map((version, index) => {
      const announcing = servers.filter(
        (server) =>
          server.contentVersion === version.contentVersion &&
          (!version.serverBuild || server.serverBuild === version.serverBuild),
      ).length;
      return fromTemplate("deployed-version", {
        release: version.release ? `v${version.release}` : "",
        content: `${version.contentVersion.slice(0, 8)}${index === 0 ? " (root)" : ""}`,
        build: version.serverBuild?.slice(0, 8) ?? "",
        image: version.digest.slice(0, 19),
        servers: count(announcing, "server"),
      });
    }),
  );
  return card;
}

/** Environments with a deployment read under way: a slow registry never piles up reads, and
 * a late answer never replaces a newer one. */
const readingDeployments = new Set();

/** The deployment block of a section: the file, a note when the environment has none, or
 * why it could not be read. */
async function showDeployment(section, servers) {
  const block = section.querySelector(".deployment");
  const environment = section.dataset.environment;
  if (readingDeployments.has(environment)) return;
  readingDeployments.add(environment);
  try {
    const response = await fetch(`deployment?environment=${environment}`);
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const { deployment } = await response.json();
    if (deployment) {
      block.replaceChildren(deploymentCard(deployment, servers));
    } else {
      // deploy/server/sloppy-tanks-update starts a new machine on production's newest server
      // until its own environment's file appears; it does not follow production's after that.
      const note = document.createElement("p");
      note.className = "empty";
      note.textContent =
        environment === "production"
          ? "No deployment file"
          : "No deployment file: a new machine starts on production's newest server";
      block.replaceChildren(note);
    }
  } catch (error) {
    const note = document.createElement("p");
    note.className = "empty";
    note.textContent = `Deployment file unavailable: ${error.message}`;
    block.replaceChildren(note);
  } finally {
    readingDeployments.delete(environment);
  }
}

/** The environment's join total. Servers built before they announced joins have no count,
 * so a sum missing some says how many servers it covers rather than passing for the total. */
function joinsText(servers) {
  const reporting = servers.filter((server) => server.joins !== undefined);
  if (!reporting.length) return "";
  const total = count(
    reporting.reduce((sum, server) => sum + server.joins, 0),
    "join",
  );
  return reporting.length === servers.length
    ? total
    : `${total} on ${reporting.length} of ${count(servers.length, "server")}`;
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

/** Times a request from this browser to the server's /health. The first request to a server
 * also pays for DNS, TCP and TLS, so it goes untimed and the next one, over the same
 * connection, is measured. The response is opaque: only the timing matters. */
async function measure(url) {
  const health = `${url.replace(/^wss?:/, "https:")}/health`;
  const ping = async () => {
    const start = performance.now();
    await fetch(health, {
      mode: "no-cors",
      cache: "no-store",
      signal: AbortSignal.timeout(PROBE_TIMEOUT_MS),
    });
    return Math.round(performance.now() - start);
  };
  probing.add(url);
  try {
    if (!latencies.has(url)) await ping();
    latencies.set(url, await ping());
  } catch {
    latencies.set(url, null);
  } finally {
    probing.delete(url);
  }
}

function latencyText(url) {
  if (!latencies.has(url)) return "";
  const ms = latencies.get(url);
  return ms === null ? "unreachable from here" : `${ms} ms from here`;
}

const startTime = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });

/** When the server started, in this browser's time zone, and its process's load, for
 * servers that announce them. */
function loadText(server) {
  const memory = (mb) => (mb < 1024 ? `${Math.round(mb)} MB` : `${+(mb / 1024).toFixed(1)} GB`);
  const parts = [];
  if (server.startedAt) parts.push(`started ${startTime.format(new Date(server.startedAt))}`);
  if (server.cpuPercent !== undefined) {
    // Like top, 100% is one core busy.
    const cores = server.cpus > 1 ? ` of one core · ${server.cpus} cores` : " of 1 core";
    parts.push(`CPU ${server.cpuPercent.toFixed(1)}%${server.cpus ? cores : ""}`);
  }
  if (server.rssMB !== undefined) {
    const host = server.hostMemoryMB ? ` of ${memory(server.hostMemoryMB)}` : "";
    parts.push(`memory ${memory(server.rssMB)}${host}`);
  }
  return parts.join(" · ");
}

function serverCard(server) {
  const details = [
    server.version && `v${server.version}`,
    `content ${server.contentVersion.slice(0, 8)}`,
    `build ${server.serverBuild.slice(0, 8)}`,
    count(players(server), "player"),
    server.joins !== undefined && count(server.joins, "join"),
    `seen ${Math.max(0, Math.round((Date.now() - server.seen) / 1000))} s ago`,
  ];
  const card = fromTemplate("server", {
    url: server.url,
    latency: latencyText(server.url),
    details: details.filter(Boolean).join(" · "),
    load: loadText(server),
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
    const response = await fetch(`servers?environment=${section.dataset.environment}&all=1`, {
      cache: "no-store",
    });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const { servers } = await response.json();
    const rooms = servers.reduce((sum, server) => sum + server.rooms.length, 0);
    summary.textContent = [
      count(servers.length, "server"),
      count(rooms, "room"),
      count(servers.reduce((sum, server) => sum + players(server), 0), "player"),
      joinsText(servers),
    ]
      .filter(Boolean)
      .join(" · ");
    // The registry answers slower than the list, so the cards never wait for it.
    void showDeployment(section, servers);
    const cards = servers.map(serverCard);
    section.querySelector(".servers").replaceChildren(...cards);
    section.querySelector(":scope > .empty").hidden = !!servers.length;
    // The lists refresh on their own clock; each card shows its server's latency once
    // the probe answers or gives up.
    servers.forEach((server, index) => {
      if (probing.has(server.url)) return;
      void measure(server.url).then(() => {
        cards[index].querySelector('[data-field="latency"]').textContent = latencyText(server.url);
      });
    });
  } catch (error) {
    summary.textContent = `unavailable: ${error.message}`;
  }
}

let refreshTimer = 0;
let refreshing = false;

/** Refreshes every list, then again after REFRESH_MS while the tab is visible. A hidden tab
 * makes no requests at all; showing it again refreshes at once. */
async function refresh() {
  refreshing = true;
  try {
    await Promise.all([...document.querySelectorAll("[data-environment]")].map(showEnvironment));
  } finally {
    refreshing = false;
  }
  if (!document.hidden) refreshTimer = setTimeout(refresh, REFRESH_MS);
}

document.addEventListener("visibilitychange", () => {
  clearTimeout(refreshTimer);
  // A refresh under way schedules the next one itself when it ends.
  if (!document.hidden && !refreshing) void refresh();
});

void refresh();

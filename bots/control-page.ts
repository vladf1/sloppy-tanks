/** Operator page. The API is open: anyone with the URL can start and stop bots. */
export const controlPage = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>Traffic Bots and Pilots</title>
<style>
  :root { color-scheme: light dark; --bg: #f6f6f3; --fg: #1d1d1b; --muted: #6b6b66; --line: #d8d8d2; --accent: #2f6f4f; --bad: #a33; }
  @media (prefers-color-scheme: dark) { :root { --bg: #161615; --fg: #ececea; --muted: #9a9a94; --line: #34342f; --accent: #6fbf8f; --bad: #e77; } }
  body { margin: 0; padding: 16px; background: var(--bg); color: var(--fg); font: 14px/1.4 system-ui, sans-serif; }
  main { max-width: 980px; margin: 0 auto; }
  h1 { font-size: 20px; margin: 0 0 12px; }
  form { display: flex; flex-wrap: wrap; gap: 8px; align-items: end; margin-bottom: 12px; }
  label { display: grid; gap: 2px; color: var(--muted); font-size: 12px; }
  input, select, button { font: inherit; padding: 6px 8px; border: 1px solid var(--line); border-radius: 6px; background: transparent; color: var(--fg); }
  input[type=number] { width: 72px; }
  button { cursor: pointer; background: var(--accent); color: var(--bg); border-color: var(--accent); }
  button.secondary { background: transparent; color: var(--fg); border-color: var(--line); }
  table { width: 100%; border-collapse: collapse; font-variant-numeric: tabular-nums; }
  th, td { text-align: left; padding: 4px 6px; border-bottom: 1px solid var(--line); white-space: nowrap; }
  th { color: var(--muted); font-weight: 500; }
  .scroll { overflow-x: auto; }
  .error { color: var(--bad); }
  .muted { color: var(--muted); }
  section { margin-top: 20px; }
  fieldset { display: flex; flex-wrap: wrap; gap: 8px; align-items: end; border: 1px solid var(--line); border-radius: 6px; padding: 8px; margin: 0; }
  fieldset[disabled] { opacity: 0.5; }
  legend { color: var(--muted); font-size: 12px; }
</style>
</head>
<body>
<main>
  <h1>Traffic bots and pilots</h1>
  <p class="muted">Traffic bots drive at random to load a server. Pilots play: the bot brain drives each one through the browser's own network client, from the region it runs in. Hunt a player to have pilots take the other side and come after them; escort one to have them follow and fight beside them.</p>
  <form id="start">
    <label>Region <select id="region">
      <option value="wnam">wnam: Western N. America</option><option value="enam">enam: Eastern N. America</option>
      <option value="sam">sam: South America</option><option value="weur">weur: Western Europe</option>
      <option value="eeur">eeur: Eastern Europe</option><option value="apac">apac: Asia-Pacific</option>
      <option value="oc">oc: Oceania</option><option value="afr">afr: Africa</option><option value="me">me: Middle East</option>
    </select></label>
    <label>Bots or pilots <input id="bots" type="number" min="1" max="32" value="1"></label>
    <label>Minutes <input id="minutes" type="number" min="1" max="360" value="5"></label>
    <label>Per room <input id="perRoom" type="number" min="1" max="8" value="1"></label>
    <label>Room code (optional) <input id="room" size="10" maxlength="8"></label>
    <label><span><input id="host" type="checkbox"> Create rooms when none are open</span></label>
    <label><span><input id="brain" type="checkbox"> Pilots (bot brain, at most 8)</span></label>
    <fieldset id="pilotOptions" disabled>
      <legend>Pilots</legend>
      <label>Mission <select id="mission"><option value="skirmish">skirmish</option><option value="hunt">hunt</option><option value="escort">escort</option></select></label>
      <label>Player name <input id="player" size="12" maxlength="24" placeholder="for hunt or escort"></label>
      <label>Skill <select id="skill"><option>easy</option><option selected>normal</option><option>hard</option></select></label>
      <label>Tank <select id="kind"><option>scout</option><option selected>balanced</option><option>heavy</option></select></label>
      <label>Style <select id="style"><option value="">by tank</option><option>scout</option><option>guard</option><option>sniper</option><option>heavy</option><option>minelayer</option><option>support</option><option>artillery</option></select></label>
      <label>Map of a created room <select id="map"><option>village</option><option>harbor</option><option>quarry</option></select></label>
      <label>Servers <select id="environment"><option value="">the Worker's</option><option>production</option><option>dev</option></select></label>
    </fieldset>
    <button>Start</button>
    <button type="button" class="secondary" id="stop">Stop region</button>
    <button type="button" class="secondary" id="stopAll">Stop all</button>
  </form>
  <p id="message" class="muted"></p>
  <div id="regions"></div>
</main>
<script>
const $ = (id) => document.getElementById(id);
async function api(path, body) {
  const response = await fetch("/api/" + path, {
    method: body ? "POST" : "GET",
    headers: { "Content-Type": "application/json" },
    body: body ? JSON.stringify(body) : undefined,
  });
  const data = await response.json();
  if (!response.ok) throw new Error(data.error || response.statusText);
  return data;
}
function say(text, error) { $("message").textContent = text; $("message").className = error ? "error" : "muted"; }
const kb = (bytes) => (bytes / 1024).toFixed(1);
const ms = (value) => (value == null ? "" : Math.round(value));
const metres = (value) => (value == null ? "" : value.toFixed(2));
const TEAMS = ["blue", "red"];
$("brain").addEventListener("change", () => { $("pilotOptions").disabled = !$("brain").checked; });
function table(head, rows) {
  const table = document.createElement("table");
  const top = document.createElement("tr");
  for (const name of head) { const th = document.createElement("th"); th.textContent = name; top.append(th); }
  table.append(top);
  for (const values of rows) { const row = document.createElement("tr"); row.append(...values.map(cell)); table.append(row); }
  const scroll = document.createElement("div");
  scroll.className = "scroll";
  scroll.append(table);
  return scroll;
}
function pilotSection(region, section) {
  const squad = region.pilots;
  const info = document.createElement("p");
  info.className = "muted";
  info.textContent = (region.config.brain.mission) + " · " + region.config.brain.skill + " " + region.config.brain.kind + " · room " + (squad.room || "choosing") + (squad.server ? " on " + squad.server : "") + " · engine content " + (squad.contentVersion || "?") + (squad.restarts ? " · engine restarts " + squad.restarts : "");
  section.append(info);
  if (squad.lastError) { const p = document.createElement("p"); p.className = "error"; p.textContent = squad.lastError; section.append(p); }
  section.append(table(
    ["Pilot", "Phase", "Team", "K/D", "Hits (players)", "Taken (players)", "RTT ms", "View delay ms", "Fire→flash ms", "Hit offset m", "Corr p95 m", "Phantom hits", "Late batches", "Last"],
    squad.pilots.map((pilot) => {
      const r = pilot.report;
      if (!r) return [pilot.name, "starting", "", "", "", "", "", "", "", "", "", "", "", pilot.lastError];
      const f = r.feel;
      return [pilot.name, r.phase, TEAMS[r.team] ?? "", r.scoreKills + "/" + r.scoreDeaths, r.tally.hits + " (" + r.tally.playerHits + ")", r.tally.hitsTaken + " (" + r.tally.playerHitsTaken + ")", ms(f.rttMs), ms(f.viewDelayMs.mean), ms(f.fireToFlashMs.mean), metres(f.hitOffsetM.mean), metres(f.correctionP95M), f.phantomStrikes, f.lateBatches, pilot.lastError ?? r.endedText ?? r.notice ?? r.status];
    }),
  ));
}
function cell(value) { const td = document.createElement("td"); td.textContent = value ?? ""; return td; }
function render(status) {
  const root = $("regions");
  root.replaceChildren();
  if (!status.regions.length) { root.textContent = "No bots running. Target: " + status.server; return; }
  for (const region of status.regions) {
    const section = document.createElement("section");
    const heading = document.createElement("h2");
    heading.style.fontSize = "16px";
    const left = Math.max(0, Math.round((region.config.stopAt - Date.now()) / 60000));
    if (region.pilots) {
      heading.textContent = region.region + " (colo " + (region.colo || "?") + ") · " + region.pilots.pilots.filter((p) => p.connected).length + "/" + region.config.bots + " pilots connected · stops in " + left + " min";
    } else {
      heading.textContent = region.region + " (colo " + (region.colo || "?") + ") · " + region.bots.filter((b) => b.connected).length + "/" + region.config.bots +
        " connected · rooms " + (region.rooms.join(", ") || "none") + " · in " + kb(region.bytesInPerSecond) + " KiB/s, out " + kb(region.bytesOutPerSecond) + " KiB/s · stops in " + left + " min";
    }
    section.append(heading);
    if (region.lastError) { const p = document.createElement("p"); p.className = "error"; p.textContent = region.lastError; section.append(p); }
    if (region.pilots) { pilotSection(region, section); $("regions").append(section); continue; }
    const table = document.createElement("table");
    const head = document.createElement("tr");
    for (const name of ["Bot", "Room", "Phase", "RTT ms", "Inputs", "Rounds", "In KiB", "Out KiB", "Last error"]) { const th = document.createElement("th"); th.textContent = name; head.append(th); }
    table.append(head);
    for (const bot of region.bots) {
      const row = document.createElement("tr");
      row.append(cell(bot.name), cell(bot.room), cell(bot.phase), cell(bot.rttMs), cell(bot.inputs), cell(bot.rounds), cell(kb(bot.bytesIn)), cell(kb(bot.bytesOut)), cell(bot.lastError));
      table.append(row);
    }
    const scroll = document.createElement("div");
    scroll.className = "scroll";
    scroll.append(table);
    section.append(scroll);
    $("regions").append(section);
  }
}
async function refresh() {
  try { render(await api("status")); say("Updated " + new Date().toLocaleTimeString()); } catch (error) { say(error.message, true); }
}
$("start").addEventListener("submit", async (event) => {
  event.preventDefault();
  try {
    const brain = $("brain").checked;
    const mission = $("mission").value === "skirmish" ? "skirmish" : $("mission").value + ($("player").value.trim() ? ":" + $("player").value.trim() : "");
    await api("start", { region: $("region").value, bots: Number($("bots").value), minutes: Number($("minutes").value), perRoom: Number($("perRoom").value), room: $("room").value.trim() || undefined, host: $("host").checked,
      ...(brain ? { brain: true, mission, skill: $("skill").value, kind: $("kind").value, style: $("style").value || undefined, map: $("map").value } : {}),
      environment: $("environment").value || undefined });
    await refresh();
  } catch (error) { say(error.message, true); }
});
$("stop").addEventListener("click", async () => { try { await api("stop", { region: $("region").value }); await refresh(); } catch (error) { say(error.message, true); } });
$("stopAll").addEventListener("click", async () => { try { await api("stop", {}); await refresh(); } catch (error) { say(error.message, true); } });
refresh();
setInterval(() => { if (!document.hidden) refresh(); }, 10000);
</script>
</body>
</html>
`;

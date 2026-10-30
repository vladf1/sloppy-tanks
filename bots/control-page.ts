/** Operator page. The API is open: anyone with the URL can start and stop bots. Plain
 * HTML, CSS and script inside a template literal, so the script uses no backticks,
 * template placeholders or backslash escapes. */
export const controlPage = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>Bots and Pilots · Sloppy Tanks</title>
<style>
  :root {
    color-scheme: light dark;
    --bg: #f1f0eb; --surface: #ffffff; --surface-2: #f6f5f0; --fg: #1c1d1a; --muted: #686a61;
    --line: #dddbd2; --accent: #4a6a2a; --accent-fg: #ffffff; --accent-soft: #e7eedc;
    --bad: #b3261e; --bad-soft: #fbe9e7; --warn: #855700; --warn-soft: #fcf0d6;
    --good: #2e7a33; --good-soft: #e2f0e3; --info: #2f5fa8; --info-soft: #e5ecf7;
    --blue: #2f6fd6; --red: #d64545;
    --shadow: 0 1px 2px rgba(30, 30, 20, 0.05), 0 6px 20px rgba(30, 30, 20, 0.05);
  }
  @media (prefers-color-scheme: dark) {
    :root {
      --bg: #10110e; --surface: #1a1c17; --surface-2: #21241e; --fg: #e9eae3; --muted: #9b9d93;
      --line: #31342c; --accent: #a5c66e; --accent-fg: #11130d; --accent-soft: #29331f;
      --bad: #f2877d; --bad-soft: #3b201d; --warn: #e9b85c; --warn-soft: #382c16;
      --good: #8acb8e; --good-soft: #1e3120; --info: #93b6f2; --info-soft: #1d2a40;
      --blue: #6f9ef2; --red: #f07c7c;
      --shadow: 0 1px 2px rgba(0, 0, 0, 0.4), 0 6px 20px rgba(0, 0, 0, 0.25);
    }
  }
  * { box-sizing: border-box; }
  body { margin: 0; background: var(--bg); color: var(--fg); font: 15px/1.45 system-ui, -apple-system, "Segoe UI", sans-serif; }
  main { max-width: 1120px; margin: 0 auto; padding: 28px 16px 48px; }
  h1, h2, h3 { margin: 0; line-height: 1.2; }
  p { margin: 0; }
  [hidden] { display: none !important; }
  .muted { color: var(--muted); }
  .num { text-align: right; font-variant-numeric: tabular-nums; }

  /* Header */
  .top { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 12px 24px; }
  .brand { display: flex; align-items: center; gap: 14px; }
  .brand svg { width: 46px; height: 46px; flex: none; color: var(--accent); }
  .eyebrow { font-size: 12px; font-weight: 700; letter-spacing: 0.14em; text-transform: uppercase; color: var(--muted); }
  h1 { font-size: 26px; letter-spacing: -0.01em; }
  .top-meta { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; font-size: 13px; }
  .pill { display: inline-flex; align-items: center; gap: 6px; padding: 4px 10px; border-radius: 999px; background: var(--surface); border: 1px solid var(--line); font-weight: 600; }
  .pill::before { content: ""; width: 7px; height: 7px; border-radius: 50%; background: var(--good); }
  .intro { margin: 14px 0 24px; max-width: 760px; color: var(--muted); }

  /* Cards and sections */
  .card { background: var(--surface); border: 1px solid var(--line); border-radius: 14px; box-shadow: var(--shadow); }
  .start { padding: 22px; }
  .start > * + * { margin-top: 22px; }
  .section-title { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; margin: 36px 0 14px; }
  .section-title h2 { font-size: 19px; }
  .block-label { display: block; font-size: 13px; font-weight: 650; margin-bottom: 8px; }
  fieldset { border: 0; margin: 0; padding: 0; min-width: 0; }
  legend { padding: 0; }

  /* Fields */
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(210px, 1fr)); gap: 16px 18px; }
  .field { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
  .field > span:first-child { font-size: 13px; font-weight: 650; }
  .hint { font-size: 12.5px; color: var(--muted); }
  input, select, button { font: inherit; color: inherit; }
  input[type=text], input[type=number], select {
    height: 40px; padding: 0 12px; border: 1px solid var(--line); border-radius: 9px;
    background: var(--surface-2); width: 100%; min-width: 0;
  }
  input[type=text]:focus, input[type=number]:focus, select:focus { outline: 2px solid var(--accent); outline-offset: 1px; border-color: transparent; }
  #room { text-transform: uppercase; letter-spacing: 0.08em; font-variant-numeric: tabular-nums; }
  #room::placeholder { text-transform: none; letter-spacing: 0; }
  .suffix { position: relative; }
  .suffix input { padding-right: 44px; }
  .suffix::after { content: attr(data-suffix); position: absolute; right: 12px; top: 50%; transform: translateY(-50%); color: var(--muted); font-size: 13px; pointer-events: none; }

  /* Radio choices: the input stays focusable, its label is what shows. */
  .choice-input { position: absolute; opacity: 0; width: 1px; height: 1px; pointer-events: none; }
  .kinds { display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 12px; }
  .kind {
    display: grid; grid-template-columns: auto 1fr; gap: 2px 14px; align-items: center; cursor: pointer;
    padding: 14px 16px; border: 1.5px solid var(--line); border-radius: 12px; background: var(--surface-2);
    transition: border-color 0.15s, background 0.15s;
  }
  .kind svg { grid-row: span 2; width: 34px; height: 34px; color: var(--muted); }
  .kind strong { font-size: 16px; }
  .kind span { font-size: 13px; color: var(--muted); }
  .kind:hover { border-color: var(--muted); }
  .choice-input:checked + .kind { border-color: var(--accent); background: var(--accent-soft); }
  .choice-input:checked + .kind svg { color: var(--accent); }
  .choice-input:focus-visible + .kind, .choice-input:focus-visible + .chip { outline: 2px solid var(--accent); outline-offset: 2px; }
  .segmented { display: inline-flex; flex-wrap: wrap; gap: 4px; padding: 4px; border-radius: 11px; background: var(--surface-2); border: 1px solid var(--line); }
  .chip { padding: 6px 14px; border-radius: 8px; cursor: pointer; font-size: 14px; font-weight: 550; color: var(--muted); white-space: nowrap; }
  .chip:hover { color: var(--fg); }
  .choice-input:checked + .chip { background: var(--surface); color: var(--fg); box-shadow: 0 1px 3px rgba(0, 0, 0, 0.12); }
  .room-row { display: flex; flex-wrap: wrap; align-items: flex-end; gap: 12px 16px; }
  .room-row .field { width: 170px; }
  .room-row .segmented { align-self: flex-end; }
  .field > .segmented { align-self: flex-start; }
  .orders { padding: 18px; border-radius: 12px; background: var(--surface-2); border: 1px solid var(--line); }
  .orders .segmented { background: var(--surface); }
  .orders .choice-input:checked + .chip { background: var(--surface-2); }
  .orders input[type=text], .orders select { background: var(--surface); }
  .orders-grid { display: grid; grid-template-columns: minmax(0, 1.4fr) minmax(0, 1fr); gap: 18px 24px; }
  .orders-grid .grid { grid-template-columns: repeat(auto-fit, minmax(130px, 1fr)); }

  /* Start footer */
  .launch { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 14px 20px; padding-top: 20px; border-top: 1px solid var(--line); }
  .summary { flex: 1 1 320px; }
  .summary strong { display: block; font-size: 16px; font-weight: 600; }
  .summary .warn-text { margin-top: 4px; font-size: 13px; color: var(--warn); }
  button { cursor: pointer; border-radius: 9px; border: 1px solid transparent; font-weight: 600; }
  button:disabled { opacity: 0.6; cursor: progress; }
  .primary { height: 44px; padding: 0 22px; background: var(--accent); color: var(--accent-fg); font-size: 15px; }
  .primary:hover:not(:disabled) { filter: brightness(1.08); }
  .ghost { height: 34px; padding: 0 12px; background: transparent; border-color: var(--line); font-size: 13.5px; }
  .ghost:hover:not(:disabled) { background: var(--surface-2); }
  .danger { color: var(--bad); }
  .danger:hover:not(:disabled) { background: var(--bad-soft); border-color: var(--bad-soft); }
  .icon-button { width: 30px; height: 30px; padding: 0; display: inline-grid; place-items: center; }
  .icon-button svg { width: 16px; height: 16px; }
  .message { font-size: 14px; font-weight: 550; }
  .message.ok { color: var(--good); }
  .message.error { color: var(--bad); }
  .note { font-size: 12.5px; color: var(--muted); }

  /* Running regions */
  #running { display: grid; gap: 16px; }
  .region { overflow: hidden; }
  .region-head { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 10px; padding: 14px 18px; border-bottom: 1px solid var(--line); background: var(--surface-2); }
  .region-name { display: flex; flex-wrap: wrap; align-items: baseline; gap: 4px 10px; }
  .region-name h3 { font-size: 17px; }
  .code { font: 600 12px ui-monospace, SFMono-Regular, Menlo, monospace; padding: 2px 7px; border-radius: 6px; background: var(--surface); border: 1px solid var(--line); }
  .run { padding: 16px 18px 18px; }
  .run + .run { border-top: 1px dashed var(--line); }
  .run-head { display: flex; flex-wrap: wrap; align-items: center; gap: 8px 12px; margin-bottom: 12px; }
  .badge { font-size: 12px; font-weight: 700; letter-spacing: 0.06em; text-transform: uppercase; padding: 3px 9px; border-radius: 6px; }
  .badge.bots { background: var(--info-soft); color: var(--info); }
  .badge.pilots { background: var(--accent-soft); color: var(--accent); }
  .run-head .describe { font-size: 13.5px; color: var(--muted); flex: 1 1 240px; }
  .stats { display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 14px; }
  .stat { padding: 7px 12px; border-radius: 9px; background: var(--surface-2); border: 1px solid var(--line); min-width: 92px; }
  .stat .label { display: block; font-size: 11px; font-weight: 650; letter-spacing: 0.05em; text-transform: uppercase; color: var(--muted); }
  .stat .value { font-weight: 650; font-variant-numeric: tabular-nums; overflow-wrap: anywhere; }
  .callout { padding: 10px 14px; border-radius: 10px; font-size: 13.5px; margin-bottom: 12px; overflow-wrap: anywhere; }
  .callout strong { display: block; }
  .callout.error { background: var(--bad-soft); color: var(--bad); }
  .callout.warn { background: var(--warn-soft); color: var(--warn); }
  .table-wrap { overflow-x: auto; border: 1px solid var(--line); border-radius: 10px; }
  table { width: 100%; border-collapse: collapse; font-size: 13.5px; font-variant-numeric: tabular-nums; }
  th, td { padding: 8px 11px; text-align: left; white-space: nowrap; border-bottom: 1px solid var(--line); }
  tr:last-child td { border-bottom: 0; }
  th { background: var(--surface-2); font-size: 11.5px; font-weight: 650; letter-spacing: 0.03em; text-transform: uppercase; color: var(--muted); }
  th { white-space: normal; vertical-align: bottom; min-width: 52px; }
  th:first-child { min-width: 0; }
  th[title] { cursor: help; text-decoration: underline dotted; text-underline-offset: 3px; }
  th small { text-transform: none; letter-spacing: 0; font-weight: 500; }
  .name-note { display: block; min-width: 170px; max-width: 300px; margin-top: 2px; white-space: normal; font-size: 12.5px; color: var(--muted); }
  .name-note.bad { color: var(--bad); }
  .sub { color: var(--muted); }
  .phase { display: inline-block; padding: 2px 8px; border-radius: 999px; font-size: 12px; font-weight: 650; background: var(--surface-2); color: var(--muted); }
  .phase.good { background: var(--good-soft); color: var(--good); }
  .phase.info { background: var(--info-soft); color: var(--info); }
  .phase.wait { background: var(--warn-soft); color: var(--warn); }
  .phase.bad { background: var(--bad-soft); color: var(--bad); }
  .team::before { content: ""; display: inline-block; width: 9px; height: 9px; border-radius: 50%; margin-right: 6px; background: currentColor; vertical-align: 0; }
  .team.blue { color: var(--blue); }
  .team.red { color: var(--red); }
  .details { margin-top: 10px; font-size: 12.5px; color: var(--muted); overflow-wrap: anywhere; }
  .empty { padding: 36px 20px; text-align: center; }
  .empty svg { width: 44px; height: 44px; color: var(--line); }
  .empty h3 { margin: 8px 0 6px; font-size: 17px; }
  .empty p { color: var(--muted); max-width: 460px; margin: 0 auto; }

  @media (max-width: 720px) {
    main { padding-top: 20px; }
    h1 { font-size: 22px; }
    .start { padding: 16px; }
    .orders { padding: 14px; }
    .orders-grid { grid-template-columns: minmax(0, 1fr); }
    .room-row .field { width: 100%; }
    .launch .primary { width: 100%; }
    .segmented { display: flex; width: 100%; }
    .chip { flex: 1; text-align: center; padding: 6px 8px; }
    .run, .region-head { padding-left: 14px; padding-right: 14px; }
  }
</style>
</head>
<body>
<main>
  <header class="top">
    <div class="brand">
      <svg viewBox="0 0 48 48" aria-hidden="true" fill="currentColor">
        <rect x="6" y="28" width="36" height="10" rx="5" opacity="0.35"/>
        <rect x="9" y="22" width="30" height="9" rx="3"/>
        <rect x="15" y="15" width="15" height="9" rx="3"/>
        <rect x="28" y="17.5" width="15" height="3.5" rx="1.75"/>
        <circle cx="12" cy="33" r="2.4" opacity="0.7"/><circle cx="20" cy="33" r="2.4" opacity="0.7"/>
        <circle cx="28" cy="33" r="2.4" opacity="0.7"/><circle cx="36" cy="33" r="2.4" opacity="0.7"/>
      </svg>
      <div><p class="eyebrow">Sloppy Tanks</p><h1>Bots and pilots</h1></div>
    </div>
    <div class="top-meta">
      <span class="pill" id="target" title="The servers a run plays on unless it picks others">Servers…</span>
      <span class="muted" id="updated" aria-live="polite">Loading…</span>
      <button type="button" class="ghost icon-button" id="refresh" title="Refresh now" aria-label="Refresh now">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9"/><path d="M13.5 2.5v3h-3"/></svg>
      </button>
    </div>
  </header>
  <p class="intro">Load bots put network load on a game server. Pilots play like people: the bot brain drives each one through the game's own network client from a Cloudflare region, so they see the room with that region's latency and can hunt or escort a player you name.</p>

  <form id="start" class="card start" novalidate>
    <fieldset>
      <legend class="block-label">What to run</legend>
      <div class="kinds">
        <input class="choice-input" type="radio" name="run" id="kind-bots" value="bots" checked>
        <label class="kind" for="kind-bots">
          <svg viewBox="0 0 32 32" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" aria-hidden="true"><path d="M4 22l6-8 5 5 6-10 7 9"/><path d="M4 27h24"/></svg>
          <strong>Load bots</strong><span>Drive at random to load a server · up to 32 a region</span>
        </label>
        <input class="choice-input" type="radio" name="run" id="kind-pilots" value="pilots">
        <label class="kind" for="kind-pilots">
          <svg viewBox="0 0 32 32" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" aria-hidden="true"><circle cx="16" cy="16" r="9"/><circle cx="16" cy="16" r="2" fill="currentColor"/><path d="M16 2v6M16 24v6M2 16h6M24 16h6"/></svg>
          <strong>Pilots</strong><span>Play like people, hunt or escort a player · up to 8 a region</span>
        </label>
      </div>
    </fieldset>

    <div class="grid">
      <label class="field"><span>Region</span>
        <select id="region">
          <option value="wnam">Western North America</option><option value="enam">Eastern North America</option>
          <option value="sam">South America</option><option value="weur">Western Europe</option>
          <option value="eeur">Eastern Europe</option><option value="apac">Asia-Pacific</option>
          <option value="oc">Oceania</option><option value="afr">Africa</option><option value="me">Middle East</option>
        </select>
        <span class="hint">Where on Cloudflare they run from</span>
      </label>
      <label class="field"><span>Servers</span>
        <select id="environment"><option value="" id="defaultServers">Default</option><option>production</option><option>dev</option></select>
        <span class="hint">The game servers to play on</span>
      </label>
      <label class="field"><span id="countLabel">Bots</span>
        <input id="count" type="number" min="1" max="32" value="2" inputmode="numeric">
        <span class="hint" id="countHint">1 to 32</span>
      </label>
      <label class="field"><span>Run for</span>
        <span class="suffix" data-suffix="min"><input id="minutes" type="number" min="1" max="360" value="5" inputmode="numeric"></span>
        <span class="hint">Stops by itself after this, up to 6 hours</span>
      </label>
    </div>

    <fieldset>
      <legend class="block-label">Room</legend>
      <div class="room-row">
        <div class="segmented">
          <input class="choice-input" type="radio" name="where" id="where-open" value="open" checked><label class="chip" for="where-open">Open rooms</label>
          <input class="choice-input" type="radio" name="where" id="where-code" value="code"><label class="chip" for="where-code">Room code</label>
          <input class="choice-input" type="radio" name="where" id="where-create" value="create"><label class="chip" for="where-create" id="createLabel">Create rooms</label>
        </div>
        <label class="field" id="roomField"><span>Code</span><input id="room" type="text" maxlength="8" placeholder="8 characters" autocomplete="off" spellcheck="false"></label>
        <label class="field" id="mapField"><span>Map</span><select id="map"><option>village</option><option>harbor</option><option>quarry</option></select></label>
        <label class="field" id="perRoomField"><span>At most per room</span><input id="perRoom" type="number" min="1" max="8" value="1" inputmode="numeric"></label>
      </div>
      <p class="hint" id="whereHint" style="margin-top: 8px"></p>
    </fieldset>

    <div class="orders" id="orders" role="group" aria-labelledby="ordersTitle">
      <p class="block-label" id="ordersTitle">Orders</p>
      <div class="orders-grid">
        <div class="field">
          <span>Mission</span>
          <div class="segmented">
            <input class="choice-input" type="radio" name="mission" id="mission-skirmish" value="skirmish" checked><label class="chip" for="mission-skirmish">Skirmish</label>
            <input class="choice-input" type="radio" name="mission" id="mission-hunt" value="hunt"><label class="chip" for="mission-hunt">Hunt</label>
            <input class="choice-input" type="radio" name="mission" id="mission-escort" value="escort"><label class="chip" for="mission-escort">Escort</label>
          </div>
          <span class="hint" id="missionHint"></span>
        </div>
        <label class="field" id="playerField"><span>Player name</span><input id="player" type="text" maxlength="24" autocomplete="off" spellcheck="false"><span class="hint">As shown in the game</span></label>
        <div class="field">
          <span>Skill</span>
          <div class="segmented">
            <input class="choice-input" type="radio" name="skill" id="skill-easy" value="easy"><label class="chip" for="skill-easy">Easy</label>
            <input class="choice-input" type="radio" name="skill" id="skill-normal" value="normal" checked><label class="chip" for="skill-normal">Normal</label>
            <input class="choice-input" type="radio" name="skill" id="skill-hard" value="hard"><label class="chip" for="skill-hard">Hard</label>
          </div>
        </div>
        <div class="grid">
          <label class="field"><span>Tank</span><select id="tank"><option value="scout">Scout</option><option value="balanced" selected>Balanced</option><option value="heavy">Heavy</option></select></label>
          <label class="field"><span>Style</span><select id="style"><option value="">The tank's own</option><option value="scout">Scout</option><option value="guard">Guard</option><option value="sniper">Sniper</option><option value="heavy">Heavy</option><option value="minelayer">Minelayer</option><option value="support">Support</option><option value="artillery">Artillery</option></select></label>
        </div>
      </div>
    </div>

    <div class="launch">
      <div class="summary">
        <strong id="summary"></strong>
        <p class="warn-text" id="formWarning" hidden></p>
        <p class="message" id="message" role="status"></p>
      </div>
      <button class="primary" id="submit">Start</button>
    </div>
  </form>

  <div class="section-title">
    <h2>Running <span class="muted" id="runningCount"></span></h2>
    <button type="button" class="ghost danger" id="stopAll" hidden>Stop everything</button>
  </div>
  <div id="running"></div>
  <p class="note" style="margin-top: 18px">A region runs one load-bot run and one pilot run at a time: starting the same kind there again replaces it. Stopping a region stops both. No sign-in: anyone with this address can start and stop runs.</p>
</main>
<script>
const $ = (id) => document.getElementById(id);
const form = $("start");
const choice = (name) => form.elements[name].value;
const REGIONS = {
  wnam: "Western North America", enam: "Eastern North America", sam: "South America",
  weur: "Western Europe", eeur: "Eastern Europe", apac: "Asia-Pacific", oc: "Oceania",
  afr: "Africa", me: "Middle East",
};
const WHERE_HINTS = {
  bots: {
    open: "They fill open rooms on standard maps, fullest first, and wait while none has a seat.",
    code: "Every bot joins this room.",
    create: "They fill open rooms and create a room whenever none has a seat.",
  },
  pilots: {
    open: "They join the fullest open room with a seat for each of them.",
    code: "They join this room on whichever server lists it.",
    create: "The first pilot creates a room on this map and hosts it; the others join it.",
  },
};
const MISSION_HINTS = {
  skirmish: "Fights like the game's own bots.",
  hunt: "Takes the other side from the player and goes after them. With no name, the nearest player.",
  escort: "Takes the player's side, follows them and fights whoever they fight.",
};
const TEAMS = ["blue", "red"];
let lastStatus = null;

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined && text !== null) node.textContent = text;
  return node;
}
async function api(path, body) {
  const response = await fetch("/api/" + path, {
    method: body ? "POST" : "GET",
    headers: { "Content-Type": "application/json" },
    body: body ? JSON.stringify(body) : undefined,
  });
  const data = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(data.error || response.statusText);
  return data;
}
function say(text, kind) {
  $("message").textContent = text;
  $("message").className = "message " + (kind || "");
}
async function busy(button, work) {
  button.disabled = true;
  try { await work(); } finally { button.disabled = false; }
}
const plural = (count, one, many) => count + " " + (count === 1 ? one : many);
const size = (bytes) => bytes < 1048576 ? (bytes / 1024).toFixed(1) + " KiB" : (bytes / 1048576).toFixed(1) + " MiB";
const rounded = (value) => (value === undefined || value === null ? "–" : String(Math.round(value)));
const metres = (value) => (value === undefined || value === null ? "–" : value.toFixed(2));
function duration(minutes) {
  if (minutes < 60) return plural(minutes, "minute", "minutes");
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return plural(hours, "hour", "hours") + (rest ? " " + plural(rest, "minute", "minutes") : "");
}
function timeLeft(stopAt) {
  const seconds = Math.max(0, Math.round((stopAt - Date.now()) / 1000));
  if (!seconds) return "stopping";
  const minutes = Math.floor(seconds / 60);
  if (minutes >= 60) return Math.floor(minutes / 60) + " h " + String(minutes % 60).padStart(2, "0") + " min";
  return minutes + ":" + String(seconds % 60).padStart(2, "0");
}
function missionText(mission) {
  const [kind, name] = mission.split(":");
  if (kind === "hunt") return name ? "Hunting " + name : "Hunting the nearest player";
  if (kind === "escort" || kind === "ally") return "Escorting " + name;
  return "Skirmish";
}

/* The start form */
function formState() {
  const pilots = choice("run") === "pilots";
  const count = Math.max(1, Math.round(Number($("count").value) || 1));
  const player = $("player").value.trim();
  const mission = choice("mission");
  return {
    pilots, count, player, mission,
    where: choice("where"),
    region: $("region").value,
    minutes: Math.max(1, Math.round(Number($("minutes").value) || 1)),
    room: $("room").value.trim().toUpperCase(),
    environment: $("environment").value,
    missionValue: mission === "skirmish" ? "skirmish" : mission + (player ? ":" + player : ""),
  };
}
function runningOfKind(state) {
  if (!lastStatus) return null;
  const list = state.pilots ? (lastStatus.pilots.regions || []) : lastStatus.regions;
  return list.find((region) => region.region === state.region) || null;
}
function showForm() {
  const state = formState();
  const kind = state.pilots ? "pilots" : "bots";
  const max = state.pilots ? 8 : 32;
  $("count").max = max;
  $("countLabel").textContent = state.pilots ? "Pilots" : "Bots";
  $("countHint").textContent = "1 to " + max;
  $("orders").hidden = !state.pilots;
  $("roomField").hidden = state.where !== "code";
  $("mapField").hidden = !(state.pilots && state.where === "create");
  $("perRoomField").hidden = state.pilots || state.where === "code";
  $("createLabel").textContent = state.pilots ? "New room" : "Create rooms";
  $("whereHint").textContent = WHERE_HINTS[kind][state.where];
  $("playerField").hidden = state.mission === "skirmish";
  $("player").placeholder = state.mission === "hunt" ? "Nearest player" : "Required";
  $("missionHint").textContent = MISSION_HINTS[state.mission];

  const noun = state.pilots ? ["pilot", "pilots"] : ["load bot", "load bots"];
  let text = plural(state.count, noun[0], noun[1]) + " in " + REGIONS[state.region] + " for " + duration(state.minutes);
  if (state.pilots) {
    if (state.mission === "hunt") text += ", " + (state.player ? "hunting " + state.player : "hunting the nearest player");
    if (state.mission === "escort") text += ", escorting " + (state.player || "a player you name");
  }
  if (state.where === "code") text += ", in room " + (state.room || "…");
  if (state.where === "create") text += state.pilots ? ", in a new " + $("map").value + " room" : ", creating rooms as needed";
  if (state.environment) text += ", on " + state.environment + " servers";
  $("summary").textContent = text + ".";
  $("submit").textContent = "Start " + plural(state.count, noun[0], noun[1]);

  const running = runningOfKind(state);
  const runningCount = running ? (state.pilots ? running.config.pilots : running.config.bots) : 0;
  let warning = running ? "Replaces the " + plural(runningCount, noun[0], noun[1]) + " running there now." : "";
  if (state.pilots && lastStatus && lastStatus.pilots.error) warning = "Pilots cannot start now: their Worker is not reachable.";
  $("formWarning").hidden = !warning;
  $("formWarning").textContent = warning;
}
form.addEventListener("input", showForm);
form.addEventListener("change", showForm);
showForm();

function check(state) {
  if (state.count > (state.pilots ? 8 : 32)) return "A region runs at most " + (state.pilots ? "8 pilots" : "32 load bots") + ".";
  if (state.minutes > 360) return "A run lasts at most 6 hours (360 minutes).";
  if (state.where === "code" && !/^[A-HJ-NP-Z2-9]{8}$/.test(state.room)) return "A room code is 8 letters and digits, as the game shows it.";
  if (state.pilots && state.mission === "escort" && !state.player) return "Name the player to escort.";
  return "";
}
form.addEventListener("submit", (event) => {
  event.preventDefault();
  const state = formState();
  const problem = check(state);
  if (problem) { say(problem, "error"); return; }
  const body = {
    region: state.region,
    bots: state.count,
    minutes: state.minutes,
    room: state.where === "code" ? state.room : undefined,
    host: state.where === "create",
    environment: state.environment || undefined,
  };
  if (state.pilots) {
    Object.assign(body, {
      brain: true, mission: state.missionValue, skill: choice("skill"), kind: $("tank").value,
      style: $("style").value || undefined, map: $("map").value,
    });
  } else {
    body.perRoom = Number($("perRoom").value) || 1;
  }
  busy($("submit"), async () => {
    try {
      await api("start", body);
      say("Started in " + REGIONS[state.region] + ".", "ok");
      await refresh();
    } catch (error) { say(error.message, "error"); }
  });
});

/* Running regions */
function phaseClass(phase) {
  if (phase === "playing") return "good";
  if (phase === "lobby" || phase === "results") return "info";
  if (phase === "ended") return "bad";
  if (phase === "left") return "";
  return "wait";
}
function stat(label, value) {
  const box = el("div", "stat");
  box.append(el("span", "label", label), el("span", "value", value));
  return box;
}
function countdown(stopAt) {
  const value = el("span", "value", timeLeft(stopAt));
  value.dataset.stopAt = stopAt;
  const box = el("div", "stat");
  box.append(el("span", "label", "Time left"), value);
  return box;
}
function callout(kind, title, text) {
  const box = el("div", "callout " + kind);
  box.append(el("strong", "", title), el("span", "", text));
  return box;
}
function table(columns, rows) {
  const wrap = el("div", "table-wrap");
  const grid = el("table");
  const head = el("tr");
  for (const column of columns) {
    const th = el("th", column.num ? "num" : "", column.label);
    if (column.unit) th.append(" ", el("small", "", column.unit));
    if (column.help) th.title = column.help;
    head.append(th);
  }
  grid.append(head);
  for (const cells of rows) {
    const row = el("tr");
    cells.forEach((cell, index) => {
      const td = el("td", columns[index].num ? "num" : "");
      if (cell instanceof Node) td.append(cell);
      else td.textContent = cell === undefined || cell === null ? "" : String(cell);
      row.append(td);
    });
    grid.append(row);
  }
  wrap.append(grid);
  return wrap;
}
const phase = (name) => el("span", "phase " + phaseClass(name), name);
function withSub(main, sub) {
  const span = el("span", "", String(main));
  span.append(" ", el("span", "sub", "(" + sub + ")"));
  return span;
}
/** A name, with what went wrong (or what the server said) under it. */
function named(name, text, bad) {
  const cell = el("span", "", name);
  if (text) cell.append(el("span", "name-note" + (bad ? " bad" : ""), text));
  return cell;
}

function botRun(region) {
  const config = region.config;
  const part = el("section", "run");
  const head = el("div", "run-head");
  let describe = config.room
    ? "Joining room " + config.room
    : "Filling open rooms, at most " + config.perRoom + " per room" + (config.host ? ", creating rooms when none has a seat" : "");
  if (config.environment) describe += " · " + config.environment + " servers";
  head.append(el("span", "badge bots", "Load bots"), el("span", "describe", describe));
  part.append(head);
  const connected = region.bots.filter((bot) => bot.connected).length;
  const stats = el("div", "stats");
  stats.append(
    stat("Connected", connected + " / " + config.bots),
    stat("Rooms", region.rooms.join(", ") || "none yet"),
    stat("Received", size(region.bytesInPerSecond) + "/s"),
    stat("Sent", size(region.bytesOutPerSecond) + "/s"),
    countdown(config.stopAt),
  );
  part.append(stats);
  if (region.lastError) part.append(callout("error", "Problem", region.lastError));
  part.append(table(
    [
      { label: "Bot" }, { label: "Room" }, { label: "Phase" },
      { label: "RTT", unit: "ms", num: true, help: "Round trip to the game server" },
      { label: "Inputs", num: true }, { label: "Rounds", num: true },
      { label: "Received", num: true }, { label: "Sent", num: true },
    ],
    region.bots.map((bot) => [
      named(bot.name, bot.lastError, true), bot.room || "–", phase(bot.phase), rounded(bot.rttMs),
      bot.inputs, bot.rounds, size(bot.bytesIn), size(bot.bytesOut),
    ]),
  ));
  return part;
}

function pilotRun(region) {
  const config = region.config;
  const brain = config.brain;
  const squad = region.squad || { pilots: [] };
  const part = el("section", "run");
  const head = el("div", "run-head");
  let describe = missionText(brain.mission) + " · " + brain.skill + " · " + brain.kind + " tank" + (brain.style ? " · " + brain.style + " style" : "");
  if (config.environment) describe += " · " + config.environment + " servers";
  head.append(el("span", "badge pilots", "Pilots"), el("span", "describe", describe));
  part.append(head);
  const connected = squad.pilots.filter((pilot) => pilot.connected).length;
  const stats = el("div", "stats");
  stats.append(
    stat("Connected", connected + " / " + config.pilots),
    stat("Room", squad.room || (config.room ? config.room : config.host ? "creating…" : "choosing…")),
    countdown(config.stopAt),
  );
  if (squad.restarts) stats.append(stat("Engine restarts", String(squad.restarts)));
  part.append(stats);
  for (const error of [region.lastError, squad.lastError]) {
    if (error) part.append(callout("error", "Problem", error));
  }
  if (squad.pilots.length) {
    part.append(table(
      [
        { label: "Pilot" }, { label: "Phase" }, { label: "Team" },
        { label: "K/D", num: true, help: "Kills and deaths this battle" },
        { label: "Hits", unit: "(players)", num: true, help: "Hits landed, and how many of them on players rather than fill bots" },
        { label: "Taken", unit: "(players)", num: true, help: "Hits taken, and how many of them from players" },
        { label: "RTT", unit: "ms", num: true, help: "Round trip to the game server" },
        { label: "View delay", unit: "ms", num: true, help: "How late the pilot sees other tanks, as a player there would" },
        { label: "Fire→flash", unit: "ms", num: true, help: "From pulling the trigger to seeing the muzzle flash" },
        { label: "Hit offset", unit: "m", num: true, help: "How far the server's hit landed from where the pilot saw it" },
        { label: "Corr. p95", unit: "m", num: true, help: "95th percentile of the prediction corrections to the pilot's own tank" },
        { label: "Phantom", num: true, help: "Shells drawn hitting that the server flew past" },
        { label: "Late", num: true, help: "Snapshot batches that arrived late" },
      ],
      squad.pilots.map((pilot) => {
        const report = pilot.report;
        if (!report) {
          return [named(pilot.name, pilot.lastError, true), phase("starting"), "", "", "", "", "", "", "", "", "", "", ""];
        }
        const feel = report.feel;
        const team = TEAMS[report.team];
        const problem = pilot.lastError || report.endedText;
        return [
          named(pilot.name, problem || report.notice, !!problem),
          phase(report.phase),
          team ? el("span", "team " + team, team) : "–",
          report.scoreKills + " / " + report.scoreDeaths,
          withSub(report.tally.hits, report.tally.playerHits),
          withSub(report.tally.hitsTaken, report.tally.playerHitsTaken),
          rounded(feel.rttMs),
          rounded(feel.viewDelayMs.mean),
          rounded(feel.fireToFlashMs.mean),
          metres(feel.hitOffsetM.mean),
          metres(feel.correctionP95M),
          feel.phantomStrikes,
          feel.lateBatches,
        ];
      }),
    ));
  }
  const details = [];
  if (squad.server) {
    try { details.push("Server " + new URL(squad.server).host); } catch { details.push("Server " + squad.server); }
  }
  if (squad.contentVersion) details.push("engine content " + squad.contentVersion);
  if (details.length) part.append(el("p", "details", details.join(" · ")));
  return part;
}

function regionCard(code, runs) {
  const card = el("article", "card region");
  const head = el("header", "region-head");
  const name = el("div", "region-name");
  const colo = (runs.bots && runs.bots.colo) || (runs.pilots && runs.pilots.colo);
  name.append(el("h3", "", REGIONS[code] || code), el("span", "code", code));
  if (colo) name.append(el("span", "muted", "from " + colo));
  const stop = el("button", "ghost danger", "Stop region");
  stop.type = "button";
  stop.dataset.stop = code;
  stop.title = "Stops this region's load bots and pilots";
  head.append(name, stop);
  card.append(head);
  if (runs.bots) card.append(botRun(runs.bots));
  if (runs.pilots) card.append(pilotRun(runs.pilots));
  return card;
}

function render(status) {
  lastStatus = status;
  let environment = "";
  try { environment = new URL(status.server).searchParams.get("environment") || ""; } catch {}
  $("target").textContent = environment ? environment + " servers" : "Servers";
  $("defaultServers").textContent = environment ? "Default (" + environment + ")" : "Default";
  const regions = new Map();
  const entry = (code) => regions.get(code) || regions.set(code, {}).get(code);
  for (const region of status.regions) entry(region.region).bots = region;
  for (const region of status.pilots.regions || []) entry(region.region).pilots = region;
  const list = $("running");
  list.replaceChildren();
  if (status.pilots.error) list.append(callout("warn", "Pilots are not available", status.pilots.error));
  $("runningCount").textContent = regions.size ? "· " + plural(regions.size, "region", "regions") : "";
  $("stopAll").hidden = !regions.size;
  if (!regions.size) {
    const empty = el("div", "card empty");
    empty.innerHTML = '<svg viewBox="0 0 48 48" fill="currentColor" aria-hidden="true"><rect x="6" y="28" width="36" height="10" rx="5"/><rect x="9" y="22" width="30" height="9" rx="3"/><rect x="15" y="15" width="15" height="9" rx="3"/><rect x="28" y="17.5" width="15" height="3.5" rx="1.75"/></svg>';
    empty.append(
      el("h3", "", "Nothing running"),
      el("p", "", "Start a run above. Every run stops by itself at its time limit, so nothing runs or costs anything between runs."),
    );
    list.append(empty);
  }
  for (const code of Object.keys(REGIONS)) {
    if (regions.has(code)) list.append(regionCard(code, regions.get(code)));
  }
  showForm();
}

async function refresh() {
  try {
    render(await api("status"));
    $("updated").textContent = "Updated " + new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  } catch (error) {
    $("updated").textContent = "Could not refresh: " + error.message;
  }
}
$("refresh").addEventListener("click", () => busy($("refresh"), refresh));
$("running").addEventListener("click", (event) => {
  const button = event.target.closest("[data-stop]");
  if (!button) return;
  const code = button.dataset.stop;
  busy(button, async () => {
    try {
      await api("stop", { region: code });
      say("Stopped " + REGIONS[code] + ".", "ok");
      await refresh();
    } catch (error) { say(error.message, "error"); }
  });
});
$("stopAll").addEventListener("click", () => {
  if (!confirm("Stop every load bot and pilot in every region?")) return;
  busy($("stopAll"), async () => {
    try {
      await api("stop", {});
      say("Stopped everything.", "ok");
      await refresh();
    } catch (error) { say(error.message, "error"); }
  });
});
setInterval(() => {
  for (const value of document.querySelectorAll("[data-stop-at]")) value.textContent = timeLeft(Number(value.dataset.stopAt));
}, 1000);
setInterval(() => { if (!document.hidden) refresh(); }, 10000);
document.addEventListener("visibilitychange", () => { if (!document.hidden) refresh(); });
refresh();
</script>
</body>
</html>
`;

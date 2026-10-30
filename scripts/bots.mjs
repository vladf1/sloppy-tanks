// Operator CLI for the traffic-bot Worker and its pilots. See bots/README.md.
import { parseArgs } from "node:util";

const endpoint = (
  process.env.SLOPPY_BOTS_URL ?? "https://sloppy-tanks-bots.vova145.workers.dev"
).replace(/\/$/, "");
// `pnpm run bots -- start …` passes its `--` through, and parseArgs reads every flag
// after a `--` as a positional, so `--minutes` and `--host` would be dropped silently.
const args = process.argv.slice(2);
if (args[0] === "--") args.shift();
const { values, positionals } = parseArgs({
  args,
  allowPositionals: true,
  options: {
    minutes: { type: "string" },
    room: { type: "string" },
    "per-room": { type: "string" },
    host: { type: "boolean", default: false },
    // Pilots: players run by the bot brain instead of random traffic.
    brain: { type: "boolean", default: false },
    mission: { type: "string" },
    hunt: { type: "string" },
    escort: { type: "string" },
    skill: { type: "string" },
    kind: { type: "string" },
    style: { type: "string" },
    map: { type: "string" },
    env: { type: "string" },
  },
});
const [command, ...rest] = positionals;

async function api(path, body) {
  const response = await fetch(`${endpoint}/api/${path}`, {
    method: body ? "POST" : "GET",
    headers: { "Content-Type": "application/json" },
    body: body ? JSON.stringify(body) : undefined,
  });
  const data = await response.json();
  if (!response.ok) throw new Error(data.error ?? response.statusText);
  return data;
}
const TEAMS = ["blue", "red"];
function printPilots(region) {
  const squad = region.pilots;
  const minutes = Math.max(0, Math.round((region.config.stopAt - Date.now()) / 60_000));
  const brain = region.config.brain;
  console.log(
    `${region.region} colo=${region.colo ?? "?"} pilots ${brain.mission} (${brain.skill} ${brain.kind}) ` +
      `room=${squad.room ?? "choosing"} engine=${squad.contentVersion ?? "?"}, stops in ${minutes} min` +
      (squad.lastError ? `\n  error: ${squad.lastError}` : ""),
  );
  for (const pilot of squad.pilots) {
    const report = pilot.report;
    if (!report) {
      console.log(
        `  ${pilot.name.padEnd(16)} starting${pilot.lastError ? ` last=${pilot.lastError}` : ""}`,
      );
      continue;
    }
    const { tally, feel } = report;
    console.log(
      `  ${pilot.name.padEnd(16)} ${report.phase.padEnd(10)} ${(TEAMS[report.team] ?? "-").padEnd(4)} ` +
        `K/D ${report.scoreKills}/${report.scoreDeaths} hits ${tally.hits} (players ${tally.playerHits}) ` +
        `taken ${tally.hitsTaken} rtt=${Math.round(feel.rttMs)}ms view=${Math.round(feel.viewDelayMs.mean)}ms ` +
        `flash=${Math.round(feel.fireToFlashMs.mean)}ms phantom=${feel.phantomStrikes}` +
        (pilot.lastError || report.endedText ? ` last=${pilot.lastError ?? report.endedText}` : ""),
    );
  }
}
function print(status) {
  if (!status.regions.length) {
    console.log(`No bots running (target ${status.server}).`);
    return;
  }
  for (const region of status.regions) {
    if (region.pilots) {
      printPilots(region);
      continue;
    }
    const connected = region.bots.filter((bot) => bot.connected).length;
    const minutes = Math.max(0, Math.round((region.config.stopAt - Date.now()) / 60_000));
    console.log(
      `${region.region} colo=${region.colo ?? "?"} ${connected}/${region.config.bots} connected, ` +
        `rooms=[${region.rooms.join(",")}] in=${(region.bytesInPerSecond / 1024).toFixed(1)} KiB/s ` +
        `out=${(region.bytesOutPerSecond / 1024).toFixed(1)} KiB/s, stops in ${minutes} min` +
        (region.lastError ? `\n  error: ${region.lastError}` : ""),
    );
    for (const bot of region.bots) {
      console.log(
        `  ${bot.name.padEnd(14)} ${String(bot.room ?? "-").padEnd(9)} ${bot.phase.padEnd(8)} ` +
          `rtt=${bot.rttMs ?? "-"}ms inputs=${bot.inputs} rounds=${bot.rounds}` +
          (bot.lastError ? ` last=${bot.lastError}` : ""),
      );
    }
  }
}

switch (command) {
  case "start": {
    const [region, bots] = rest;
    if (!region) throw new Error("Usage: pnpm run bots -- start <region> [bots] [--minutes N]");
    const mission =
      values.mission ??
      (values.hunt !== undefined ? `hunt:${values.hunt}` : undefined) ??
      (values.escort !== undefined ? `escort:${values.escort}` : undefined);
    const brain = values.brain || mission !== undefined;
    const result = await api("start", {
      region,
      bots: bots === undefined ? undefined : Number(bots),
      minutes: values.minutes === undefined ? undefined : Number(values.minutes),
      perRoom: values["per-room"] === undefined ? undefined : Number(values["per-room"]),
      room: values.room,
      host: values.host,
      environment: values.env,
      ...(brain
        ? {
            brain: true,
            mission: mission ?? "skirmish",
            skill: values.skill,
            kind: values.kind,
            style: values.style,
            map: values.map,
          }
        : {}),
    });
    console.log(`Started ${result.config.bots} ${brain ? "pilots" : "bots"} in ${region}.`);
    print(await api("status"));
    break;
  }
  case "stop":
    console.log(`Stopped: ${(await api("stop", rest[0] ? { region: rest[0] } : {})).stopped}`);
    break;
  case "status":
    print(await api("status"));
    break;
  default:
    console.log(`Usage:
  pnpm run bots -- start <region> [bots=1]    [--minutes 5]  [--host] [--room CODE] [--per-room 1]
                                              [--env production|dev]
  pnpm run bots -- start <region> [pilots=1]  --brain [--room CODE | --host [--map village]]
                                              [--hunt NAME | --escort NAME | --mission M]
                                              [--skill easy|normal|hard] [--kind scout|balanced|heavy]
                                              [--style ROLE] [--minutes 5] [--env production|dev]
  pnpm run bots -- stop [region]              stop one region, or all
  pnpm run bots -- status
Regions: wnam enam sam weur eeur apac oc afr me
Pilots play through the browser's client with the bot brain; --hunt or --escort imply --brain.
Missions: skirmish, hunt, hunt:NAME, escort:NAME. At most 8 pilots a region.`);
}

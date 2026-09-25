// Operator CLI for the traffic-bot Worker. See bots/README.md.
import { parseArgs } from "node:util";

const endpoint = (
  process.env.SLOPPY_BOTS_URL ?? "https://sloppy-tanks-bots.vova145.workers.dev"
).replace(/\/$/, "");
const { values, positionals } = parseArgs({
  allowPositionals: true,
  options: {
    minutes: { type: "string" },
    room: { type: "string" },
    "per-room": { type: "string" },
    host: { type: "boolean", default: false },
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
function print(status) {
  if (!status.regions.length) {
    console.log(`No bots running (target ${status.server}).`);
    return;
  }
  for (const region of status.regions) {
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
    if (!region) throw new Error("Usage: npm run bots -- start <region> [bots] [--minutes N]");
    const result = await api("start", {
      region,
      bots: bots === undefined ? undefined : Number(bots),
      minutes: values.minutes === undefined ? undefined : Number(values.minutes),
      perRoom: values["per-room"] === undefined ? undefined : Number(values["per-room"]),
      room: values.room,
      host: values.host,
    });
    console.log(`Started ${result.config.bots} bots in ${region}.`);
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
  npm run bots -- start <region> [bots=1]    [--minutes 30] [--host] [--room CODE] [--per-room 1]
  npm run bots -- stop [region]              stop one region, or all
  npm run bots -- status
Regions: wnam enam sam weur eeur apac oc afr me`);
}

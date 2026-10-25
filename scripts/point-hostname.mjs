import { Resolver } from "node:dns/promises";
import { isIP } from "node:net";
import { createInterface } from "node:readline/promises";
import { Writable } from "node:stream";
import { fileURLToPath } from "node:url";
import { gameServer } from "./servers.mjs";

/** Point one of a game server's hostnames (deploy/servers.json) at a machine by editing
 * its A record through Cloudflare's API, then wait until public resolvers return the new
 * address. It points the first hostname with a DNS record of its own (not made from
 * `{dashed-ip}`) unless `--hostname` names another. It asks for a Cloudflare API token
 * with Zone > DNS > Edit on the hostname's zone and the IP, defaulting to the machine in
 * the list; the token is only sent to Cloudflare. The A record must already exist, and
 * it stays DNS only: Caddy answers the ACME challenges and serves players' wss itself.
 * `--dry-run` shows the record and the change without making it.
 *
 *   bun run server:point-hostname [--dev] [--hostname <name>] [--dry-run]
 *
 * Run it after the new machine checks out through its nip.io name, then provision again
 * so Caddy obtains the hostname's certificate at once (crates/server/README.md). */
const API_URL = "https://api.cloudflare.com/client/v4";
const API_TIMEOUT_MS = 30_000;
const PUBLIC_RESOLVERS = ["8.8.8.8", "1.1.1.1"];
// The record's TTL bounds how long a resolver keeps the old address.
const PROPAGATION_TIMEOUT_MS = 10 * 60_000;
const PROPAGATION_POLL_MS = 10_000;

/** The zone among `zoneNames` that holds `hostname`: the longest one it ends with. */
export function zoneFor(hostname, zoneNames) {
  const zone = zoneNames
    .filter((name) => hostname === name || hostname.endsWith(`.${name}`))
    .sort((a, b) => b.length - a.length)[0];
  if (!zone) {
    throw new Error(`The token reaches no zone holding ${hostname}: ${zoneNames.join(", ")}`);
  }
  return zone;
}

/** The single A record named `hostname`; anything else is for a person to sort out. */
export function aRecord(records, hostname) {
  const matches = records.filter((record) => record.type === "A" && record.name === hostname);
  if (matches.length === 1) return matches[0];
  if (!matches.length) throw new Error(`${hostname} has no A record; add it in the dashboard`);
  throw new Error(`${hostname} has ${matches.length} A records; leave exactly one`);
}

/** The errors in a Cloudflare API reply; empty when it succeeded. */
export function cloudflareErrors(reply) {
  if (typeof reply?.success !== "boolean") {
    return [`unexpected reply: ${JSON.stringify(reply).slice(0, 300)}`];
  }
  if (reply.success) return [];
  const messages = (reply.errors ?? []).map(({ code, message }) => `${message} (${code})`);
  return messages.length ? messages : ["failed without a message"];
}

/** One Cloudflare API call; returns its `result` or throws its errors. */
async function cloudflare(token, method, path, body) {
  const response = await fetch(`${API_URL}${path}`, {
    method,
    headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
    body: body && JSON.stringify(body),
    signal: AbortSignal.timeout(API_TIMEOUT_MS),
  });
  const reply = await response.json().catch(() => null);
  const errors = cloudflareErrors(reply);
  if (!response.ok || errors.length) {
    throw new Error(`Cloudflare ${method} ${path}: ${errors.join("; ") || response.status}`);
  }
  return reply.result;
}

async function prompts() {
  // Readline echoes through this stream, which stays silent while the token is typed.
  let muted = false;
  const output = new Writable({
    write(chunk, _encoding, done) {
      if (!muted) process.stdout.write(chunk);
      done();
    },
  });
  const rl = createInterface({ input: process.stdin, output, terminal: true });
  return {
    async secret(question) {
      process.stdout.write(question);
      muted = true;
      const answer = await rl.question("");
      muted = false;
      process.stdout.write("\n");
      return answer.trim();
    },
    async text(question) {
      return (await rl.question(question)).trim();
    },
    close: () => rl.close(),
  };
}

/** Polls the public resolvers until each returns exactly `ip` for `hostname`. */
async function waitForPublicDns(hostname, ip) {
  const deadline = Date.now() + PROPAGATION_TIMEOUT_MS;
  for (;;) {
    const answers = await Promise.all(
      PUBLIC_RESOLVERS.map(async (server) => {
        const resolver = new Resolver();
        resolver.setServers([server]);
        const addresses = await resolver.resolve4(hostname).catch(() => []);
        return { server, addresses };
      }),
    );
    console.log(
      answers
        .map(({ server, addresses }) => `${server}: ${addresses.join(", ") || "none"}`)
        .join("   "),
    );
    if (answers.every(({ addresses }) => addresses.length === 1 && addresses[0] === ip)) return;
    if (Date.now() > deadline) {
      throw new Error(`Public resolvers still do not return ${ip} for ${hostname}`);
    }
    await new Promise((resolve) => setTimeout(resolve, PROPAGATION_POLL_MS));
  }
}

async function main() {
  const dev = process.argv.includes("--dev");
  const dryRun = process.argv.includes("--dry-run");
  const server = gameServer(dev);
  const index = process.argv.indexOf("--hostname");
  const hostname = index === -1 ? server.dnsNames[0] : process.argv[index + 1];
  if (!hostname) {
    throw new Error(`${server.role} in deploy/servers.json has no hostname with its own record`);
  }
  if (!server.dnsNames.includes(hostname)) {
    throw new Error(
      `${hostname} is not one of ${server.role}'s own hostnames: ${server.dnsNames.join(", ")}`,
    );
  }
  if (!process.stdin.isTTY) throw new Error("Run it in a terminal: it asks for the token");
  console.log(`Pointing ${hostname} through Cloudflare${dryRun ? " (dry run)" : ""}`);

  const ask = await prompts();
  let token;
  let ip;
  try {
    token = await ask.secret("Cloudflare API token (Zone > DNS > Edit): ");
    const answer = await ask.text(`IP [${server.ip}]: `);
    ip = answer || server.ip;
  } finally {
    ask.close();
  }
  if (!token) throw new Error("No token given");
  if (isIP(ip) !== 4) throw new Error(`Not an IPv4 address: ${ip}`);

  const zones = await cloudflare(token, "GET", "/zones?per_page=50");
  const zoneName = zoneFor(
    hostname,
    zones.map(({ name }) => name),
  );
  const zone = zones.find(({ name }) => name === zoneName);
  const records = await cloudflare(
    token,
    "GET",
    `/zones/${zone.id}/dns_records?${new URLSearchParams({ type: "A", name: hostname })}`,
  );
  const record = aRecord(records, hostname);
  const change = record.content !== ip || record.proxied;
  console.log(
    `${hostname} in ${zoneName}: ${record.content}${record.proxied ? " (proxied)" : ""}, ` +
      `TTL ${record.ttl === 1 ? "auto" : `${record.ttl} s`}` +
      (change ? `; setting ${ip}, DNS only` : "; already DNS only at that address"),
  );
  if (dryRun) return;

  if (change) {
    await cloudflare(token, "PATCH", `/zones/${zone.id}/dns_records/${record.id}`, {
      content: ip,
      proxied: false,
    });
    console.log(`Cloudflare set ${hostname} to ${ip}; waiting for public resolvers`);
  }
  await waitForPublicDns(hostname, ip);
  console.log(
    `${hostname} resolves to ${ip}. ` +
      `Next: bun run server:provision${dev ? " --dev" : ""} so Caddy obtains its certificate now.`,
  );
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}

import { Resolver } from "node:dns/promises";
import { isIP } from "node:net";
import { createInterface } from "node:readline/promises";
import { Writable } from "node:stream";
import { fileURLToPath } from "node:url";
import { gameServer } from "./servers.mjs";

/** Point one of a game server's hostnames (deploy/servers.json) at a machine through
 * Namecheap's Dynamic DNS, then wait until public resolvers return the new address. It
 * points the first hostname with a DNS record of its own (not made from `{dashed-ip}`)
 * unless `--hostname` names another. It asks for the domain's Dynamic DNS password
 * (Namecheap: Advanced DNS > Dynamic DNS) and the IP, defaulting to the machine in the
 * list; the password is only sent to Namecheap. The record must be an
 * "A + Dynamic DNS Record" in Namecheap's Advanced DNS.
 *
 *   pnpm run server:point-hostname [--dev] [--hostname <name>]
 *
 * Run it after the new machine checks out through its nip.io name, then provision again
 * so Caddy obtains the hostname's certificate at once (crates/server/README.md). */
const UPDATE_URL = "https://dynamicdns.park-your-domain.com/update";
const PUBLIC_RESOLVERS = ["8.8.8.8", "1.1.1.1"];
const PROPAGATION_TIMEOUT_MS = 10 * 60_000;
const PROPAGATION_POLL_MS = 10_000;

/** Namecheap names a record by its host within the domain: sloppy-tanks-server, fridman.me. */
export function namecheapRecord(hostname) {
  const labels = hostname.split(".");
  if (labels.length < 3) throw new Error(`Not a subdomain: ${hostname}`);
  return { host: labels.slice(0, -2).join("."), domain: labels.slice(-2).join(".") };
}

/** The errors in Namecheap's XML reply; empty when the update succeeded. */
export function namecheapErrors(reply) {
  const count = reply.match(/<ErrCount>(\d+)<\/ErrCount>/);
  if (!count) return [`unexpected reply: ${reply.trim().slice(0, 300)}`];
  if (Number(count[1]) === 0) return [];
  const messages = [...reply.matchAll(/<Err\d+>([^<]*)<\/Err\d+>/g)].map((match) => match[1]);
  return messages.length ? messages : [`${count[1]} errors without a message`];
}

async function prompts() {
  // Readline echoes through this stream, which stays silent while the password is typed.
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
  if (!process.stdin.isTTY) throw new Error("Run it in a terminal: it asks for the password");
  const { host, domain } = namecheapRecord(hostname);
  console.log(`Pointing ${hostname} (host ${host} in ${domain}) through Namecheap Dynamic DNS`);

  const ask = await prompts();
  let password;
  let ip;
  try {
    password = await ask.secret("Dynamic DNS password: ");
    const answer = await ask.text(`IP [${server.ip ?? "none in the list"}]: `);
    ip = answer || server.ip;
  } finally {
    ask.close();
  }
  if (!password) throw new Error("No password given");
  if (isIP(ip ?? "") !== 4) throw new Error(`Not an IPv4 address: ${ip}`);

  // The password goes in the query string, as Namecheap requires, so the URL is never printed.
  const url = new URL(UPDATE_URL);
  url.search = new URLSearchParams({ host, domain, password, ip }).toString();
  const response = await fetch(url, { signal: AbortSignal.timeout(30_000) });
  const errors = namecheapErrors(await response.text());
  if (!response.ok || errors.length) {
    throw new Error(`Namecheap refused the update: ${errors.join("; ") || response.status}`);
  }
  console.log(`Namecheap set ${hostname} to ${ip}; waiting for public resolvers`);
  await waitForPublicDns(hostname, ip);
  console.log(
    `${hostname} resolves to ${ip}. ` +
      `Next: pnpm run server:provision${dev ? " --dev" : ""} so Caddy obtains its certificate now.`,
  );
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}

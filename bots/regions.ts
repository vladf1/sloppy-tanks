/**
 * What the two bot Workers share: the regions their Durable Objects run in, request
 * checks, the server directory and run limits. `worker.ts` is the control plane and runs
 * the load bots; `pilots-worker.ts` runs the pilots, which it reaches over a service
 * binding.
 */

/** Durable Object location hints; each region runs at most one swarm object per Worker. */
export const REGIONS = ["wnam", "enam", "sam", "weur", "eeur", "apac", "oc", "afr", "me"] as const;
export type Region = (typeof REGIONS)[number];
/** The directory environments a run may play on; the Worker's own by default. */
export const ENVIRONMENTS = ["production", "dev"] as const;
export type Environment = (typeof ENVIRONMENTS)[number];

export const DEFAULT_MINUTES = 5;
// Every running bot keeps its Durable Object awake and billed for wall time, so runs always end.
export const MAX_MINUTES = 6 * 60;
// Directory polls and joins share the server's per-IP limits (120 listings, 60 joins a minute).
export const MAINTAIN_MS = 10_000;
// The alarm restarts bots after a runtime restart evicts the in-memory sockets.
export const WATCHDOG_MS = 30_000;
export const CONNECT_TIMEOUT_MS = 10_000;

/** The server directory settings both Workers' `vars` carry. */
export interface DirectoryEnv {
  /** The server directory, whose SERVER_ENVIRONMENT servers the bots play on. */
  DIRECTORY_URL: string;
  SERVER_ENVIRONMENT: string;
}

export const directoryList = (env: DirectoryEnv, environment?: Environment) =>
  `${env.DIRECTORY_URL}/servers?environment=${environment ?? env.SERVER_ENVIRONMENT}`;

export function json(value: unknown, status = 200): Response {
  return Response.json(value, { status, headers: { "Cache-Control": "no-store" } });
}

export function region(value: unknown): Region {
  const found = REGIONS.find((item) => item === value);
  if (!found) {
    throw new Error("region must be one of " + REGIONS.join(", "));
  }
  return found;
}

/** A start request's `environment`, if it names one. */
export function environment(value: unknown): Environment | undefined {
  if (value === undefined) {
    return undefined;
  }
  const found = ENVIRONMENTS.find((item) => item === value);
  if (!found) {
    throw new Error("environment must be one of " + ENVIRONMENTS.join(", "));
  }
  return found;
}

/** A start request's `room`, if it names one. */
export function roomCode(value: unknown): string | undefined {
  const room = value ? String(value).toUpperCase() : undefined;
  if (room !== undefined && !/^[A-Z2-9]{8}$/.test(room)) {
    throw new Error("room must be an 8-character room code");
  }
  return room;
}

export function integer(
  value: unknown,
  fallback: number,
  min: number,
  max: number,
  label: string,
): number {
  const number = value === undefined || value === "" ? fallback : Number(value);
  if (!Number.isInteger(number) || number < min || number > max) {
    throw new Error(`${label} must be an integer from ${min} to ${max}`);
  }
  return number;
}

/** The Cloudflare colo this code runs in: location hints are best-effort, so swarms
 * report where they actually run. */
export async function currentColo(): Promise<string | undefined> {
  const response = await fetch("https://cloudflare.com/cdn-cgi/trace");
  return /^colo=(\w+)$/m.exec(await response.text())?.[1];
}

import path from "node:path";
import { parseArgs } from "node:util";
import { startDevServer, startStaticServer } from "./site-server.ts";
import { defaultBase, root } from "./site-build.ts";

const { values } = parseArgs({
  options: {
    host: { type: "string", default: "127.0.0.1" },
    port: { type: "string" },
    strictPort: { type: "boolean", default: false },
    open: { type: "boolean", default: false },
    preview: { type: "boolean", default: false },
    base: { type: "string", default: process.env.DEPLOY_BASE || defaultBase },
    // The checkout's build unless a directory is given (relative to the working one).
    outdir: { type: "string", default: path.join(root, "dist") },
  },
});
let port = Number(values.port ?? process.env.PORT ?? (values.preview ? 4173 : 5173));
if (!Number.isInteger(port) || port < 0 || port > 65535) throw new Error("Invalid port");
let server;
while (!server) {
  try {
    const options = { hostname: values.host, port, base: values.base };
    server = values.preview
      ? startStaticServer({ ...options, directory: values.outdir })
      : startDevServer(options);
  } catch (error) {
    if (
      (error as NodeJS.ErrnoException).code !== "EADDRINUSE" ||
      values.strictPort ||
      port === 0 ||
      port === 65535
    )
      throw error;
    console.log(`Port ${port} is in use, trying another one...`);
    port++;
  }
}
const url = new URL(values.base, server.url).href;
console.log(`Bun ${values.preview ? "preview" : "dev server"}: ${url}`);
if (values.open)
  Bun.spawn(
    process.platform === "darwin"
      ? ["open", url]
      : process.platform === "win32"
        ? ["cmd", "/c", "start", "", url]
        : ["xdg-open", url],
  );
for (const signal of ["SIGINT", "SIGTERM"] as const)
  process.on(signal, () => {
    void server.stop();
    process.exit(0);
  });

import { defineConfig } from "vite";
import { fileURLToPath } from "node:url";
import { baseRedirect } from "./scripts/base-redirect.ts";
import { pageHealth } from "./scripts/page-health.ts";

const base = process.env.DEPLOY_BASE || "/sloppy-tanks/";

export default defineConfig({
  base,
  // Preview launchers assign a free port through PORT; Vite does not read it itself.
  server: { port: Number(process.env.PORT) || undefined },
  plugins: [baseRedirect(base), pageHealth()],
  build: {
    rolldownOptions: {
      input: {
        main: fileURLToPath(new URL("./index.html", import.meta.url)),
      },
    },
  },
});

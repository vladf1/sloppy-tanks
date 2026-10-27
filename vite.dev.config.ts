import { mergeConfig } from "vite";
import config from "./vite.config.ts";
import { devSite, devPages } from "./scripts/dev-site.ts";
import { DIRECTORY_URL } from "./scripts/servers.mjs";

// The dev site always offers multiplayer, on the dev servers the directory lists. Vite
// reads VITE_* values from process.env after loading this file, and the startup build
// reads the directory directly, so setting them here covers both bundles.
process.env.VITE_DIRECTORY_URL = DIRECTORY_URL;
process.env.VITE_SERVER_ENVIRONMENT = "dev";
// It also keeps the development network tools a room page offers locally: the transport
// delay parameters and their slider, and the fairness comparisons.
process.env.VITE_DEV_TOOLS = "1";

// Load the shared configuration with the Cloudflare root asset base.
// The shared startup plugin also needs this base, so build:dev sets DEPLOY_BASE.
export default mergeConfig(config, {
  base: "/",
  plugins: [devSite()],
  build: {
    outDir: "dist-dev",
    rolldownOptions: {
      input: Object.fromEntries(
        ["test-pages.html", ...devPages.map((page) => page.path)].map((path) => [path, path]),
      ),
    },
  },
});

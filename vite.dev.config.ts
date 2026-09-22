import { mergeConfig } from "vite";
import config from "./vite.config.ts";
import { devSite, devPages } from "./scripts/dev-site.ts";

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

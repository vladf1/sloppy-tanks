import { defineConfig } from "vite";

export default defineConfig({
  base: "/sloppy-tanks/",
  build: {
    rolldownOptions: {
      output: {
        codeSplitting: {
          groups: [{ name: "vendor", test: /[\\/]node_modules[\\/]/ }],
        },
      },
    },
  },
});

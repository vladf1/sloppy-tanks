# Temporary Cloudflare test links

- `pnpm run tunnel` (`scripts/tunnel.mjs`) does the setup: it builds `dist/`,
  serves only `dist/` under the default `/sloppy-tanks/` base (nothing else in the
  checkout is reachable) on `127.0.0.1:4179` (`PORT` overrides), starts
  `cloudflared tunnel --url`, prints the public link and stops both on Ctrl-C.
  `--no-build` reuses the current `dist/`. Opened locally, `http://localhost:4179/`
  shows the link; through the tunnel, `/` goes to the game. Never tunnel the Vite
  dev server or the repository.
- Agents start it with the `tunnel` entry in `.claude/launch.json`, whose preview
  is the link page, and read the link from its output. Reuse a running tunnel
  when appropriate. If sandbox restrictions block local binding or external
  DNS/network access, request narrow execution escalation; do not treat that
  failure as a broken application.
- Verify the public page and assets, then start a game through that URL in a
  browser before reporting success. For touch work, verify that touch controls
  appear and pause/resume works with touch input.
- Keep the server and tunnel running for the requested testing session. Tell
  the user the link is temporary and requires this Mac to remain awake and
  connected. Restart it after source changes; it serves the build, not the
  source. When asked to stop, stop only this tunnel's command. Do not change the
  existing Pages deployments or save temporary hostnames as permanent project
  URLs.

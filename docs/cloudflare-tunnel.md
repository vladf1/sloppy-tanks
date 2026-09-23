# Temporary Cloudflare test links

Create a Cloudflare tunnel **only when the user explicitly requests one**.
Do not create public links automatically for development or browser checks.

- Use a fresh production build and expose only `dist/`, not the repository or
  a development server. Preserve the default `/sloppy-tanks/` asset base by
  serving a temporary directory containing a `sloppy-tanks` symlink to `dist`.
- Check `command -v cloudflared` and select an unused local port. For example,
  run the static server and tunnel as separate long-running processes:

  ```sh
  npm run build
  tunnel_root=$(mktemp -d /tmp/sloppy-tunnel-XXXXXX)
  ln -s "$PWD/dist" "$tunnel_root/sloppy-tanks"
  python3 -m http.server 4179 --bind 127.0.0.1 --directory "$tunnel_root"
  ```

  ```sh
  cloudflared tunnel --url http://127.0.0.1:4179 --no-autoupdate
  ```

- For an agent-managed link, launch both processes detached (for example,
  Python `subprocess.Popen` with `start_new_session=True` and stdin set to
  `DEVNULL`), redirect output to temporary logs, and record their PIDs. Reuse
  an existing verified server when appropriate. If sandbox restrictions block
  local binding or external DNS/network access, request narrow execution
  escalation; do not treat that failure as a broken application.
- Read the assigned `https://….trycloudflare.com` hostname from the tunnel log
  and append `/sloppy-tanks/`. Verify the public page and assets, then start a
  game through that URL in a browser before reporting success. For touch work,
  verify that touch controls appear and pause/resume works with touch input.
- Keep the server and tunnel running for the requested testing session. Tell
  the user the link is temporary and requires this Mac to remain awake and
  connected. Rebuild after source changes; the static server serves `dist/`.
  When asked to stop, terminate only the recorded processes belonging to this
  tunnel. Do not change the existing Pages deployments or save temporary
  hostnames as permanent project URLs.

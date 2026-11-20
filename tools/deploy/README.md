# Cloudflare deployment tools

Wrangler and its esbuild/workerd dependencies live in this separate package.
Cloudflare deployment jobs install it on demand using its own `bun.lock` and
isolated linking. The root install leaves it alone.
Only esbuild and workerd may run dependency install scripts in this package.

Install Node.js 26.13.1 on PATH to match the deployment workflow. On macOS,
`brew install node` installs Node 26. Check `node --version` if another Node is
already on PATH. Normal game development and the build checks use Bun.
From the repository root, use `bun run wrangler` for any Wrangler command:

```sh
bun run wrangler login
bun run wrangler dev --config bots/wrangler.jsonc --port 8788 --var DIRECTORY_URL:http://127.0.0.1:8787
bun run bots:deploy
bun run deploy:dev
```

The launcher first runs a frozen Bun install in this directory, then runs the pinned
Wrangler CLI on Node from the repository root. It skips Bun's `node` shim, takes the
first Node.js 22 or newer on PATH, and fails with setup instructions when there is none.
The first invocation installs the tools; later invocations check the lockfile against
the existing installation.
Deployment scripts use this same launcher automatically. Normal development and
bot control (`bun run bots`) do not need it.

Wrangler always uses Node because Miniflare needs Undici's dispatcher, which Bun
1.4.4's built-in replacement ignores
([upstream issue](https://github.com/oven-sh/bun/issues/39247)). `wrangler dev` is a
manual tool for local Worker development, not part of the build, tests, CI or
deployment scripts. The game dev server and native Rust multiplayer server do
not use Wrangler.

To update Wrangler, run `bun add --cwd tools/deploy wrangler@<version> --exact`
from the repository root and commit this package's manifest and lockfile.

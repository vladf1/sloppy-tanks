# Cloudflare deployment tools

Wrangler and its esbuild/workerd dependencies live here so normal game installs,
builds and GitHub Pages CI do not install them. This is a separate package, outside
the root postinstall, with its own committed `bun.lock` and isolated linking.
Only esbuild and workerd may run dependency install scripts in this package.

From the repository root, use `bun run wrangler` for any Wrangler command:

```sh
bun run wrangler login
bun run bots:deploy
bun run deploy:dev
```

The launcher first runs a frozen install in this directory, then runs the pinned
Wrangler CLI on Bun from the repository root. The first invocation installs the
tools; later invocations check the lockfile against the existing installation.
Deployment scripts use this same launcher automatically. Normal development and
bot control (`bun run bots`) do not need it.

Local Worker HTTP serving (`wrangler dev`) currently hangs under Bun 1.4.4:
Miniflare needs Undici's dispatcher, which Bun's built-in replacement ignores
([upstream issue](https://github.com/oven-sh/bun/issues/39247)). Deployment dry runs
work; local Worker serving remains an upstream limitation. The game dev server
and native Rust multiplayer server do not use Wrangler.

To update Wrangler, run `bun add --cwd tools/deploy wrangler@<version> --exact`
from the repository root and commit this package's manifest and lockfile.

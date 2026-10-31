# Sloppy Tanks workflow

Resolve paths from the current Git checkout instead of assuming a historical
worktree or a developer's home directory. The default reference is
`https://sloppytanks.com/` unless the user selects another build.

## Find the existing harness

Read `AGENTS.md`, `README.md` and the Desktop UI gauntlet section of
`scripts/README.md` in the chosen checkout. Confirm `package.json` defines
`check:desktop-ui`. If missing, inspect existing worktrees/branches containing the
harness before recreating it or claiming it is available on main.

The native shell is `crates/desktop` (winit + egui + wgpu/Metal), driven by the same
Rust game as the web shell. Primary owners:

- Native screens: `crates/desktop/src/ui/`; native minimum window size: `app.rs`.
- Web appearance: `src/style.css`, `src/game/ui*.ts`, `settings-dialog.ts`,
  `round-recap.ts`, `nerd-stats.ts`, `src/net/network-ui.ts`, `multiplayer.css`.
- Native fixtures and GPU capture: `crates/desktop/src/ui/snapshots.rs`.
- Production fixture rendering: `scripts/desktop-ui-capture.mjs`.
- Randomized A/B sheets and HTML: `scripts/desktop-ui-report.mjs`.
- Builds, tests, frozen references and source-fingerprint checks:
  `scripts/desktop-ui-gauntlet.mjs`.

The normal comparison sizes are 1280×800 and 1600×900 points at 2× pixels. The native
minimum is currently 1100×720; verify it in source before relying on it. The user
accepts a desktop minimum and does not require the web app's entire responsive
layout system. Preserve native Fullscreen, Quit and room-code lookup behavior.

## Run a campaign

Use a new evidence root for a new campaign so its first round captures the requested
current production build. Subsequent rounds reuse that campaign's frozen reference.
For example, choose an unused descriptive campaign name:

```sh
export PATH=/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH
pnpm install
pnpm run check:desktop-ui -- --out artifacts/performance/desktop-ui/parity-audit --round baseline
# Make supported fixes; keep this evidence root:
pnpm run check:desktop-ui -- --out artifacts/performance/desktop-ui/parity-audit --round corrected
```

The runner preserves immutable reference directories and rejects fixture drift.
`--refresh-reference` fetches the live URL again; it does not replay an archived
build. Use it for an intentional update to the reference, with a new campaign when
the build changes. To repair fixtures during a frozen campaign, inspect whether
the original build can be replayed from verified immutable assets and adapt the
capture mechanism if needed. Save corrected captures separately with the original
build/asset hashes. If replay is unavailable, record that blocker rather than
silently comparing against a newer deployment.

The runner runs desktop tests, generates GPU snapshots and A/B reports, and
writes `round.json` with status `needs-independent-review`. Record the independent
critic's result separately as `verdict.json` or `review.md` with the source hash and
specific accepted differences. Never edit a fingerprint to bypass a mismatch.

The harness needs Metal GPU access and installed Chrome; Chrome stays headless.
If the sandbox denies GPU/browser startup, request the appropriate execution
permission. If a managed worktree is outside the writable roots, use its returned
absolute path and normal approved file operations; do not edit the primary checkout
as a fallback. A task-specific `CARGO_TARGET_DIR` can reuse an existing build cache
when appropriate, without changing repository Cargo config.

Use CI’s default profiles for final checks so egui’s debug assertions run; the
optimized capture build does not exercise them. Keep validation in the desktop
scope unless other source changed:

```sh
cargo fmt --all --check
cargo clippy --locked -p sloppy-desktop --all-targets -- -D warnings
cargo test --locked -p sloppy-desktop
pnpm exec prettier --check scripts/desktop-ui-*.mjs
pnpm exec eslint scripts/desktop-ui-*.mjs --max-warnings 0
git diff --check
```

Use the broader project checks when the actual change requires them. Inspect native
minimum-size captures separately if needed; the web's stacked layout at 1100×720
is not an acceptance requirement for the native app.

## Judge real parity, not fixture mistakes

Cover both single-player and multiplayer setup, HUD, first person/low hull, respawn,
pause/controls, Settings, results, room host/guest/editing/connection states, and
both Stats for nerds panels. Expand the inventory when the current app adds states;
do not treat a historical scenario count as the completeness bar.

The capture script exposes production UI constructors only in locally intercepted
responses. Do not replace production CSS/markup with local source and call it a
production reference. Minified-module signatures can drift: repair the adapter
against actual served code, fail clearly when discovery breaks, and document any
instrumentation. Do not alter production or create rooms on its servers for fixture
screenshots.

Known comparison traps:

- Await menu readiness, image decode and fonts; opening-animation frames can look empty.
- Match player names, host roles, ready controls and room compatibility. Native test
  builds use a `test-content` sentinel; translate only that compatibility sentinel
  to the captured build's content version, preserving incompatible/full-room cases.
- Stats for nerds fixtures carry the page's own rows (`nerd_rows` for `Shell::Page`),
  written from the readings and rates the native panel shows.
- Set room results before applying the final lobby when the production renderer
  computes its heading during `lobby()`. Supply death cause for room respawn.
- First person uses a null viewer point for edge damage feedback and an active eye
  button. Directional damage can make a symmetric health rim look asymmetric.
- Flat arena stand-ins and held animation isolate shell layout. They do not prove
  live scene compositing, timing, input, performance or long-run cleanup.

These comparisons do not need a packaged app. `pnpm run desktop[:mac]` stamps
multiplayer versions but may also sign and submit notarization when configured.
Inspect that path before using it; do not add upload authorization to a UI task.
`cargo build --locked -p sloppy-desktop --profile desktop` is a local compile only;
an unstamped executable is suitable for local UI inspection but not proof that it
can join the production servers. Keep packaging/release verification separate.

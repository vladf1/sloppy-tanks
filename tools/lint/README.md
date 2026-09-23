# Lint toolchain

The game compiles with **TypeScript 7.0.2** (root `package.json`). TypeScript
ESLint 8.70 only supports compiler APIs below TypeScript 6.1, so this isolated
package pins TypeScript 6.0.3 for parsing and type-aware lint analysis. It never
compiles the game.

The root `postinstall` runs `npm ci --prefix tools/lint`, so both lockfiles
belong in version control. This avoids unsupported peer overrides while keeping
TypeScript 7 builds.

`eslint.config.mjs` applies the recommended JavaScript and TypeScript rules
everywhere and type-aware checks to `src/`. Prettier owns formatting. If newer
TypeScript syntax is rejected by the linter, update this toolchain deliberately
rather than suppressing its compatibility checks. See the
[typed-linting documentation](https://typescript-eslint.io/getting-started/typed-linting/).

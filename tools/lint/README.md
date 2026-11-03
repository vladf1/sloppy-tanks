# Lint toolchain

`bun check` checks the game. TypeScript ESLint 8.70 uses TypeScript compiler
APIs below 6.1, so this isolated package pins TypeScript 6.0.3 for parsing. It never
compiles or checks the game build.

The root `postinstall` runs `bun install --cwd tools/lint --frozen-lockfile`.
The root and lint `bun.lock` files belong in version control. ESLint runs through Bun with the
recommended JavaScript and TypeScript rules everywhere; Prettier owns formatting. Update this
package deliberately if new syntax needs parser support.

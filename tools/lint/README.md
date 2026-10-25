# Lint toolchain

Bun checks and bundles the game. TypeScript ESLint 8.70 uses TypeScript compiler
APIs below 6.1, so this isolated package pins TypeScript 6.0.3 for parsing and
type-aware lint analysis. It never compiles or checks the game build.

The root `postinstall` runs `bun install --cwd tools/lint --frozen-lockfile`.
Both `bun.lock` files belong in version control. ESLint runs through Bun with the
same recommended JavaScript, TypeScript and type-aware rules; Prettier owns
formatting. Update this package deliberately if new syntax needs parser support.

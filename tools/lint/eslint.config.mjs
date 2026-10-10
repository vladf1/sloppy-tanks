import { fileURLToPath } from "node:url";
import { includeIgnoreFile } from "@eslint/compat";
import js from "@eslint/js";
import tseslint from "typescript-eslint";
import prettier from "eslint-config-prettier";
import globals from "globals";

export default tseslint.config(
  // Whatever Git ignores (build outputs at any depth, generated code, caches) is never
  // source, so a stale output left in a checkout cannot fail the lint.
  includeIgnoreFile(fileURLToPath(new URL("../../.gitignore", import.meta.url))),
  {
    ignores: [".claude/**", "public/**", "scripts/benchmarks/**"],
  },
  js.configs.recommended,
  {
    languageOptions: { globals: { ...globals.browser, ...globals.node } },
    rules: { "no-unused-vars": ["error", { argsIgnorePattern: "^_", varsIgnorePattern: "^_" }] },
  },
  {
    files: ["**/*.ts"],
    extends: [tseslint.configs.recommended],
    rules: {
      "@typescript-eslint/consistent-type-imports": "error",
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
    },
  },
  {
    files: ["src/**/*.ts"],
    extends: [tseslint.configs.recommendedTypeChecked],
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: fileURLToPath(new URL("../..", import.meta.url)),
      },
    },
    rules: { eqeqeq: ["error", "always"], "one-var": ["error", "never"] },
  },
  prettier,
  // After `prettier`, which turns curly off.
  { files: ["src/**/*.ts"], rules: { curly: ["error", "all"] } },
);

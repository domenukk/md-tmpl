// @ts-check
import eslint from "@eslint/js";
import tseslint from "typescript-eslint";
import globals from "globals";

export default tseslint.config(
  // Global ignores: build output, dependencies and coverage.
  {
    ignores: ["dist/**", "node_modules/**", "coverage/**"],
  },

  // Base JavaScript recommended rules (applies to all files).
  eslint.configs.recommended,

  // Strong, type-aware linting for the TypeScript sources.
  {
    files: ["src/**/*.ts"],
    extends: [
      ...tseslint.configs.strictTypeChecked,
      ...tseslint.configs.stylisticTypeChecked,
    ],
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
      globals: {
        ...globals.node,
      },
    },
    rules: {
      // `describe`/`it`/`test` from the Node.js built-in test runner return
      // promises that are intentionally not awaited at registration time.
      // This is the officially documented handling for `node:test`, not a
      // blanket disable of the rule.
      "@typescript-eslint/no-floating-promises": [
        "error",
        {
          allowForKnownSafeCalls: [
            { from: "package", name: "describe", package: "node:test" },
            { from: "package", name: "it", package: "node:test" },
            { from: "package", name: "test", package: "node:test" },
          ],
        },
      ],
      // A leading underscore marks an identifier as intentionally unused. This
      // is required for parameters mandated by an implemented interface (e.g.
      // the `encoding` arg of `MemoryFs.readFileSync`) that a given
      // implementation does not need. Everything without the prefix is still
      // reported, so genuine dead bindings are not hidden.
      "@typescript-eslint/no-unused-vars": [
        "error",
        {
          args: "after-used",
          argsIgnorePattern: "^_",
          varsIgnorePattern: "^_",
          caughtErrorsIgnorePattern: "^_",
        },
      ],
    },
  },

  // The ESLint config file itself is plain JS: turn off type-aware rules
  // that require a TypeScript program.
  {
    files: ["eslint.config.mjs"],
    extends: [tseslint.configs.disableTypeChecked],
    languageOptions: {
      globals: {
        ...globals.node,
      },
    },
  },
);

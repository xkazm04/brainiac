import { dirname } from "node:path";
import { fileURLToPath } from "node:url";

import { FlatCompat } from "@eslint/eslintrc";

/*
 * ESLint for the console.
 *
 * There was no eslint dependency and no lint script here at all, which made
 * "lint is clean" a true statement about nothing. This config exists so the
 * claim costs something.
 *
 * `eslint-config-next` is still an eslintrc-shaped package (no flat export as of
 * 15.5), so FlatCompat is the supported bridge rather than a workaround.
 *
 * THE RULE ABOUT RULES: this was added to an existing ~200-file codebase, not to
 * a green field. Where a stock rule disagreed with an established, deliberate
 * pattern in this repo at scale, the rule is downgraded to `warn` with the
 * reason written down — never satisfied by rewriting working code. A lint config
 * that arrives with a thousand-line refactor attached is one nobody adopts.
 */

const compat = new FlatCompat({ baseDirectory: dirname(fileURLToPath(import.meta.url)) });

const config = [
  {
    ignores: [
      "node_modules/**",
      // `next build` writes to .next-build (see package.json); .next is the dev
      // server's. Both are generated, and so is anything else matching.
      ".next/**",
      ".next-*/**",
      "next-env.d.ts",
      // Generated from ../openapi.json by `npm run gen:api`.
      "src/lib/api-schema.d.ts",
    ],
  },
  ...compat.extends("next/core-web-vitals", "next/typescript"),
  {
    rules: {
      // ── downgraded, with reasons ──────────────────────────────────────
      //
      // The console renders design-system colour bands, mermaid diagrams and
      // one-off marketing surfaces; <img> for a decorative asset is a
      // deliberate choice in several of them and next/image would add a loader
      // round trip for no benefit. Worth a nudge, not a block.
      "@next/next/no-img-element": "warn",
      // The API surface is typed from an OpenAPI schema that models several
      // payloads as open records; a handful of boundary casts go through
      // `any` on purpose (lib/api.ts documents them). Flagging is useful;
      // failing the build over it would push people to `unknown`-and-cast,
      // which is the same hole with more ceremony.
      "@typescript-eslint/no-explicit-any": "warn",
      // Prefixed-underscore args are the repo's existing convention for
      // deliberately-unused route handler params (`_req`).
      "@typescript-eslint/no-unused-vars": [
        "warn",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_", caughtErrors: "none" },
      ],
    },
  },
];

export default config;

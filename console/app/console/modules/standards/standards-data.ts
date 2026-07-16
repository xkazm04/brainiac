/*
 * Demo fixtures for the standards board — the Meridian org's library, shaped
 * to exercise every state the surface renders: adopted rules with provenance
 * and a pulse, a proposal fresh off the drift detector, a decreed rule with
 * no evidence, and a retirement. Served only behind <DemoBanner> when the
 * brainiac server is unreachable.
 */

import type { LibraryStandard, StandardDetail } from "@/lib/types";

export const DEMO_STANDARDS: LibraryStandard[] = [
  {
    id: "demo-std-unwrap",
    origin: "human",
    stack: "rust",
    category: "errors",
    slug: "no-unwrap-in-handlers",
    statement: "Request handlers never unwrap; they map errors to typed responses.",
    rationale:
      "The June psp-gateway incident: one unwrap on a malformed webhook took down the whole payments API for 40 minutes.",
    detail_md:
      "```rust\n// ✗ retire\nlet body: Webhook = serde_json::from_slice(&raw).unwrap();\n\n// ✓ copy\nlet body: Webhook = serde_json::from_slice(&raw)\n    .map_err(|e| HttpError::bad_request(e))?;\n```",
    enforcement: "mandatory",
    lifecycle: "adopted",
    adopted_at: "2026-07-01T09:12:00Z",
    decreed: false,
  },
  {
    id: "demo-std-pg-serial",
    origin: "human",
    stack: "rust",
    category: "testing",
    slug: "pg-tests-take-the-harness-lock",
    statement:
      "Every Postgres test binary takes the shared advisory lock before touching the database.",
    rationale:
      "Two test binaries truncating one database mid-run destroyed each other's fixtures — twice — before the lock existed.",
    detail_md: null,
    enforcement: "recommended",
    lifecycle: "adopted",
    adopted_at: "2026-06-20T14:30:00Z",
    decreed: false,
  },
  {
    id: "demo-std-retry",
    origin: "sweep",
    stack: "general",
    category: "practice",
    slug: "service-retry-policy",
    statement: "Exponential backoff with full jitter, max 30s.",
    rationale: "payments retries 3x fixed, data retries with full jitter",
    detail_md: null,
    enforcement: "recommended",
    lifecycle: "proposed",
    adopted_at: null,
    decreed: false,
  },
  {
    id: "demo-std-barrels",
    origin: "agent",
    stack: "typescript",
    category: "imports",
    slug: "no-barrel-exports",
    statement: "Import from the module, not from an index barrel.",
    rationale:
      "Barrels turned a one-line change into a full type-check of the console; build time doubled before anyone noticed why.",
    detail_md:
      "```ts\n// ✗ retire\nimport { Stamp } from \"@/library\";\n\n// ✓ copy\nimport { Stamp } from \"@/library/primitives\";\n```",
    enforcement: "recommended",
    lifecycle: "adopted",
    adopted_at: "2026-07-08T10:00:00Z",
    decreed: false,
  },
  {
    id: "demo-std-interfaces",
    origin: "human",
    stack: "typescript",
    category: "style",
    slug: "prefer-interface-for-props",
    statement: "Component props are declared as interfaces, not type aliases.",
    rationale: "Superseded: the distinction stopped paying for the churn it caused in review.",
    detail_md: null,
    enforcement: "experimental",
    lifecycle: "deprecated",
    adopted_at: "2026-05-02T08:00:00Z",
    decreed: false,
  },
  {
    id: "demo-std-spaces",
    origin: "human",
    stack: "general",
    category: "style",
    slug: "spaces-not-tabs",
    statement: "Spaces.",
    rationale: null,
    detail_md: null,
    enforcement: "recommended",
    lifecycle: "adopted",
    adopted_at: "2026-04-15T12:00:00Z",
    decreed: true,
  },
];

const detail = (
  id: string,
  extra: Pick<StandardDetail, "provenance" | "usage" | "versions">,
): StandardDetail => {
  const s = DEMO_STANDARDS.find((r) => r.id === id)!;
  return { ...s, ...extra };
};

/** Detail per rule — provenance, the per-team pulse, version history. */
export const DEMO_STANDARD_DETAILS: Record<string, StandardDetail> = {
  "demo-std-unwrap": detail("demo-std-unwrap", {
    provenance: [
      { kind: "memory", ref_id: "3f6f2a10-0000-4000-8000-000000000041" },
      { kind: "memory", ref_id: "3f6f2a10-0000-4000-8000-000000000042" },
    ],
    usage: [
      { team: "payments", uses: 61 },
      { team: "platform", uses: 34 },
      { team: "data", uses: 12 },
    ],
    versions: [
      {
        rev: 2,
        statement: "Request handlers never unwrap; they map errors to typed responses.",
        enforcement: "mandatory",
        created_at: "2026-07-01T09:12:00Z",
      },
      {
        rev: 1,
        statement: "Avoid unwrap in request handlers.",
        enforcement: "recommended",
        created_at: "2026-06-18T16:40:00Z",
      },
    ],
  }),
  "demo-std-pg-serial": detail("demo-std-pg-serial", {
    provenance: [{ kind: "memory", ref_id: "3f6f2a10-0000-4000-8000-000000000043" }],
    usage: [
      { team: "platform", uses: 18 },
      { team: "payments", uses: 9 },
    ],
    versions: [
      {
        rev: 1,
        statement:
          "Every Postgres test binary takes the shared advisory lock before touching the database.",
        enforcement: "recommended",
        created_at: "2026-06-20T14:30:00Z",
      },
    ],
  }),
  "demo-std-retry": detail("demo-std-retry", {
    // Fresh off the drift detector: the divergence IS the evidence.
    provenance: [{ kind: "divergence", ref_id: "3f6f2a10-0000-4000-8000-000000000061" }],
    usage: [],
    versions: [
      {
        rev: 1,
        statement: "Exponential backoff with full jitter, max 30s.",
        enforcement: "recommended",
        created_at: "2026-07-14T11:05:00Z",
      },
    ],
  }),
  "demo-std-barrels": detail("demo-std-barrels", {
    provenance: [{ kind: "memory", ref_id: "3f6f2a10-0000-4000-8000-000000000044" }],
    usage: [
      { team: "platform", uses: 27 },
      { team: null, uses: 3 },
    ],
    versions: [
      {
        rev: 1,
        statement: "Import from the module, not from an index barrel.",
        enforcement: "recommended",
        created_at: "2026-07-08T10:00:00Z",
      },
    ],
  }),
  "demo-std-interfaces": detail("demo-std-interfaces", {
    provenance: [{ kind: "memory", ref_id: "3f6f2a10-0000-4000-8000-000000000045" }],
    usage: [{ team: "platform", uses: 2 }],
    versions: [
      {
        rev: 1,
        statement: "Component props are declared as interfaces, not type aliases.",
        enforcement: "experimental",
        created_at: "2026-05-02T08:00:00Z",
      },
    ],
  }),
  "demo-std-spaces": detail("demo-std-spaces", {
    // The decreed rule: no evidence rows at all — the human's name IS the
    // attribution, and the surface must render that honestly.
    provenance: [],
    usage: [{ team: "payments", uses: 4 }],
    versions: [
      {
        rev: 1,
        statement: "Spaces.",
        enforcement: "recommended",
        created_at: "2026-04-15T12:00:00Z",
      },
    ],
  }),
};

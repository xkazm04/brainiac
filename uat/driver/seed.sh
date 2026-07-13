#!/usr/bin/env bash
# Seed the Company: Meridian corpus -> company extension -> decoys -> per-Character tokens.
#
#   DATABASE_URL=postgres://brainiac:brainiac@localhost:5433/brainiac_uat_<run> ./seed.sh
#
# Run this BEFORE any session. Plant, drain, review — then run. A decoy injected mid-run is a
# different experiment.
set -euo pipefail

: "${DATABASE_URL:?DATABASE_URL must be set — and it MUST be a run-scoped database}"
API="${BRAINIAC_API:-http://localhost:8600}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

case "$DATABASE_URL" in
  *brainiac_uat_*) ;;
  *) echo "REFUSING: DATABASE_URL is not a run-scoped uat database."
     echo "Two runs sharing a database cross-pollute each other's memories — including each"
     echo "other's DECOYS — and silently poison both. Use brainiac_uat_<run-id>."
     exit 1 ;;
esac

# ── 1. Meridian, day one ──────────────────────────────────────────────────────
# There is no dedicated seed command. `eval` re-seeds the tenant from the fixture tree and is
# DESTRUCTIVE by design (main.rs: "DESTRUCTIVE to the connected database (re-seeds the tenant)").
# That destructiveness is exactly what we want from a seeder — but it is why the guard above
# exists. Seeding also gives us a free retrieval baseline for the report header.
echo "==> Seeding Meridian (fixtures/v1) + capturing the retrieval baseline"
cargo run -q -p brainiac-server -- eval \
  --fixtures "$ROOT/fixtures/v1" \
  --profile retrieval \
  --out "$ROOT/uat/runs/${RUN_ID:-current}/seed-eval.json"

# Validate the corpus before trusting anything built on it.
cargo run -q -p brainiac-server -- fixtures lint --fixtures "$ROOT/fixtures/v1" || {
  echo "Fixture tree failed its own integrity check. Stop." ; exit 1 ; }

# ── 2. The company extension ──────────────────────────────────────────────────
# team-web + the 6 new principals from company.md. These do not exist in fixtures/v1.
# TODO(fixtures/v2): checkout-web has no corpus. Every web journey is untestable until it does,
# and every cross-STACK claim is `not probed` — never `clean` — until fixtures carry real code.
echo "==> Applying the company extension (team-web, 6 new principals)"
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f "$(dirname "$0")/company_extension.sql"

# ── 3. The decoys ─────────────────────────────────────────────────────────────
# D1 (canonical lie), D2 (raw lie — the one that tests whether review protects anyone at all),
# D4 (a credential pasted into a transcript). See ../decoys.md for the expected safe behavior of
# each; a decoy without a pre-registered expectation is a rorschach test.
echo "==> Planting decoys"
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f "$(dirname "$0")/decoys.sql"

# ── 4. Per-Character tokens ───────────────────────────────────────────────────
# Minted, scoped brk_ tokens — NOT env tokens. Env tokens carry every scope (auth.rs:1-6), so
# running the contractor or the leak probes on one would hide exactly the H4 failure we are
# hunting and report a permission model that does not exist.
echo "==> Minting scoped tokens (one per Character)"
: "${BRAINIAC_BOOTSTRAP_TOKEN:?operator token required to mint}"
mkdir -p "$ROOT/uat/runs/${RUN_ID:-current}"
: > "$ROOT/uat/.tokens"   # gitignored

while IFS='|' read -r slug principal scopes; do
  [ -z "$slug" ] && continue
  secret=$(curl -sS -X POST "$API/v1/tokens" \
    -H "Authorization: Bearer $BRAINIAC_BOOTSTRAP_TOKEN" \
    -H 'Content-Type: application/json' \
    -d "{\"name\":\"uat-$slug\",\"user_id\":\"$principal\",\"scopes\":[$scopes]}" \
    | python -c 'import sys,json; print(json.load(sys.stdin)["secret"])')
  echo "TOK_${slug}=${secret}" >> "$ROOT/uat/.tokens"
done <<'EOF'
ADA|user-pay-dev1|"read","write"
PETRA|user-pay-lead|"read","write"
TOMAS|user-plat-dev1|"read","write"
INGRID|user-data-analyst1|"read","write"
JONAS|user-web-dev1|"read","write"
MIRA|user-pay-new|"read","write"
SAM|user-staff|"read"
RAFAEL|user-contractor|"read"
DANA|user-em|"read"
YUSUF|user-plat-sec|"read"
LARS|user-plat-lead|"read","write"
NADIA|user-pay-oncall|"read","write"
EOF

# ── 5. Drain, then verify ─────────────────────────────────────────────────────
echo "==> Waiting for the pipeline to drain"
until curl -sS "$API/v1/queue/health" | grep -q '"depth":0'; do sleep 5; done

echo
echo "Seeded. Before you trust a single number, confirm:"
echo "  - the WORKER is running (extract/embed/resolve/contradict/promote). Without it the"
echo "    store never grows, no relay completes, and the run reports green on an empty corpus."
echo "  - which BYOM provider and embedder this run used. MockProvider measures PLUMBING, not"
echo "    knowledge; the deterministic embedder's numbers are plumbing numbers (PLAN.md dev. 4)."
echo "    Whichever it was, it goes in the report headline — not a footnote."

#!/usr/bin/env bash
#
# Brainiac — walk a fresh deployment OFF the break-glass admin token.
#
# A first boot runs on BRAINIAC_TOKENS: an env map whose JSON key is a bearer
# secret. Those entries carry EVERY scope and bypass the scope system entirely
# (crates/brainiac-server/src/auth.rs — `scopes = None` makes `allows()` always
# true), and they are un-revocable and un-audited. This script performs the
# hand-off documented in docs/deploy/alibaba.md § "Auth & the break-glass token"
# so it happens once, correctly, instead of being skipped:
#
#   1. mint a scoped, revocable `brk_` key via POST /v1/tokens
#   2. verify the new key actually authenticates (GET /v1/reviews/promotions)
#   3. rewrite BRAINIAC_API_TOKEN in .env.deploy and restart the console
#   4. tell you to delete BRAINIAC_TOKENS and keep it offline
#
#   scripts/first-boot.sh
#   SCOPES=admin scripts/first-boot.sh            # console key-management too
#   TOKEN_NAME=mcp-agent SCOPES=read,write scripts/first-boot.sh --to-file
#
# SECRETS: no secret value is ever printed. The minted key goes straight from
# the API response into $ENV_FILE; only its 12-char display prefix is shown.
# Bearer tokens are passed to curl through a stdin config file (`curl -K -`),
# never on a command line, so they never appear in the host process list.
#
# IDEMPOTENT: re-running when BRAINIAC_API_TOKEN is already a `brk_` key
# verifies it and re-reports the remaining steps without minting anything. A
# live key with the same name also stops the run, because the plaintext of the
# existing one can never be recovered and a silent second key is just litter.
#
# Env overrides (all optional):
#   COMPOSE_FILE  compose file to target  (default docker-compose.deploy.yml)
#   ENV_FILE      env file to read/write  (default .env.deploy)
#   SERVICE       server service name     (default server)
#   TOKEN_NAME    name for the minted key (default console)
#   SCOPES        comma-separated scopes  (default the full non-admin set)
#   CURL_IMAGE    throwaway curl image    (default curlimages/curl:latest)
#   SECRETS_DIR   where --to-file drops keys (default ./secrets, self-ignored)
#
# Flags:
#   --to-file     mint + verify, then write the secret to a 0600 file under
#                 $SECRETS_DIR instead of into $ENV_FILE, and restart nothing.
#                 This is the path for agent / MCP / CI keys, which do not
#                 belong in the console's env.

set -euo pipefail

COMPOSE_FILE="${COMPOSE_FILE:-docker-compose.deploy.yml}"
ENV_FILE="${ENV_FILE:-.env.deploy}"
SERVICE="${SERVICE:-server}"
TOKEN_NAME="${TOKEN_NAME:-console}"
CURL_IMAGE="${CURL_IMAGE:-curlimages/curl:latest}"
SECRETS_DIR="${SECRETS_DIR:-./secrets}"
# The console's knowledge-plane scopes. Every name here is checked against
# auth.rs::SCOPES, which is the mintable vocabulary:
#   read write kb:read kb:publish lib:read lib:propose lib:publish admin
# `admin` is deliberately EXCLUDED by default so a leaked console key cannot
# mint further keys. The trade-off (documented in docs/deploy/alibaba.md): the
# console's own /keys, org-directory and blast-radius modules require `admin`
# and will 403 — re-run with SCOPES=admin if you want them in-console.
SCOPES="${SCOPES:-read,write,kb:read,kb:publish,lib:read,lib:propose,lib:publish}"

WRITE_ENV=1
for arg in "$@"; do
  case "$arg" in
    --to-file) WRITE_ENV=0 ;;
    -h|--help) sed -n '2,50p' "$0"; exit 0 ;;
    *) echo "x Unknown argument: $arg (try --help)" >&2; exit 2 ;;
  esac
done

# Resolve relative paths from the repo root, so this works from any cwd.
cd "$(dirname "$0")/.."

[ -f "$ENV_FILE" ] || { echo "x No $ENV_FILE — copy .env.deploy.example first." >&2; exit 1; }

dc() { docker compose -f "$COMPOSE_FILE" --env-file "$ENV_FILE" "$@"; }

# Read one KEY=value out of the env file without sourcing it (BRAINIAC_TOKENS
# is raw JSON — braces and quotes make `.` on this file unsafe).
env_get() {
  awk -v k="$1" '
    index($0, k "=") == 1 {
      sub("^" k "=", "")
      sub(/[[:space:]]+$/, "")
      # strip one layer of matching quotes if present
      if ($0 ~ /^".*"$/ || $0 ~ /^'"'"'.*'"'"'$/) $0 = substr($0, 2, length($0) - 2)
      print; exit
    }' "$ENV_FILE"
}

API_TOKEN="$(env_get BRAINIAC_API_TOKEN)"
BREAK_GLASS="$(env_get BRAINIAC_TOKENS)"
[ -n "$API_TOKEN" ] || { echo "x BRAINIAC_API_TOKEN is not set in $ENV_FILE." >&2; exit 1; }

# The server's 8600 is intentionally NOT published to the host (only the
# console's 80 is), so talk to it from a throwaway container that shares the
# server container's network namespace.
SERVER_CID="$(dc ps -q "$SERVICE" 2>/dev/null || true)"
if [ -z "$SERVER_CID" ]; then
  echo "x Service '$SERVICE' is not running. Start the stack first:" >&2
  echo "    docker compose -f $COMPOSE_FILE --env-file $ENV_FILE up -d" >&2
  exit 1
fi

# api <method> <path> <bearer> [json-body]
# Everything sensitive travels in a curl config file on stdin; stdout is the
# response body with the HTTP status appended as a final line.
api() {
  method="$1"; path="$2"; bearer="$3"; body="${4:-}"
  {
    printf 'silent\nshow-error\n'
    printf 'request = "%s"\n' "$method"
    printf 'url = "http://localhost:8600%s"\n' "$path"
    printf 'header = "Authorization: Bearer %s"\n' "$bearer"
    if [ -n "$body" ]; then
      printf 'header = "Content-Type: application/json"\n'
      # curl's config parser unescapes \\ and \" inside a quoted value.
      printf 'data = "%s"\n' "$(printf '%s' "$body" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g')"
    fi
    printf 'write-out = "\\n%%{http_code}"\n'
  } | docker run --rm -i --network "container:$SERVER_CID" "$CURL_IMAGE" -K -
}

http_code() { printf '%s' "$1" | tail -n1; }
http_body() { printf '%s' "$1" | sed '$d'; }

# Prove a bearer authenticates, using an endpoint that needs only `read` (it is
# a plain DB listing — no embedding call, so no LLM spend) and is therefore
# satisfied by every scope set this script can mint, `admin` included.
verify_token() {
  resp="$(api GET '/v1/reviews/promotions?limit=1' "$1")"
  [ "$(http_code "$resp")" = "200" ]
}

remaining_steps() {
  if [ -n "$BREAK_GLASS" ]; then
    cat <<EOF

Remaining step — retire the break-glass map:
  1. Copy the BRAINIAC_TOKENS line out of $ENV_FILE into a password manager.
     It is un-revocable, so losing it means losing the ability to mint or
     revoke keys; leaking it means an un-auditable full-authority key.
  2. Delete that line from $ENV_FILE.
  3. docker compose -f $COMPOSE_FILE --env-file $ENV_FILE up -d $SERVICE

An empty map is fine: the server only logs a warning at boot, and \`brk_\` keys
keep resolving from the api_tokens table. Restore the line temporarily whenever
you need to mint or revoke.
EOF
  else
    echo
    echo "ok BRAINIAC_TOKENS is no longer in $ENV_FILE — the break-glass map is retired."
    echo "   Keep it offline; restore it temporarily to mint or revoke keys."
  fi
  echo
  echo "List keys:   GET  /v1/tokens        Revoke:  POST /v1/tokens/{id}/revoke"
}

# ── already migrated? ───────────────────────────────────────────────────
if [ "${API_TOKEN#brk_}" != "$API_TOKEN" ] && [ "$WRITE_ENV" = "1" ]; then
  echo "-> BRAINIAC_API_TOKEN is already a managed key (${API_TOKEN:0:12}…). Verifying ..."
  if verify_token "$API_TOKEN"; then
    echo "ok It authenticates. Nothing to mint."
  else
    echo "x It does NOT authenticate (revoked, or from another deployment)." >&2
    echo "  Mint a replacement with the break-glass secret, or set" >&2
    echo "  BRAINIAC_API_TOKEN back to it and re-run." >&2
    exit 1
  fi
  remaining_steps
  exit 0
fi

# ── the minting token must be able to mint ──────────────────────────────
echo "-> Checking the current token can mint (needs the \`admin\` scope) ..."
existing="$(api GET '/v1/tokens' "$API_TOKEN")"
case "$(http_code "$existing")" in
  200) ;;
  401) echo "x 401 — BRAINIAC_API_TOKEN is not a key of the BRAINIAC_TOKENS map and not a live brk_ key." >&2; exit 1 ;;
  403) echo "x 403 — that token lacks the \`admin\` scope, so it cannot mint. Use the break-glass secret." >&2; exit 1 ;;
  *)   echo "x Unexpected response from GET /v1/tokens:" >&2; http_body "$existing" >&2; exit 1 ;;
esac

# Idempotence guard: a live key already carries this name. Its plaintext is
# unrecoverable by design, so minting a second one would just leave an
# unusable row behind. Make the operator choose.
# TokenSummary objects are flat (no nested objects), so splitting the response
# on `{` puts one key per line — no JSON parser, no field-order assumption.
if http_body "$existing" | tr '{' '\n' \
     | grep -F "\"name\":\"$TOKEN_NAME\"" \
     | grep -q '"revoked_at":null'; then
  echo "x A live key named '$TOKEN_NAME' already exists in this org." >&2
  echo "  Its secret can never be re-read. Either re-run with TOKEN_NAME=<other>," >&2
  echo "  or revoke the old one first (GET /v1/tokens for its id, then" >&2
  echo "  POST /v1/tokens/{id}/revoke)." >&2
  exit 1
fi

# ── mint ────────────────────────────────────────────────────────────────
# Build the JSON scopes array from the comma list.
scopes_json="$(printf '%s' "$SCOPES" | awk -F, '{ out=""; for (i=1;i<=NF;i++) { gsub(/^[ \t]+|[ \t]+$/, "", $i); if ($i != "") out = out (out=="" ? "" : ",") "\"" $i "\"" } print "[" out "]" }')"
echo "-> Minting '$TOKEN_NAME' with scopes $SCOPES ..."
minted="$(api POST '/v1/tokens' "$API_TOKEN" "{\"name\":\"$TOKEN_NAME\",\"scopes\":$scopes_json}")"
if [ "$(http_code "$minted")" != "201" ]; then
  # No secret in a failure body — safe to show.
  echo "x Mint failed (HTTP $(http_code "$minted")):" >&2
  http_body "$minted" >&2
  exit 1
fi

# Never echo $minted from here on: it carries the plaintext secret.
NEW_TOKEN="$(http_body "$minted" | sed -n 's/.*"token":"\(brk_[0-9a-f]*\)".*/\1/p')"
minted=""
[ -n "$NEW_TOKEN" ] || { echo "x Minted, but could not parse the secret out of the 201 response." >&2; exit 1; }
echo "ok Minted ${NEW_TOKEN:0:12}… (shown once by the API; never retrievable again)"

echo "-> Verifying the new key ..."
verify_token "$NEW_TOKEN" || { echo "x The new key did not authenticate — leaving $ENV_FILE untouched." >&2; exit 1; }
echo "ok Verified."

if [ "$WRITE_ENV" = "0" ]; then
  # Self-ignoring drop box, same idiom as scripts/backup.sh's ./backups.
  mkdir -p "$SECRETS_DIR"
  [ -f "$SECRETS_DIR/.gitignore" ] || printf '*\n!.gitignore\n' > "$SECRETS_DIR/.gitignore"
  out="$SECRETS_DIR/$TOKEN_NAME.key"
  ( umask 077; printf '%s\n' "$NEW_TOKEN" > "$out" )
  chmod 600 "$out" 2>/dev/null || true
  echo
  echo "ok Secret written to $out (0600, git-ignored). $ENV_FILE untouched."
  echo "   Move it into your password manager / the agent's config, then delete it."
  exit 0
fi

# ── swap it into the env file ───────────────────────────────────────────
# The secret goes through the environment, not argv: /proc/<pid>/cmdline is
# world-readable, /proc/<pid>/environ is not.
# Backup lands in a self-ignoring dir (scripts/backup.sh's idiom) rather than
# next to the env file — `.env.deploy.bak` at the repo root is NOT covered by
# .gitignore's `.env.deploy` entry and would be a committable secret.
mkdir -p "$SECRETS_DIR"
[ -f "$SECRETS_DIR/.gitignore" ] || printf '*\n!.gitignore\n' > "$SECRETS_DIR/.gitignore"
backup="$SECRETS_DIR/$(basename "$ENV_FILE").$(date +%Y%m%d-%H%M%S).bak"
( umask 077; cat "$ENV_FILE" > "$backup" )
tmp="$ENV_FILE.tmp.$$"
BRK="$NEW_TOKEN" awk '
  BEGIN { done = 0 }
  index($0, "BRAINIAC_API_TOKEN=") == 1 && !done { print "BRAINIAC_API_TOKEN=" ENVIRON["BRK"]; done = 1; next }
  { print }
  END { if (!done) print "BRAINIAC_API_TOKEN=" ENVIRON["BRK"] }
' "$ENV_FILE" > "$tmp"
chmod 600 "$tmp" 2>/dev/null || true
mv "$tmp" "$ENV_FILE"
echo "ok BRAINIAC_API_TOKEN in $ENV_FILE now points at the managed key (backup: $backup)."

echo "-> Restarting the console on the new key ..."
dc up -d console
echo "ok Console restarted."

remaining_steps

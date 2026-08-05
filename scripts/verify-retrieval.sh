#!/usr/bin/env bash
#
# Brainiac — prove the INSTALL WORKS, not that a page renders.
#
# `curl http://localhost/` only proves the console's pitch page is being served.
# It is served by a static-ish Next route that never touches the API, so it
# stays green through a dead database, a wrong BRAINIAC_API_TOKEN, an embedder
# with no credit, and a worker that never starts. Every one of those breaks the
# product completely while that check passes.
#
# This exercises the actual claim — "org memory an agent can retrieve from" —
# end to end:
#
#   1. POST /v1/memories        write a uniquely-marked statement
#   2. GET  /v1/sources/{id}    poll until the pipeline has processed it
#   3. POST /v1/memories/search retrieve it back by its marker
#
# That covers auth, scopes, RLS, the Postgres write path, the job queue, the
# worker, the embedder, and hybrid retrieval. If it prints `ok`, the install
# works.
#
#   scripts/verify-retrieval.sh                 # docker deploy stack
#   BRAINIAC_API_URL=http://127.0.0.1:8600 \
#   BRAINIAC_API_TOKEN=... scripts/verify-retrieval.sh    # direct (dev)
#
# WHAT IT LEAVES BEHIND: one memory, in the org the token belongs to, whose
# text says what it is ("Brainiac install verification …") and carries the
# marker below. That is deliberate — a check that writes nothing cannot prove
# the write path. Find it later with the marker in the archive module and
# deprecate it if you would rather it were not there. Re-running mints another;
# on a real org, run this once at install time.
#
# SECRETS: the bearer travels through a curl config file on stdin (`curl -K -`),
# never on a command line, so it is not in the host process list. No secret
# value is ever printed.
#
# Env overrides (all optional):
#   BRAINIAC_API_URL    talk to this URL directly instead of via docker
#   BRAINIAC_API_TOKEN  bearer to use (required in direct mode; read from the
#                       env file in docker mode)
#   COMPOSE_FILE        default docker-compose.deploy.yml
#   ENV_FILE            default .env.deploy
#   SERVICE             server service name, default `server`
#   CURL_IMAGE          default curlimages/curl:latest
#   TIMEOUT_SECS        how long to wait for the pipeline, default 180

set -euo pipefail

COMPOSE_FILE="${COMPOSE_FILE:-docker-compose.deploy.yml}"
ENV_FILE="${ENV_FILE:-.env.deploy}"
SERVICE="${SERVICE:-server}"
CURL_IMAGE="${CURL_IMAGE:-curlimages/curl:latest}"
TIMEOUT_SECS="${TIMEOUT_SECS:-180}"

case "${1:-}" in
  -h|--help) sed -n '2,45p' "$0"; exit 0 ;;
  "") ;;
  *) echo "x Unknown argument: ${1} (try --help)" >&2; exit 2 ;;
esac

# Resolve relative paths from the repo root, so this works from any cwd.
cd "$(dirname "$0")/.."

# A marker no corpus could contain by accident — this is what makes step 3 a
# real assertion rather than "some hit came back".
MARKER="brainiac-install-check-$(date +%Y%m%dT%H%M%S)-$$"
STATEMENT="Brainiac install verification: this deployment's write-then-retrieve round trip was confirmed. Verification marker $MARKER."

# ── transport ───────────────────────────────────────────────────────────
# Two modes. Direct: an operator or a dev already has a reachable URL. Docker:
# the deploy stack does NOT publish the server's 8600 to the host (only Caddy's
# 80/443 are public), so we borrow the server container's network namespace —
# the same trick scripts/first-boot.sh uses.

if [ -n "${BRAINIAC_API_URL:-}" ]; then
  MODE=direct
  BASE="${BRAINIAC_API_URL%/}"
  TOKEN="${BRAINIAC_API_TOKEN:-}"
  [ -n "$TOKEN" ] || { echo "x BRAINIAC_API_URL is set but BRAINIAC_API_TOKEN is not." >&2; exit 1; }
  command -v curl >/dev/null || { echo "x curl not found on PATH." >&2; exit 1; }
else
  MODE=docker
  BASE="http://localhost:8600"
  [ -f "$ENV_FILE" ] || { echo "x No $ENV_FILE. Set BRAINIAC_API_URL + BRAINIAC_API_TOKEN to check a non-docker stack." >&2; exit 1; }
  # Read one KEY=value without sourcing the file (BRAINIAC_TOKENS is raw JSON).
  TOKEN="$(awk -v k=BRAINIAC_API_TOKEN '
    index($0, k "=") == 1 {
      sub("^" k "=", ""); sub(/[[:space:]]+$/, "")
      if ($0 ~ /^".*"$/ || $0 ~ /^'"'"'.*'"'"'$/) $0 = substr($0, 2, length($0) - 2)
      print; exit
    }' "$ENV_FILE")"
  [ -n "$TOKEN" ] || { echo "x BRAINIAC_API_TOKEN is not set in $ENV_FILE." >&2; exit 1; }
  SERVER_CID="$(docker compose -f "$COMPOSE_FILE" --env-file "$ENV_FILE" ps -q "$SERVICE" 2>/dev/null || true)"
  if [ -z "$SERVER_CID" ]; then
    echo "x Service '$SERVICE' is not running. Start the stack first:" >&2
    echo "    docker compose -f $COMPOSE_FILE --env-file $ENV_FILE up -d" >&2
    exit 1
  fi
fi

# api <method> <path> [json-body] -> body, with the HTTP status as a final line.
api() {
  method="$1"; path="$2"; body="${3:-}"
  {
    printf 'silent\nshow-error\n'
    printf 'request = "%s"\n' "$method"
    printf 'url = "%s%s"\n' "$BASE" "$path"
    printf 'header = "Authorization: Bearer %s"\n' "$TOKEN"
    if [ -n "$body" ]; then
      printf 'header = "Content-Type: application/json"\n'
      # curl's config parser unescapes \\ and \" inside a quoted value.
      printf 'data = "%s"\n' "$(printf '%s' "$body" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g')"
    fi
    printf 'write-out = "\\n%%{http_code}"\n'
  } | if [ "$MODE" = direct ]; then
        curl -K -
      else
        # --entrypoint is explicit rather than relying on curlimages/curl's
        # default, so a CURL_IMAGE override (an air-gapped host substituting
        # whatever image it already has) works without also having curl as its
        # entrypoint.
        docker run --rm -i --entrypoint curl --network "container:$SERVER_CID" "$CURL_IMAGE" -K -
      fi
}

# Same, but a TRANSPORT failure (curl could not connect at all) becomes a
# sentence instead of a bare `exit 7` from `set -e` — "connection refused" is
# the single most likely first result of this script on a half-up stack.
api_or_die() {
  out="$(api "$@" || true)"
  # curl still emits the write-out line when the connection never happened, and
  # its %{http_code} is then `000`. Both shapes mean "no HTTP round trip".
  case "${out:-}" in
    "" | *000) fail "could not reach the server at $BASE (no HTTP response)." \
      "In docker mode the stack may still be starting: docker compose -f $COMPOSE_FILE ps" \
      "In direct mode, check BRAINIAC_API_URL and that \`brainiac serve\` is running." ;;
  esac
  printf '%s' "$out"
}

http_code() { printf '%s' "$1" | tail -n1; }
http_body() { printf '%s' "$1" | sed '$d'; }

fail() {
  echo "x $1" >&2
  shift
  if [ $# -gt 0 ]; then printf '%s\n' "$@" >&2; fi
  exit 1
}

# ── 1. write ────────────────────────────────────────────────────────────
echo "-> 1/3 writing a marked memory (POST /v1/memories) ..."
resp="$(api_or_die POST /v1/memories "{\"content\":\"$STATEMENT\",\"kind\":\"fact\"}")"
case "$(http_code "$resp")" in
  202) ;;
  401) fail "401 — the token is not a key of BRAINIAC_TOKENS and not a live brk_ key." ;;
  403) fail "403 — the token lacks the \`write\` scope. Mint one with SCOPES=read,write." ;;
  # A teamless principal is refused HERE, at the boundary, rather than writing a
  # memory nobody can ever read. That refusal is the server telling you the same
  # thing this script used to have to infer three steps later (see the RLS-count
  # branch in step 3) — so name it here, with the shipped config it describes.
  400)
    case "$(http_body "$resp")" in
      *"no team"*)
        fail "the write was REFUSED: this key's principal belongs to no team." \
          "" \
          "$(http_body "$resp")" \
          "" \
          "The shipped example config has exactly this shape:" \
          "" \
          "  BRAINIAC_TOKENS={\"secret\":{\"org\":\"…\",\"user\":\"…\",\"teams\":[]}}" \
          "                                                ^^^^^^^^^^^^" \
          "" \
          "Fix: verify with a key whose principal belongs to a team — put a real" \
          "team uuid in the token's \`teams\` array AND make sure the user is a row" \
          "in team_members for it (RLS reads the table, not the token)." ;;
      *) fail "400 from POST /v1/memories:" "$(http_body "$resp")" ;;
    esac ;;
  *)   fail "unexpected response from POST /v1/memories:" "$(http_body "$resp")" ;;
esac
SOURCE_ID="$(http_body "$resp" | sed -n 's/.*"source_id":"\([0-9a-f-]*\)".*/\1/p')"
[ -n "$SOURCE_ID" ] || fail "could not read source_id from the receipt:" "$(http_body "$resp")"
echo "   source $SOURCE_ID accepted"

# ── 2. wait for the pipeline ────────────────────────────────────────────
# This is the step a page-render check can never do: it proves the queue is
# being drained by a live worker and that extraction + embedding both ran.
echo "-> 2/3 waiting for the pipeline (GET /v1/sources/$SOURCE_ID, up to ${TIMEOUT_SECS}s) ..."
deadline=$(( $(date +%s) + TIMEOUT_SECS ))
status=""
while [ "$(date +%s)" -lt "$deadline" ]; do
  # Deliberately NOT api_or_die: this is a poll loop, and a single blip (a
  # container-runtime hiccup, the server restarting) must cost one iteration,
  # not the whole check. The deadline below is what actually fails.
  s="$(api GET "/v1/sources/$SOURCE_ID" || true)"
  case "${s:-}" in "" | *000) sleep 3; continue ;; esac
  [ "$(http_code "$s")" = "200" ] || fail "unexpected response from GET /v1/sources:" "$(http_body "$s")"
  status="$(http_body "$s" | sed -n 's/.*"status":"\([a-z]*\)".*/\1/p')"
  case "$status" in
    processed) break ;;
    failed) fail "the pipeline job FAILED for this source. Check the worker logs:" \
                 "  docker compose -f $COMPOSE_FILE logs $SERVICE | tail -50" ;;
  esac
  sleep 3
done
[ "$status" = "processed" ] || fail "still '$status' after ${TIMEOUT_SECS}s — the worker is not draining the queue." \
  "Check GET /v1/queue/health and the server logs."
# How many memories the pipeline produced, AS SEEN BY THIS PRINCIPAL. Both
# halves matter: the count is RLS-scoped, so `0` here after a successful
# `processed` almost always means "written but invisible to you", which is a
# very different (and far more confusing) failure than "nothing was extracted".
PRODUCED="$(http_body "$s" | sed -n 's/.*"memories":\([0-9]*\).*/\1/p')"
echo "   processed (${PRODUCED:-?} memories visible to this key)"

# ── 3. retrieve it back ─────────────────────────────────────────────────
echo "-> 3/3 retrieving it back (POST /v1/memories/search) ..."
r="$(api_or_die POST /v1/memories/search "{\"query\":\"$MARKER\",\"k\":10}")"
case "$(http_code "$r")" in
  200) ;;
  403) fail "403 — the token lacks the \`read\` scope." ;;
  *)   fail "unexpected response from POST /v1/memories/search:" "$(http_body "$r")" ;;
esac
if ! http_body "$r" | grep -qF "$MARKER"; then
  # Two very different diagnoses, told apart by the RLS-scoped count above.
  if [ "${PRODUCED:-0}" = "0" ]; then
    fail "the source processed, but this key can see NO memory from it." \
      "" \
      "The usual cause is a TEAMLESS principal, and it is worth reading twice" \
      "because the shipped example config has exactly this shape:" \
      "" \
      "  BRAINIAC_TOKENS={\"secret\":{\"org\":\"…\",\"user\":\"…\",\"teams\":[]}}" \
      "                                                ^^^^^^^^^^^^" \
      "A memory defaults to visibility='team' and takes its team from the" \
      "writer's FIRST team. With no teams that column is NULL, and the RLS" \
      "policy (migrations/0001_init.sql, memories_read) matches team-visible" \
      "rows by \`team_id IN (…)\` — NULL matches nothing, for anyone, forever." \
      "The write succeeds, the pipeline succeeds, and the memory is unreadable." \
      "" \
      "Fix: verify with a key whose principal belongs to a team — put a real" \
      "team uuid in the token's \`teams\` array AND make sure the user is a row" \
      "in team_members for it (RLS reads the table, not the token)." \
      "" \
      "If the principal DOES have a team, check the worker logs instead: a" \
      "chunk dropped by the secret firewall or collapsed by dedup also lands here."
  fi
  fail "the memory was written, processed and is visible ($PRODUCED), but does NOT" \
    "come back from search. Retrieval is the product; treat this as a broken install." \
    "Most likely the embedder: check BRAINIAC_EMBEDDER and its provider key," \
    "or set BRAINIAC_EMBEDDER=deterministic to rule the provider out."
fi

cat <<EOF

ok Retrieval works end to end — write -> pipeline -> search returned the marked memory.
   Marker: $MARKER
   It is a real memory in this org. Find it by that marker in the console's
   archive module and deprecate it if you would rather it were not there.
EOF

#!/usr/bin/env bash
#
# Brainiac — smoke check for a running stack.
#
# Written for the TEST stack (docker-compose.test.yml → server on 127.0.0.1:8601,
# console on 127.0.0.1:3200), but it is just HTTP: point it at any stack.
#
#   cp .env.test.example .env.test && edit
#   docker compose -f docker-compose.test.yml --env-file .env.test up -d --build
#   ./scripts/smoke-test-env.sh
#
#   API_URL=http://127.0.0.1:8600 CONSOLE_URL=http://127.0.0.1:3100 \
#     ./scripts/smoke-test-env.sh                      # some other stack
#
# What it proves, against endpoints that actually exist in the source:
#   1. GET /health                  → 200 {"status":"ok"}   (http.rs::health,
#                                      unauthenticated liveness probe)
#   2. GET /v1/queue/health         → 401 without a token   (auth is on)
#   3. GET /v1/queue/health         → 200 with the admin token; the body carries
#                                      `queue`/`ready`/`in_flight`, which means
#                                      the server reached Postgres, migrations
#                                      ran and the job queue is readable
#                                      (http.rs::queue_health, `admin` scope)
#   4. GET /openapi.json            → 200 (the OpenAPI document the console's
#                                      generated types come from)
#   5. GET <console>/               → 200 (Next runtime is serving the public
#                                      pitch; middleware.ts allows "/")
#   6. GET <console>/api/status     → 401 (the console gate fails CLOSED for
#                                      /api/* without a session — middleware.ts)
#
# Env:
#   API_URL       default http://127.0.0.1:8601   (docker-compose.test.yml)
#   CONSOLE_URL   default http://127.0.0.1:3200
#   BRAINIAC_API_TOKEN   admin-scope bearer; read from $ENV_FILE when unset
#   ENV_FILE      env file to source for the token (default ./.env.test)
#   SMOKE_RETRIES seconds to wait for /health before failing (default 60)
#
# Exit 0 = every check passed. Non-zero = at least one failed (the count).
# No credential value is printed: the token is only ever sent as a header.

set -euo pipefail

cd "$(dirname "$0")/.."

API_URL="${API_URL:-http://127.0.0.1:8601}"
CONSOLE_URL="${CONSOLE_URL:-http://127.0.0.1:3200}"
ENV_FILE="${ENV_FILE:-./.env.test}"
SMOKE_RETRIES="${SMOKE_RETRIES:-60}"

# The token normally lives in the same env file compose was started with.
if [ -z "${BRAINIAC_API_TOKEN:-}" ] && [ -f "$ENV_FILE" ]; then
  set -a
  # shellcheck disable=SC1090  # operator-provided path; contents are env vars
  . "$ENV_FILE"
  set +a
fi

command -v curl >/dev/null 2>&1 || { echo "x curl is not on PATH." >&2; exit 2; }

fails=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1" >&2; fails=$((fails + 1)); }

# code <url> [header...] → prints the HTTP status only.
code() {
  local url="$1"; shift
  curl -s -o /dev/null -w '%{http_code}' --max-time 10 "$@" "$url" || echo "000"
}
# body <url> [header...] → prints the response body (empty on transport error).
body() {
  local url="$1"; shift
  curl -s --max-time 10 "$@" "$url" || true
}

echo "-> Waiting for $API_URL/health (<= ${SMOKE_RETRIES}s) ..."
waited=0
until [ "$(code "$API_URL/health")" = "200" ]; do
  sleep 1
  waited=$((waited + 1))
  if [ "$waited" -ge "$SMOKE_RETRIES" ]; then
    echo "x $API_URL/health never returned 200." >&2
    echo "  docker compose -f docker-compose.test.yml --env-file $ENV_FILE logs --tail 50 server" >&2
    exit 1
  fi
done

echo "-> Smoke checks:"

# 1. Liveness. The handler returns exactly {"status":"ok"} — assert the body,
#    not just the code, so a reverse proxy answering 200 cannot fake it.
h="$(body "$API_URL/health")"
case "$h" in
  *'"status"'*'"ok"'*) pass "GET /health → 200 $h" ;;
  *) fail "GET /health body is not {\"status\":\"ok\"}: $h" ;;
esac

# 2. Auth is actually on. queue_health requires the `admin` scope; an
#    unauthenticated 200 here would mean the whole token layer is bypassed.
c="$(code "$API_URL/v1/queue/health")"
if [ "$c" = "401" ]; then
  pass "GET /v1/queue/health (no token) → 401"
else
  fail "GET /v1/queue/health (no token) → $c, expected 401 — auth may be off"
fi

# 3. The data plane. A 200 here means: token resolved, Postgres reachable,
#    migrations applied, queue.jobs readable.
if [ -z "${BRAINIAC_API_TOKEN:-}" ]; then
  fail "BRAINIAC_API_TOKEN is unset and $ENV_FILE has none — cannot check the data plane"
else
  auth=(-H "Authorization: Bearer $BRAINIAC_API_TOKEN")
  c="$(code "$API_URL/v1/queue/health" "${auth[@]}")"
  if [ "$c" != "200" ]; then
    fail "GET /v1/queue/health (admin token) → $c (403 = token lacks the admin scope)"
  else
    qb="$(body "$API_URL/v1/queue/health" "${auth[@]}")"
    case "$qb" in
      *'"queue"'*'"ready"'*'"in_flight"'*)
        pass "GET /v1/queue/health → 200 $qb" ;;
      *)
        fail "GET /v1/queue/health returned 200 but an unexpected body: $qb" ;;
    esac
  fi
fi

# 4. OpenAPI document — the contract the console's generated types are built
#    from (npm run gen:api). Cheap, and catches a half-initialized router.
c="$(code "$API_URL/openapi.json")"
[ "$c" = "200" ] && pass "GET /openapi.json → 200" || fail "GET /openapi.json → $c"

# 5. Console runtime, public surface.
c="$(code "$CONSOLE_URL/")"
[ "$c" = "200" ] && pass "GET $CONSOLE_URL/ → 200" \
  || fail "GET $CONSOLE_URL/ → $c (console container up? \`docker compose -f docker-compose.test.yml logs console\`)"

# 6. Console gate fails closed. /api/* returns 401 JSON rather than an HTML
#    login page, by design (middleware.ts) — so this is the check that the test
#    host is running the same fail-closed gate production does.
c="$(code "$CONSOLE_URL/api/status")"
if [ "$c" = "401" ]; then
  pass "GET $CONSOLE_URL/api/status → 401 (gate fails closed)"
elif [ "$c" = "200" ]; then
  # console/Dockerfile pins NODE_ENV=production, so auth.ts::isUnlockedByDefault()
  # is false in the container and a 200 here means the console is NOT the image
  # this stack is supposed to run (e.g. a `next dev` process on the same port).
  fail "GET $CONSOLE_URL/api/status → 200 without a session — the gate is open; is this really the containerised console?"
else
  fail "GET $CONSOLE_URL/api/status → $c, expected 401"
fi

echo
if [ "$fails" -gt 0 ]; then
  echo "x SMOKE FAILED — $fails check(s) did not pass" >&2
  exit 1
fi
echo "ok SMOKE PASSED — server, data plane and console are healthy at $API_URL / $CONSOLE_URL"

#!/usr/bin/env bash
#
# Brainiac — restore drill. Proves a dump is actually restorable, without going
# anywhere near a live database.
#
# Spins a throwaway pgvector container, restores the newest (or a named) dump
# into it, asserts the result is a real Brainiac schema with real rows, then
# destroys the container — including on failure. Exits non-zero on ANY failed
# assertion, so it is safe to run from cron or CI:
#
#   scripts/restore-drill.sh                     # newest dump in ./backups
#   scripts/restore-drill.sh backups/brainiac-20260726-031500.dump
#
# An untested backup is a rumour. Run this monthly, and always before you rely
# on a dump for a migration rollback. See docs/deploy/backups.md.
#
# TWO layers keep this away from live data, because "the drill wiped prod" is
# the one failure this script must never have:
#   1. It restores inside a container it creates itself (`docker run`, no ports,
#      its own ephemeral volume) — it never runs `docker compose exec`, so it
#      cannot reach the deploy or dev stack's Postgres at all.
#   2. Inside that container it restores into a scratch database whose name is
#      NOT the production one, and it hard-fails if the two ever match.
#
# Env overrides (all optional):
#   BACKUP_DIR         where dumps live        (default ./backups)
#   PG_IMAGE           scratch image           (default pgvector/pgvector:pg17,
#                                               same tag as docker-compose.yml)
#   PG_USER            role to restore as      (default brainiac)
#   PG_DB              the PRODUCTION database name this drill must never touch
#                      (default brainiac) — used only as the forbidden value
#   DRILL_DB           scratch database name   (default brainiac_drill; must
#                                               differ from PG_DB)
#   DRILL_REQUIRE_ROWS comma-separated tables that must be NON-EMPTY
#                      (default "orgs,memories"; set to "" for a brand-new host
#                      whose corpus is legitimately still empty)
#   DRILL_TIMEOUT      seconds to wait for the scratch DB (default 60)

set -euo pipefail

cd "$(dirname "$0")/.."

BACKUP_DIR="${BACKUP_DIR:-./backups}"
PG_IMAGE="${PG_IMAGE:-pgvector/pgvector:pg17}"
PG_USER="${PG_USER:-brainiac}"
PG_DB="${PG_DB:-brainiac}"
DRILL_DB="${DRILL_DB:-brainiac_drill}"
DRILL_REQUIRE_ROWS="${DRILL_REQUIRE_ROWS-orgs,memories}"
DRILL_TIMEOUT="${DRILL_TIMEOUT:-60}"

# ── layer 2: refuse to name the scratch DB after the production one ─────────
# Even though the container is disposable, an operator who overrides DRILL_DB to
# "brainiac" is telling us they have confused this with scripts/restore.sh. Stop
# rather than teach the habit that a drill and a real restore look the same.
if [ "$DRILL_DB" = "$PG_DB" ]; then
  echo "x REFUSING: scratch database '$DRILL_DB' is the production database name." >&2
  echo "  This script only ever restores into a throwaway database. To restore" >&2
  echo "  for real, use:  scripts/restore.sh <dump>" >&2
  exit 2
fi
# `postgres` and `template*` are the cluster's own databases — restoring a
# schema into them would work and would be a terrible idea.
case "$DRILL_DB" in
  postgres|template0|template1)
    echo "x REFUSING: '$DRILL_DB' is a cluster-managed database, not a scratch one." >&2
    exit 2
    ;;
esac

DUMP="${1:-}"
if [ -z "$DUMP" ]; then
  DUMP="$(ls -1t "$BACKUP_DIR"/brainiac-*.dump 2>/dev/null | head -1 || true)"
  [ -n "$DUMP" ] || {
    echo "x No dumps in $BACKUP_DIR — run scripts/backup.sh first." >&2
    exit 2
  }
  echo "-> Newest dump: $DUMP"
fi
[ -f "$DUMP" ] || { echo "x No such dump file: $DUMP" >&2; exit 2; }
command -v docker >/dev/null 2>&1 || { echo "x docker is not on PATH." >&2; exit 2; }

CID_NAME="brainiac-restore-drill-$$"
RESTORE_LOG="/tmp/drill-restore.$$"
cleanup() {
  # Always tear the scratch container down — success, assertion failure, or ^C.
  docker rm -f "$CID_NAME" >/dev/null 2>&1 || true
  rm -f "$RESTORE_LOG"
}
trap cleanup EXIT INT TERM

# Ephemeral superuser password; the container publishes no ports, so this never
# leaves the Docker network. Falls back to a timestamp if openssl is absent.
scratch_pw="$(openssl rand -hex 16 2>/dev/null || date +%s%N)"

echo "-> Starting scratch Postgres ($PG_IMAGE) as $CID_NAME ..."
# POSTGRES_DB=postgres: the image only bootstraps the maintenance database, and
# the scratch database is created explicitly below — so the name that receives
# the restore is one this script chose and guarded, never an inherited default.
# No `-p` publish and no named volume: the container is unreachable from outside
# the Docker host and everything in it dies with `docker rm -f`.
docker run -d --name "$CID_NAME" \
  -e POSTGRES_USER="$PG_USER" \
  -e POSTGRES_PASSWORD="$scratch_pw" \
  -e POSTGRES_DB=postgres \
  "$PG_IMAGE" >/dev/null

echo "-> Waiting for it to accept connections (<= ${DRILL_TIMEOUT}s) ..."
waited=0
until docker exec "$CID_NAME" pg_isready -U "$PG_USER" -d postgres >/dev/null 2>&1; do
  sleep 1
  waited=$((waited + 1))
  if [ "$waited" -ge "$DRILL_TIMEOUT" ]; then
    echo "x Scratch Postgres never became ready. Container logs:" >&2
    docker logs --tail 30 "$CID_NAME" >&2 || true
    exit 1
  fi
done

# Admin queries go to the maintenance DB; assertions go to the scratch DB.
adm() { docker exec -i "$CID_NAME" psql -U "$PG_USER" -d postgres -At -q -c "$1"; }
q()   { docker exec -i "$CID_NAME" psql -U "$PG_USER" -d "$DRILL_DB" -At -q -c "$1"; }

echo "-> Creating throwaway database '$DRILL_DB' (production DB '$PG_DB' is untouched) ..."
adm "CREATE DATABASE \"$DRILL_DB\"" >/dev/null

# The dump GRANTs to brainiac_app (migration 0001 creates that role). pg_dump —
# unlike pg_dumpall — does not carry roles, so pre-create it or every GRANT in
# the archive errors out. Roles are cluster-wide, hence the maintenance DB.
adm "DO \$\$ BEGIN IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'brainiac_app') THEN CREATE ROLE brainiac_app NOLOGIN; END IF; END \$\$;" >/dev/null

echo "-> Restoring $DUMP into '$DRILL_DB' ..."
docker cp "$DUMP" "$CID_NAME:/tmp/drill.dump" >/dev/null
restore_rc=0
docker exec -i "$CID_NAME" \
  pg_restore -U "$PG_USER" -d "$DRILL_DB" --no-owner /tmp/drill.dump >"$RESTORE_LOG" 2>&1 || restore_rc=$?
if [ "$restore_rc" -ne 0 ]; then
  # pg_restore returns 1 for non-fatal warnings too; the assertions below are
  # the real verdict, so surface the noise and keep going.
  echo "! pg_restore exited $restore_rc — last lines:" >&2
  tail -20 "$RESTORE_LOG" >&2 || true
fi
rm -f "$RESTORE_LOG"

# ── assertions ──────────────────────────────────────────────────────────────
fails=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1" >&2; fails=$((fails + 1)); }

echo "-> Asserting the restore is real:"

# 1. sqlx's migration ledger — the single best proof this is a migrated
#    Brainiac database and not an empty shell.
applied="$(q "SELECT count(*) FROM _sqlx_migrations WHERE success" 2>/dev/null || echo "")"
# Guarded: `set -o pipefail` would abort the whole drill if `migrations/` were
# absent (running from a partial checkout), turning a missing directory into a
# fake restore failure. An unknown expectation is reported, not fatal.
expected="$(find migrations -maxdepth 1 -name '*.sql' 2>/dev/null | wc -l | tr -d ' ' || echo 0)"
if [ -z "$applied" ]; then
  fail "_sqlx_migrations table is missing — this is not a Brainiac dump"
elif [ "$applied" -eq 0 ]; then
  fail "_sqlx_migrations has 0 successful rows"
elif [ "$expected" -eq 0 ]; then
  # No migrations/ dir to compare against (partial checkout) — the ledger is
  # still proof of a migrated database, we just cannot say "matches".
  pass "_sqlx_migrations: $applied applied migrations (no migrations/ here to compare)"
elif [ "$applied" -ne "$expected" ]; then
  # Not fatal: a dump legitimately predates migrations added since it was taken.
  pass "_sqlx_migrations: $applied applied migrations (this checkout has $expected — older dump?)"
else
  pass "_sqlx_migrations: $applied applied migrations (matches this checkout)"
fi

# 2. Core tables of every plane: identity, corpus, retrieval, auth, job queue,
#    documents, projects.
for t in orgs users teams memories sources provenance memory_embeddings \
         api_tokens documents projects queue.jobs; do
  if [ "$(q "SELECT to_regclass('$t') IS NOT NULL" 2>/dev/null)" = "t" ]; then
    pass "table $t exists"
  else
    fail "table $t is MISSING"
  fi
done

# 3. pgvector — without the extension the embeddings columns cannot have
#    restored, and retrieval would be dead on arrival.
if [ "$(q "SELECT count(*) FROM pg_extension WHERE extname = 'vector'" 2>/dev/null)" = "1" ]; then
  pass "extension vector installed"
else
  fail "extension vector is MISSING"
fi

# 4. RLS is load-bearing in this schema (CLAUDE.md § Design standards) — a
#    restore that lost the policies would silently serve cross-tenant rows.
for t in memories sources provenance; do
  if [ "$(q "SELECT relrowsecurity FROM pg_class WHERE oid = '$t'::regclass" 2>/dev/null)" = "t" ]; then
    pass "row-level security enabled on $t"
  else
    fail "row-level security NOT enabled on $t"
  fi
done
policies="$(q "SELECT count(*) FROM pg_policies WHERE schemaname = 'public'" 2>/dev/null || echo "")"
if [ -n "$policies" ] && [ "$policies" -gt 0 ]; then
  pass "$policies RLS policies restored"
else
  fail "no RLS policies in the restored schema"
fi

# 5. Actual data. A structurally perfect restore of zero rows is still a lost
#    corpus, so key tables must be non-empty (override with DRILL_REQUIRE_ROWS).
if [ -n "$DRILL_REQUIRE_ROWS" ]; then
  old_ifs="$IFS"; IFS=','
  for t in $DRILL_REQUIRE_ROWS; do
    IFS="$old_ifs"
    n="$(q "SELECT count(*) FROM $t" 2>/dev/null || echo "")"
    if [ -z "$n" ]; then
      fail "row count on $t could not be read"
    elif [ "$n" -gt 0 ]; then
      pass "$t has $n rows"
    else
      fail "$t is EMPTY (if this host's corpus really is empty, run with DRILL_REQUIRE_ROWS=\"\")"
    fi
    IFS=','
  done
  IFS="$old_ifs"
fi

echo
if [ "$fails" -gt 0 ]; then
  echo "x DRILL FAILED — $fails assertion(s) did not hold for $DUMP" >&2
  echo "  (scratch container $CID_NAME is being destroyed; nothing live was touched)" >&2
  exit 1
fi
echo "ok DRILL PASSED — $DUMP restores into a complete, populated Brainiac schema."
echo "   scratch db '$DRILL_DB' in container $CID_NAME — destroyed on exit."

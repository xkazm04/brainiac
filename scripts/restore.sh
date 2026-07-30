#!/usr/bin/env bash
#
# Brainiac — restore a pg_dump archive produced by scripts/backup.sh.
#
# Feeds the custom-format archive to pg_restore inside the pgvector container.
# DESTRUCTIVE: --clean --if-exists DROPs and recreates the dumped objects in the
# target database, replacing existing data. Point it at the right stack.
#
#   scripts/restore.sh backups/brainiac-YYYYmmdd-HHMMSS.dump
#   COMPOSE_FILE=docker-compose.yml scripts/restore.sh <dump>   # local dev stack
#   scripts/restore.sh <dump> --yes                             # skip the prompt
#   scripts/restore.sh --latest                                 # newest dump in ./backups
#
# To verify a dump WITHOUT touching a live database, use scripts/restore-drill.sh
# — it restores into a throwaway container and asserts the result. Full operator
# guide: docs/deploy/backups.md.
#
# For a clean drill, stop the API first so nothing holds locks mid-restore:
#   docker compose -f docker-compose.deploy.yml stop server
#   scripts/restore.sh <dump> --yes
#   docker compose -f docker-compose.deploy.yml start server
#
# Env overrides: COMPOSE_FILE, PG_SERVICE, PG_USER, PG_DB, POSTGRES_PASSWORD
# (same defaults as backup.sh).

set -euo pipefail

COMPOSE_FILE="${COMPOSE_FILE:-docker-compose.deploy.yml}"
PG_SERVICE="${PG_SERVICE:-postgres}"
PG_USER="${PG_USER:-brainiac}"
PG_DB="${PG_DB:-brainiac}"

ASSUME_YES=0
LATEST=0
DUMP=""
for arg in "$@"; do
  case "$arg" in
    --yes|-y)  ASSUME_YES=1 ;;
    --latest)  LATEST=1 ;;
    *)         DUMP="$arg" ;;
  esac
done

cd "$(dirname "$0")/.."

# --latest resolves the newest dump in ./backups (what you almost always want
# in a real recovery, and what restore-drill.sh verifies).
if [ "$LATEST" -eq 1 ] && [ -z "$DUMP" ]; then
  DUMP="$(ls -1t ./backups/brainiac-*.dump 2>/dev/null | head -1 || true)"
  [ -n "$DUMP" ] || { echo "x --latest: no dumps in ./backups" >&2; exit 1; }
  echo "-> --latest resolved to $DUMP"
fi

if [ -z "$DUMP" ]; then
  echo "usage: scripts/restore.sh <dump-file>|--latest [--yes]" >&2
  echo "recent dumps:" >&2
  ls -1t ./backups/brainiac-*.dump 2>/dev/null | head -5 >&2 || true
  exit 2
fi
if [ ! -f "$DUMP" ]; then
  echo "x No such dump file: $DUMP" >&2
  exit 1
fi

echo "About to RESTORE '$DUMP'"
echo "  into database '$PG_DB' (service '$PG_SERVICE', $COMPOSE_FILE)."
echo "  This DROPs and recreates the dumped objects (--clean --if-exists);"
echo "  existing data is replaced."
if [ "$ASSUME_YES" -ne 1 ]; then
  printf "Type 'restore' to proceed: "
  read -r reply
  [ "$reply" = "restore" ] || { echo "Aborted."; exit 1; }
fi

exec_args=(-T)
if [ -n "${POSTGRES_PASSWORD:-}" ]; then
  exec_args=(-e "PGPASSWORD=$POSTGRES_PASSWORD" -T)
fi

echo "-> Restoring ... (harmless 'does not exist, skipping' notices on a fresh DB are expected)"
# pg_restore reads the custom-format archive from stdin (-T keeps the pipe clean).
docker compose -f "$COMPOSE_FILE" exec "${exec_args[@]}" "$PG_SERVICE" \
  pg_restore -U "$PG_USER" -d "$PG_DB" --clean --if-exists --no-owner < "$DUMP"

echo "ok Restore complete. Restart the API to reconnect cleanly:"
echo "    docker compose -f $COMPOSE_FILE restart server"

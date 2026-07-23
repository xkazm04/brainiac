#!/usr/bin/env bash
#
# Brainiac — Postgres logical backup.
#
# Runs pg_dump inside the pgvector container defined by a docker-compose file
# and writes a compressed custom-format archive to ./backups/. Works unchanged
# against BOTH stacks (same service name / user / db in each):
#
#   scripts/backup.sh                                   # deploy stack (default)
#   COMPOSE_FILE=docker-compose.yml scripts/backup.sh   # local dev stack
#
# The archive is pg_restore-ready — recover it with scripts/restore.sh.
#
# Env overrides (all optional):
#   COMPOSE_FILE     compose file to target   (default docker-compose.deploy.yml)
#   PG_SERVICE       compose service name     (default postgres)
#   PG_USER          postgres role            (default brainiac)
#   PG_DB            database name            (default brainiac)
#   BACKUP_DIR       where dumps land         (default ./backups)
#   RETENTION_DAYS   prune dumps older than N (default 14; 0 = keep all)
#   POSTGRES_PASSWORD  passed through as PGPASSWORD if set (socket auth is
#                      trust inside the container, so this is belt-and-braces)

set -euo pipefail

COMPOSE_FILE="${COMPOSE_FILE:-docker-compose.deploy.yml}"
PG_SERVICE="${PG_SERVICE:-postgres}"
PG_USER="${PG_USER:-brainiac}"
PG_DB="${PG_DB:-brainiac}"
BACKUP_DIR="${BACKUP_DIR:-./backups}"
RETENTION_DAYS="${RETENTION_DAYS:-14}"

# Resolve relative paths from the repo root, so the script is cron-safe from
# any working directory.
cd "$(dirname "$0")/.."

mkdir -p "$BACKUP_DIR"
# Keep dumps out of git without touching the repo's root .gitignore.
if [ ! -f "$BACKUP_DIR/.gitignore" ]; then
  printf '*\n!.gitignore\n' > "$BACKUP_DIR/.gitignore"
fi

stamp="$(date +%Y%m%d-%H%M%S)"
out="$BACKUP_DIR/brainiac-$stamp.dump"

exec_args=(-T)
if [ -n "${POSTGRES_PASSWORD:-}" ]; then
  exec_args=(-e "PGPASSWORD=$POSTGRES_PASSWORD" -T)
fi

echo "-> Dumping '$PG_DB' from service '$PG_SERVICE' ($COMPOSE_FILE) ..."
# -Fc = compressed custom archive → restore with pg_restore (scripts/restore.sh).
docker compose -f "$COMPOSE_FILE" exec "${exec_args[@]}" "$PG_SERVICE" \
  pg_dump -U "$PG_USER" -d "$PG_DB" -Fc > "$out"

size="$(wc -c < "$out" | tr -d ' ')"
if [ "$size" -lt 1000 ]; then
  echo "x Dump is suspiciously small ($size bytes) — is the container up and the DB name right?" >&2
  rm -f "$out"
  exit 1
fi
echo "ok Wrote $out ($size bytes)"

# Retention prune (skip when RETENTION_DAYS=0).
if [ "$RETENTION_DAYS" -gt 0 ]; then
  find "$BACKUP_DIR" -maxdepth 1 -name 'brainiac-*.dump' -mtime +"$RETENTION_DAYS" -print -delete
fi

echo "Restore with: scripts/restore.sh $out"

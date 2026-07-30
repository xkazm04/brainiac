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
#   BACKUP_ENV_FILE  extra env file to source first (default ./.env.backup, and
#                    only if it exists) — cron has almost no environment, so
#                    this is where the offsite settings below live on a host
#
# OPTIONAL offsite push (docs/deploy/backups.md). A dump that only ever lives on
# the box it was taken from does not survive that box dying. Set BACKUP_S3_BUCKET
# and the archive is handed to scripts/backup-offsite.sh once it is written and
# verified, which copies it to any S3-compatible store (AWS S3, Alibaba OSS,
# Cloudflare R2, MinIO). Leave BACKUP_S3_BUCKET unset and that helper is a silent
# no-op — this script behaves exactly as it did before offsite existed.
#
# The BACKUP_S3_* variable names, credential handling and exit codes are
# documented in scripts/backup-offsite.sh; no credential value is ever stored
# here, prompted for, or printed.

set -euo pipefail

# Resolve relative paths from the repo root, so the script is cron-safe from
# any working directory.
cd "$(dirname "$0")/.."

# Optional env file (cron runs with a near-empty environment). Values in the
# file WIN over the ambient environment — it is the host's backup config.
BACKUP_ENV_FILE="${BACKUP_ENV_FILE:-./.env.backup}"
if [ -f "$BACKUP_ENV_FILE" ]; then
  set -a
  # shellcheck disable=SC1090  # operator-provided path, contents are env vars
  . "$BACKUP_ENV_FILE"
  set +a
fi

COMPOSE_FILE="${COMPOSE_FILE:-docker-compose.deploy.yml}"
PG_SERVICE="${PG_SERVICE:-postgres}"
PG_USER="${PG_USER:-brainiac}"
PG_DB="${PG_DB:-brainiac}"
BACKUP_DIR="${BACKUP_DIR:-./backups}"
RETENTION_DAYS="${RETENTION_DAYS:-14}"

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

# ── OPTIONAL offsite push ───────────────────────────────────────────────────
# Deliberately runs AFTER the local dump is written and sanity checked, so a
# broken offsite config can never cost you the local copy — it only makes this
# script exit non-zero, which is how cron mails you about it.
#
# backup-offsite.sh exits 0 immediately when BACKUP_S3_BUCKET is unset, so the
# default path prints nothing and costs nothing.
if [ -x ./scripts/backup-offsite.sh ]; then
  ./scripts/backup-offsite.sh "$out"
elif [ -f ./scripts/backup-offsite.sh ]; then
  # A checkout that lost the +x bit (Windows, a zip download) must not silently
  # skip the offsite copy the operator configured.
  bash ./scripts/backup-offsite.sh "$out"
elif [ -n "${BACKUP_S3_BUCKET:-}" ]; then
  echo "x BACKUP_S3_BUCKET is set but scripts/backup-offsite.sh is missing." >&2
  echo "  The local dump $out is fine; only the offsite copy was skipped." >&2
  exit 3
fi

echo "Restore with: scripts/restore.sh $out"
echo "Prove it:     scripts/restore-drill.sh $out"

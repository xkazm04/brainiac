#!/usr/bin/env bash
#
# Brainiac — push one backup archive to an S3-compatible object store.
#
# A dump that only ever lives on the box it was taken from does not survive that
# box dying. scripts/backup.sh calls this helper right after it writes and sanity
# checks a dump; you can also call it by hand to ship an older archive:
#
#   scripts/backup-offsite.sh backups/brainiac-20260726-031500.dump
#   scripts/backup-offsite.sh --latest          # newest dump in $BACKUP_DIR
#   scripts/backup-offsite.sh --check           # config only; uploads nothing
#
# OPT-IN AND SILENT BY DEFAULT: with BACKUP_S3_BUCKET unset this exits 0 having
# done nothing, so every existing backup workflow is unchanged until an operator
# deliberately turns it on.
#
# Env (names only — this script never contains, prompts for, or prints a
# credential VALUE; it reads them from the environment and exports them only
# into the `aws` subshell, never onto a command line, since argv is world
# readable via `ps` and the environment is not):
#
#   BACKUP_S3_BUCKET            bucket, optionally with a key prefix
#                               ("my-bucket" or "my-bucket/brainiac/prod").
#                               UNSET ⇒ offsite disabled (the default).
#   BACKUP_S3_PREFIX            key prefix, appended to whatever BACKUP_S3_BUCKET
#                               already carries ("brainiac/prod"). Optional — the
#                               two spellings are equivalent, so pick one:
#                                 BACKUP_S3_BUCKET=my-bucket/brainiac/prod
#                                 BACKUP_S3_BUCKET=my-bucket BACKUP_S3_PREFIX=brainiac/prod
#                               Leading/trailing slashes are trimmed either way.
#   BACKUP_S3_ACCESS_KEY_ID     access key id      (required when bucket is set)
#   BACKUP_S3_SECRET_ACCESS_KEY secret access key  (required when bucket is set)
#   BACKUP_S3_ENDPOINT          endpoint URL for non-AWS stores, e.g.
#                               https://oss-ap-southeast-1.aliyuncs.com (Alibaba
#                               OSS), https://<acct>.r2.cloudflarestorage.com
#   BACKUP_S3_REGION            region (default us-east-1; OSS/R2/MinIO ignore it)
#   BACKUP_S3_STORAGE_CLASS     e.g. STANDARD_IA / GLACIER_IR (default: unset)
#   BACKUP_DIR                  where dumps live (default ./backups) — used by
#                               --latest only
#   AWS_CLI                     aws binary to use (default `aws` on PATH)
#   BACKUP_ENV_FILE             env file to source first (default ./.env.backup,
#                               only if it exists) — cron has almost no
#                               environment, so that file is where these live on
#                               a real host
#
# Why BACKUP_S3_* and not AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY: the deploy
# stack already uses those two for the Bedrock LLM provider
# (docker-compose.deploy.yml), and backup storage must not silently inherit —
# or be mistaken for — an inference credential.
#
# Exit codes:  0 uploaded (or offsite disabled)  ·  2 usage/missing file
#              3 offsite is configured but could not run (missing aws CLI,
#                missing credentials, or the upload itself failed)
#
# Remote retention is the bucket's lifecycle rule, NOT this script — see
# docs/deploy/backups.md § Offsite.

set -euo pipefail

# Resolve relative paths from the repo root so this is cron-safe from any cwd.
cd "$(dirname "$0")/.."

# Optional env file. Values in the file WIN over the ambient environment: it is
# the host's backup configuration, and cron jobs arrive with almost none.
BACKUP_ENV_FILE="${BACKUP_ENV_FILE:-./.env.backup}"
if [ -f "$BACKUP_ENV_FILE" ]; then
  set -a
  # shellcheck disable=SC1090  # operator-provided path; contents are env vars
  . "$BACKUP_ENV_FILE"
  set +a
fi

BACKUP_DIR="${BACKUP_DIR:-./backups}"

CHECK_ONLY=0
ARCHIVE=""
for arg in "$@"; do
  case "$arg" in
    --check)  CHECK_ONLY=1 ;;
    --latest) ARCHIVE="$(ls -1t "$BACKUP_DIR"/brainiac-*.dump 2>/dev/null | head -1 || true)" ;;
    -h|--help)
      echo "usage: scripts/backup-offsite.sh <dump-file>|--latest|--check"
      exit 0
      ;;
    -*)
      echo "x Unknown argument: $arg" >&2
      exit 2
      ;;
    *) ARCHIVE="$arg" ;;
  esac
done

# ── the no-op path ──────────────────────────────────────────────────────────
# Unset bucket ⇒ offsite is off. Say so only in --check mode; during a cron
# backup this must be completely silent, or the daily mail becomes noise the
# operator learns to ignore.
if [ -z "${BACKUP_S3_BUCKET:-}" ]; then
  if [ "$CHECK_ONLY" -eq 1 ]; then
    echo "offsite: DISABLED (BACKUP_S3_BUCKET is unset)"
    echo "  Enable it by setting BACKUP_S3_BUCKET + BACKUP_S3_ACCESS_KEY_ID +"
    echo "  BACKUP_S3_SECRET_ACCESS_KEY (+ BACKUP_S3_ENDPOINT for non-AWS stores)"
    echo "  in $BACKUP_ENV_FILE — see docs/deploy/backups.md § Offsite."
  fi
  exit 0
fi

# ── configured: from here on, problems are loud ─────────────────────────────
aws_bin="${AWS_CLI:-aws}"
if ! command -v "$aws_bin" >/dev/null 2>&1; then
  echo "x BACKUP_S3_BUCKET is set but '$aws_bin' is not installed." >&2
  echo "  Debian/Ubuntu: sudo apt-get install -y awscli (or the official AWS CLI installer)." >&2
  echo "  The local dump is unaffected; only the offsite copy was skipped." >&2
  exit 3
fi
if [ -z "${BACKUP_S3_ACCESS_KEY_ID:-}" ] || [ -z "${BACKUP_S3_SECRET_ACCESS_KEY:-}" ]; then
  echo "x BACKUP_S3_BUCKET is set but BACKUP_S3_ACCESS_KEY_ID / BACKUP_S3_SECRET_ACCESS_KEY are not." >&2
  echo "  Set both (in $BACKUP_ENV_FILE for cron) or unset BACKUP_S3_BUCKET to disable offsite." >&2
  exit 3
fi

# Trim a leading s3:// and any trailing / the operator may have typed, then
# rebuild the canonical destination.
dest_path="${BACKUP_S3_BUCKET#s3://}"
dest_path="${dest_path%/}"

# Optional separate prefix. Appended to whatever the bucket string already
# carries, so `BACKUP_S3_BUCKET=my-bucket/a` + `BACKUP_S3_PREFIX=b` → `my-bucket/a/b`
# and the two spellings of the same destination stay interchangeable. Slashes are
# normalised on both sides so "/brainiac/" and "brainiac" mean the same thing —
# a doubled slash would create a distinct, empty-named "directory" in S3.
if [ -n "${BACKUP_S3_PREFIX:-}" ]; then
  prefix="${BACKUP_S3_PREFIX#/}"
  prefix="${prefix%/}"
  [ -n "$prefix" ] && dest_path="$dest_path/$prefix"
fi

aws_args=(--only-show-errors)
[ -n "${BACKUP_S3_ENDPOINT:-}" ] && aws_args+=(--endpoint-url "$BACKUP_S3_ENDPOINT")
[ -n "${BACKUP_S3_STORAGE_CLASS:-}" ] && aws_args+=(--storage-class "$BACKUP_S3_STORAGE_CLASS")

# `aws` with the credentials in scope. Nothing else in this script may print
# them, and no caller ever sees them in argv.
run_aws() {
  (
    export AWS_ACCESS_KEY_ID="$BACKUP_S3_ACCESS_KEY_ID"
    export AWS_SECRET_ACCESS_KEY="$BACKUP_S3_SECRET_ACCESS_KEY"
    export AWS_DEFAULT_REGION="${BACKUP_S3_REGION:-us-east-1}"
    "$aws_bin" "$@"
  )
}

if [ "$CHECK_ONLY" -eq 1 ]; then
  echo "offsite: ENABLED"
  echo "  destination: s3://$dest_path/"
  echo "  endpoint:    ${BACKUP_S3_ENDPOINT:-<AWS default>}"
  echo "  region:      ${BACKUP_S3_REGION:-us-east-1}"
  echo "  credentials: BACKUP_S3_ACCESS_KEY_ID + BACKUP_S3_SECRET_ACCESS_KEY present"
  echo "-> Listing the destination to prove the key can reach it ..."
  # A ListObjects that returns nothing is still a successful auth round-trip;
  # only a non-zero exit means the credential or endpoint is wrong.
  if run_aws s3 ls "s3://$dest_path/" "${aws_args[@]:1}" >/dev/null 2>&1; then
    echo "ok Bucket is reachable with these credentials."
    exit 0
  fi
  echo "x Could not list s3://$dest_path/ — wrong endpoint/region, or the key" >&2
  echo "  lacks ListBucket. (PutObject-only keys fail this check but can still" >&2
  echo "  upload; try a real backup to be sure.)" >&2
  exit 3
fi

if [ -z "$ARCHIVE" ]; then
  echo "usage: scripts/backup-offsite.sh <dump-file>|--latest|--check" >&2
  exit 2
fi
if [ ! -f "$ARCHIVE" ]; then
  echo "x No such archive: $ARCHIVE" >&2
  exit 2
fi

dest="s3://$dest_path/$(basename "$ARCHIVE")"
echo "-> Pushing offsite to $dest ..."
if ! run_aws s3 cp "$ARCHIVE" "$dest" "${aws_args[@]}"; then
  echo "x Offsite push FAILED (the local archive $ARCHIVE is intact)." >&2
  echo "  Check BACKUP_S3_ENDPOINT / BACKUP_S3_REGION and that the key may" >&2
  echo "  PutObject on this bucket:  scripts/backup-offsite.sh --check" >&2
  exit 3
fi
echo "ok Offsite copy at $dest"
echo "   (remote retention is the bucket's lifecycle rule, not this script — docs/deploy/backups.md)"

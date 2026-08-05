#!/usr/bin/env bash
#
# Brainiac — install (or remove) the daily backup cron entry.
#
# This is the executable form of docs/deploy/alibaba.md § "Backup & restore":
# it writes exactly that crontab line for the CURRENT user, pointing at THIS
# checkout. Idempotent — re-running replaces the entry instead of stacking a
# second one, because every line it owns carries a unique trailing tag.
#
#   scripts/install-backup-cron.sh              # install / refresh
#   scripts/install-backup-cron.sh --dry-run    # print the plan, change nothing
#   scripts/install-backup-cron.sh --uninstall  # remove the entry
#
# Env overrides (all optional):
#   BACKUP_SCHEDULE  five-field cron schedule   (default "15 3 * * *" = 03:15 daily)
#   BACKUP_LOG       log file the entry appends (default /var/log/brainiac-backup.log)
#   CRONTAB          crontab binary             (default `crontab` on PATH)
#
# The entry runs `scripts/backup.sh`, which sources ./.env.backup when present —
# that is where POSTGRES_PASSWORD and the optional offsite (BACKUP_S3_*) settings
# belong, since cron gives the job almost no environment. See
# docs/deploy/backups.md.

set -euo pipefail

cd "$(dirname "$0")/.."
REPO="$PWD"

BACKUP_SCHEDULE="${BACKUP_SCHEDULE:-15 3 * * *}"
BACKUP_LOG="${BACKUP_LOG:-/var/log/brainiac-backup.log}"
CRONTAB="${CRONTAB:-crontab}"

MODE="install"
for arg in "$@"; do
  case "$arg" in
    --uninstall) MODE="uninstall" ;;
    --dry-run)   MODE="dry-run" ;;
    -h|--help)
      echo "usage: scripts/install-backup-cron.sh [--dry-run|--uninstall]"
      exit 0
      ;;
    *)
      echo "x Unknown argument: $arg" >&2
      echo "usage: scripts/install-backup-cron.sh [--dry-run|--uninstall]" >&2
      exit 2
      ;;
  esac
done

# Tag is repo-path-scoped, so two checkouts on one host get two independent
# entries and neither installer clobbers the other's.
TAG="# brainiac-backup:$REPO"
ENTRY="$BACKUP_SCHEDULE cd $REPO && ./scripts/backup.sh >> $BACKUP_LOG 2>&1 $TAG"

# ── refuse to pretend ───────────────────────────────────────────────────────
# No cron on the box means the entry would be written nowhere and the operator
# would believe backups are running. Fail loudly instead.
if ! command -v "$CRONTAB" >/dev/null 2>&1; then
  echo "x No '$CRONTAB' binary on PATH — this host has no user crontab." >&2
  echo "  Install one (Debian/Ubuntu: sudo apt-get install -y cron && sudo systemctl enable --now cron)," >&2
  echo "  or schedule '$REPO/scripts/backup.sh' with systemd timers / your platform's scheduler." >&2
  exit 1
fi
if ! command -v docker >/dev/null 2>&1; then
  echo "! docker is not on PATH for this shell; the cron job will fail unless it is" >&2
  echo "  on cron's PATH too (/usr/bin:/bin by default)." >&2
fi

# `crontab -l` exits 1 with "no crontab for <user>" when the table is empty —
# that is not an error here.
current="$($CRONTAB -l 2>/dev/null || true)"
# Everything except the lines this script owns.
without_ours="$(printf '%s\n' "$current" | grep -F -v -- "$TAG" || true)"

case "$MODE" in
  uninstall)
    if ! printf '%s\n' "$current" | grep -F -q -- "$TAG"; then
      echo "No brainiac backup entry in the crontab of '$(id -un)' — nothing to do."
      exit 0
    fi
    echo "Will REMOVE this crontab entry for user '$(id -un)':"
    printf '%s\n' "$current" | grep -F -- "$TAG" | sed 's/^/    /'
    # Removing the ONLY entry leaves nothing for grep to match; its exit 1 would
    # abort the script under `set -e -o pipefail` AFTER crontab had already
    # succeeded, reporting a failure that did not happen. Materialise the
    # remainder first, then feed it in.
    remaining="$(printf '%s\n' "$without_ours" | grep -v '^[[:space:]]*$' || true)"
    printf '%s' "${remaining:+$remaining
}" | $CRONTAB -
    n_remaining=0
    [ -n "$remaining" ] && n_remaining="$(printf '%s\n' "$remaining" | wc -l | tr -d ' ')"
    echo "ok Removed. Remaining crontab entries: $n_remaining"
    ;;
  install|dry-run)
    logdir="$(dirname "$BACKUP_LOG")"
    # cron runs `cmd >> log`; if the redirect cannot be opened the command never
    # runs at all, so an unwritable log path is fatal, not cosmetic.
    if [ ! -w "$logdir" ] && [ ! -w "$BACKUP_LOG" ]; then
      echo "x Log path '$BACKUP_LOG' is not writable by $(id -un)." >&2
      echo "  Fix it:   sudo install -o $(id -un) -m 0644 /dev/null $BACKUP_LOG" >&2
      echo "  Or pick another: BACKUP_LOG=$REPO/backups/backup.log scripts/install-backup-cron.sh" >&2
      exit 1
    fi
    if [ ! -x "$REPO/scripts/backup.sh" ]; then
      echo "! $REPO/scripts/backup.sh is not executable — cron needs it to be:" >&2
      echo "    chmod +x $REPO/scripts/backup.sh" >&2
    fi

    echo "Will install this crontab entry for user '$(id -un)':"
    echo "    $ENTRY"
    echo "  repo:     $REPO"
    echo "  schedule: $BACKUP_SCHEDULE"
    echo "  log:      $BACKUP_LOG"
    echo "  stack:    \${COMPOSE_FILE:-docker-compose.deploy.yml} (set COMPOSE_FILE in $REPO/.env.backup to change)"
    if printf '%s\n' "$current" | grep -F -q -- "$TAG"; then
      echo "  (an entry for this repo already exists and will be REPLACED)"
    fi
    if [ "$MODE" = "dry-run" ]; then
      echo "-- dry run: crontab NOT modified --"
      exit 0
    fi

    printf '%s\n%s\n' "$without_ours" "$ENTRY" | grep -v '^$' | $CRONTAB -
    echo "ok Installed. Verify with: $CRONTAB -l"
    echo "   Prove it end to end now:  ./scripts/backup.sh && ./scripts/restore-drill.sh"
    # Offsite is configured either in this shell or (the normal case, since cron
    # has no environment) in the env file backup.sh sources at run time. This is
    # informational only — the file is NOT sourced here, so nothing in it can
    # quietly rewrite the crontab entry being installed.
    if [ -n "${BACKUP_S3_BUCKET:-}" ] ||
       grep -qs '^[[:space:]]*BACKUP_S3_BUCKET=[^[:space:]]' "${BACKUP_ENV_FILE:-./.env.backup}"; then
      echo "   Offsite push is configured; check the credential reaches the bucket:"
      echo "     ./scripts/backup-offsite.sh --check"
    else
      echo "   Offsite copies are OFF (BACKUP_S3_BUCKET unset). A dump that only lives"
      echo "   on this box dies with it — docs/deploy/backups.md § Offsite."
    fi
    ;;
esac

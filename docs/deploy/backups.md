# Backups, offsite copies and restore drills

The whole corpus — memories, provenance, embeddings, tokens, documents,
projects, the job queue — is a single Postgres database. Everything else in the
stack is replaceable from the repo. So the backup story is exactly one story:
**can you get that database back on a new box.**

This page is the operator guide for the four scripts that answer it:

| Script | What it does | Where it runs |
|---|---|---|
| `scripts/backup.sh` | `pg_dump -Fc` into `./backups/`, prune old dumps, hand the archive to the offsite helper | the host running the stack |
| `scripts/backup-offsite.sh` | copy one archive to an S3-compatible bucket (opt-in, silent no-op when unconfigured) | same host, called by `backup.sh` |
| `scripts/install-backup-cron.sh` | install/remove the daily cron entry, idempotently | same host, once |
| `scripts/restore-drill.sh` | restore the newest dump into a throwaway database and assert it is real | anywhere with Docker |
| `scripts/restore.sh` | the actual recovery — destructive, into a live database | during an incident |

---

## 1. Take a backup

```bash
./scripts/backup.sh                                   # → ./backups/brainiac-<ts>.dump
COMPOSE_FILE=docker-compose.yml ./scripts/backup.sh   # the local dev stack instead
```

`pg_dump -Fc` runs *inside* the Postgres container, so nothing needs a Postgres
client on the host. The output is a compressed custom-format archive — the
format `pg_restore` can filter and reorder, not a flat `.sql` file.

`backups/` gets its own `.gitignore` on first run, so dumps can never be
committed by accident.

**Retention.** Local dumps older than `RETENTION_DAYS` (default 14) are deleted
after each successful run. `RETENTION_DAYS=0` keeps everything.

**Sanity check.** A dump under 1000 bytes is deleted and the script exits
non-zero — that is what a stopped container or a wrong `PG_DB` produces, and a
0-byte file sitting in `backups/` looking like a backup is worse than no file.

## 2. Install the daily cron entry

The cron line was documentation for a while (`docs/deploy/alibaba.md` § Backup
& restore). It is now installable:

```bash
./scripts/install-backup-cron.sh --dry-run    # print the plan, change nothing
./scripts/install-backup-cron.sh              # install / refresh
./scripts/install-backup-cron.sh --uninstall  # remove it
crontab -l                                    # verify
```

It writes the documented line for the **current user**, pointed at **this
checkout**:

```
15 3 * * * cd /path/to/brainiac && ./scripts/backup.sh >> /var/log/brainiac-backup.log 2>&1 # brainiac-backup:/path/to/brainiac
```

- **Idempotent.** Every line it owns carries the trailing
  `# brainiac-backup:<repo path>` tag; a re-run strips tagged lines before
  appending, so the entry can never stack up. The tag includes the repo path, so
  two checkouts on one host get two independent entries and neither installer
  touches the other's.
- **It refuses to pretend.** No `crontab` binary → hard error telling you to
  install cron or use a systemd timer, rather than silently doing nothing while
  you believe backups are running. An unwritable `BACKUP_LOG` is also fatal:
  cron cannot open the redirect, so the command never runs at all.
- Override the schedule or log path: `BACKUP_SCHEDULE="30 2 * * *"`,
  `BACKUP_LOG=$PWD/backups/backup.log`.

**Cron has almost no environment.** Put anything `backup.sh` needs in
`.env.backup` at the repo root (git-ignored, sourced automatically):

```bash
# .env.backup — host backup config, NEVER committed
COMPOSE_FILE=docker-compose.deploy.yml
RETENTION_DAYS=14
# offsite (optional, see below)
BACKUP_S3_BUCKET=my-bucket
BACKUP_S3_PREFIX=brainiac/prod
BACKUP_S3_ENDPOINT=https://oss-ap-southeast-1.aliyuncs.com
BACKUP_S3_ACCESS_KEY_ID=...
BACKUP_S3_SECRET_ACCESS_KEY=...
```

`.env.backup` is git-ignored (repo root `.gitignore`), alongside `.env.deploy`
and `.env.test`.

## 3. Offsite

A dump that only ever lives on the box it was taken from does not survive that
box dying — and "the box died" is the failure backups exist for. Set
`BACKUP_S3_BUCKET` and every new archive is copied to any S3-compatible store
(AWS S3, Alibaba OSS, Cloudflare R2, MinIO) via the `aws` CLI:

```bash
scripts/backup-offsite.sh --check      # config + a real auth round-trip, uploads nothing
scripts/backup-offsite.sh --latest     # ship the newest local dump by hand
```

| Variable | Meaning |
|---|---|
| `BACKUP_S3_BUCKET` | bucket, optionally with a key prefix (`my-bucket/brainiac/prod`). **Unset ⇒ the whole feature is off** |
| `BACKUP_S3_PREFIX` | key prefix as a separate variable, appended to the bucket string. Optional — `BUCKET=my-bucket/brainiac/prod` and `BUCKET=my-bucket` + `PREFIX=brainiac/prod` are the same destination; stray slashes are trimmed |
| `BACKUP_S3_ACCESS_KEY_ID` | access key id |
| `BACKUP_S3_SECRET_ACCESS_KEY` | secret access key |
| `BACKUP_S3_ENDPOINT` | endpoint URL for non-AWS stores |
| `BACKUP_S3_REGION` | default `us-east-1`; OSS/R2/MinIO ignore it |
| `BACKUP_S3_STORAGE_CLASS` | e.g. `STANDARD_IA`, `GLACIER_IR` |
| `AWS_CLI` | binary to use, default `aws` on `PATH` |

Design rules this follows, deliberately:

- **Names, never values.** No credential value appears in any script, and none
  is prompted for. The two keys are exported only into the `aws` subshell —
  never onto a command line, because `argv` is world-readable via `ps` and the
  environment is not.
- **`BACKUP_S3_*`, not `AWS_ACCESS_KEY_ID`.** The deploy stack already uses the
  `AWS_*` names for the Bedrock LLM provider. Backup storage must not silently
  inherit — or be mistaken for — an inference credential.
- **Off by default, silently.** With `BACKUP_S3_BUCKET` unset the helper exits 0
  having printed nothing, so the daily cron mail stays empty and stays worth
  reading.
- **Local copy first.** The push happens only after the dump is written and
  sanity-checked. A broken offsite config can cost you the *copy*, never the
  *dump*; it just exits 3 so cron mails you.
- **Remote retention is the bucket's job.** `RETENTION_DAYS` prunes locally
  only. Set a lifecycle rule on the bucket — the script will never delete a
  remote object.

## 4. Prove the backup works

> An untested backup is a rumour.

```bash
./scripts/restore-drill.sh                        # newest dump in ./backups
./scripts/restore-drill.sh backups/brainiac-<ts>.dump
```

The drill starts a throwaway `pgvector/pgvector:pg17` container with no
published ports, creates a scratch database, restores into it, asserts, then
destroys the container — on success, on failure, and on `^C` (an `EXIT` trap).

**It cannot touch live data**, by two independent mechanisms:

1. It uses `docker run` to create its own container; it never runs
   `docker compose exec`, so the deploy and dev stacks are unreachable to it.
2. Inside that container it restores into `DRILL_DB` (default `brainiac_drill`)
   and **hard-fails if that name equals the production DB name** `PG_DB`
   (default `brainiac`), or is a cluster-managed database. Wanting a real
   restore means wanting `scripts/restore.sh`, and the drill says so.

What it asserts — all against objects that exist in `migrations/`:

| # | Assertion | Why |
|---|---|---|
| 1 | `_sqlx_migrations` has > 0 successful rows | proves this is a migrated Brainiac DB, not an empty shell. The count is compared to the `.sql` files in `migrations/`; a lower count only warns, since an older dump legitimately predates newer migrations |
| 2 | tables `orgs`, `users`, `teams`, `memories`, `sources`, `provenance`, `memory_embeddings`, `api_tokens`, `documents`, `projects`, `queue.jobs` exist | one table from every plane: identity (0001), corpus (0001), auth (0003), documents (0017), projects (0034), job queue (0001) |
| 3 | extension `vector` is installed | without pgvector the embedding columns cannot have restored and retrieval is dead on arrival |
| 4 | RLS enabled on `memories`, `sources`, `provenance`, and > 0 policies in `public` | RLS is load-bearing here, not decoration (`CLAUDE.md` § Design standards). A restore that lost the policies would silently serve cross-tenant rows |
| 5 | `orgs` and `memories` are non-empty | a structurally perfect restore of zero rows is still a lost corpus. Override with `DRILL_REQUIRE_ROWS="..."`, or `""` on a host whose corpus really is still empty |

It prints `ok DRILL PASSED` / `x DRILL FAILED` and exits 0/1, so it is safe to
run from cron or CI.

`pg_restore` exiting non-zero is *reported but not fatal* — it returns 1 for
non-fatal warnings too (ownership, extension comments). The assertions are the
verdict.

## 5. Restore for real

```bash
docker compose -f docker-compose.deploy.yml stop server   # drop the lock holders
./scripts/restore.sh --latest --yes                       # or a named dump
docker compose -f docker-compose.deploy.yml start server
```

`restore.sh` is **destructive**: `pg_restore --clean --if-exists` drops and
recreates the dumped objects in the target database. Without `--yes` it makes
you type `restore` first. `does not exist, skipping` notices on a fresh
database are expected.

Recovering onto a **new box**: bring the stack up first
(`docker compose -f docker-compose.deploy.yml --env-file .env.deploy up -d`) so
`migrate()` creates the roles and the schema, `stop server`, then restore.

## 6. What the operator still owns

The scripts do not, and cannot, cover these:

- **Running the drill.** Nothing schedules `restore-drill.sh`. Run it monthly,
  and always before a risky migration. (It is cron-safe if you want it daily:
  same pattern as the backup entry, a different `BACKUP_SCHEDULE`.)
- **Watching the log.** `install-backup-cron.sh` wires `>> $BACKUP_LOG 2>&1`;
  nothing alerts on failure. At minimum check `tail /var/log/brainiac-backup.log`
  and that the newest file in `backups/` is from today.
- **Bucket lifecycle + access.** Remote retention, versioning, and a key scoped
  to `PutObject` on one prefix are yours to configure in the object store.
- **Restoring the offsite copy.** `backup-offsite.sh` only uploads. Pulling an
  object back down is `aws s3 cp s3://... ./backups/` by hand.
- **Secrets are not in the dump.** `pg_dump` carries data, not `.env.deploy` /
  `.env.backup`. Losing the box loses `BRAINIAC_TOKENS`, `CONSOLE_PASSCODE`,
  `CONSOLE_SESSION_SECRET` and provider keys unless you kept them elsewhere.
- **Roles are not in the dump either.** `pg_dump` (unlike `pg_dumpall`) does not
  carry roles; `brainiac_app` comes back from migration `0001` when the server
  boots against a fresh database. The drill pre-creates it for the same reason.
- **RPO/RTO.** Daily at 03:15 means up to ~24 h of memories can be lost. If that
  is too much, raise the cadence (`BACKUP_SCHEDULE`); the dump is small.
- **The byte-exact alternative.** For a pre-migration snapshot, stop the stack
  and tar the named volume — `docs/deploy/alibaba.md` § Backup & restore.

## Quick reference

```bash
./scripts/install-backup-cron.sh --dry-run   # what would be installed
./scripts/install-backup-cron.sh             # install the daily entry
./scripts/backup.sh                          # one backup now
./scripts/backup-offsite.sh --check          # is offsite wired up?
./scripts/restore-drill.sh                   # prove the newest dump restores
./scripts/restore.sh --latest --yes          # DESTRUCTIVE real restore
```

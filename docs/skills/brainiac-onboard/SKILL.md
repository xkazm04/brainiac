---
name: brainiac-onboard
description: Connect this repository to the org's Brainiac memory. Pairs the repo with the Brainiac console (device-flow approval), writes a project-scoped API key into .env without ever displaying it, verifies connectivity end to end, enriches CLAUDE.md/AGENTS.md with memory conventions, and optionally registers the MCP server and syncs the org's skill library. Safe to re-run.
---

# Brainiac onboarding

You are onboarding this repository onto Brainiac — the org's governed memory.
The outcome: a working `.env` with a **project-scoped** API key, verified
connectivity, and agent instructions (CLAUDE.md/AGENTS.md) that teach future
sessions to use org memory.

## Security invariants (non-negotiable)

1. **The API key must never appear in your output, your reasoning, or any
   command's printed result.** It travels exactly once: server response →
   shell variable → `.env`. Never `echo` it, never `cat .env` after writing
   it, never paste it into a command line as a literal — always re-read it
   with `$(grep '^BRAINIAC_API_TOKEN=' .env | cut -d= -f2-)` inside the
   command that needs it.
2. **Never commit the key.** Verify `.env` is gitignored before writing it;
   add it to `.gitignore` if missing.
3. **Do not work around a denied or failed approval.** If the console denies
   the pairing or the repo isn't whitelisted, report it and stop — the fix
   (registering the repo under a project) belongs to the console operator.

## Step 0 — Preflight

Collect, in one shot:

- `git remote get-url origin` — fail early with a clear message if this is
  not a git repo or has no `origin` remote.
- The Brainiac API URL: `$BRAINIAC_API_URL` from the environment or an
  existing `.env`; otherwise **ask the user** for it. Confirm it answers:
  `curl -fsS "$API/health"`.
- Whether `.env` exists and already contains `BRAINIAC_API_TOKEN`. If it
  does, ask the user whether to keep the existing key (then skip to Step 2)
  or pair for a fresh one.
- Check `.gitignore` covers `.env` (fix if not).

## Step 1 — Pair and receive the key

The pairing is a device-authorization flow against Brainiac itself. Because
shell state does not persist between your tool calls, keep the device code in
a scratch file, not a variable.

**1a. Start the pairing** (one command; note only safe fields are printed —
the response's `device_code` goes straight to the scratch file):

```bash
API="<the API URL>"
REMOTE="$(git remote get-url origin)"
curl -fsS -X POST "$API/v1/onboard/start" -H 'content-type: application/json' \
  -d "{\"remote\":\"$REMOTE\",\"label\":\"$(whoami)@$(hostname)\"}" \
  > .brainiac-pairing.json
jq '{user_code, verification_url, remote, expires_in_secs}' .brainiac-pairing.json
```

**1b. Tell the user** (verbatim, filling in the values):

> Open **{verification_url}** and approve the pairing request showing code
> **{user_code}** for **{remote}**. If the console says the repo isn't
> registered, an admin must add it under a project in the Projects module
> first. The code expires in 15 minutes.

**1c. Poll until decided.** Run this loop (re-run the command if it returns
`still pending`; each invocation polls for ~2 minutes so it never hits the
tool timeout). The token is appended to `.env` inside the loop and is never
printed:

```bash
API="<the API URL>"
DC="$(jq -r .device_code .brainiac-pairing.json)"
for i in $(seq 1 24); do
  RESP="$(curl -fsS -X POST "$API/v1/onboard/poll" -H 'content-type: application/json' \
    -d "{\"device_code\":\"$DC\"}")"
  ST="$(printf '%s' "$RESP" | jq -r .status)"
  case "$ST" in
    approved)
      grep -q '^BRAINIAC_API_URL=' .env 2>/dev/null || printf 'BRAINIAC_API_URL=%s\n' "$API" >> .env
      printf 'BRAINIAC_API_TOKEN=%s\n' "$(printf '%s' "$RESP" | jq -r .token)" >> .env
      printf 'approved — project: %s · key written to .env\n' "$(printf '%s' "$RESP" | jq -r .project_name)"
      rm -f .brainiac-pairing.json
      exit 0 ;;
    pending) sleep 5 ;;
    *) echo "pairing ended: $ST"; rm -f .brainiac-pairing.json; exit 1 ;;
  esac
done
echo "still pending"
```

On `denied`/`expired`: report it to the user and stop (see invariant 3).

## Step 2 — Verify end to end

Prove the key works under its real scopes — a live search, authenticated as
the new key, exercises auth, scopes, and RLS in one shot:

```bash
API="$(grep '^BRAINIAC_API_URL=' .env | tail -1 | cut -d= -f2-)"
curl -fsS -X POST "$API/v1/memories/search" \
  -H "authorization: Bearer $(grep '^BRAINIAC_API_TOKEN=' .env | tail -1 | cut -d= -f2-)" \
  -H 'content-type: application/json' \
  -d '{"query":"onboarding connectivity check","k":1}' | jq '{ok: true, hits: (.hits|length)}'
```

`{"ok": true, ...}` (any hit count, including 0 on a fresh org) means
onboarding succeeded. A 401/403 means the key or scopes are wrong — report,
don't retry blindly.

## Step 3 — Enrich CLAUDE.md / AGENTS.md

Merge (never clobber) a Brainiac section into the repo's agent instructions.
If `CLAUDE.md` exists, append there; mirror into `AGENTS.md` only if that
file already exists. If the section (`## Brainiac org memory`) already
exists, update it in place — this keeps re-runs idempotent. Template,
adjusted to what you learned during pairing:

```markdown
## Brainiac org memory

This repo is connected to Brainiac (org memory), project: **<project_name>**.
Credentials live in `.env` (`BRAINIAC_API_URL`, `BRAINIAC_API_TOKEN`) — never
commit or print them.

- **Before designing or deciding**: search org memory for prior art —
  `POST $BRAINIAC_API_URL/v1/memories/search` with `{"query": ..., "k": 5}`
  (bearer: the `.env` token). Decisions, pitfalls, and how-tos from other
  sessions and teams live there.
- **After a decision ships or a pitfall bites**: write it back —
  `POST $BRAINIAC_API_URL/v1/memories` with `{"content": "<one
  self-contained statement>"}`. It enters a governed review pipeline; write
  facts, not transcripts.
- If the `brainiac` MCP server is registered, prefer its tools
  (`memory_search`, `memory_context`, `memory_add`) over raw REST.
```

## Step 4 (offer, don't assume) — MCP registration

Ask the user whether to register Brainiac's MCP server so agent sessions get
first-class tools. This currently requires the `brainiac` binary on PATH
**and** direct database reachability (`DATABASE_URL`) — true for self-hosted
and local deployments only. If both are present:

```bash
claude mcp add brainiac --scope project \
  -e BRAINIAC_MCP_TOKEN="$(grep '^BRAINIAC_API_TOKEN=' .env | tail -1 | cut -d= -f2-)" \
  -e DATABASE_URL="$DATABASE_URL" \
  -- brainiac mcp
```

(The command substitution keeps the token out of the transcript; it lands in
the project's MCP config, which the user should also gitignore if their
`.mcp.json` is committed.) If the binary or DB access is missing, skip with a
note — REST from Step 3 covers the same operations.

## Step 5 (offer, don't assume) — Sync the org's skill library

Brainiac distributes org-ratified agent skills. Offer to pull them:

```bash
API="$(grep '^BRAINIAC_API_URL=' .env | tail -1 | cut -d= -f2-)"
AUTH="authorization: Bearer $(grep '^BRAINIAC_API_TOKEN=' .env | tail -1 | cut -d= -f2-)"
curl -fsS -H "$AUTH" "$API/v1/library/skills" | jq -r '.skills[].slug' | while read -r slug; do
  mkdir -p ".claude/skills/$slug"
  curl -fsS -H "$AUTH" "$API/v1/library/skills/$slug/download" \
    | jq -r .body > ".claude/skills/$slug/SKILL.md"
  echo "synced skill: $slug"
done
```

(Requires the key to carry `lib:read`; the default onboarding key carries
`read,write` only, so a 403 here is expected on default keys — tell the user
an admin can mint a key with `lib:read` if they want library sync.)

## Wrap up

Report to the user, in plain sentences: which project the repo paired to,
that the key is in `.env` (scoped `read,write`, revocable in the console's
Keys module), the verification result, which files you touched, and which
optional steps were taken or skipped and why.

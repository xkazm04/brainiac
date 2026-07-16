# Quick onboarding from the Keys module — feasibility and design

**Status:** research, not built. Asked 2026-07-15.
**Question:** can the console open a task in the operator's local terminal, running
Claude Code, to seed a key into `.env`, verify connectivity, and do it against a
project path the user types in?

**Answer: yes, and by a better route than the one the question assumes.** The
browser cannot launch a process — but it does not have to.

---

## 1. What is actually available

Everything below was verified against the CLI and this machine's registry on
2026-07-15 (Claude Code `2.1.210`), not from documentation alone.

### 1.1 There IS a deep link — `claude-cli://`

This is the finding that changes the design. Claude Code registers an OS URL
protocol handler on first interactive session, per user:

```
HKCU:\Software\Classes\claude-cli
  (default)    = "URL:Claude Code URL Handler"
  URL Protocol =
  shell\open\command =
    "…\npm\node_modules\@anthropic-ai\claude-code\bin\claude.exe" --handle-uri "%1"
```

A link on a web page can therefore hand a prompt and a working directory to the
user's own terminal. Documented at `code.claude.com/docs/en/deep-links`:

| part | meaning |
| --- | --- |
| `claude-cli://open` | the only supported path |
| `?q=` | prompt text, URL-encoded, ~5,000 char ceiling, `%0A` for newlines |
| `?cwd=` | absolute local path — **this is the "project path from user input"** |
| `?repo=` | `owner/name`, resolved against clones Claude has already seen |

Terminal selection is the OS's problem, not ours: Windows picks Windows Terminal
→ PowerShell → `cmd.exe`; macOS reuses the most recent terminal; Linux honours
`$TERMINAL`.

Two caveats worth writing down before anyone builds on this:

- **`--handle-uri` is not in `claude --help`.** It is an internal flag the
  registration points at. We depend on the *scheme*, which is documented; we must
  not shell out to `--handle-uri` ourselves.
- **Registration is not guaranteed.** It happens on first interactive session and
  can be turned off (`disableDeepLinkRegistration: "disable"`). A browser cannot
  feature-detect a protocol handler — a click at a missing scheme fails silently
  or shows an OS error. So the deep link can never be the *only* path; it is an
  accelerator over a copyable command. There is also a second, separate
  `claude://` key here belonging to the Claude desktop app — not ours, do not
  target it.

### 1.2 The CLI surface we would drive (verified via `claude --help`)

- `claude "prompt"` interactive · `-p/--print` non-interactive · stdin/heredoc.
- `--add-dir`, `--model`, `--session-id <uuid>`, `--settings <file-or-json>`.
- `--permission-mode` — real choices are `acceptEdits`, `auto`,
  `bypassPermissions`, `manual`, `dontAsk`, `plan`.
- `--mcp-config <configs…>` and `--strict-mcp-config` **do exist on the CLI**
  (worth stating: this was reported as uncertain; it is not).
- `claude mcp add <name> <commandOrUrl> [args…]`, with `--transport http`,
  `--header`, `-e KEY=val`.

### 1.3 What does NOT exist

- **No local HTTP/IPC surface on Claude Code.** No localhost port, no socket, no
  pipe a page can reach. Headless (`-p`) gives stdout/stderr and an exit code.
  Remote Control routes through the Anthropic API — it is not a local endpoint.
- **No one-command MCP registration from a URL.** `claude mcp add` is per-server.
- **No way to seed an API key and skip first-run prompts** without config files
  already on disk.

---

## 2. Why the obvious design is the wrong one

The literal reading — "console launches CMD, Claude writes the key into `.env`" —
has us hand an agent a secret and a filesystem path and ask it to edit a file. It
would work. It is still the wrong shape, for three reasons:

1. **It needs the key in the prompt.** A `claude-cli://` URI carries `q=` as
   plain text. That URI goes through the OS handler, and on the way it is liable
   to land in shell history, terminal scrollback, and the agent's own session
   transcript. Minting a token and pasting it into a URL is the one thing an
   access module should not do.
2. **Writing `.env` does not need a model.** `echo` does. Spending an LLM turn —
   with filesystem permissions — on a two-line file edit is expensive theatre,
   and it puts a nondeterministic actor between the operator and their secrets.
3. **It cannot report back.** The browser gets no exit code, no stdout, nothing.
   Whatever happens in that terminal, the console's onboarding UI still says
   "waiting". Success and silence look identical — the failure mode this repo has
   already been burned by.

## 3. The design that fits

Three tiers, each independently useful. Ship 3.1 first; it is most of the value.

### 3.1 A copyable command (no deep link, works everywhere) — **recommended**

The Keys module already mints a token and shows it once. Add, beside it, the exact
command that consumes it:

```bash
# the operator pastes this in their project directory
printf 'BRAINIAC_API_URL=%s\nBRAINIAC_API_TOKEN=%s\n' \
  "https://brainiac.example/v1" "bx_live_…" >> .env
```

Honest, inspectable, no model, no deep link, no OS assumptions, and it works on a
locked-down laptop. The token stays in the clipboard and the file — the two places
it has to be anyway.

### 3.2 Connectivity + permission check — as an MCP registration, not a task

This is the part worth automating, because it answers a question the operator
genuinely cannot answer alone: *can this machine reach the org, and what is this
key allowed to see?*

We already ship an MCP server over stdio (`crates/brainiac-server/src/mcp.rs`,
`memory_search` / `memory_context` / `memory_add` / `entity_lookup`, authenticated
as the developer's own token so RLS applies per call). So invert the flow — the
console does not drive Claude Code; it hands the operator a registration:

```bash
claude mcp add brainiac -e BRAINIAC_API_TOKEN=bx_live_… -- brainiac mcp
```

Then their agent has org memory for good, not just during onboarding — and the
first `memory_search` IS the connectivity-and-permissions test, run as them, from
their device, under their RLS scope. Nothing to fake, nothing to report back to a
browser tab.

The console's job shrinks to what a browser is good at: show the command, and
tell them what a healthy first result looks like.

### 3.3 The deep link, as an accelerator only

Where it earns its place is the thing a command cannot do: put a *task* in front
of the operator with their project already open.

```ts
// The path comes from user input, so it is the one field we must not trust.
const uri =
  `claude-cli://open?cwd=${encodeURIComponent(projectPath)}` +
  `&q=${encodeURIComponent(prompt)}`;
```

with `prompt` naming the *steps*, never the secret:

> Add Brainiac to this project: read `.env`, add `BRAINIAC_API_URL`, then register
> the MCP server. The token is in my clipboard — ask me for it.

Rules this must follow, or it should not ship:

- **Never interpolate a token into `q=`.** Ask the operator to paste it.
- **Validate `cwd` in the UI** — absolute, no control characters. The docs
  constrain it; we should not rely on the handler to be the only check.
- **Render it as a link the user clicks**, never `window.location = uri` on load.
  A page that silently spawns terminals is malware behaviour.
- **Offer the copyable command next to it, always** (3.1), because we cannot
  detect whether the scheme is registered.
- **Never suggest `--dangerously-skip-permissions`** in a generated prompt. The
  whole point of the gate this product sells is that a human signs.

### 3.4 The local helper — noted and not recommended

The Agent SDK (`@anthropic-ai/claude-agent-sdk`) could run a localhost daemon the
console POSTs to, which would give real status back. It is the only design that
closes the reporting gap in §2.3. It also means shipping and versioning a second
long-lived process, on every operator's machine, holding a token, listening on a
port, for an onboarding flow that runs once. Not worth it now. Revisit only if
onboarding becomes a recurring surface rather than a first-run one.

---

## 4. Recommendation

1. Ship §3.1 — the copyable `.env` line — in the Keys module next to the token.
2. Ship §3.2 — the `claude mcp add` line — as the connectivity check. It is
   better onboarding *and* better product: it ends with the agent wired in.
3. Add §3.3 behind those two, as a "open this in Claude Code" link, once there is
   a real task worth seeding.
4. Do not build §3.4.

The thing to resist is the demo-shaped version: a button that spawns a terminal,
tells the user it worked, and cannot know whether it did.

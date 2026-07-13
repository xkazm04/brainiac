#!/usr/bin/env bash
# One MCP tool call, as a given Character, through the REAL agent path.
#
#   ./mcp_call.sh <character-token> <tool> '<json-args>'
#
# e.g. ./mcp_call.sh "$TOK_ADA" memory_context '{"task_hint":"refund-worker retry timeouts"}'
#
# This pipes JSON-RPC over stdio into `brainiac mcp` — the same handler a real Claude Code or
# Cursor session hits (crates/brainiac-server/src/mcp.rs, tested in tests/mcp_pg.rs).
#
# NEVER approximate this with a REST call, and never hand-write a payload. The entire trial's
# fidelity rests on arm C reading exactly what a real agent would read. If you fake the payload,
# you are no longer measuring the product.
set -euo pipefail

TOKEN="${1:?character token required}"
TOOL="${2:?tool name required}"
ARGS="${3:-{}}"

: "${DATABASE_URL:?DATABASE_URL must be set}"

REQ=$(cat <<EOF
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"uat-driver","version":"1"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"${TOOL}","arguments":${ARGS}}}
EOF
)

# The tool result arrives as a JSON string inside a text content block — unwrap it so the
# caller sees what the model sees.
echo "$REQ" \
  | BRAINIAC_MCP_TOKEN="$TOKEN" cargo run -q -p brainiac-server -- mcp 2>/dev/null \
  | grep '"id":2' \
  | tail -1

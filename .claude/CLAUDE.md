# CLAUDE.md

This file provides guidance to Claude Code when working with code in this repository.

<!-- vibeman:context-map:start -->
## Context Map

This project has a Vibeman-generated context map at `context-map.json` (repo root). It maps every file to a feature ("context"), grouped by business domain. **Before editing code, read `context-map.json` to find the relevant context and scope your changes to its `filePaths`.** The `index` field is a quick one-line-per-context overview. If you change which files a context owns, update `context-map.json` to match (or run Vibeman's refresh) so it stays accurate.
<!-- vibeman:context-map:end -->

## Brainiac org memory

This repo is connected to Brainiac (org memory), project: **brainiac**.
Credentials live in `.env` (`BRAINIAC_API_URL`, `BRAINIAC_API_TOKEN`) — never
commit or print them.

- **Before designing or deciding**: search org memory for prior art —
  `POST $BRAINIAC_API_URL/v1/memories/search` with `{"query": ..., "k": 5}`
  (bearer: the `.env` token). Decisions, pitfalls, and how-tos from other
  sessions live there.
- **After a decision ships or a pitfall bites**: write it back —
  `POST $BRAINIAC_API_URL/v1/memories` with `{"content": "<one
  self-contained statement>"}`. It enters a governed review pipeline; write
  facts, not transcripts.

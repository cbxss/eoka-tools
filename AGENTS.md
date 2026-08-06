# AGENTS.md

## No comments

Do not write comments in code in this repo. No `//`, `///`, `//!` in Rust;
no `//` in Go, including exported-identifier doc comments. Code must be
self-explanatory through naming and structure. If you think a comment is
needed, restructure or rename instead.

This applies to all crates and modules in this repo, not just new code —
when you touch a file, strip comments you find in the code you're editing.

Markdown docs (README.md, PROTOCOL.md, this file) are not code comments and
are unaffected by this rule.

## Tool protocol source of truth

Use `crates/eoka-protocol` as the canonical definition point for Eoka tools,
operation metadata, request/input types, schema generation, manifests, and tool
exposure rules. Do not duplicate tool descriptors independently in the CLI, MCP
server, SDK, or Tack adapter.

When adding or changing a tool, update the protocol catalog first and project
the downstream surfaces from it. The CLI `tools manifest` output, `eoka-tack`
tool registration, SDK request helpers, and MCP input types should stay derived
from `eoka-protocol` so paths, schemas, tags, exposure, and capability metadata
cannot drift.

## Tasks

Use `mise` for repo workflows. Prefer `mise run conformance` after protocol,
CLI, MCP, SDK, or Tack changes, and `mise run release-check` before release or
PR handoff.

## Self-hosted runners

Only workflows that cannot be triggered by `pull_request` or
`pull_request_target` (i.e. triggers restricted to `push` on protected
branches/tags, or `workflow_dispatch`) may target the self-hosted runner
(`[self-hosted, macOS, ARM64]`). `pull_request`-triggered workflows
(`ci.yml`) must stay on GitHub-hosted runners — a fork PR's workflow run
must never execute on our own hardware.

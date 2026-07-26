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

## Self-hosted runners

Only workflows that cannot be triggered by `pull_request` or
`pull_request_target` (i.e. triggers restricted to `push` on protected
branches/tags, or `workflow_dispatch`) may target the self-hosted runner
(`[self-hosted, macOS, ARM64]`). `pull_request`-triggered workflows
(`ci.yml`) must stay on GitHub-hosted runners — a fork PR's workflow run
must never execute on our own hardware.

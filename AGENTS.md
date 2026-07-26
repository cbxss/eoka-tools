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

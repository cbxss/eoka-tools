# eoka-agent

AI agent interaction layer for eoka browser automation. Rust crate, lives at `/Users/cbass/Code/eoka-agent`.

## Structure

- `src/lib.rs` — `AgentPage` struct, `InteractiveElement`, all public API (click, fill, select, scroll, navigate, extract, etc.)
- `src/observe.rs` — JS injection that enumerates interactive DOM elements, returns them as JSON
- `src/annotate.rs` — Injects numbered red overlay labels, takes screenshot, cleans up
- `src/main.rs` — MCP server binary entry point
- `src/mcp.rs` — MCP server implementation (tools, state management, stdio transport)
- `examples/demo.rs` — End-to-end demo (form fill, screenshot, extraction)

## Dependencies

- `eoka` (local, at `../eoka`) — CDP-based browser automation (Page, Browser, stealth, mouse/keyboard)
- `rmcp` — MCP server framework (stdio transport)
- `serde`, `serde_json`, `tokio`, `schemars`, `anyhow`, `base64`

## Key patterns

- `AgentPage` wraps an `eoka::Page` reference with a lifetime
- Elements are index-based and ephemeral — indices are only valid until the next `observe()` call
- `observe()` runs JS in the page to find all interactive elements, parses the JSON result into `Vec<InteractiveElement>`
- Annotated screenshots inject a temporary DOM overlay, screenshot, then remove it
- Viewport-only filtering is on by default to reduce token count
- CSS selectors are auto-generated for each element and used internally for actions

## MCP server

The binary target exposes the agent as an MCP server over stdio. Single browser instance, lazy-launched on first `navigate` call.

### State

The server stores `Browser`, `Page`, and `Vec<InteractiveElement>` separately (not `AgentPage`) to avoid lifetime issues. `AgentPage` is created temporarily for `observe` and `screenshot` calls. Action tools (`click`, `fill`, etc.) use `Page` methods directly with stored element selectors.

### Tools

`navigate`, `observe`, `screenshot`, `click`, `fill`, `select`, `hover`, `type_key`, `scroll`, `find_text`, `extract`, `page_text`, `page_info`, `back`, `forward`, `close`

### Setup

```sh
# Add to Claude Code
claude mcp add eoka-agent -- cargo run --manifest-path /Users/cbass/Code/eoka-agent/Cargo.toml

# Or after cargo install
claude mcp add eoka-agent -- eoka-agent
```

## Build & run

```sh
cargo build
cargo run --example demo
cargo run --bin eoka-agent  # start MCP server
```

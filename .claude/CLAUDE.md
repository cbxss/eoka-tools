# eoka-agent

AI agent interaction layer for eoka browser automation. Rust crate in eoka-tools workspace.

## Structure

- `src/lib.rs` — `AgentPage`, `Session`, `InteractiveElement`, all public API (click, fill, select, scroll, navigate, extract, etc.)
- `src/observe.rs` — JS injection that enumerates interactive DOM elements, returns them as JSON
- `src/annotate.rs` — Injects numbered red overlay labels, takes screenshot, cleans up
- `src/main.rs` — MCP server binary entry point
- `src/mcp.rs` — MCP server implementation (multi-tab state management, tools, stdio transport)
- `examples/demo.rs` — End-to-end demo (form fill, screenshot, extraction)

## Dependencies

- `eoka` (0.3.4) — CDP-based browser automation (Page, Browser, stealth, mouse/keyboard, tab management)
- `rmcp` — MCP server framework (stdio transport)
- `serde`, `serde_json`, `tokio`, `schemars`, `anyhow`, `base64`

## Key patterns

- `AgentPage` wraps an `eoka::Page` reference with a lifetime (for library use)
- `Session` owns Browser + Page for simpler single-tab usage
- MCP server manages multiple tabs with `BrowserState` (HashMap of tab ID → TabState)
- Elements are index-based and ephemeral — indices are only valid until the next `observe()` call
- `observe()` runs JS in the page to find all interactive elements, parses the JSON result into `Vec<InteractiveElement>`
- Annotated screenshots inject a temporary DOM overlay, screenshot, then remove it
- Viewport-only filtering is on by default to reduce token count
- CSS selectors are auto-generated for each element and used internally for actions

## MCP server

The binary target exposes the agent as an MCP server over stdio. Browser is lazy-launched on first `navigate` call. Supports multiple tabs.

### State

The server maintains `BrowserState` with:
- `Browser` instance
- `HashMap<String, TabState>` for multi-tab support (each tab has Page + elements)
- `current_tab_id` tracks the active tab

### Tools

**Tab Management:**
- `list_tabs` — list all open tabs (* marks current)
- `new_tab` — open a new tab (optionally with URL)
- `switch_tab` — switch to a tab by ID
- `close_tab` — close a tab by ID

**Navigation:**
`navigate`, `back`, `forward`

**Observation:**
`observe`, `screenshot`, `find_text`, `page_text`, `page_info`

**Actions:**
`click`, `fill`, `select`, `hover`, `type_key`, `scroll`

**Other:**
`extract`, `cookies`, `set_cookie`, `close`

### Setup

```sh
# Add to Claude Code
claude mcp add eoka-agent -- cargo run --manifest-path /Users/cbass/Code/eoka-tools/crates/eoka-agent/Cargo.toml

# Or after cargo install
claude mcp add eoka-agent -- eoka-agent
```

## Build & run

```sh
cargo build -p eoka-agent
cargo run -p eoka-agent --example demo
cargo run -p eoka-agent  # start MCP server
```

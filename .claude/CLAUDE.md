# eoka-mcp

AI agent interaction layer for eoka browser automation. Rust crate in eoka-tools workspace.

## Structure

- `src/lib.rs` — `Session`, `InteractiveElement`, all public API (click, fill, select, scroll, navigate, extract, etc.)
- `src/observe.rs` — JS injection that enumerates interactive DOM elements, returns them as JSON
- `src/annotate.rs` — Injects numbered red overlay labels, takes screenshot, cleans up
- `src/target.rs` — Smart targeting with live resolution (text:, placeholder:, css:, id:, role:)
- `src/spa/` — SPA router detection and navigation (React Router, Next.js, Vue Router, etc.)
- `src/main.rs` — MCP server binary entry point
- `src/mcp/` — MCP server implementation (mod.rs, types.rs, state.rs, error.rs, helpers.rs)
- `examples/demo.rs` — End-to-end demo (form fill, screenshot, extraction)

## Dependencies

- `eoka` (0.3.4) — CDP-based browser automation (Page, Browser, stealth, mouse/keyboard, tab management)
- `rmcp` — MCP server framework (stdio transport)
- `serde`, `serde_json`, `tokio`, `schemars`, `anyhow`, `base64`

## Key patterns

- `Session` owns Browser + Page — primary API for library usage
- MCP server manages multiple tabs with `BrowserState` (HashMap of tab ID → TabState), uses raw `Page` directly
- `observe()` runs JS in the page to find all interactive elements, parses the JSON result into `Vec<InteractiveElement>`
- Annotated screenshots inject a temporary DOM overlay, screenshot, then remove it
- Viewport-only filtering is on by default to reduce token count
- CSS selectors are auto-generated for each element and used internally for actions

## Targeting (click, fill, hover, scroll)

**Index (cached):** `0`, `15` — from observe/screenshot, can go stale

**Live (resolved at action time):**
- `text:Submit` or just `Submit` — find by visible text
- `placeholder:Enter code` — find by placeholder
- `css:form button` — CSS selector
- `id:submit-btn` — find by ID
- `role:button` — find by tag/role

Non-numeric targets default to live text search. Indices need cached elements.

## Observe filtering

Reduce token usage with filtered observation:
- `observe(filter: "inputs")` — only form elements
- `observe(filter: "buttons")` — only buttons/links
- `observe(max: 10)` — limit to 10 elements

## Action batching

Execute multiple actions in one call to reduce round trips:
```json
batch([
  { "action": "fill", "target": "placeholder:code", "text": "ABC123" },
  { "action": "click", "target": "text:Submit" }
])
```

## Auto-retry

`click` and `fill` automatically retry once on stale element errors (re-observe and re-resolve).

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

**SPA Navigation:**
- `spa_info` — detect router type (React Router, Next.js, Vue Router, etc.)
- `spa_navigate` — navigate SPA without page reload
- `history_go` — browser history navigation (delta: -1=back, 1=forward)

**JavaScript Execution:**
- `extract` — run JS, return result as JSON. `js=` inline or `file=` absolute path
- `exec` — run JS for side effects, no return. `js=` inline or `file=` absolute path

**Console/Errors:**
- `console` — read captured console output (log/warn/error/info/debug). Auto-injects capture on first call. `clear?`, `level?`
- `errors` — read captured JS errors and unhandled rejections. Auto-injects on first call. `clear?`

**State Persistence:**
- `save_state(path)` — save cookies + localStorage + sessionStorage to JSON file (captures httpOnly via CDP)
- `load_state(path, navigate?)` — restore saved state, optionally navigate to saved URL first

**Other:**
`cookies`, `set_cookie`, `close`

### Setup

```sh
# Add to Claude Code
claude mcp add eoka-mcp -- cargo run --manifest-path /Users/cbass/Code/eoka-tools/crates/eoka-mcp/Cargo.toml

# Or after cargo install
claude mcp add eoka-mcp -- eoka-mcp
```

## Build & run

```sh
cargo build -p eoka-mcp
cargo run -p eoka-mcp --example demo
cargo run -p eoka-mcp  # start MCP server
```

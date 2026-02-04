# eoka-agent

AI agent interaction layer for browser automation. Use directly in Rust or via MCP server.

Part of the [eoka-tools](https://github.com/cbxss/eoka-tools) workspace.

## Quick Start

```rust
use eoka_agent::Session;

let mut session = Session::launch().await?;
session.goto("https://example.com").await?;

// Observe → act by index → repeat
session.observe().await?;
println!("{}", session.element_list());
session.click(0).await?;

session.close().await?;
```

## Features

- **observe()** — enumerate all interactive elements with Shadow DOM support
- **element_list()** — compact text format for LLM consumption: `[0] <button> "Submit"`
- **screenshot()** — annotated PNG with numbered red boxes on each element
- **Index-based actions** — `click(i)`, `fill(i, text)`, `select(i, value)`, `hover(i)`
- **Auto-wait** — actions wait for network idle and DOM stability
- **Stale detection** — detects moved/removed elements with helpful error messages

## Element List Format

```
[0] <input type="text"> placeholder="Customer name"
[1] <input type="tel"> placeholder="Telephone"
[2] <button> "Submit"
```

## MCP Server

The crate includes an MCP server binary for use with Claude Desktop, Claude Code, etc.

### Setup

```sh
# Install
cargo install eoka-agent

# Add to Claude Code
claude mcp add eoka-agent -- eoka-agent
```

### Tools

| Tool | Description |
|------|-------------|
| `navigate` | Go to URL (launches browser on first call) |
| `observe` | List all interactive elements |
| `screenshot` | Annotated screenshot with numbered elements |
| `click` | Click element by index |
| `fill` | Type into input by index |
| `select` | Select dropdown option by index |
| `hover` | Hover over element by index |
| `scroll` | Scroll page or element into view |
| `type_key` | Press keyboard key (Enter, Tab, etc.) |
| `find_text` | Search elements by text content |
| `extract` | Run JavaScript and return result |
| `page_text` | Get visible text content |
| `page_info` | Get current URL and title |
| `cookies` | Get all cookies |
| `set_cookie` | Set a cookie |
| `back` / `forward` | Browser history navigation |
| `close` | Close browser |

## Examples

```sh
cargo run -p eoka-agent --example demo
```

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

## Live Targeting

Actions support both index-based and live targeting:

```
click(target: "2")              // By index from observe()
click(target: "text:Submit")    // By visible text
click(target: "css:button.primary")  // By CSS selector
click(target: "id:submit-btn")  // By element ID
click(target: "placeholder:Email")   // By placeholder text
click(target: "role:button")    // By tag or ARIA role
```

Live targets resolve at action time via JS injection, avoiding stale element issues in dynamic pages.

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

**Tab Management:**
| Tool | Description |
|------|-------------|
| `list_tabs` | List all open tabs with IDs, titles, URLs |
| `new_tab` | Open new tab (optionally with URL) |
| `switch_tab` | Switch to tab by ID |
| `close_tab` | Close tab by ID |

**Navigation:**
| Tool | Description |
|------|-------------|
| `navigate` | Go to URL (launches browser on first call) |
| `back` / `forward` | Browser history navigation |
| `spa_info` | Detect SPA router (React, Next.js, Vue, etc.) |
| `spa_navigate` | Navigate SPA without page reload |

**Observation:**
| Tool | Description |
|------|-------------|
| `observe` | List interactive elements (filter by type, limit count) |
| `screenshot` | Annotated screenshot with numbered elements |
| `find_text` | Search elements by text content |
| `page_text` | Get visible text content |
| `page_info` | Get current URL and title |

**Actions (support live targeting: `text:Submit`, `css:button`, `id:btn`):**
| Tool | Description |
|------|-------------|
| `click` | Click element by index or live target |
| `fill` | Type into input field |
| `select` | Select dropdown option |
| `hover` | Hover over element |
| `scroll` | Scroll page or element into view |
| `type_key` | Press keyboard key (Enter, Tab, etc.) |
| `batch` | Execute multiple actions in one call |

**Other:**
| Tool | Description |
|------|-------------|
| `extract` | Run JavaScript and return result |
| `cookies` | Get all cookies |
| `set_cookie` | Set a cookie |
| `close` | Close browser |

## Examples

```sh
cargo run -p eoka-agent --example demo
```

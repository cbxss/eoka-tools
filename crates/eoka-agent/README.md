# eoka-agent

AI agent interaction layer for browser automation. Rust library + MCP server.

Part of the [eoka-tools](https://github.com/cbxss/eoka-tools) workspace.

## Quick Start

```rust
use eoka_agent::Session;

let mut session = Session::launch().await?;
session.goto("https://example.com").await?;

session.observe().await?;
println!("{}", session.element_list());
session.click(0).await?;

session.close().await?;
```

## Features

- **Observe/act loop** — `observe()` enumerates interactive elements, act by index
- **Annotated screenshots** — numbered red boxes on each element
- **Auto-wait + stale detection** — actions wait for stability, detect moved/removed elements
- **Live targeting** — `text:Submit`, `css:button.primary`, `id:btn`, `placeholder:Email`
- **SPA support** — detect and navigate React Router, Next.js, Vue Router, etc.
- **Extract** — run JS expressions and get typed results back

## MCP Server

```sh
cargo install eoka-agent
claude mcp add eoka-agent -- eoka-agent
```

44 tools: tab management, navigation, observe/screenshot, click/fill/select/hover/scroll, SPA navigation, JS execution, cookies, batch actions.

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
- **CAPTCHA solving** — hCaptcha, reCAPTCHA v2/v3, and AWS WAF via Anti-Captcha

## MCP Server

```sh
cargo install eoka-agent
claude mcp add eoka-agent -- eoka-agent
```

Includes CAPTCHA tools. For AWS WAF, provide `website_key`, `iv`, and `context` from `window.gokuProps`; use the returned `token` as the `aws-waf-token` cookie and retain the returned `user_agent`.

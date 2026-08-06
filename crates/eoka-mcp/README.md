# eoka-mcp

Stdio MCP server for Eoka browser automation. The MCP binary supports MCP `2026-07-28` with stateless `server/discover` requests. Shared browser primitives and the Rust `Session` API live in `eoka-server`.

Part of the [eoka-tools](https://github.com/shrimp-software/eoka-tools) workspace.

## MCP Server

```sh
cargo install eoka-mcp
claude mcp add eoka -- eoka-mcp
```

Includes CAPTCHA tools. For AWS WAF, provide `website_key`, `iv`, and `context` from `window.gokuProps`; use the returned `token` as the `aws-waf-token` cookie and retain the returned `user_agent`.

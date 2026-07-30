# eoka-tools

Companion tools for [eoka](https://github.com/cbxss/eoka), the low-level CDP browser automation library.

## Components

| Component | Purpose |
|---|---|
| [**eoka-mcp**](crates/eoka-mcp) | Stdio MCP server and Rust `Session` API with observe/act browser tools |
| [**eoka-cli**](crates/eoka-cli) | Interactive shell CLI for browser automation and debugging |
| [**eoka-server**](crates/eoka-server) | Shared browser runtime used by eoka-mcp and the [Go client](clients/go) |
| [**eoka-runner**](crates/eoka-runner) | Declarative YAML automation runner |
| [**eoka-captcha**](crates/captcha) | Optional Anti-Captcha integrations |
| [**eoka-email**](crates/eoka-email) | IMAP helpers for OTP and verification-link flows |
| [**eoka-proxy**](crates/eoka-proxy) | Shared proxy parsing and configuration |

## Choose an interface

- Use `eoka-mcp` when an MCP client needs browser tools.
- Use `eoka-cli` for interactive exploration and debugging.
- Use `eoka-runner` for versioned, repeatable YAML workflows.
- Use the Go client and `eoka-server` when embedding browser automation in a Go service.

## MCP quick start

```sh
cargo install eoka-mcp
claude mcp add eoka -- eoka-mcp
```

The MCP server communicates over standard input and output. It supports MCP `2026-07-28` through stateless `server/discover` requests while retaining compatibility with legacy stdio clients. It creates and closes its browser through the shared eoka-server runtime for the lifetime of the MCP connection.

## Rust session API

```rust
use eoka_mcp::Session;

let mut session = Session::launch().await?;
session.goto("https://example.com").await?;
session.observe().await?;
session.click(0).await?;
session.close().await?;
```

## YAML runner quick start

```yaml
name: Example
target:
  url: https://example.com
actions:
  - click:
      text: More information
  - screenshot:
      path: result.png
```

```sh
cargo install eoka-runner
eoka-runner automation.yaml
```

## Go client

```sh
go get github.com/shrimp-software/eoka-tools/clients/go
```

The client starts `eoka-server` and can download a verified prebuilt server binary automatically. See the [Go client README](clients/go/README.md) for usage and supported platforms.

## Development

```sh
cargo test --workspace
cd clients/go && go test ./...
```

## Documentation

- [eoka-mcp](crates/eoka-mcp/README.md)
- [eoka CLI skill](crates/eoka-cli/SKILL.md)
- [eoka-runner](crates/eoka-runner/README.md)
- [Go client](clients/go/README.md)
- [server protocol](PROTOCOL.md)

## License

MIT

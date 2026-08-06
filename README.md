# eoka-tools

Companion tools for [eoka](https://github.com/shrimp-software/eoka), the low-level CDP browser automation library.

## Components

| Component | Purpose |
|---|---|
| [**eoka-mcp**](crates/eoka-mcp) | Stdio MCP server for browser tools |
| [**eoka-cli**](crates/eoka-cli) | Interactive shell CLI for browser automation and debugging |
| [**eoka-server**](crates/eoka-server) | Shared browser runtime, `Session` API, and browser helper primitives |
| [**eoka-protocol**](crates/eoka-protocol) | Shared daemon protocol types and tool catalog metadata |
| [**eoka-sdk**](crates/eoka-sdk) | Rust SDK for typed Eoka browser sessions |
| [**eoka-tack**](crates/eoka-tack) | Tack `ToolSet` adapter generated from the Eoka protocol catalog |
| [**eoka-runner**](crates/eoka-runner) | Declarative YAML automation runner |
| [**eoka-captcha**](crates/captcha) | Optional Anti-Captcha integrations |
| [**eoka-email**](crates/eoka-email) | IMAP helpers for OTP and verification-link flows |
| [**eoka-proxy**](crates/eoka-proxy) | Shared proxy parsing and configuration |

## Choose an interface

- Use `eoka-mcp` when an MCP client needs browser tools.
- Use `eoka-cli` for interactive exploration and debugging.
- Use `eoka tack` when an agent needs Tack TypeScript execution
  against the active browser session.
- Use `eoka-runner` for versioned, repeatable YAML workflows.
- Use the Go client and `eoka-server` when embedding browser automation in a Go service.

## MCP quick start

```sh
cargo install eoka-mcp
claude mcp add eoka -- eoka-mcp
```

The MCP server communicates over standard input and output. It supports MCP `2026-07-28` through stateless `server/discover` requests. It creates and closes its browser through the shared eoka-server runtime for the lifetime of the MCP connection.

## Rust session API

```rust
use eoka_server::Session;

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
mise run conformance
mise run release-check
cd clients/go && go test ./...
```

Without mise:

```sh
cargo test --workspace
bash scripts/release-check.sh
```

Use the local Tack override when testing unpublished Tack changes from a sibling
checkout:

```sh
bash scripts/release-check.sh --with-local-tack
```

Install the local CLI from this checkout:

```sh
mise run install-cli
```

Inspect the generated protocol-backed tool catalog:

```sh
eoka tools manifest --json
```

Run Tack TypeScript against the active browser session:

```sh
eoka tack 'return await tools.invoke({ path: "eoka.info", input: {} });'
```

## Documentation

- [eoka-mcp](crates/eoka-mcp/README.md)
- [eoka CLI skill](crates/eoka-cli/SKILL.md)
- [eoka-runner](crates/eoka-runner/README.md)
- [Go client](clients/go/README.md)
- [server protocol](PROTOCOL.md)

Install the Codex skill with the open skills CLI:

```sh
npx skills add https://github.com/shrimp-software/eoka-tools/tree/main/crates/eoka-cli --skill eoka -a codex -y
```

## License

MIT

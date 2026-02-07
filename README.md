# eoka-tools

Browser automation toolkit built on [eoka](https://github.com/cbxss/eoka).

## Crates

| Crate | Description |
|-------|-------------|
| [**eoka-agent**](crates/eoka-agent) | AI agent interaction layer — MCP server, observe/act loop for LLMs |
| [**eoka-email**](crates/eoka-email) | IMAP helpers — OTP codes, verification links, email polling |
| [**eoka-runner**](crates/eoka-runner) | Config-based automation — YAML configs, CLI, scripted execution |

## Architecture

```
eoka (core)              Low-level CDP browser automation
    │
    ├── eoka-agent       AI agent layer (MCP, Session, observe/act)
    │
    └── eoka-runner      Scripted automation (Config, Runner, CLI)
```

## Quick Start

### eoka-agent (for AI/LLM integration)

```rust
use eoka_agent::Session;

let mut session = Session::launch().await?;
session.goto("https://example.com").await?;
session.observe().await?;
println!("{}", session.element_list());
// [0] <a> "More information..."
session.click(0).await?;
```

Or use via MCP server:

```sh
cargo install eoka-agent
claude mcp add eoka-agent -- eoka-agent
```

### eoka-runner (for scripted automation)

```yaml
# automation.yaml
name: "Example"
target:
  url: "https://example.com"
actions:
  - click:
      text: "More information"
  - screenshot:
      path: "result.png"
```

```sh
cargo install eoka-runner
eoka-runner automation.yaml
```

## Installation

```sh
# Both crates
cargo install eoka-agent eoka-runner

# Or from source
git clone https://github.com/cbxss/eoka-tools
cd eoka-tools
cargo install --path crates/eoka-agent
cargo install --path crates/eoka-runner
```

## Examples

```sh
# eoka-agent examples
cargo run -p eoka-agent --example demo
cargo run -p eoka-agent --example agent_loop

# eoka-runner
eoka-runner crates/eoka-runner/configs/example.yaml --check
eoka-runner crates/eoka-runner/configs/example.yaml -v
```

## Documentation

- [eoka-agent README](crates/eoka-agent/README.md) — MCP tools, Session API
- [eoka-runner README](crates/eoka-runner/README.md) — YAML config format, CLI options

## License

MIT

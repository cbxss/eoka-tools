# eoka-tools: Browser Automation Toolkit

## Overview

A collection of browser automation tools built on top of `eoka`. This workspace contains multiple crates for different use cases:

- **eoka-agent** — AI agent interaction layer (MCP server, observe/act loop)
- **eoka-runner** — Config-based scripted automation (YAML configs, CLI)

## Architecture

```
eoka (core)              Low-level CDP browser automation
    │
    ├── eoka-agent       AI agent layer (MCP, Session, observe/act)
    │
    └── eoka-runner      Scripted automation (Config, Runner, CLI)
            │
            └── eoka-swarm   Multi-browser dashboard (imports eoka-runner)
```

## Workspace Structure

```
eoka-tools/
├── Cargo.toml              # workspace root
├── README.md
├── crates/
│   ├── eoka-agent/         # AI agent tools
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs      # Session, AgentPage, InteractiveElement
│   │       ├── observe.rs  # DOM element enumeration
│   │       ├── annotate.rs # Screenshot annotations
│   │       └── mcp.rs      # MCP server
│   │
│   └── eoka-runner/        # Config-based automation
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── config/
│           │   ├── mod.rs
│           │   ├── schema.rs   # Config structs
│           │   ├── actions.rs  # Action enum
│           │   └── parser.rs   # YAML parsing
│           ├── runner/
│           │   ├── mod.rs
│           │   ├── executor.rs # Action execution
│           │   └── result.rs   # Success/failure
│           └── bin/
│               └── main.rs     # CLI binary
│
└── configs/                # Example configs
    ├── example.yaml
    └── bestbuy.yaml
```

---

# eoka-agent

Current codebase. AI agent interaction layer for LLM-driven browser automation.

**Use case**: Claude/GPT observes page → reasons about elements → decides action → executes

**Key types**:
- `Session` — owns browser, provides observe/act API
- `AgentPage` — wraps Page with element indexing
- `InteractiveElement` — indexed element with selector, text, bbox

**MCP tools**: navigate, observe, screenshot, click, fill, select, etc.

---

# eoka-runner

Config-based scripted automation. Define browser actions in YAML, execute deterministically.

**Use case**: Predefined automation flows (add to cart, login, form fill) without LLM

## Config Format

```yaml
name: "Best Buy Add to Cart"

browser:
  headless: false
  proxy: null
  user_agent: null

target:
  url: "https://www.bestbuy.com/product/..."

actions:
  - wait_for_network_idle:
      idle_ms: 1000
      timeout_ms: 15000

  - try_click_any:
      texts: ["Accept", "Close", "No Thanks"]

  - click:
      text: "Add to Cart"
      human: true
      scroll_into_view: true

  - wait_for_text:
      text: "Added to Cart"
      timeout_ms: 5000

  - screenshot:
      path: "cart_success.png"

success:
  any:
    - url_contains: "/cart"
    - text_contains: "item in cart"

on_failure:
  screenshot: "error_{timestamp}.png"
  retry:
    attempts: 3
    delay_ms: 2000
```

## Action Types

### Navigation
- `goto: { url }` — Navigate to URL
- `back` — Browser back
- `forward` — Browser forward
- `reload` — Refresh page

### Waiting
- `wait: { ms }` — Fixed delay
- `wait_for_network_idle: { idle_ms, timeout_ms }`
- `wait_for: { selector, timeout_ms }` — Wait for element
- `wait_for_visible: { selector, timeout_ms }`
- `wait_for_hidden: { selector, timeout_ms }`
- `wait_for_text: { text, timeout_ms }`
- `wait_for_url: { contains, timeout_ms }`

### Clicking
- `click: { selector | text, human, scroll_into_view }`
- `try_click: { selector | text }` — No error if missing
- `try_click_any: { texts }` — Click first found

### Input
- `fill: { selector | text, value, human }` — Clear and type
- `type: { selector | text, value }` — Append text
- `clear: { selector }`

### Scrolling
- `scroll: { direction, amount }`
- `scroll_to: { selector | text }`

### Debug
- `screenshot: { path }`
- `log: { message }`
- `assert_text: { text }`
- `assert_url: { contains }`

### Control Flow
- `if_text_exists: { text, then, else }`
- `if_selector_exists: { selector, then, else }`
- `repeat: { times, actions }`

## CLI

```bash
# Run config
eoka-runner bestbuy.yaml

# Options
eoka-runner bestbuy.yaml --headless --verbose

# Validate only
eoka-runner --check bestbuy.yaml
```

## Library API

```rust
use eoka_runner::{Config, Runner};

let config = Config::load("bestbuy.yaml")?;
let mut runner = Runner::new(&config.browser).await?;
let result = runner.run(&config).await?;

// Access page for swarm integration
let page = runner.page();
```

---

## Implementation Plan

### Phase 0: Workspace Setup ✓
1. [x] Convert to workspace with root Cargo.toml
2. [x] Move current code to `crates/eoka-agent/`
3. [x] Create `crates/eoka-runner/` skeleton
4. [x] Verify both crates build

### Phase 1: eoka-runner Config System ✓
5. [x] Config schema structs (`config/schema.rs`)
6. [x] Action enum with all variants (`config/actions.rs`)
7. [x] YAML parser with custom deserialization
8. [x] Unit tests for config parsing (16 tests)

### Phase 2: eoka-runner Executor ✓
9. [x] Runner struct with browser management
10. [x] Action executor — all 24 action types implemented
11. [x] Success/failure condition checking
12. [x] Error handling and retry logic with on_failure support

### Phase 3: CLI ✓
13. [x] CLI binary with clap
14. [x] Verbose/debug output modes (-v, -vv, -q)
15. [ ] Xvfb auto-detection (Linux) — deferred

### Phase 4: Polish ✓
16. [x] Example config (configs/example.yaml)
17. [x] Documentation (README.md for workspace + each crate)
18. [x] Integration tests (moved to crates/eoka-agent/tests/)

---

## Dependencies

### eoka-agent (current)
```toml
[dependencies]
eoka = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
rmcp = { version = "0.1", features = ["server", "transport-io"] }
schemars = "1"
anyhow = "1"
base64 = "0.22"
```

### eoka-runner (new)
```toml
[dependencies]
eoka = "0.3"
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
tokio = { version = "1", features = ["full"] }
thiserror = "2"
tracing = "0.1"
clap = { version = "4", features = ["derive"] }

[dev-dependencies]
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

---

## Notes

- eoka-agent and eoka-runner are independent — neither depends on the other
- Both depend directly on `eoka` core
- Human-like actions (mouse movement, typing delay) should be default in runner
- Consider variable substitution later (`${ENV_VAR}`)

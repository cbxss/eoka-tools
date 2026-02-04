# eoka-runner

Config-based browser automation. Define actions in YAML, execute deterministically.

Part of the [eoka-tools](https://github.com/cbxss/eoka-tools) workspace.

## Quick Start

```yaml
# automation.yaml
name: "Example"
target:
  url: "https://example.com"
actions:
  - wait_for_network_idle:
      timeout_ms: 5000
  - click:
      text: "More information"
  - screenshot:
      path: "result.png"
success:
  any:
    - url_contains: "/info"
```

```sh
eoka-runner automation.yaml
```

## Features

- **YAML configs** — define automation flows without writing code
- **Parameterized configs** — `${variable}` substitution for reusable flows
- **30 action types** — navigation, clicking, input, scrolling, cookies, JS, conditionals
- **Text targeting** — click by visible text, not just CSS selectors
- **Human-like actions** — optional mouse movement and typing delays
- **Retry logic** — automatic retries with configurable delay
- **Success conditions** — verify URL or text content after completion
- **Failure screenshots** — capture state on error for debugging

## CLI Usage

```sh
# Run config
eoka-runner config.yaml

# Run with parameters
eoka-runner login.yaml -P email=user@example.com -P password=secret

# Validate without running
eoka-runner config.yaml --check

# Verbose output
eoka-runner config.yaml -v      # info level
eoka-runner config.yaml -vv     # debug level

# Override headless mode
eoka-runner config.yaml --headless

# Quiet (errors only)
eoka-runner config.yaml -q
```

## Config Format

```yaml
name: "Automation Name"

# Parameters (optional) - for reusable configs
params:
  email:
    required: true
    description: "User email"
  timeout:
    default: "5000"
    description: "Wait timeout"

browser:
  headless: false
  proxy: "http://user:pass@host:port"  # optional
  user_agent: "Custom UA"               # optional
  viewport:                             # optional
    width: 1920
    height: 1080

target:
  url: "https://example.com"

actions:
  # Use ${param_name} for substitution
  - fill:
      selector: "#email"
      value: "${email}"

success:
  any:  # OR conditions
    - url_contains: "/success"
    - text_contains: "Thank you"
  # or use 'all' for AND conditions

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
- `clear: { selector | text }` — Clear input field
- `select: { selector | text, value }` — Select dropdown option
- `press_key: { key }` — Press key (Enter, Tab, Escape, ArrowDown, etc.)

### Mouse
- `hover: { selector | text }` — Hover over element

### Cookies
- `set_cookie: { name, value, domain?, path? }` — Set a cookie
- `delete_cookie: { name, domain? }` — Delete a cookie

### JavaScript
- `execute: { js }` — Run arbitrary JavaScript

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

### Composition
- `include: { path, params? }` — Include another config's actions

## Reusable Flows with Include

Create reusable building blocks:

```yaml
# flows/dismiss_cookies.yaml
name: "Dismiss Cookies"
target:
  url: "about:blank"
actions:
  - try_click_any:
      texts: ["Accept", "Accept All", "OK", "Got it"]
```

```yaml
# flows/login.yaml
name: "Login"
params:
  email: { required: true }
  password: { required: true }
target:
  url: "about:blank"
actions:
  - fill: { selector: "#email", value: "${email}" }
  - fill: { selector: "#password", value: "${password}" }
  - click: { text: "Sign In" }
```

Compose them in your main config:

```yaml
name: "Checkout Flow"
target:
  url: "https://shop.example.com"
actions:
  - include: { path: "flows/dismiss_cookies.yaml" }
  - include:
      path: "flows/login.yaml"
      params:
        email: "user@example.com"
        password: "secret"
  - click: { text: "Checkout" }
```

Include paths are relative to the config file's directory.

## Library Usage

```rust
use eoka_runner::{Config, Params, Runner};

// Simple usage
let config = Config::load("automation.yaml")?;
let mut runner = Runner::new(&config.browser).await?;
let result = runner.run(&config).await?;
println!("Success: {}", result.success);
runner.close().await?;

// With parameters
let params = Params::new()
    .set("email", "user@example.com")
    .set("password", "secret");
let config = Config::load_with_params("login.yaml", &params)?;
```

## Examples

See the `configs/` directory in this crate for example YAML configs.

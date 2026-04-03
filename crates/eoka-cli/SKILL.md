# eoka — Browser Automation CLI

Fast browser automation CLI built on the eoka CDP engine. Client-daemon architecture: the first command auto-launches a background daemon that keeps Chrome alive, making subsequent commands instant (~10ms).

## Install

```bash
cargo install --path crates/eoka-cli
# Or run from workspace:
cargo run -p eoka-cli -- <command>
```

## Core Workflow

```bash
eoka open https://example.com          # Navigate (launches browser on first call)
eoka snapshot -i                        # Get accessibility tree with @e1, @e2 refs
eoka click @e2                          # Click by ref from snapshot
eoka fill @e3 "test@example.com"        # Fill input by ref
eoka screenshot -o page.png             # Take screenshot
eoka close                              # Close browser and daemon
```

## Selectors (Target Syntax)

All action commands (`click`, `fill`, `hover`, `scroll`, `select`) accept these target formats:

| Format | Example | Description |
|--------|---------|-------------|
| Ref | `@e1` | From snapshot, resolves via CDP |
| Index | `0`, `5` | From observe, uses cached elements |
| Text | `text:Submit` | Find by visible text |
| Placeholder | `placeholder:Email` | Find by placeholder |
| CSS | `css:#submit-btn` | CSS selector |
| ID | `id:login` | Find by element ID |
| Role | `role:button` | Find by ARIA role |
| Bare text | `"Submit"` | Defaults to text search |

## Commands

### Navigation

```bash
eoka open <url> [--headers '{"Auth": "Bearer ..."}'] [--bypass-csp] [--user-agent UA]
eoka back
eoka forward
eoka reload
```

### Observation (for AI agents)

```bash
eoka snapshot                           # Full accessibility tree with @eN refs
eoka snapshot -i                        # Interactive elements only
eoka observe                            # List elements with indices
eoka observe --filter inputs --max 10   # Only form elements, limit 10
eoka screenshot -o page.png             # Plain screenshot
eoka screenshot --annotate -o page.png  # Annotated with numbered labels
eoka info                               # Current URL and title (JSON)
eoka text                               # All visible text
eoka find "search term"                 # Find elements by text
```

### Actions

```bash
eoka click @e1                          # Click element
eoka fill @e3 "hello"                   # Clear and type into input
eoka select @e5 "Option B"             # Select dropdown option
eoka hover @e2                          # Hover (trigger tooltips/menus)
eoka key Enter                          # Press keyboard key
eoka scroll down                        # Scroll: up/down/top/bottom or element target
eoka double-click @e4                   # Double-click
```

### JavaScript

```bash
eoka eval "document.title"              # Execute JS, return result
eoka eval -f ./script.js                # Execute from file
eoka eval -f ./big_script.js --no-return  # Fire-and-forget (avoids CDP crash on large results)
eoka eval -f ./script.js --max-size 4096  # Truncate result to 4KB (prevents CDP size crash)
eoka exec "localStorage.clear()"       # Execute for side effects (no return)
```

Use `--no-return` or `--max-size` when evaluating scripts that produce large results (e.g. dumping buffers, DOM trees). Without these, CDP can crash on responses exceeding ~10MB.

### Network (Pentesting)

```bash
# Fetch from browser context — uses real cookies, TLS fingerprint, bypasses Cloudflare
eoka fetch https://api.target.com/me
eoka fetch https://api.target.com/data -m POST --headers '{"Content-Type": "application/json"}' -b '{"key": "value"}'
eoka fetch https://target.com/oauth --redirect manual   # Capture redirect URL
```

### Cookies

```bash
eoka cookies                            # Get all cookies (JSON)
eoka set-cookie session abc123 --domain .target.com
eoka delete-cookie tracking
eoka clear-cookies
```

### Storage

```bash
eoka storage                            # All localStorage
eoka storage token                      # Get specific key
eoka storage --session-storage          # sessionStorage instead
eoka set-storage theme dark
eoka dump-storage                       # Both local and session storage
```

### State Persistence

```bash
eoka save-state ./auth.json             # Save cookies + storage + URL
eoka load-state ./auth.json             # Restore and navigate to saved URL
eoka load-state ./auth.json --no-navigate  # Restore without navigating
```

### Extra Headers

```bash
eoka headers '{"Authorization": "Bearer eyJ...", "X-Custom": "value"}'
```

### Console / Errors

```bash
eoka console                            # Read console output
eoka console --level error              # Filter by level
eoka console --clear                    # Clear after reading
eoka errors                             # Read uncaught JS errors
```

### Tabs

```bash
eoka tab list                           # List tabs (* = current)
eoka tab new https://other.com          # Open new tab
eoka tab switch <tab-id>                # Switch to tab
eoka tab close <tab-id>                 # Close tab
```

### Wait

```bash
eoka wait 2000                          # Wait 2 seconds
eoka wait --text "Welcome"              # Wait for text to appear
eoka wait --url "**/dashboard"          # Wait for URL pattern
eoka wait --load networkidle            # Wait for network idle
```

### Fake Camera

Inject a fake video stream into `getUserMedia` — replaces the real camera with frames from a video file. No OS virtual cam driver needed; works at the JS level.

```bash
eoka fake-camera /path/to/face.mp4              # Inject fake camera from video
eoka fake-camera /path/to/face.webm --loop      # Loop the video
eoka open https://app-with-camera.com            # Page sees fake stream from getUserMedia
eoka eval "navigator.mediaDevices.getUserMedia({video:true}).then(s => s.getVideoTracks().length)"
# Returns 1
```

Internally: overrides `navigator.mediaDevices.getUserMedia`, draws video frames to a hidden canvas via `requestAnimationFrame`, returns `canvas.captureStream(30)`. Also fakes `enumerateDevices` to report a camera and grants camera permissions via CDP. Video is base64-encoded inline (works for files up to ~5MB). Persists across navigations via `addScriptToEvaluateOnNewDocument`.

### WASM Memory

Direct access to WebAssembly linear memory without eval gymnastics. Auto-detects memory exports (`window.__ft.mem`, `Module.wasmMemory`, or scans globals).

```bash
eoka wasm info                                   # List detected WASM memory instances
eoka wasm read 0x360000 32                       # Hex dump 32 bytes at address
eoka wasm read 0x360000 1024 --memory "window.__ft.mem"  # Explicit memory path
eoka wasm write 0x360000 deadbeef                # Write bytes at address
eoka wasm write 0x360000 "ff d8 ff e0"           # Spaces in hex are stripped
eoka wasm find "ff d8 ff e0"                     # Search for JPEG magic bytes
eoka wasm find "00 00 00 01" --start 0x300000 --end 0x400000 --max 5
```

Reads are chunked (64KB) to avoid CDP message size limits. Output is formatted as hex dump (xxd-style). Addresses support `0x` hex prefix or decimal.

### Network Interception

Capture and modify HTTP requests using the CDP Fetch domain. Useful for intercepting encrypted API payloads, replaying modified requests, or mocking responses.

```bash
eoka intercept add "*/api/data*"                            # Log matching requests
eoka intercept add "*/biometrics/*" --capture /tmp/req.json  # Capture request body to file
eoka intercept add "*/config" --respond /tmp/mock.json --status 200  # Mock response from file
eoka intercept list                                          # List active rules
eoka intercept log                                           # Show intercepted request log
eoka intercept log --clear                                   # Show and clear log
eoka intercept remove 1                                      # Remove rule by ID
eoka intercept remove all                                    # Remove all rules, disable interception
```

URL patterns use glob matching (`*` matches any sequence). When `--capture` is set, the full request (URL, method, headers, postData) is saved as JSON. When `--respond` is set, the file contents are returned instead of forwarding the request. Events are processed before each CLI command.

### SPA Navigation

```bash
eoka spa-info                           # Detect router (React, Next.js, Vue, etc.)
eoka spa-navigate /dashboard            # Navigate without page reload
```

## Sessions

Run multiple isolated browsers:

```bash
eoka --session pentest1 open https://target-a.com
eoka --session pentest2 open https://target-b.com
eoka --session pentest1 snapshot        # Interacts with first browser
```

## Options

| Flag | Description |
|------|-------------|
| `--session <name>` | Isolated browser session (default: "default") |
| `--json` | JSON output mode (for agent integration) |
| `--headed` | Show browser window (default: headless) |

## Environment Variables

| Variable | Description |
|----------|-------------|
| `EOKA_HEADLESS` | `false` or `0` to show browser window |
| `EOKA_PROXY` | Proxy: `host:port` or `host:port:user:pass` |
| `EOKA_PROXY_FILE` | File with proxies (one per line, random selection) |
| `EOKA_PATCH_BINARY` | `true` to apply stealth binary patches |
| `EOKA_CHROME_ARGS` | Extra Chrome flags, colon-separated (e.g. `--use-fake-ui-for-media-stream:--allow-insecure-localhost`) |
| `EOKA_IDLE_TIMEOUT` | Daemon idle timeout in ms (default: 1800000 = 30min) |

## Daemon Management

```bash
eoka status                             # Check if daemon is running
eoka kill                               # Force-kill daemon
eoka close                              # Graceful close (browser + daemon)
```

The daemon auto-starts on first command and auto-exits after 30 minutes of inactivity. Logs are at `/tmp/eoka/eoka-<session>.log`.

## AI Agent Integration

Use `--json` for structured output:

```bash
eoka snapshot -i --json                 # Returns JSON with tree + refs
eoka info --json                        # Returns {"url": "...", "title": "..."}
eoka cookies --json                     # Returns cookie array
```

### Optimal AI Workflow

```bash
# 1. Navigate and observe
eoka open https://target.com
eoka snapshot -i --json                 # Parse refs

# 2. Interact using refs
eoka click @e2
eoka fill @e3 "input text"

# 3. Re-snapshot after page changes
eoka snapshot -i --json

# 4. Chain commands for efficiency
eoka fill @e1 "user" && eoka fill @e2 "pass" && eoka click @e3
```

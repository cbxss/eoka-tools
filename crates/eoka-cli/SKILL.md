---
name: eoka
description: "Drive a real Chrome browser from the shell via the eoka CLI. Persistent daemon keeps Chrome alive between commands (~10ms per call). Use for browser automation, protected-site sessions and AWS WAF CAPTCHA solving, headless screenshots, CDP fetch interception, WASM memory, fake-camera injection, and SPA navigation. Triggers on: eoka, browser cli, AWS WAF, captcha, anti-captcha, headless screenshot, intercept request, fake camera, wasm memory, cdp fetch, browser daemon, eoka snapshot, eoka click, eoka fill."
---

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
| Index | `[0]`, `0`, `index:0` | From observe's full interactive list, not DOM `querySelectorAll()` indices |
| Text | `text:Submit` | Find by visible text |
| Placeholder | `placeholder:Email` | Find by placeholder |
| CSS | `css:#submit-btn` | CSS selector |
| ID | `id:login` | Find by element ID |
| Role | `role:button` | Find by ARIA role |
| Bare text | `"Submit"` | Defaults to text search |

## Commands

### Navigation

```bash
eoka open <url> [--headers '{"Auth": "Bearer ..."}'] [--bypass-csp] [--user-agent UA] [--load-state ./auth.json]
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
eoka fetch https://target.com/app.js --body-only          # Print only response body for grep/sed/awk
```

### Cookies

```bash
eoka cookies                            # Get all cookies (JSON)
eoka set-cookie session abc123 --domain .target.com
eoka delete-cookie tracking
eoka clear-cookies
```

### CAPTCHA

For install and AWS WAF usage, read [references/captcha.md](references/captcha.md).

```bash
eoka captcha solve --captcha-type recaptcha_v3 --website-url https://target.com --website-key SITE_KEY --page-action submit
eoka captcha inject TOKEN --captcha-type recaptcha
eoka captcha solve --captcha-type recaptcha_v3 --website-url https://target.com --website-key SITE_KEY --page-action submit --inject
eoka captcha inject TOKEN --captcha-type recaptcha --click-after "text:Continue Booking"
```

`captcha inject` sets common response fields and calls discovered grecaptcha/hCaptcha callbacks. Use `--callback window.someCallback` when the page exposes a specific callback. Some pages do not automatically retry after a callback fires; use `--click-after` to click the continuation control after injection.

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
eoka load-state ./auth.json             # Restore, navigate to saved URL, then reload so app auth initializes
eoka load-state ./auth.json --no-navigate  # Restore into current origin; reloads current web page when possible
eoka open /camping/campsites/71576 --load-state ./auth.json  # Restore before this deep-link navigation
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

### Script Blocking (NoScript-style)

Disable JS execution by default and allow it only on specific domains — the
same idea as the NoScript extension, applied per top-level navigation (CDP
has no per-frame equivalent, so a third-party iframe on an allowed page runs
JS too; there's no way to allow the main frame while blocking an embedded
one).

```bash
eoka --no-js open https://target.com                   # start blocked (Safest-style)
eoka js allow example.com                               # exception: JS runs here (and subdomains)
eoka js mode allow-all                                  # flip default: run JS everywhere...
eoka js block evil.example                               # ...except this domain
eoka js remove example.com                               # drop an allow/block exception
eoka js list                                             # current mode + exception lists
```

`--js-allow <DOMAIN>` / `--js-block <DOMAIN>` (repeatable) seed exceptions at
launch instead of adding them after the fact. All of `--no-js`,
`--js-allow`, `--js-block` are ignored in `--cdp`/`--auto-connect` mode,
same as `--proxy`/`--headed`/etc. — eoka never touches your own live tabs.

The policy is re-evaluated on `open` and `reload` (against the destination
and current URL respectively). `back`/`forward` don't re-evaluate it — CDP
doesn't expose the destination URL before they complete, so they carry
whatever was last applied.

## Sessions

Run multiple isolated browsers — each `--session` name gets its own daemon,
Chrome instance, and Unix socket, so they never share state:

```bash
eoka --session pentest1 open https://target-a.com
eoka --session pentest2 open https://target-b.com
eoka --session pentest1 snapshot        # Interacts with first browser
```

Combine with `--proxy` to run one session through a proxy and another
direct:

```bash
eoka --session tor --proxy socks5://127.0.0.1:9050 open https://check.torproject.org
eoka --session regular open https://example.com
```

List every session (running or stale), like `tmux ls`:

```bash
eoka sessions                           # or: eoka ls
eoka --json sessions                    # structured output
```

## Driving Your Real Chrome

Three ways to use eoka against an existing Chrome instead of a fresh headless one:

### 1. Attach via CDP (live control)

Start Chrome with remote debugging, then point eoka at it:

```bash
google-chrome --remote-debugging-port=9222 &
eoka --cdp 9222 snapshot                # snapshots the front tab
eoka --cdp 9222 click @e3
eoka --cdp 9222 open https://gmail.com  # opens a NEW tab — your tabs stay
eoka --cdp 9222 close                   # disconnects, leaves Chrome running
eoka --auto-connect snapshot            # find any Chrome on 9222-9229
eoka cdp-url --port 9222                # print ws:// URL (for piping)
```

In CDP mode, eoka:
- skips evasion-script injection (your tabs stay clean),
- disables the stealth CDP filter (full DevTools-equivalent control),
- auto-attaches to the most recent user tab on first command,
- never sends `Browser.close` to a Chrome it doesn't own.

### 2. Clone state from a running Chrome

Snapshot cookies + storage from your real Chrome, then drive a fresh headless
session with that auth:

```bash
# Either save to a file...
eoka clone-from 9222 --to state.json
eoka --state state.json open https://protected-app.com

# ...or hydrate directly into a launched headless session:
eoka --clone-state-from 9222 open https://protected-app.com
```

Captures HttpOnly cookies via CDP (which JS can't reach), localStorage,
sessionStorage, and the User-Agent.

### 3. Clone the profile directory

Copy your Chrome profile to a tempdir and launch headless against the copy.
Picks up encrypted cookies via the OS keyring as the same user:

```bash
eoka --from-profile auto open https://protected-app.com
eoka --from-profile ~/.config/google-chrome/Default open https://x.com
```

Caveats:
- Chrome refuses two instances on one user-data-dir, hence the copy.
- Chrome 127+ on Windows uses App-Bound Encryption keyed to the binary path —
  cookies may silently fail to decrypt if eoka's Chrome isn't the same install.

## Options

| Flag | Description |
|------|-------------|
| `--session <name>` | Isolated browser session (default: "default") |
| `--json` | JSON output mode (for agent integration) |
| `--headed` | Show browser window (default: headless) |
| `--cdp <PORT\|URL>` | Connect to a running Chrome (`9222` or `ws://...`) instead of launching |
| `--auto-connect` | Discover a Chrome on ports 9222–9229 |
| `--clone-state-from <PORT\|URL>` | After launch, hydrate cookies/storage from a running Chrome |
| `--from-profile <auto\|PATH>` | Clone an existing Chrome profile and launch against the copy |
| `--proxy <URL>` | Proxy for the launched browser: `socks5://host:port` or `http://host:port`, optionally with `user:pass@`. Conflicts with `--proxy-file` |
| `--proxy-file <FILE>` | Pick a proxy at random from a file (one per line, `#` comments allowed). Conflicts with `--proxy` |
| `--no-js` | Start with JS execution blocked by default (see Script Blocking) |
| `--js-allow <DOMAIN>` | Seed a JS allow exception at launch. Repeatable |
| `--js-block <DOMAIN>` | Seed a JS block exception at launch. Repeatable |

## Environment Variables

| Variable | Description |
|----------|-------------|
| `EOKA_HEADLESS` | `false` or `0` to show browser window |
| `EOKA_PROXY` | Fallback for `--proxy` when the flag isn't passed: `socks5://user:pass@host:port` or `http://user:pass@host:port`; legacy `host:port[:user:pass]` is supported |
| `EOKA_PROXY_FILE` | Fallback for `--proxy-file` when the flag isn't passed |
| `EOKA_NO_JS` | Fallback for `--no-js` when the flag isn't passed |
| `EOKA_PATCH_BINARY` | `true` to apply stealth binary patches |
| `EOKA_CHROME_ARGS` | Extra Chrome flags, colon-separated (e.g. `--use-fake-ui-for-media-stream:--allow-insecure-localhost`) |
| `EOKA_IDLE_TIMEOUT` | Daemon idle timeout in ms (default: 1800000 = 30min) |
| `EOKA_CDP` | Default `--cdp` value (port or ws:// URL) |
| `EOKA_AUTO_CONNECT` | `1` to default to `--auto-connect` |
| `EOKA_FROM_PROFILE` | Default `--from-profile` value (`auto` or path) |

## Daemon Management

```bash
eoka status                             # Check if daemon is running (for --session)
eoka sessions                           # List every session, running or stale (alias: ls)
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

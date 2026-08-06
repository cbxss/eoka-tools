---
name: eoka
description: "Drive a real Chrome browser from the shell with eoka. Use for browser automation, protected sessions, agent-friendly observation, Tack TypeScript tools, CAPTCHA solving, network recording/HAR export/interception, browser-context fetch, state cloning, WASM memory, fake camera, SPA navigation, and daemon/session management."
---

# eoka

Use `eoka` when the task needs a real Chrome browser instead of plain HTTP or a synthetic DOM. Commands auto-start a per-session daemon, so repeated calls are fast and keep browser state.

## Agent Defaults

Prefer `--agent` for machine callers. It implies JSON output, adds `meta`, returns structured errors, and makes `observe` return structured element objects.

```bash
eoka --agent doctor
eoka --agent open https://example.com
eoka --agent observe --max 20
eoka --agent batch '[{"cmd":"observe","args":{"max":5}}]'
```

Use `--json` only when preserving older command-specific JSON shapes matters. `tools manifest --json` returns raw `{ "tools": [...] }`; `--agent tools manifest --json` returns the standard response envelope.

## Core Browser Flow

```bash
eoka open https://example.com
eoka snapshot -i
eoka observe --filter inputs --max 10
eoka click @e2
eoka fill @e3 "test@example.com"
eoka screenshot --annotate -o page.png
eoka close
```

Targets accepted by action commands:

| Target | Example |
| --- | --- |
| snapshot ref | `@e1` |
| observe index | `[0]`, `0`, `index:0` |
| text | `text:Submit` or `Submit` |
| placeholder | `placeholder:Email` |
| CSS | `css:#submit` |
| ID | `id:login` |
| role | `role:button` |

## Automation Tools

```bash
eoka eval "document.title"
eoka eval -f ./script.js --max-size 4096
eoka exec "localStorage.clear()"
eoka wait --text "Welcome"
eoka wait --url "/dashboard"
eoka tab list
eoka tab new https://example.com
```

Use `--no-return` or `--max-size` for JavaScript that may produce large results.

## Tack

`eoka tack` runs Tack TypeScript with Eoka tools registered from `eoka-protocol`.

```bash
eoka tools manifest --json
eoka tack 'return await tools.invoke({ path: "eoka.info", input: {} });'
eoka tack --raw-json 'const hits = await tools.search({ query: "info", limit: 3 }); return hits.data.map(t => t.path);'
eoka tack --capability network --raw-json 'return await tools.search({ query: "network.record", limit: 5 });'
eoka tack --all-tools --raw-json 'return await tools.search({ query: "network", limit: 20 });'
```

Default Tack exposure is agent-safe. Use `--capability <name>` for opt-in groups such as `network`, or `--all-tools` for every non-lifecycle tool.

## Network

```bash
eoka fetch https://api.target.com/me
eoka fetch https://api.target.com/data -m POST --headers '{"Content-Type":"application/json"}' -b '{"key":"value"}'
eoka network record start --pattern "*/api/*" --max-body-bytes 10485760 --clear
eoka network wait --pattern "*/api/*" --status 200 --timeout 10000 --include-existing
eoka network log --limit 50 --compact
eoka network show 12 --body --max-body 65536
eoka network har /tmp/session.har
eoka network export /tmp/session.json --format json
eoka network intercept add "*/api/data*" --capture /tmp/req.json
eoka network intercept add "*/config" --respond /tmp/mock.json --status 200
eoka network intercept log --clear
```

`fetch` runs inside the browser context with active cookies and fingerprint. Network bodies are captured unredacted by default; treat exports as sensitive.

## State, Sessions, And Real Chrome

```bash
eoka save-state ./auth.json
eoka load-state ./auth.json
eoka open /dashboard --load-state ./auth.json
eoka clone-from 9222 --to state.json
eoka --clone-state-from 9222 open https://protected-app.com
eoka --from-profile auto open https://protected-app.com
eoka --cdp 9222 snapshot
eoka --auto-connect info
eoka cdp-url --port 9222
eoka --session work open https://example.com
eoka sessions
eoka status
eoka kill
```

In `--cdp` and `--auto-connect` mode, eoka attaches to an existing Chrome and does not close a browser it does not own.

## CAPTCHA, Media, WASM, SPA

```bash
eoka captcha solve --captcha-type recaptcha_v3 --website-url https://target.com --website-key SITE_KEY --page-action submit
eoka captcha solve --captcha-type recaptcha_v3 --website-url https://target.com --website-key SITE_KEY --inject
eoka captcha inject TOKEN --captcha-type recaptcha --click-after "text:Continue"
eoka fake-camera /path/to/face.webm --loop-video
eoka wasm info
eoka wasm read 0x360000 32
eoka wasm write 0x360000 deadbeef
eoka wasm find "ff d8 ff e0" --max 5
eoka spa-info
eoka spa-navigate /dashboard
```

CAPTCHA solving uses `ANTI_CAPTCHA_KEY` unless `--api-key` is passed.

## Launch Flags

Common global flags:

| Flag | Use |
| --- | --- |
| `--session <name>` | isolate browser state |
| `--agent` | stable machine output and metadata |
| `--json` | legacy JSON mode |
| `--headed` | visible browser |
| `--cdp <PORT\|URL>` | attach to Chrome |
| `--auto-connect` | discover Chrome on 9222-9229 |
| `--clone-state-from <PORT\|URL>` | hydrate launched browser from live Chrome |
| `--from-profile <auto\|PATH>` | launch from copied profile |
| `--proxy <URL>` / `--proxy-file <FILE>` | launched-browser proxy |
| `--no-js`, `--js-allow`, `--js-block` | per-domain JavaScript policy |

Useful env vars: `EOKA_CDP`, `EOKA_AUTO_CONNECT`, `EOKA_FROM_PROFILE`, `EOKA_PROXY`, `EOKA_PROXY_FILE`, `EOKA_NO_JS`, `EOKA_IDLE_TIMEOUT`, `ANTI_CAPTCHA_KEY`.

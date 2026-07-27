# eoka-server wire protocol

`eoka-server` is a sidecar process wrapping the `eoka` Rust crate (stealth
Chrome automation). It is spawned as a child process and speaks **newline-
delimited JSON-RPC 2.0-ish** over its stdin/stdout. This document is the
contract between `crates/eoka-server` (the Rust implementation) and
`clients/go` (the Go SDK) — both are built independently against this spec.

## Transport

- One JSON object per line on stdin (request) and stdout (response).
  No embedded newlines in a single message; `serde_json::to_writer` +
  a trailing `\n` on the Rust side, `bufio.Scanner`/`bufio.Reader` line
  reads on the Go side.
- stdout carries **only** protocol responses. All logs/diagnostics go to
  stderr (Rust: `tracing` defaulting to stderr). The Go client must never
  parse stderr; it's for humans/debugging only.
- Requests may be pipelined (client can send request 2 before response 1
  arrives), but v1 `eoka-server` processes them **sequentially** in receipt
  order (CDP itself is effectively serial per browser). Responses are
  written in the order requests complete, which — given sequential
  processing — is the same order they were received. Clients MUST match
  responses to requests via `id`, not arrival order, to stay forward
  compatible with a future concurrent server.
- Process lifecycle: server exits (code 0) after it processes a
  `browser.close` request and flushes its response. It also exits (non-zero)
  if stdin closes (EOF) — the Go client treats stdin close as its shutdown
  signal when killing the child.

## Message shapes

### Request

```json
{"id": 1, "method": "page.goto", "params": {"pageId": "ABCD1234", "url": "https://example.com"}}
```

- `id`: JSON number, positive integer, chosen by the client, unique per
  in-flight request. Echoed back verbatim.
- `method`: string, `"<namespace>.<verb>"`.
- `params`: object, method-specific (may be omitted/`{}` for no-arg
  methods, but callers should always send `{}` rather than omitting the
  key, for parser simplicity).

### Success response

```json
{"id": 1, "result": {}}
```

`result` is always a JSON object (possibly empty `{}`), never a bare
scalar/array, so it can grow fields without breaking clients.

### Error response

```json
{"id": 1, "error": {"code": "ElementNotFound", "message": "no element matches selector \"#go\""}}
```

Exactly one of `result` / `error` is present.

## Error codes

Mapped from `eoka::Error`:

| code | meaning |
|---|---|
| `ElementNotFound` | selector matched nothing in the DOM |
| `ElementNotVisible` | selector matched, but element isn't visible/clickable |
| `Timeout` | a `wait_for*` / timed operation exceeded its deadline |
| `RetryExhausted` | `with_retry`-style operation exhausted attempts |
| `Cdp` | raw CDP protocol error passed through |
| `InvalidPage` | `pageId` doesn't refer to a known open page |
| `InvalidParams` | request `params` failed to deserialize / missing required field |
| `UnknownMethod` | `method` not recognized |
| `Internal` | anything else (IO errors, JSON errors, panics caught at boundary) |

`message` is a human-readable string for logs/debugging; Go client code
should branch on `code`, not parse `message`.

## Methods (v1)

`pageId` is the opaque string returned by `browser.new_page` (backed by
`Page::target_id()`). All `page.*` methods take it as the first params
field. Element handles never cross the wire — everything is
`(pageId, selector)`.

### browser.*

| method | params | result |
|---|---|---|
| `browser.launch` | `{"headless": bool, "userAgent"?: string, "proxy"?: {"server": "socks5://host:port", "username"?: string, "password"?: string}}` | `{}` |
| `browser.new_page` | `{"url": string \| null}` (navigates if given, else `about:blank`) | `{"pageId": string}` |
| `browser.tabs` | `{}` | `{"tabs": [{"id": string, "title": string, "url": string}]}` |
| `browser.close_tab` | `{"pageId": string}` | `{}` |
| `browser.close` | `{}` | `{}` — server exits after responding |

### page.*

| method | params | result |
|---|---|---|
| `page.goto` | `{"pageId", "url"}` | `{}` |
| `page.click` | `{"pageId", "selector"}` | `{}` |
| `page.human_click` | `{"pageId", "selector"}` | `{}` |
| `page.fill` | `{"pageId", "selector", "value"}` | `{}` |
| `page.human_fill` | `{"pageId", "selector", "value"}` | `{}` |
| `page.type_into` | `{"pageId", "selector", "text"}` | `{}` |
| `page.text` | `{"pageId"}` | `{"text": string}` |
| `page.content` | `{"pageId"}` | `{"html": string}` |
| `page.title` | `{"pageId"}` | `{"title": string}` |
| `page.url` | `{"pageId"}` | `{"url": string}` |
| `page.get_text` | `{"pageId", "selector"}` | `{"text": string}` (element `.text()`) |
| `page.get_attribute` | `{"pageId", "selector", "name"}` | `{"value": string \| null}` |
| `page.exists` | `{"pageId", "selector"}` | `{"exists": bool}` |
| `page.wait_for` | `{"pageId", "selector", "timeoutMs"}` | `{}` |
| `page.wait_for_visible` | `{"pageId", "selector", "timeoutMs"}` | `{}` |
| `page.wait_for_text` | `{"pageId", "text", "timeoutMs"}` | `{}` |
| `page.evaluate` | `{"pageId", "js"}` | `{"result": <any JSON value>}` |
| `page.execute` | `{"pageId", "js"}` | `{}` |
| `page.fetch` | `{"pageId", "url", "method"?, "headers"?, "body"?, "redirect"?}` | `{"url", "status", "ok", "headers", "body"}` — performs browser-context fetch with page credentials |
| `page.capture_state` | `{"pageId"}` | `{"state": {"cookies", "localStorage", "sessionStorage", "userAgent", "url"}}` |
| `page.restore_state` | `{"pageId", "state": {"cookies", "localStorage", "sessionStorage", "userAgent", "url"}}` | `{}` — restores all cookies, including HttpOnly cookies, then reloads `state.url` |
| `page.screenshot` | `{"pageId"}` | `{"dataBase64": string}` (PNG) |
| `page.select` | `{"pageId", "selector", "value"}` | `{}` |
| `page.hover` | `{"pageId", "selector"}` | `{}` |
| `page.press_key` | `{"pageId", "key"}` | `{}` |
| `page.close` | `{"pageId"}` | `{}` |

## Go-side API shape (informative, not part of the wire contract)

```go
b, err := eoka.Launch(ctx)                       // spawns eoka-server, sends browser.launch
p, err := b.NewPage(ctx, "https://example.com")  // browser.new_page
err = p.Click(ctx, "#submit")
err = p.Fill(ctx, "#user", "bob")
text, err := p.Text(ctx)
png, err := p.Screenshot(ctx)
err = b.Close(ctx)                               // browser.close, waits for child exit
```

Every method takes `context.Context` first for cancellation/timeout. Errors
are typed (`*eoka.Error` with a `Code` field matching the table above) so
callers can `errors.As` / compare `err.Code == eoka.ElementNotFound`.

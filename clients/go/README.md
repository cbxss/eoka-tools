# eoka Go client

A Go client for [`eoka`](https://github.com/cbxss/eoka), a Rust stealth
Chrome-automation library, via its `eoka-server` sidecar process. This
package spawns `eoka-server` as a child process and drives it over
newline-delimited JSON on stdin/stdout, per the contract in
[`PROTOCOL.md`](../../PROTOCOL.md).

## Install

```
go get github.com/shrimp-software/eoka-tools/clients/go
```

Import it as `eoka`:

```go
import eoka "github.com/shrimp-software/eoka-tools/clients/go"
```

## Prerequisites

You need a Chrome/Chromium install and an `eoka-server` binary. The client
locates the binary in this order:

1. `eoka.WithServerPath("/path/to/eoka-server")` passed to `Launch`, if given.
2. The `EOKA_SERVER_BIN` environment variable, if set.
3. `eoka-server` on `PATH`.
4. A prebuilt binary downloaded from this repo's GitHub Releases
   (`eoka-server-v0.1.8`), cached under `os.UserCacheDir()/eoka/` and
   verified against the release's published sha256 checksums before use.
   Supported platforms: linux/amd64, darwin/amd64, darwin/arm64,
   windows/amd64. Pass `eoka.WithNoAutoDownload()` to disable this and get
   `eoka.ErrServerBinaryNotFound` instead if steps 1–3 don't resolve.

For any other platform, or to build from source, run
`cargo build -p eoka-server --release` from this repo and point at the
resulting binary via step 1 or 2 above.

## Usage

```go
ctx := context.Background()

b, err := eoka.Launch(ctx, eoka.WithProxy("socks5://user:password@host:1080"))
if err != nil {
    log.Fatal(err)
}
defer b.Close(ctx)

page, err := b.NewPage(ctx, "https://example.com")
if err != nil {
    log.Fatal(err)
}

if err := page.Fill(ctx, "#user", "bob"); err != nil {
    log.Fatal(err)
}
if err := page.Click(ctx, "#submit"); err != nil {
    log.Fatal(err)
}

text, err := page.Text(ctx)
png, err := page.Screenshot(ctx)
```

`CaptureState` and `RestoreState` preserve a complete authenticated browser
session. They include CDP cookies (including HttpOnly cookies), local storage,
session storage, the user agent, and the current URL. The SDK returns the
typed state; applications should persist it with owner-only permissions.

`WithProxy` accepts authenticated `socks5://` and `http://` URLs. Credentials
are split from the browser proxy endpoint before the JSON-RPC request and are
never included in client validation errors.

See [`example/main.go`](example/main.go) for a full runnable program
(`go run ./example`) — it requires a real `eoka-server` binary and Chrome,
so it is not part of `go test`.

## Errors

Protocol errors from `eoka-server` are `*eoka.Error`, with a `Code` field
matching PROTOCOL.md's error table (`ElementNotFound`, `ElementNotVisible`,
`Timeout`, `RetryExhausted`, `Cdp`, `InvalidPage`, `InvalidParams`,
`UnknownMethod`, `Internal`):

```go
err := page.Click(ctx, "#missing")
if eoka.HasCode(err, eoka.ErrCodeElementNotFound) {
    // ...
}

var eErr *eoka.Error
if errors.As(err, &eErr) {
    fmt.Println(eErr.Code, eErr.Message)
}
```

Transport-level failures (the `eoka-server` process died, its pipes closed,
a response failed to decode) come back as plain Go errors, not `*eoka.Error`
— they aren't protocol error codes from the server.

Every method takes a `context.Context` first; if it's cancelled or times out
while a call is in flight, the method returns `ctx.Err()` and the call is
abandoned (a late response, if one ever arrives, is discarded).

## Testing

`go test ./...` runs entirely against an in-memory fake server built on
`io.Pipe` (see `helpers_test.go`) — no Chrome or `eoka-server` binary
required. It covers request/response correlation, error-code mapping,
context timeout/cleanup, and large (>64KB) payloads to prove the line
reader doesn't truncate.

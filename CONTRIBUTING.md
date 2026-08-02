# Contributing

Thanks for helping improve eoka-tools.

## Development

Install a recent stable Rust toolchain. The Go SDK also requires Go 1.21 or
newer.

```bash
cargo fmt --all --check
cargo clippy --all-features -- -D warnings
cargo test --all-features
```

For the Go SDK:

```bash
cd clients/go
test -z "$(gofmt -l .)"
go vet ./...
go test ./... -race
```

## Pull requests

Keep changes focused and update the relevant crate README when user-facing
behavior changes. Workspace crates should keep their `Cargo.toml` metadata ready
for crates.io.

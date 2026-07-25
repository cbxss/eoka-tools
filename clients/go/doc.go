// Package eoka is a Go client for eoka-server, a sidecar process that
// wraps the eoka Rust stealth-browser-automation crate. The client spawns
// eoka-server as a child process and drives it over newline-delimited JSON
// on stdin/stdout, per the contract in PROTOCOL.md.
//
// Typical usage:
//
//	b, err := eoka.Launch(ctx)
//	p, err := b.NewPage(ctx, "https://example.com")
//	err = p.Click(ctx, "#submit")
//	text, err := p.Text(ctx)
//	err = b.Close(ctx)
package eoka

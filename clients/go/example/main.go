// Command example demonstrates the eoka Go client end-to-end against a real
// eoka-server binary and a real Chrome instance.
//
// This is NOT run as part of `go test` — it requires an eoka-server binary
// (see EOKA_SERVER_BIN / eoka.WithServerPath in the package docs) and a
// Chrome/Chromium install on PATH for eoka-server to launch. Run it with:
//
//	go run ./example
package main

import (
	"context"
	"fmt"
	"log"
	"os"
	"time"

	eoka "github.com/cbxss/eoka-tools/clients/go"
)

func main() {
	if err := run(); err != nil {
		log.Fatalf("example: %v", err)
	}
}

func run() error {
	ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
	defer cancel()

	b, err := eoka.Launch(ctx, eoka.WithHeadless(true), eoka.WithStderr(os.Stderr))
	if err != nil {
		return fmt.Errorf("launch: %w", err)
	}
	defer func() {
		closeCtx, closeCancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer closeCancel()
		if err := b.Close(closeCtx); err != nil {
			log.Printf("close: %v", err)
		}
	}()

	page, err := b.NewPage(ctx, "https://example.com")
	if err != nil {
		return fmt.Errorf("new page: %w", err)
	}

	title, err := page.Title(ctx)
	if err != nil {
		return fmt.Errorf("title: %w", err)
	}
	fmt.Printf("page title: %s\n", title)

	// example.com has no form, but this shows the intended shape: fill a
	// field and click a button by selector. Swap in real selectors for
	// whatever page you're driving.
	if ok, err := page.Exists(ctx, "input#user"); err == nil && ok {
		if err := page.Fill(ctx, "input#user", "bob"); err != nil {
			return fmt.Errorf("fill: %w", err)
		}
		if err := page.Click(ctx, "button#submit"); err != nil {
			return fmt.Errorf("click: %w", err)
		}
	}

	text, err := page.Text(ctx)
	if err != nil {
		return fmt.Errorf("text: %w", err)
	}
	fmt.Printf("page text (first 200 chars): %.200s\n", text)

	png, err := page.Screenshot(ctx)
	if err != nil {
		return fmt.Errorf("screenshot: %w", err)
	}

	f, err := os.CreateTemp("", "eoka-example-*.png")
	if err != nil {
		return fmt.Errorf("create temp file: %w", err)
	}
	defer f.Close()

	if _, err := f.Write(png); err != nil {
		return fmt.Errorf("write screenshot: %w", err)
	}
	fmt.Printf("screenshot written to %s\n", f.Name())

	return nil
}

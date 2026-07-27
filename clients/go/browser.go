package eoka

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"time"
)

const closeTimeout = 5 * time.Second

var ErrServerBinaryNotFound = errors.New("eoka: eoka-server binary not found (set EOKA_SERVER_BIN, pass eoka.WithServerPath, or add eoka-server to PATH)")

type Browser struct {
	cmd *exec.Cmd
	t   *transport
}

type TabInfo struct {
	ID    string `json:"id"`
	Title string `json:"title"`
	URL   string `json:"url"`
}

type options struct {
	headless       bool
	userAgent      string
	serverPath     string
	stderr         io.Writer
	noAutoDownload bool
}

type Option func(*options)

func WithHeadless(headless bool) Option {
	return func(o *options) { o.headless = headless }
}

// WithUserAgent starts the browser with a specific, coherent browser identity.
// Use this when a persisted login session must retain the same fingerprint.
func WithUserAgent(userAgent string) Option {
	return func(o *options) { o.userAgent = userAgent }
}

func WithVisible() Option {
	return WithHeadless(false)
}

func WithServerPath(path string) Option {
	return func(o *options) { o.serverPath = path }
}

func WithStderr(w io.Writer) Option {
	return func(o *options) { o.stderr = w }
}

func WithNoAutoDownload() Option {
	return func(o *options) { o.noAutoDownload = true }
}

func resolveServerPath(ctx context.Context, explicit string, allowDownload bool) (string, error) {
	if explicit != "" {
		return explicit, nil
	}
	if p := os.Getenv("EOKA_SERVER_BIN"); p != "" {
		return p, nil
	}
	if p, err := exec.LookPath("eoka-server"); err == nil {
		return p, nil
	}
	if !allowDownload {
		return "", ErrServerBinaryNotFound
	}
	return ensureServerBinary(ctx)
}

func Launch(ctx context.Context, opts ...Option) (*Browser, error) {
	o := options{headless: true, stderr: io.Discard}
	for _, opt := range opts {
		opt(&o)
	}

	binPath, err := resolveServerPath(ctx, o.serverPath, !o.noAutoDownload)
	if err != nil {
		return nil, err
	}

	cmd := exec.Command(binPath)
	cmd.Stderr = o.stderr

	stdin, err := cmd.StdinPipe()
	if err != nil {
		return nil, fmt.Errorf("eoka: creating stdin pipe: %w", err)
	}
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return nil, fmt.Errorf("eoka: creating stdout pipe: %w", err)
	}

	if err := cmd.Start(); err != nil {
		return nil, fmt.Errorf("eoka: starting %s: %w", binPath, err)
	}

	t := newTransport(stdin, stdout)
	go t.readLoop()

	b := &Browser{cmd: cmd, t: t}

	launchParams := map[string]any{"headless": o.headless}
	if o.userAgent != "" {
		launchParams["userAgent"] = o.userAgent
	}
	if err := t.call(ctx, "browser.launch", launchParams, nil); err != nil {
		t.shutdown()
		b.killProcess()
		_ = cmd.Wait()
		return nil, err
	}

	return b, nil
}

func (b *Browser) NewPage(ctx context.Context, url string) (*Page, error) {
	var urlParam any
	if url != "" {
		urlParam = url
	}

	var res struct {
		PageID string `json:"pageId"`
	}
	if err := b.t.call(ctx, "browser.new_page", map[string]any{"url": urlParam}, &res); err != nil {
		return nil, err
	}
	return &Page{id: res.PageID, b: b}, nil
}

func (b *Browser) Tabs(ctx context.Context) ([]TabInfo, error) {
	var res struct {
		Tabs []TabInfo `json:"tabs"`
	}
	err := b.t.call(ctx, "browser.tabs", map[string]any{}, &res)
	return res.Tabs, err
}

func (b *Browser) CloseTab(ctx context.Context, pageID string) error {
	return b.t.call(ctx, "browser.close_tab", map[string]any{"pageId": pageID}, nil)
}

func (b *Browser) Close(ctx context.Context) error {
	callErr := b.t.call(ctx, "browser.close", map[string]any{}, nil)
	t := time.NewTimer(closeTimeout)
	defer t.Stop()

	done := make(chan error, 1)
	go func() { done <- b.cmd.Wait() }()

	select {
	case <-done:
	case <-ctx.Done():
		b.killProcess()
		<-done
		if callErr == nil {
			callErr = ctx.Err()
		}
	case <-t.C:
		b.killProcess()
		<-done
	}

	return callErr
}

func (b *Browser) killProcess() {
	if b.cmd.Process != nil {
		_ = b.cmd.Process.Kill()
	}
}

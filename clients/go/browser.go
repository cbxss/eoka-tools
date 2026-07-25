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

// closeTimeout bounds how long Close waits for the eoka-server child
// process to exit gracefully after browser.close before force-killing it.
const closeTimeout = 5 * time.Second

// ErrServerBinaryNotFound is returned by Launch when the eoka-server binary
// cannot be located via WithServerPath, the EOKA_SERVER_BIN environment
// variable, or the PATH.
var ErrServerBinaryNotFound = errors.New("eoka: eoka-server binary not found (set EOKA_SERVER_BIN, pass eoka.WithServerPath, or add eoka-server to PATH)")

// Browser is a running eoka-server child process controlling one Chrome
// instance. Pages created from it (via NewPage) share its connection.
type Browser struct {
	cmd *exec.Cmd
	t   *transport
}

// TabInfo describes one open browser tab, as returned by Browser.Tabs.
type TabInfo struct {
	ID    string `json:"id"`
	Title string `json:"title"`
	URL   string `json:"url"`
}

type options struct {
	headless   bool
	serverPath string
	stderr     io.Writer
}

// Option configures Launch.
type Option func(*options)

// WithHeadless sets whether Chrome runs headless. Launch defaults to true.
func WithHeadless(headless bool) Option {
	return func(o *options) { o.headless = headless }
}

// WithVisible launches Chrome with a visible window; shorthand for WithHeadless(false).
func WithVisible() Option {
	return WithHeadless(false)
}

// WithServerPath sets an explicit path to the eoka-server binary. It takes
// precedence over the EOKA_SERVER_BIN environment variable and the PATH.
func WithServerPath(path string) Option {
	return func(o *options) { o.serverPath = path }
}

// WithStderr redirects the eoka-server child process's stderr, which
// carries logs/diagnostics only (never protocol data), to w. By default
// stderr is discarded.
func WithStderr(w io.Writer) Option {
	return func(o *options) { o.stderr = w }
}

// resolveServerPath picks the eoka-server binary: an explicit WithServerPath
// option first, then EOKA_SERVER_BIN, then a PATH lookup.
func resolveServerPath(explicit string) (string, error) {
	if explicit != "" {
		return explicit, nil
	}
	if p := os.Getenv("EOKA_SERVER_BIN"); p != "" {
		return p, nil
	}
	if p, err := exec.LookPath("eoka-server"); err == nil {
		return p, nil
	}
	return "", ErrServerBinaryNotFound
}

// Launch spawns the eoka-server binary as a child process and starts a
// browser instance in it (sending browser.launch). ctx governs only the
// launch handshake; once Launch returns successfully the process outlives
// ctx and is controlled via Browser.Close.
func Launch(ctx context.Context, opts ...Option) (*Browser, error) {
	o := options{headless: true, stderr: io.Discard}
	for _, opt := range opts {
		opt(&o)
	}

	binPath, err := resolveServerPath(o.serverPath)
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

	if err := t.call(ctx, "browser.launch", map[string]any{"headless": o.headless}, nil); err != nil {
		t.shutdown()
		b.killProcess()
		_ = cmd.Wait()
		return nil, err
	}

	return b, nil
}

// NewPage opens a new browser tab (browser.new_page). An empty url navigates
// to about:blank; otherwise the tab navigates to url before returning.
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

// Tabs lists all currently open tabs (browser.tabs).
func (b *Browser) Tabs(ctx context.Context) ([]TabInfo, error) {
	var res struct {
		Tabs []TabInfo `json:"tabs"`
	}
	err := b.t.call(ctx, "browser.tabs", map[string]any{}, &res)
	return res.Tabs, err
}

// CloseTab closes the tab identified by pageID (browser.close_tab).
func (b *Browser) CloseTab(ctx context.Context, pageID string) error {
	return b.t.call(ctx, "browser.close_tab", map[string]any{"pageId": pageID}, nil)
}

// Close sends browser.close and waits for the eoka-server child process to
// exit, force-killing it if it doesn't exit within a few seconds or ctx is
// done first.
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

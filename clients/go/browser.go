package eoka

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net"
	"net/url"
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
	timezone       string
	viewportWidth  int
	viewportHeight int
	proxy          string
	userDataDir    string
	serverPath     string
	stderr         io.Writer
	noAutoDownload bool
}

type Option func(*options)

func WithHeadless(headless bool) Option {
	return func(o *options) { o.headless = headless }
}

func WithUserAgent(userAgent string) Option {
	return func(o *options) { o.userAgent = userAgent }
}

// WithTimezone sets the browser's IANA timezone, for example
// "America/Los_Angeles". Keep it coherent with the network used by the
// browser instead of accepting a randomized stealth default.
func WithTimezone(timezone string) Option {
	return func(o *options) { o.timezone = timezone }
}

// WithViewport sets the browser viewport and virtual screen dimensions. Both
// values must be positive; invalid dimensions are rejected before launch.
func WithViewport(width, height int) Option {
	return func(o *options) {
		o.viewportWidth = width
		o.viewportHeight = height
	}
}

func WithProxy(proxyURL string) Option {
	return func(o *options) { o.proxy = proxyURL }
}

// WithUserDataDir keeps a dedicated Chromium profile between launches. Use a
// private, single-purpose directory; it contains browser state in addition to
// any session file the caller manages separately.
func WithUserDataDir(path string) Option {
	return func(o *options) { o.userDataDir = path }
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
	launchParams, err := o.launchParams()
	if err != nil {
		return nil, err
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

	if err := t.call(ctx, "browser.launch", launchParams, nil); err != nil {
		t.shutdown()
		b.killProcess()
		_ = cmd.Wait()
		return nil, err
	}

	return b, nil
}

func (o options) launchParams() (map[string]any, error) {
	params := map[string]any{"headless": o.headless}
	if o.userAgent != "" {
		params["userAgent"] = o.userAgent
	}
	if o.timezone != "" {
		params["timezone"] = o.timezone
	}
	if o.userDataDir != "" {
		params["userDataDir"] = o.userDataDir
	}
	if o.viewportWidth != 0 || o.viewportHeight != 0 {
		if o.viewportWidth <= 0 || o.viewportHeight <= 0 {
			return nil, fmt.Errorf("eoka: viewport dimensions must be positive")
		}
		params["viewportWidth"] = o.viewportWidth
		params["viewportHeight"] = o.viewportHeight
	}
	if o.proxy == "" {
		return params, nil
	}
	proxy, err := parseProxy(o.proxy)
	if err != nil {
		return nil, err
	}
	params["proxy"] = map[string]any{
		"server":   proxy.server,
		"username": proxy.username,
		"password": proxy.password,
	}
	return params, nil
}

type proxyConfig struct {
	server   string
	username string
	password string
}

func parseProxy(value string) (proxyConfig, error) {
	parsed, err := url.Parse(value)
	if err != nil || (parsed.Scheme != "socks5" && parsed.Scheme != "http") {
		return proxyConfig{}, errors.New("eoka: proxy must use socks5:// or http://")
	}
	if parsed.Hostname() == "" || parsed.Port() == "" {
		return proxyConfig{}, errors.New("eoka: proxy must include host and port")
	}
	if (parsed.Path != "" && parsed.Path != "/") || parsed.RawQuery != "" || parsed.Fragment != "" {
		return proxyConfig{}, errors.New("eoka: proxy cannot include a path, query, or fragment")
	}
	username := ""
	password := ""
	if parsed.User != nil {
		username = parsed.User.Username()
		password, _ = parsed.User.Password()
	}
	if (username == "") != (password == "") {
		return proxyConfig{}, errors.New("eoka: proxy credentials require both username and password")
	}
	return proxyConfig{
		server:   parsed.Scheme + "://" + net.JoinHostPort(parsed.Hostname(), parsed.Port()),
		username: username,
		password: password,
	}, nil
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

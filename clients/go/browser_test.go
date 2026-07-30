package eoka

import (
	"context"
	"encoding/json"
	"strings"
	"testing"
)

func TestLaunchParamsIncludeRedactedProxyFields(t *testing.T) {
	params, err := (options{headless: true, proxy: "socks5://name%40example:pass%3Aword@127.0.0.1:1080"}).launchParams()
	if err != nil {
		t.Fatalf("launchParams: %v", err)
	}
	proxy, ok := params["proxy"].(map[string]any)
	if !ok {
		t.Fatalf("proxy params missing: %#v", params)
	}
	if got, want := proxy["server"], "socks5://127.0.0.1:1080"; got != want {
		t.Fatalf("server = %v, want %v", got, want)
	}
	if got, want := proxy["username"], "name@example"; got != want {
		t.Fatalf("username = %v, want %v", got, want)
	}
	if got, want := proxy["password"], "pass:word"; got != want {
		t.Fatalf("password = %v, want %v", got, want)
	}
}

func TestLaunchParamsRejectInvalidProxy(t *testing.T) {
	_, err := (options{proxy: "https://127.0.0.1:1080"}).launchParams()
	if err == nil {
		t.Fatal("launchParams succeeded for an unsupported proxy")
	}
	if got := err.Error(); strings.Contains(got, "127.0.0.1") {
		t.Fatalf("proxy error leaked input: %q", got)
	}
}

func TestLaunchParamsIncludeTimezoneAndViewport(t *testing.T) {
	params, err := (options{
		headless:       true,
		timezone:       "America/Los_Angeles",
		viewportWidth:  1920,
		viewportHeight: 1080,
	}).launchParams()
	if err != nil {
		t.Fatalf("launchParams: %v", err)
	}
	if got, want := params["timezone"], "America/Los_Angeles"; got != want {
		t.Fatalf("timezone = %v, want %v", got, want)
	}
	if got, want := params["viewportWidth"], 1920; got != want {
		t.Fatalf("viewportWidth = %v, want %v", got, want)
	}
	if got, want := params["viewportHeight"], 1080; got != want {
		t.Fatalf("viewportHeight = %v, want %v", got, want)
	}
}

func TestLaunchParamsIncludeUserDataDir(t *testing.T) {
	params, err := (options{headless: true, userDataDir: "/private/profile"}).launchParams()
	if err != nil {
		t.Fatalf("launchParams: %v", err)
	}
	if got, want := params["userDataDir"], "/private/profile"; got != want {
		t.Fatalf("userDataDir = %v, want %v", got, want)
	}
}

func TestLaunchParamsRejectIncompleteViewport(t *testing.T) {
	_, err := (options{viewportWidth: 1920}).launchParams()
	if err == nil || !strings.Contains(err.Error(), "viewport") {
		t.Fatalf("launchParams error = %v, want viewport validation error", err)
	}
}

func TestBrowserNewPageTabsCloseTab(t *testing.T) {
	var lastNewPageURL any
	haveURL := false

	b := newFakeBrowser(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		switch method {
		case "browser.new_page":
			var p struct {
				URL *string `json:"url"`
			}
			_ = json.Unmarshal(params, &p)
			haveURL = true
			if p.URL != nil {
				lastNewPageURL = *p.URL
			} else {
				lastNewPageURL = nil
			}
			return map[string]any{"pageId": "PAGE1"}, nil
		case "browser.tabs":
			return map[string]any{"tabs": []TabInfo{{ID: "PAGE1", Title: "Example", URL: "https://example.com/"}}}, nil
		case "browser.close_tab":
			var p struct {
				PageID string `json:"pageId"`
			}
			_ = json.Unmarshal(params, &p)
			if p.PageID != "PAGE1" {
				return nil, &rpcError{Code: ErrCodeInvalidPage, Message: "unknown pageId"}
			}
			return map[string]any{}, nil
		}
		return nil, &rpcError{Code: ErrCodeUnknownMethod, Message: method}
	})

	ctx := context.Background()

	page, err := b.NewPage(ctx, "https://example.com")
	if err != nil {
		t.Fatalf("NewPage: %v", err)
	}
	if page.ID() != "PAGE1" {
		t.Fatalf("unexpected page id: %s", page.ID())
	}
	if !haveURL || lastNewPageURL != "https://example.com" {
		t.Fatalf("expected url param %q, got %v", "https://example.com", lastNewPageURL)
	}

	if _, err := b.NewPage(ctx, ""); err != nil {
		t.Fatalf("NewPage(empty): %v", err)
	}
	if lastNewPageURL != nil {
		t.Fatalf("expected null url param for empty string, got %v", lastNewPageURL)
	}

	tabs, err := b.Tabs(ctx)
	if err != nil {
		t.Fatalf("Tabs: %v", err)
	}
	if len(tabs) != 1 || tabs[0].ID != "PAGE1" || tabs[0].Title != "Example" {
		t.Fatalf("unexpected tabs: %+v", tabs)
	}

	if err := b.CloseTab(ctx, "PAGE1"); err != nil {
		t.Fatalf("CloseTab: %v", err)
	}
	if err := b.CloseTab(ctx, "NOPE"); !HasCode(err, ErrCodeInvalidPage) {
		t.Fatalf("expected InvalidPage error, got %v", err)
	}
}

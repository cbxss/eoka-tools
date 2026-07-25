package eoka

import (
	"context"
	"encoding/json"
	"testing"
)

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
	if err := b.CloseTab(ctx, "NOPE"); !IsInvalidPage(err) {
		t.Fatalf("expected InvalidPage error, got %v", err)
	}
}

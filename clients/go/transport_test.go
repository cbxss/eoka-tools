package eoka

import (
	"context"
	"encoding/json"
	"errors"
	"strings"
	"testing"
	"time"
)

func TestTransportRoundTrip(t *testing.T) {
	tr := newFakeTransport(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		if method != "browser.tabs" {
			return nil, &rpcError{Code: ErrCodeUnknownMethod, Message: "unexpected method " + method}
		}
		return map[string]any{"tabs": []TabInfo{{ID: "abc", Title: "Example", URL: "https://example.com"}}}, nil
	})

	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	var res struct {
		Tabs []TabInfo `json:"tabs"`
	}
	if err := tr.call(ctx, "browser.tabs", map[string]any{}, &res); err != nil {
		t.Fatalf("call: %v", err)
	}
	if len(res.Tabs) != 1 || res.Tabs[0].ID != "abc" || res.Tabs[0].Title != "Example" {
		t.Fatalf("unexpected tabs: %+v", res.Tabs)
	}
}

func TestTransportErrorMapping(t *testing.T) {
	tr := newFakeTransport(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		return nil, &rpcError{Code: ErrCodeElementNotFound, Message: `no element matches selector "#go"`}
	})

	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	err := tr.call(ctx, "page.click", map[string]any{"pageId": "p1", "selector": "#go"}, nil)
	if err == nil {
		t.Fatal("expected an error")
	}

	var typed *Error
	if !errors.As(err, &typed) {
		t.Fatalf("expected *eoka.Error, got %T: %v", err, err)
	}
	if typed.Code != ErrCodeElementNotFound {
		t.Fatalf("unexpected code: %s", typed.Code)
	}
	if !HasCode(err, ErrCodeElementNotFound) {
		t.Fatal("HasCode(err, ErrCodeElementNotFound) = false, want true")
	}
	if HasCode(err, ErrCodeTimeout) {
		t.Fatal("HasCode(err, ErrCodeTimeout) = true, want false")
	}
}

func TestTransportContextTimeout(t *testing.T) {
	block := make(chan struct{})
	t.Cleanup(func() { close(block) })

	tr := newFakeTransport(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		<-block // never respond before the test's context deadline
		return map[string]any{}, nil
	})

	ctx, cancel := context.WithTimeout(context.Background(), 50*time.Millisecond)
	defer cancel()

	err := tr.call(ctx, "page.wait_for", map[string]any{"pageId": "p1", "selector": "#x", "timeoutMs": int64(1000)}, nil)
	if !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("expected context.DeadlineExceeded, got %v", err)
	}

	tr.mu.Lock()
	n := len(tr.pending)
	tr.mu.Unlock()
	if n != 0 {
		t.Fatalf("pending map leaked %d entries after context timeout", n)
	}
}

func TestTransportLargePayload(t *testing.T) {
	const size = 200 * 1024 // well over bufio.Scanner's default 64KB max token size
	want := strings.Repeat("eoka-html-payload-", size/18+1)[:size]

	tr := newFakeTransport(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		return map[string]any{"html": want}, nil
	})

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	var res struct {
		HTML string `json:"html"`
	}
	if err := tr.call(ctx, "page.content", map[string]any{"pageId": "p1"}, &res); err != nil {
		t.Fatalf("call: %v", err)
	}
	if len(res.HTML) != size {
		t.Fatalf("payload truncated: got %d bytes, want %d", len(res.HTML), size)
	}
	if res.HTML != want {
		t.Fatal("payload content does not round-trip exactly")
	}
}

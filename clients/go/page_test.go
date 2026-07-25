package eoka

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"testing"
	"time"
)

func newFakePage(t *testing.T, handle fakeHandler) *Page {
	t.Helper()
	b := newFakeBrowser(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		var p map[string]any
		_ = json.Unmarshal(params, &p)
		if p["pageId"] != "P1" {
			return nil, &rpcError{Code: ErrCodeInvalidPage, Message: "bad pageId"}
		}
		return handle(id, method, params)
	})
	return &Page{id: "P1", b: b}
}

func TestPageNoResultMethods(t *testing.T) {
	seen := map[string]bool{}
	page := newFakePage(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		seen[method] = true
		return map[string]any{}, nil
	})

	ctx := context.Background()
	checks := []struct {
		name string
		call func() error
	}{
		{"page.goto", func() error { return page.Goto(ctx, "https://example.com") }},
		{"page.click", func() error { return page.Click(ctx, "#go") }},
		{"page.human_click", func() error { return page.HumanClick(ctx, "#go") }},
		{"page.fill", func() error { return page.Fill(ctx, "#user", "bob") }},
		{"page.human_fill", func() error { return page.HumanFill(ctx, "#user", "bob") }},
		{"page.type_into", func() error { return page.TypeInto(ctx, "#user", "bob") }},
		{"page.wait_for", func() error { return page.WaitFor(ctx, "#go", time.Second) }},
		{"page.wait_for_visible", func() error { return page.WaitForVisible(ctx, "#go", time.Second) }},
		{"page.wait_for_text", func() error { return page.WaitForText(ctx, "hello", time.Second) }},
		{"page.execute", func() error { return page.Execute(ctx, "1+1") }},
		{"page.select", func() error { return page.Select(ctx, "#opt", "a") }},
		{"page.hover", func() error { return page.Hover(ctx, "#go") }},
		{"page.press_key", func() error { return page.PressKey(ctx, "Enter") }},
		{"page.close", func() error { return page.Close(ctx) }},
	}

	for _, c := range checks {
		if err := c.call(); err != nil {
			t.Fatalf("%s: %v", c.name, err)
		}
		if !seen[c.name] {
			t.Fatalf("%s: server never saw the expected method", c.name)
		}
	}
}

func TestPageGetters(t *testing.T) {
	page := newFakePage(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		switch method {
		case "page.text":
			return map[string]any{"text": "hello world"}, nil
		case "page.content":
			return map[string]any{"html": "<html></html>"}, nil
		case "page.title":
			return map[string]any{"title": "Example Domain"}, nil
		case "page.url":
			return map[string]any{"url": "https://example.com/"}, nil
		case "page.get_text":
			return map[string]any{"text": "Click me"}, nil
		case "page.exists":
			return map[string]any{"exists": true}, nil
		}
		return nil, &rpcError{Code: ErrCodeUnknownMethod, Message: method}
	})

	ctx := context.Background()

	if v, err := page.Text(ctx); err != nil || v != "hello world" {
		t.Fatalf("Text: %q, %v", v, err)
	}
	if v, err := page.Content(ctx); err != nil || v != "<html></html>" {
		t.Fatalf("Content: %q, %v", v, err)
	}
	if v, err := page.Title(ctx); err != nil || v != "Example Domain" {
		t.Fatalf("Title: %q, %v", v, err)
	}
	if v, err := page.URL(ctx); err != nil || v != "https://example.com/" {
		t.Fatalf("URL: %q, %v", v, err)
	}
	if v, err := page.GetText(ctx, "a"); err != nil || v != "Click me" {
		t.Fatalf("GetText: %q, %v", v, err)
	}
	if v, err := page.Exists(ctx, "a"); err != nil || !v {
		t.Fatalf("Exists: %v, %v", v, err)
	}
}

func TestPageGetAttribute(t *testing.T) {
	page := newFakePage(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		var p struct {
			Name string `json:"name"`
		}
		_ = json.Unmarshal(params, &p)
		if p.Name == "href" {
			return map[string]any{"value": "https://example.com/more"}, nil
		}
		return map[string]any{"value": nil}, nil
	})

	ctx := context.Background()

	v, ok, err := page.GetAttribute(ctx, "a", "href")
	if err != nil || !ok || v != "https://example.com/more" {
		t.Fatalf("GetAttribute(href): %q, %v, %v", v, ok, err)
	}

	v, ok, err = page.GetAttribute(ctx, "a", "data-missing")
	if err != nil || ok || v != "" {
		t.Fatalf("GetAttribute(data-missing): %q, %v, %v", v, ok, err)
	}
}

func TestPageEvaluate(t *testing.T) {
	page := newFakePage(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		var p struct {
			JS string `json:"js"`
		}
		_ = json.Unmarshal(params, &p)
		switch p.JS {
		case "1+1":
			return map[string]any{"result": 2}, nil
		case "document.title":
			return map[string]any{"result": "Example Domain"}, nil
		}
		return map[string]any{"result": nil}, nil
	})

	ctx := context.Background()

	raw, err := page.Evaluate(ctx, "1+1")
	if err != nil || string(raw) != "2" {
		t.Fatalf("Evaluate: %s, %v", raw, err)
	}

	title, err := EvaluateAs[string](ctx, page, "document.title")
	if err != nil || title != "Example Domain" {
		t.Fatalf("EvaluateAs[string]: %q, %v", title, err)
	}

	n, err := EvaluateAs[int](ctx, page, "1+1")
	if err != nil || n != 2 {
		t.Fatalf("EvaluateAs[int]: %d, %v", n, err)
	}
}

func TestPageScreenshot(t *testing.T) {
	want := []byte("\x89PNGfake-image-bytes")
	page := newFakePage(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		return map[string]any{"dataBase64": base64.StdEncoding.EncodeToString(want)}, nil
	})

	got, err := page.Screenshot(context.Background())
	if err != nil {
		t.Fatalf("Screenshot: %v", err)
	}
	if string(got) != string(want) {
		t.Fatalf("Screenshot bytes mismatch: got %q, want %q", got, want)
	}
}

func TestPageErrorPropagation(t *testing.T) {
	page := newFakePage(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		return nil, &rpcError{Code: ErrCodeElementNotVisible, Message: "not visible"}
	})

	err := page.Click(context.Background(), "#hidden")
	if !IsElementNotVisible(err) {
		t.Fatalf("expected ElementNotVisible, got %v", err)
	}
}

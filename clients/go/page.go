package eoka

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"time"
)

// Page is a handle to one open browser tab. It holds no local DOM state —
// every method round-trips to eoka-server via (pageId, selector), per
// PROTOCOL.md. Pages share their owning Browser's transport/process.
type Page struct {
	id string
	b  *Browser
}

// ID returns the page's opaque identifier (Page::target_id() on the server).
func (p *Page) ID() string { return p.id }

// call issues a page.* request, automatically attaching this page's id.
func (p *Page) call(ctx context.Context, method string, params map[string]any, result any) error {
	if params == nil {
		params = map[string]any{}
	}
	params["pageId"] = p.id
	return p.b.t.call(ctx, method, params, result)
}

// Goto navigates the page to url (page.goto).
func (p *Page) Goto(ctx context.Context, url string) error {
	return p.call(ctx, "page.goto", map[string]any{"url": url}, nil)
}

// Click clicks the first element matching selector (page.click).
func (p *Page) Click(ctx context.Context, selector string) error {
	return p.call(ctx, "page.click", map[string]any{"selector": selector}, nil)
}

// HumanClick clicks selector using human-like pointer movement (page.human_click).
func (p *Page) HumanClick(ctx context.Context, selector string) error {
	return p.call(ctx, "page.human_click", map[string]any{"selector": selector}, nil)
}

// Fill clears selector's value and types value into it (page.fill).
func (p *Page) Fill(ctx context.Context, selector, value string) error {
	return p.call(ctx, "page.fill", map[string]any{"selector": selector, "value": value}, nil)
}

// HumanFill clears selector and types value with human-like timing (page.human_fill).
func (p *Page) HumanFill(ctx context.Context, selector, value string) error {
	return p.call(ctx, "page.human_fill", map[string]any{"selector": selector, "value": value}, nil)
}

// TypeInto types text into selector without clearing its existing value (page.type_into).
func (p *Page) TypeInto(ctx context.Context, selector, text string) error {
	return p.call(ctx, "page.type_into", map[string]any{"selector": selector, "text": text}, nil)
}

// Text returns the page's visible text content (page.text).
func (p *Page) Text(ctx context.Context) (string, error) {
	var res struct {
		Text string `json:"text"`
	}
	err := p.call(ctx, "page.text", nil, &res)
	return res.Text, err
}

// Content returns the page's full HTML source (page.content).
func (p *Page) Content(ctx context.Context) (string, error) {
	var res struct {
		HTML string `json:"html"`
	}
	err := p.call(ctx, "page.content", nil, &res)
	return res.HTML, err
}

// Title returns the page's document title (page.title).
func (p *Page) Title(ctx context.Context) (string, error) {
	var res struct {
		Title string `json:"title"`
	}
	err := p.call(ctx, "page.title", nil, &res)
	return res.Title, err
}

// URL returns the page's current URL (page.url).
func (p *Page) URL(ctx context.Context) (string, error) {
	var res struct {
		URL string `json:"url"`
	}
	err := p.call(ctx, "page.url", nil, &res)
	return res.URL, err
}

// GetText returns the visible text of the first element matching selector (page.get_text).
func (p *Page) GetText(ctx context.Context, selector string) (string, error) {
	var res struct {
		Text string `json:"text"`
	}
	err := p.call(ctx, "page.get_text", map[string]any{"selector": selector}, &res)
	return res.Text, err
}

// GetAttribute returns the named attribute of the first element matching
// selector (page.get_attribute). ok is false if the attribute is absent.
func (p *Page) GetAttribute(ctx context.Context, selector, name string) (value string, ok bool, err error) {
	var res struct {
		Value *string `json:"value"`
	}
	if err = p.call(ctx, "page.get_attribute", map[string]any{"selector": selector, "name": name}, &res); err != nil {
		return "", false, err
	}
	if res.Value == nil {
		return "", false, nil
	}
	return *res.Value, true, nil
}

// Exists reports whether selector matches any element in the DOM (page.exists).
func (p *Page) Exists(ctx context.Context, selector string) (bool, error) {
	var res struct {
		Exists bool `json:"exists"`
	}
	err := p.call(ctx, "page.exists", map[string]any{"selector": selector}, &res)
	return res.Exists, err
}

// WaitFor waits for selector to appear in the DOM, up to timeout (page.wait_for).
func (p *Page) WaitFor(ctx context.Context, selector string, timeout time.Duration) error {
	return p.call(ctx, "page.wait_for", map[string]any{"selector": selector, "timeoutMs": timeout.Milliseconds()}, nil)
}

// WaitForVisible waits for selector to become visible/clickable, up to timeout (page.wait_for_visible).
func (p *Page) WaitForVisible(ctx context.Context, selector string, timeout time.Duration) error {
	return p.call(ctx, "page.wait_for_visible", map[string]any{"selector": selector, "timeoutMs": timeout.Milliseconds()}, nil)
}

// WaitForText waits for text to appear on the page, up to timeout (page.wait_for_text).
func (p *Page) WaitForText(ctx context.Context, text string, timeout time.Duration) error {
	return p.call(ctx, "page.wait_for_text", map[string]any{"text": text, "timeoutMs": timeout.Milliseconds()}, nil)
}

// Evaluate runs js in the page and returns its result as raw JSON (page.evaluate).
// Use EvaluateAs to decode the result into a Go type in one step.
func (p *Page) Evaluate(ctx context.Context, js string) (json.RawMessage, error) {
	var res struct {
		Result json.RawMessage `json:"result"`
	}
	err := p.call(ctx, "page.evaluate", map[string]any{"js": js}, &res)
	return res.Result, err
}

// EvaluateAs runs js in page and decodes its JSON result into a value of type T.
func EvaluateAs[T any](ctx context.Context, page *Page, js string) (T, error) {
	var out T
	raw, err := page.Evaluate(ctx, js)
	if err != nil {
		return out, err
	}
	if len(raw) == 0 || string(raw) == "null" {
		return out, nil
	}
	if err := json.Unmarshal(raw, &out); err != nil {
		return out, fmt.Errorf("eoka: decoding evaluate result: %w", err)
	}
	return out, nil
}

// Execute runs js in the page, discarding its result (page.execute).
func (p *Page) Execute(ctx context.Context, js string) error {
	return p.call(ctx, "page.execute", map[string]any{"js": js}, nil)
}

// Screenshot captures the page as a PNG (page.screenshot), decoding the
// server's base64 payload into raw image bytes.
func (p *Page) Screenshot(ctx context.Context) ([]byte, error) {
	var res struct {
		DataBase64 string `json:"dataBase64"`
	}
	if err := p.call(ctx, "page.screenshot", nil, &res); err != nil {
		return nil, err
	}
	data, err := base64.StdEncoding.DecodeString(res.DataBase64)
	if err != nil {
		return nil, fmt.Errorf("eoka: decoding screenshot data: %w", err)
	}
	return data, nil
}

// Select sets a <select> element's value (page.select).
func (p *Page) Select(ctx context.Context, selector, value string) error {
	return p.call(ctx, "page.select", map[string]any{"selector": selector, "value": value}, nil)
}

// Hover moves the mouse over the first element matching selector (page.hover).
func (p *Page) Hover(ctx context.Context, selector string) error {
	return p.call(ctx, "page.hover", map[string]any{"selector": selector}, nil)
}

// PressKey presses a key, optionally with modifiers such as "Ctrl+A" (page.press_key).
func (p *Page) PressKey(ctx context.Context, key string) error {
	return p.call(ctx, "page.press_key", map[string]any{"key": key}, nil)
}

// Close closes this tab (page.close).
func (p *Page) Close(ctx context.Context) error {
	return p.call(ctx, "page.close", nil, nil)
}

package eoka

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"time"
)

type Page struct {
	id string
	b  *Browser
}

func (p *Page) ID() string { return p.id }

func (p *Page) call(ctx context.Context, method string, params map[string]any, result any) error {
	if params == nil {
		params = map[string]any{}
	}
	params["pageId"] = p.id
	return p.b.t.call(ctx, method, params, result)
}

func (p *Page) Goto(ctx context.Context, url string) error {
	return p.call(ctx, "page.goto", map[string]any{"url": url}, nil)
}

func (p *Page) Click(ctx context.Context, selector string) error {
	return p.call(ctx, "page.click", map[string]any{"selector": selector}, nil)
}

func (p *Page) HumanClick(ctx context.Context, selector string) error {
	return p.call(ctx, "page.human_click", map[string]any{"selector": selector}, nil)
}

func (p *Page) Fill(ctx context.Context, selector, value string) error {
	return p.call(ctx, "page.fill", map[string]any{"selector": selector, "value": value}, nil)
}

func (p *Page) HumanFill(ctx context.Context, selector, value string) error {
	return p.call(ctx, "page.human_fill", map[string]any{"selector": selector, "value": value}, nil)
}

func (p *Page) TypeInto(ctx context.Context, selector, text string) error {
	return p.call(ctx, "page.type_into", map[string]any{"selector": selector, "text": text}, nil)
}

func (p *Page) Text(ctx context.Context) (string, error) {
	var res struct {
		Text string `json:"text"`
	}
	err := p.call(ctx, "page.text", nil, &res)
	return res.Text, err
}

func (p *Page) Content(ctx context.Context) (string, error) {
	var res struct {
		HTML string `json:"html"`
	}
	err := p.call(ctx, "page.content", nil, &res)
	return res.HTML, err
}

func (p *Page) Title(ctx context.Context) (string, error) {
	var res struct {
		Title string `json:"title"`
	}
	err := p.call(ctx, "page.title", nil, &res)
	return res.Title, err
}

func (p *Page) URL(ctx context.Context) (string, error) {
	var res struct {
		URL string `json:"url"`
	}
	err := p.call(ctx, "page.url", nil, &res)
	return res.URL, err
}

func (p *Page) GetText(ctx context.Context, selector string) (string, error) {
	var res struct {
		Text string `json:"text"`
	}
	err := p.call(ctx, "page.get_text", map[string]any{"selector": selector}, &res)
	return res.Text, err
}

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

func (p *Page) Exists(ctx context.Context, selector string) (bool, error) {
	var res struct {
		Exists bool `json:"exists"`
	}
	err := p.call(ctx, "page.exists", map[string]any{"selector": selector}, &res)
	return res.Exists, err
}

func (p *Page) WaitFor(ctx context.Context, selector string, timeout time.Duration) error {
	return p.call(ctx, "page.wait_for", map[string]any{"selector": selector, "timeoutMs": timeout.Milliseconds()}, nil)
}

func (p *Page) WaitForVisible(ctx context.Context, selector string, timeout time.Duration) error {
	return p.call(ctx, "page.wait_for_visible", map[string]any{"selector": selector, "timeoutMs": timeout.Milliseconds()}, nil)
}

func (p *Page) WaitForText(ctx context.Context, text string, timeout time.Duration) error {
	return p.call(ctx, "page.wait_for_text", map[string]any{"text": text, "timeoutMs": timeout.Milliseconds()}, nil)
}

func (p *Page) Evaluate(ctx context.Context, js string) (json.RawMessage, error) {
	var res struct {
		Result json.RawMessage `json:"result"`
	}
	err := p.call(ctx, "page.evaluate", map[string]any{"js": js}, &res)
	return res.Result, err
}

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

func (p *Page) Execute(ctx context.Context, js string) error {
	return p.call(ctx, "page.execute", map[string]any{"js": js}, nil)
}

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

func (p *Page) Select(ctx context.Context, selector, value string) error {
	return p.call(ctx, "page.select", map[string]any{"selector": selector, "value": value}, nil)
}

func (p *Page) Hover(ctx context.Context, selector string) error {
	return p.call(ctx, "page.hover", map[string]any{"selector": selector}, nil)
}

func (p *Page) PressKey(ctx context.Context, key string) error {
	return p.call(ctx, "page.press_key", map[string]any{"key": key}, nil)
}

func (p *Page) Close(ctx context.Context) error {
	return p.call(ctx, "page.close", nil, nil)
}
